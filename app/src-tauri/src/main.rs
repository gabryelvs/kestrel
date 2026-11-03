#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    only_system_and_app_dlls();
    // The flags the restore point helper and the uninstaller use are answered here, before Tauri and WebView2 start
    // (and so before anything has touched COM), and never open a window.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if let Some(code) = kestrel_app::cli::handle(&args, kestrel_tweaks::journal_file) {
        std::process::exit(code);
    }
    kestrel_app::run()
}

/// From here on, a DLL that is loaded by name is looked for in the system folder and next to Kestrel.exe, and nowhere
/// else: not in the current folder, not in the PATH. Kestrel runs as administrator and now writes to the system, so a
/// planted DLL in a folder anyone can write to must not be able to run as one. DLLs that were loaded before this
/// (everything Kestrel.exe imports) are already in.
///
/// So a DLL Kestrel loads at run time by its name has to be in one of those two places. Today those are all in
/// System32 (the window libraries `tao` and `wry` ask for), and the two that matter more are loaded by other means:
/// NVML by its full path in the system folder (`kestrel-probe`'s `load_nvml`), and `SrClient.dll` with
/// `LOAD_LIBRARY_SEARCH_SYSTEM32` (`kestrel-tweaks`'s restore point). WebView2's loader is linked in, and PresentMon is
/// a program of its own.
#[cfg(windows)]
fn only_system_and_app_dlls() {
    use windows_sys::Win32::System::LibraryLoader::{
        SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    // SAFETY: takes flags only and changes how this process looks for DLLs from now on. If it fails (Windows without
    // the update that added it), the process keeps Windows' usual search order, which is how it ran before.
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR)
    };
}
