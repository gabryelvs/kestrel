//! The Tauri side of the benchmark: the real platform (start key, beeps, probes, events), the commands the UI calls,
//! and helpers the commands share. Every command that touches the controller runs off the UI thread (`async`),
//! because claiming and releasing the start key need the UI thread free.
use super::beep;
use super::capture::Capture;
use super::controller::{BenchState, Controller, Platform, ACTIVE_MARKER};
use crate::hardware;
use kestrel_bench::{
    result, settings, text, Benchmark, BenchmarkSummary, Comparison, Context, GameProfile, GpuLink,
    Store, ValorantSettings, PROFILES,
};
use kestrel_core::Report;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut};

/// Points in a result's frame-time graph.
const GRAPH_POINTS: usize = 600;
const NOT_FOUND: &str = "That benchmark doesn't exist.";

/// The start key, F10 with no modifiers (decision 37).
pub fn start_key() -> Shortcut {
    Shortcut::new(None, Code::F10)
}

struct TauriPlatform {
    app: AppHandle,
}

impl Platform for TauriPlatform {
    // Both calls ask the UI thread to do the work and wait for it, so they must not run on the UI thread, or with a
    // lock held that a key press needs. Only the plugin's `register` and `unregister` are used: they touch its table
    // of shortcuts after the UI thread has finished, whereas `register_multiple` and `unregister_all` hold the table
    // while they wait for it, and a key press arriving then would deadlock.
    fn register_start_key(&self) -> bool {
        self.app.global_shortcut().register(start_key()).is_ok()
    }

    // Nothing registered is not an error: the controller calls this at the start of every session and whenever one
    // ends, so the failed `unregister` of a key that isn't held is dropped.
    fn unregister_start_key(&self) {
        let _ = self.app.global_shortcut().unregister(start_key());
    }

    fn game_running(&self, process_name: &str) -> bool {
        kestrel_probe::process::game_running(process_name)
    }

    fn beep(&self, cue: beep::Cue) {
        beep::play(cue);
    }

    fn busy_now(&self) -> Vec<(String, f32)> {
        kestrel_probe::busy_now()
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.name, p.cpu_percent))
            .collect()
    }

    fn game_settings(&self) -> Option<ValorantSettings> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        settings::read(
            &PathBuf::from(local)
                .join("VALORANT")
                .join("Saved")
                .join("Config"),
        )
    }

    fn gpu_link(&self, pnp_device_id: &str) -> Option<GpuLink> {
        kestrel_probe::gpu_link(pnp_device_id).map(hardware::gpu_link_from)
    }

    fn emit(&self, state: Option<&BenchState>) {
        let _ = self.app.emit("bench-state", state);
    }
}

/// The benchmark controller, with saved results in the app's own folder.
pub fn controller(app: &AppHandle) -> tauri::Result<Controller> {
    let data = app.path().app_data_dir()?;
    // Beside the benchmarks folder, not inside it, so no benchmark id (bench_delete("bench-work"), say) can reach it.
    let work = data.join("bench-work");
    let capture = capture_engine(app);
    // A crash during a capture leaves the marker behind: end that capture session first.
    if work.join(ACTIVE_MARKER).exists() {
        capture.clean_up_stale_session();
        let _ = std::fs::remove_file(work.join(ACTIVE_MARKER));
    }
    let platform = Arc::new(TauriPlatform { app: app.clone() });
    let store = Arc::new(Store::new(data.join("benchmarks")));
    Ok(Controller::new(
        platform,
        capture,
        store,
        work,
        env!("CARGO_PKG_VERSION"),
    ))
}

#[cfg(windows)]
fn capture_engine(app: &AppHandle) -> Arc<dyn Capture> {
    // Only the bundled copy, never a PresentMon.exe found on PATH. If the resource can't be resolved, the empty path
    // makes every capture fail with "PresentMon is missing" (capture::MISSING), and the session shows that error.
    let exe = app
        .path()
        .resolve(
            "presentmon/PresentMon.exe",
            tauri::path::BaseDirectory::Resource,
        )
        .unwrap_or_default();
    Arc::new(super::capture::PresentMon::new(exe))
}

#[cfg(not(windows))]
fn capture_engine(_app: &AppHandle) -> Arc<dyn Capture> {
    // Development off Windows: made-up frames, three seconds per capture.
    Arc::new(super::capture::Simulated {
        seconds_per_capture: 3.0,
    })
}

/// The PC as the last scan saw it, for a benchmark's context.
pub fn context_from(report: &Report) -> Context {
    let s = &report.snapshot;
    let gpu = kestrel_core::gpu::main_gpu(&s.gpus);
    Context {
        power_plan: s.windows.power_plan.as_ref().map(|p| p.name.clone()),
        ram_mts: kestrel_core::ram::configured_mts(&s.memory.modules),
        gpu: gpu.map(|g| g.name.clone()),
        gpu_driver: gpu.map(kestrel_core::gpu::display_driver_version),
        // The card whose PCIe link the captures read; an empty id means Windows didn't name it.
        gpu_pnp_device_id: gpu
            .map(|g| g.pnp_device_id.clone())
            .filter(|id| !id.is_empty()),
        // Read while the captures run (Controller::pace).
        gpu_link: None,
        windows_build: s.os.build,
        display_hz: s
            .displays
            .iter()
            .find(|d| d.is_primary && !d.is_virtual)
            .map(|d| d.current_hz),
        game_settings: None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchDetail {
    pub benchmark: Benchmark,
    /// The middle run's frame times, squeezed to at most 600 points.
    pub graph: Vec<f32>,
    /// Against the baseline; None for the baseline itself.
    pub comparison: Option<Comparison>,
    pub baseline_name: Option<String>,
}

pub fn detail(store: &Store, id: &str) -> Result<BenchDetail, String> {
    let benchmark = store.get(id).ok_or(NOT_FOUND)?;
    let graph = result::middle_run(&benchmark.runs)
        .and_then(|i| store.frame_times(id, i + 1))
        .map(|times| result::graph_points(&times, GRAPH_POINTS))
        .unwrap_or_default();
    let baseline = store
        .baseline()
        .filter(|b| b != id)
        .and_then(|b| store.get(&b));
    Ok(BenchDetail {
        comparison: baseline.as_ref().map(|b| result::compare(b, &benchmark)),
        baseline_name: baseline.map(|b| b.name),
        benchmark,
        graph,
    })
}

pub fn copy_results(store: &Store, id: &str) -> Result<String, String> {
    let d = detail(store, id)?;
    Ok(text::results_text(
        &d.benchmark,
        d.comparison.as_ref(),
        d.baseline_name.as_deref(),
    ))
}

#[tauri::command(async)]
pub fn bench_profiles() -> Vec<GameProfile> {
    PROFILES.to_vec()
}

#[tauri::command(async)]
pub fn bench_state(controller: State<'_, Controller>) -> Result<Option<BenchState>, String> {
    Ok(controller.state())
}

#[tauri::command]
pub async fn bench_start(app: AppHandle, profile: String) -> Result<BenchState, String> {
    let report = crate::latest_report(&app).await?;
    let controller = app.state::<Controller>();
    controller.start(&profile, context_from(&report))
}

#[tauri::command(async)]
pub fn bench_start_run(controller: State<'_, Controller>) -> Result<(), String> {
    controller.start_countdown()
}

#[tauri::command(async)]
pub fn bench_cancel(controller: State<'_, Controller>) -> Result<(), String> {
    controller.cancel();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_redo(controller: State<'_, Controller>) -> Result<(), String> {
    controller.redo();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_keep(controller: State<'_, Controller>) -> Result<(), String> {
    controller.keep();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_dismiss(controller: State<'_, Controller>) -> Result<(), String> {
    controller.dismiss();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_list(controller: State<'_, Controller>) -> Result<Vec<BenchmarkSummary>, String> {
    Ok(controller.store().list())
}

#[tauri::command(async)]
pub fn bench_get(controller: State<'_, Controller>, id: String) -> Result<BenchDetail, String> {
    detail(controller.store(), &id)
}

#[tauri::command(async)]
pub fn bench_rename(
    controller: State<'_, Controller>,
    id: String,
    name: String,
) -> Result<(), String> {
    controller
        .store()
        .rename(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn bench_delete(controller: State<'_, Controller>, id: String) -> Result<(), String> {
    controller.store().delete(&id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn bench_set_baseline(controller: State<'_, Controller>, id: String) -> Result<(), String> {
    controller
        .store()
        .set_baseline(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn bench_compare(
    controller: State<'_, Controller>,
    baseline_id: String,
    current_id: String,
) -> Result<Comparison, String> {
    let store = controller.store();
    let baseline = store.get(&baseline_id).ok_or(NOT_FOUND)?;
    let current = store.get(&current_id).ok_or(NOT_FOUND)?;
    Ok(result::compare(&baseline, &current))
}

#[tauri::command(async)]
pub fn bench_copy_results(controller: State<'_, Controller>, id: String) -> Result<String, String> {
    copy_results(controller.store(), &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_bench::{RunMetrics, RunRecord};

    const CARD: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";

    #[test]
    fn the_context_comes_from_the_scan() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let mut snapshot = kestrel_core::owner_pc_fixture();
        for gpu in &mut snapshot.gpus {
            gpu.pnp_device_id = match gpu.vendor {
                kestrel_core::GpuVendor::Nvidia => CARD.into(),
                _ => "ROOT\\DISPLAY\\0000".into(),
            };
        }
        let report = kestrel_core::build_report(snapshot.clone(), date, "test");
        let c = context_from(&report);
        assert_eq!(c.power_plan.as_deref(), Some("Balanced"));
        assert_eq!(c.ram_mts, Some(3600));
        assert_eq!(
            c.gpu.as_deref(),
            Some("NVIDIA GeForce RTX 3060 Ti"),
            "the real GPU, not Parsec's virtual one"
        );
        assert_eq!(c.gpu_driver.as_deref(), Some("610.74"));
        assert_eq!((c.windows_build, c.display_hz), (26200, Some(360)));
        assert!(
            c.game_settings.is_none(),
            "added when the benchmark is saved"
        );
        assert_eq!(
            c.gpu_pnp_device_id.as_deref(),
            Some(CARD),
            "the real card's id, not Parsec's"
        );
        assert!(c.gpu_link.is_none(), "read while the captures run");
        // Windows gave the main card no id: there's no card to read the link of.
        for gpu in &mut snapshot.gpus {
            gpu.pnp_device_id.clear();
        }
        let c = context_from(&kestrel_core::build_report(snapshot, date, "test"));
        assert_eq!(c.gpu_pnp_device_id, None);
    }

    fn saved(store: &Store, created_at: &str, avg: f64) -> String {
        let runs: Vec<RunRecord> = [avg - 5.0, avg, avg + 5.0]
            .iter()
            .map(|&a| RunRecord {
                metrics: RunMetrics {
                    avg_fps: a,
                    low1_fps: a / 2.0,
                    low01_fps: a / 3.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect();
        let mut bench = Benchmark {
            name: "Benchmark".into(),
            created_at: created_at.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            result: kestrel_bench::result::combine(&runs),
            runs,
            ..Default::default()
        };
        let frames = vec![
            vec![4.0_f32; 1_000],
            vec![4.5_f32; 1_000],
            vec![5.0_f32; 1_000],
        ];
        store.save(&mut bench, &frames).unwrap()
    }

    #[test]
    fn details_compare_with_the_baseline_and_draw_the_middle_run() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let first = saved(&store, "2026-10-12T21:30:05Z", 400.0);
        let second = saved(&store, "2026-10-13T08:00:00Z", 440.0);
        let d = detail(&store, &second).unwrap();
        assert_eq!(
            d.baseline_name.as_deref(),
            Some(kestrel_bench::STARTING_POINT)
        );
        let c = d.comparison.unwrap();
        assert_eq!(
            (c.baseline_id.as_str(), c.current_id.as_str()),
            (first.as_str(), second.as_str())
        );
        assert!(!d.graph.is_empty() && d.graph.len() <= 600);
        assert!(
            d.graph.iter().all(|&t| t == 4.5),
            "the middle run (run 2, 4.5 ms frames) draws the graph"
        );
        assert!(
            detail(&store, &first).unwrap().comparison.is_none(),
            "the baseline isn't compared with itself"
        );
        assert!(detail(&store, "nope").is_err());
        let text = copy_results(&store, &second).unwrap();
        assert!(text.starts_with("Kestrel benchmark: Benchmark ("), "{text}");
        assert!(
            text.contains("Versus Starting point: average FPS +10.0% (real gain)"),
            "{text}"
        );
    }
}
