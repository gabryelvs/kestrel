//! Kestrel's Tauri shell: the scan, live load and the benchmark (stage 2). It writes only inside its own folder.
use kestrel_core::Report;
use kestrel_probe::load::{LoadSample, LoadSampler};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

pub mod bench;

use bench::commands;
use bench::controller::Controller;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct AppState {
    /// The last scan; `None` until the first one finishes.
    report: Mutex<Option<Report>>,
    /// One sampler for the whole run, created and primed in `setup`: its first reading is unreliable.
    load: Mutex<LoadSampler>,
}

/// The last report; scans first if there isn't one yet.
#[tauri::command]
async fn get_report(state: State<'_, AppState>) -> Result<Report, String> {
    let cached = state.report.lock().map_err(|e| e.to_string())?.clone();
    match cached {
        Some(report) => Ok(report),
        None => rescan(state).await,
    }
}

/// Scans the PC again (on a worker thread) and keeps the result.
#[tauri::command]
async fn rescan(state: State<'_, AppState>) -> Result<Report, String> {
    let report = tauri::async_runtime::spawn_blocking(|| kestrel_probe::collect_report(VERSION))
        .await
        .map_err(|e| e.to_string())?;
    *state.report.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(report)
}

/// The last scan, scanning first if needed: a benchmark records the PC's context from it.
pub(crate) async fn latest_report(app: &AppHandle) -> Result<Report, String> {
    get_report(app.state::<AppState>()).await
}

/// One live-load reading; the UI polls this about once a second.
// Keep `async`: the PDH and NVML reads must stay off the UI thread.
#[tauri::command(async)]
fn get_load(state: State<'_, AppState>) -> Result<LoadSample, String> {
    let mut sampler = state.load.lock().map_err(|e| e.to_string())?;
    Ok(sampler.sample())
}

/// Pretty JSON for the "Copy diagnostics" button: the last scan, the saved benchmarks, and VALORANT's settings.
#[tauri::command(async)]
fn get_diagnostics(
    state: State<'_, AppState>,
    bench: State<'_, Controller>,
) -> Result<String, String> {
    let report = state
        .report
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Run a scan first.")?;
    let diagnostics = serde_json::json!({
        "report": report,
        "benchmarks": bench.store().list(),
        "valorantSettings": bench.game_settings(),
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
            });
            app.manage(commands::controller(app.handle())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_report,
            rescan,
            get_load,
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
            commands::bench_copy_results
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kestrel");
}
