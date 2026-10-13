//! Whether a game is running, from the list of process names. Kestrel never opens the game's process: a Toolhelp32
//! snapshot only lists names.

/// True when a process named `process_name` (any letter case) is running.
pub fn game_running(process_name: &str) -> bool {
    #[cfg(windows)]
    {
        running_on_windows(process_name)
    }
    #[cfg(not(windows))]
    {
        // Development off Windows: set KESTREL_FAKE_GAME to pretend the game runs.
        let _ = process_name;
        std::env::var_os("KESTREL_FAKE_GAME").is_some()
    }
}

#[cfg(windows)]
fn running_on_windows(process_name: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    // SAFETY: a process snapshot copies the list of process names; it opens no process.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut found = false;
    // SAFETY: `snapshot` is open and `entry` is a valid PROCESSENTRY32W with dwSize set.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        if crate::parse::exe_name(&entry.szExeFile).eq_ignore_ascii_case(process_name) {
            found = true;
            break;
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: the snapshot was opened above and is closed once.
    unsafe { CloseHandle(snapshot) };
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn a_process_that_does_not_exist_is_not_running() {
        assert!(!game_running("definitely-not-a-real-game-kestrel.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn this_test_process_is_found_by_name_in_any_case() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy().to_uppercase();
        assert!(game_running(&name));
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_game_runs_only_when_faked() {
        // Off Windows, `game_running` only reads KESTREL_FAKE_GAME, and this is the only test that calls it there (the
        // not-running test above is Windows-only), so setting the variable can't race another test.
        std::env::remove_var("KESTREL_FAKE_GAME");
        assert!(!game_running("VALORANT-Win64-Shipping.exe"));
        std::env::set_var("KESTREL_FAKE_GAME", "1");
        assert!(game_running("VALORANT-Win64-Shipping.exe"));
        std::env::remove_var("KESTREL_FAKE_GAME");
        assert_eq!(crate::busy_now(), Some(Vec::new()));
    }
}
