//! Kestrel's Tauri shell: the scan, live load and clocks, the benchmark with the PCIe link it measures, and the Tweaks
//! tab's commands. It writes to Windows only through `kestrel-tweaks`, and only when the person presses Apply or one
//! of the undo buttons; anything else it writes stays inside its own folders.
use chrono::NaiveDate;
use kestrel_core::topology::CoreClock;
use kestrel_core::Report;
use kestrel_probe::clocks::ClockSampler;
use kestrel_probe::load::{LoadSample, LoadSampler};
use kestrel_tweaks::{Engine, System};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

pub mod bench;
pub mod cli;
pub mod hardware;
pub mod tweaks;

use bench::commands;
use bench::controller::Controller;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct AppState {
    /// The last scan as the probe returned it; `None` until the first one finishes. Commands hand it out through
    /// `hardware::with_links`, so a benchmark saved since the scan shows at once.
    report: Mutex<Option<Report>>,
    /// One sampler for the whole run, created and primed in `setup`: its first reading is unreliable.
    load: Mutex<LoadSampler>,
    /// One sampler for the whole run, created in `setup`: it collects once then, so the first poll has a baseline.
    clocks: Mutex<ClockSampler>,
}

impl AppState {
    /// The Windows build the last scan saw, or 0 before there has been one: the Kestrel report names it.
    fn build(&self) -> u32 {
        self.report
            .lock()
            .ok()
            .and_then(|report| report.as_ref().map(|r| r.snapshot.os.build))
            .unwrap_or(0)
    }
}

/// The day the findings are judged on (BIOS and driver age).
fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// The last report, with the newest PCIe link under load; scans first if there isn't one yet.
#[tauri::command]
async fn get_report(
    state: State<'_, AppState>,
    bench: State<'_, Controller>,
) -> Result<Report, String> {
    let cached = state.report.lock().map_err(|e| e.to_string())?.clone();
    match cached {
        Some(report) => Ok(hardware::with_links(report, bench.store(), today())),
        None => rescan(state, bench).await,
    }
}

/// Scans the PC again (on a worker thread) and keeps the probe's report; returns it with the newest link under load.
#[tauri::command]
async fn rescan(
    state: State<'_, AppState>,
    bench: State<'_, Controller>,
) -> Result<Report, String> {
    let report = tauri::async_runtime::spawn_blocking(|| kestrel_probe::collect_report(VERSION))
        .await
        .map_err(|e| e.to_string())?;
    *state.report.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(hardware::with_links(report, bench.store(), today()))
}

/// The last scan, scanning first if needed: a benchmark records the PC's context from it.
pub(crate) async fn latest_report(app: &AppHandle) -> Result<Report, String> {
    get_report(app.state::<AppState>(), app.state::<Controller>()).await
}

/// One live-load reading; the UI polls this about once a second.
// Keep `async`: the PDH and NVML reads must stay off the UI thread.
#[tauri::command(async)]
fn get_load(state: State<'_, AppState>) -> Result<LoadSample, String> {
    let mut sampler = state.load.lock().map_err(|e| e.to_string())?;
    Ok(sampler.sample())
}

/// Each physical core's clock right now; the Hardware page polls this about once a second while it's shown.
// Keep `async`: the PDH read must stay off the UI thread.
#[tauri::command(async)]
fn get_cpu_clocks(state: State<'_, AppState>) -> Result<Vec<CoreClock>, String> {
    let mut sampler = state.clocks.lock().map_err(|e| e.to_string())?;
    Ok(sampler.sample())
}

/// Pretty JSON for the "Copy diagnostics" button: the last scan (with the newest link under load, as the app shows it),
/// the saved benchmarks, VALORANT's settings, the benchmark session as it is now (a failed one carries PresentMon's
/// error), or null when there is none, and the Kestrel report with what the tweaks see of the accounts and System
/// Protection.
#[tauri::command(async)]
fn get_diagnostics(
    state: State<'_, AppState>,
    bench: State<'_, Controller>,
    engine: State<'_, Arc<Engine>>,
    pc: State<'_, Arc<dyn System>>,
) -> Result<String, String> {
    let report = state
        .report
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Run a scan first.")?;
    let build = report.snapshot.os.build;
    let diagnostics = serde_json::json!({
        "report": hardware::with_links(report, bench.store(), today()),
        "benchmarks": bench.store().list(),
        "valorantSettings": bench.game_settings(),
        // An owned snapshot: no platform call, and the controller's lock is released before this is serialised.
        "benchSession": bench.state(),
        "tweaks": tweaks::diagnostics(&engine, &**pc, build),
    });
    serde_json::to_string_pretty(&diagnostics).map_err(|e| e.to_string())
}

/// True when Kestrel runs as administrator, which its manifest asks for at launch.
#[tauri::command]
fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        // SAFETY: IsUserAnAdmin takes no arguments and only reads this process's token.
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed && *shortcut == commands::start_key()
                    {
                        // The plugin holds its shortcut lock while this runs on the UI thread, and the controller
                        // may release the key (which needs both): hand the press to a worker thread.
                        let controller = app.state::<Controller>().inner().clone();
                        std::thread::spawn(move || controller.start_pressed());
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Sampled once here so the UI's first poll already has a baseline to measure against.
            let mut sampler = LoadSampler::new();
            sampler.sample();
            app.manage(AppState {
                report: Mutex::new(None),
                load: Mutex::new(sampler),
                clocks: Mutex::new(ClockSampler::new()),
            });
            app.manage(commands::controller(app.handle())?);
            // Building the engine touches nothing, on the PC or in the journal's folder: the first command that needs
            // them is the one that finds a missing or refused folder.
            let pc = kestrel_tweaks::this_pc();
            app.manage(tweaks::engine(pc.clone(), kestrel_tweaks::journal_file()));
            app.manage(pc);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_report,
            rescan,
            get_load,
            get_cpu_clocks,
            get_diagnostics,
            is_elevated,
            commands::bench_profiles,
            commands::bench_state,
            commands::bench_start,
            commands::bench_start_run,
            commands::bench_cancel,
            commands::bench_redo,
            commands::bench_keep,
            commands::bench_dismiss,
            commands::bench_list,
            commands::bench_get,
            commands::bench_rename,
            commands::bench_delete,
            commands::bench_set_baseline,
            commands::bench_compare,
            commands::bench_copy_results,
            tweaks::commands::tweaks_view,
            tweaks::commands::tweaks_preview,
            tweaks::commands::tweaks_restore_point,
            tweaks::commands::tweaks_apply,
            tweaks::commands::tweaks_undo,
            tweaks::commands::tweaks_apply_again,
            tweaks::commands::tweaks_restore_anyway,
            tweaks::commands::tweaks_revert_all,
            tweaks::commands::tweaks_use_plan_again,
            tweaks::commands::tweaks_remove_plan,
            tweaks::commands::tweaks_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kestrel");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(report: Option<Report>) -> AppState {
        AppState {
            report: Mutex::new(report),
            load: Mutex::new(LoadSampler::new()),
            clocks: Mutex::new(ClockSampler::new()),
        }
    }

    #[test]
    fn the_build_comes_from_the_last_scan_and_is_zero_before_one() {
        assert_eq!(state(None).build(), 0);
        let report = kestrel_core::build_report(kestrel_core::owner_pc_fixture(), today(), VERSION);
        assert_eq!(state(Some(report)).build(), 26200);
    }
}
