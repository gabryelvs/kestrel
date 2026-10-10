//! Kestrel's Tauri shell: hands the probe's report and live load to the UI. Read-only in stage 1.
use kestrel_core::Report;
use kestrel_probe::load::{LoadSample, LoadSampler};
use std::sync::Mutex;
use tauri::{Manager, State};

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

/// One live-load reading; the UI polls this about once a second.
// Keep `async`: the PDH and NVML reads must stay off the UI thread.
#[tauri::command(async)]
fn get_load(state: State<'_, AppState>) -> Result<LoadSample, String> {
    let mut sampler = state.load.lock().map_err(|e| e.to_string())?;
    Ok(sampler.sample())
}

/// Pretty JSON of the last report, for the "Copy diagnostics" button.
#[tauri::command]
fn get_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.report.lock().map_err(|e| e.to_string())?;
    let report = guard.as_ref().ok_or("Run a scan first.")?;
    serde_json::to_string_pretty(report).map_err(|e| e.to_string())
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
        .setup(|app| {
            // Sampled once here so the UI's first poll already has a baseline to measure against.
            let mut sampler = LoadSampler::new();
            sampler.sample();
            app.manage(AppState {
                report: Mutex::new(None),
                load: Mutex::new(sampler),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_report,
            rescan,
            get_load,
            get_diagnostics,
            is_elevated
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kestrel");
}
