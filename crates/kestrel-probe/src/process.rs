//! Whether a game is running, from the list of process names. Kestrel never opens the game's process: a Toolhelp32
//! snapshot only lists names.

/// Whether a process named `process_name` (any letter case) is in the list of running processes. `None` when the list
/// couldn't be read to the point of knowing: Windows wouldn't make it, or stopped giving entries before the end and
/// before the game turned up. That is not the same as `Some(false)`, and whatever must not guess asks this (the tweaks
/// won't change Windows with the game open, and an unreadable list isn't evidence that it is closed).
pub fn game_listed(process_name: &str) -> Option<bool> {
    #[cfg(windows)]
    {
        listed_on_windows(process_name)
    }
    #[cfg(not(windows))]
    {
        // Development off Windows: set KESTREL_FAKE_GAME to pretend the game runs.
        let _ = process_name;
        Some(std::env::var_os("KESTREL_FAKE_GAME").is_some())
    }
}

/// True when a process named `process_name` (any letter case) is running. A list that can't be read says false, which
/// is the safe side for a benchmark waiting for the game to open; `game_listed` is the question for when it isn't.
pub fn game_running(process_name: &str) -> bool {
    game_listed(process_name).unwrap_or(false)
}

/// What reading the next entry of the process list gave.
#[cfg(any(windows, test))]
enum Step {
    /// A process, by the name of its program.
    Process(String),
    /// There are no more entries: the list was read to its end.
    End,
    /// Windows wouldn't give the entry.
    Failed,
}

/// Whether `process_name` is among the processes `next` hands over, in any letter case, reading only as far as it is
/// found. `None` if a read fails first: the rest of the list might have had it.
#[cfg(any(windows, test))]
fn find_in(process_name: &str, mut next: impl FnMut() -> Step) -> Option<bool> {
    loop {
        match next() {
            Step::Process(name) if name.eq_ignore_ascii_case(process_name) => return Some(true),
            Step::Process(_) => {}
            Step::End => return Some(false),
            Step::Failed => return None,
        }
    }
}

#[cfg(windows)]
fn listed_on_windows(process_name: &str) -> Option<bool> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    // SAFETY: a process snapshot copies the list of process names; it opens no process.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut first = true;
    let found = find_in(process_name, || {
        let reading_the_first = std::mem::take(&mut first);
        let copied = if reading_the_first {
            // SAFETY: `snapshot` is open and `entry` is a valid PROCESSENTRY32W with dwSize set.
            unsafe { Process32FirstW(snapshot, &mut entry) }
        } else {
            // SAFETY: as above.
            unsafe { Process32NextW(snapshot, &mut entry) }
        };
        if copied != 0 {
            Step::Process(crate::parse::exe_name(&entry.szExeFile))
        } else if !reading_the_first
            // Running out of entries is how a list ends: `ERROR_NO_MORE_FILES`, which `last_os_error` takes from
            // `GetLastError` at once, before anything else can set it. Any other error is a list that couldn't be read.
            && std::io::Error::last_os_error().raw_os_error() == Some(ERROR_NO_MORE_FILES as i32)
        {
            Step::End
        } else {
            Step::Failed
        }
    });
    // SAFETY: the snapshot was opened above and is closed once.
    unsafe { CloseHandle(snapshot) };
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads `steps` in order as the process list, and says what it made of them and how many reads that took.
    fn find(name: &str, steps: Vec<Step>) -> (Option<bool>, usize) {
        let mut reads = 0;
        let mut steps = steps.into_iter();
        let found = find_in(name, || {
            reads += 1;
            steps.next().expect("read past the end of the list")
        });
        (found, reads)
    }

    fn process(name: &str) -> Step {
        Step::Process(name.to_string())
    }

    #[test]
    fn a_game_in_the_list_is_found_in_any_letter_case_and_the_rest_is_not_read() {
        let steps = vec![
            process("System"),
            process("valorant-win64-shipping.EXE"),
            process("never read"),
        ];
        assert_eq!(find("VALORANT-Win64-Shipping.exe", steps), (Some(true), 2));
    }

    #[test]
    fn a_list_read_to_its_end_without_the_game_says_it_is_not_there() {
        let steps = vec![process("System"), process("explorer.exe"), Step::End];
        assert_eq!(find("game.exe", steps), (Some(false), 3));
    }

    #[test]
    fn a_list_that_fails_before_the_game_is_found_cannot_say() {
        // An unreadable list is no evidence that the game is closed.
        let steps = vec![process("System"), Step::Failed];
        assert_eq!(find("game.exe", steps), (None, 2));
        assert_eq!(find("game.exe", vec![Step::Failed]), (None, 1));
    }

    #[test]
    fn the_game_found_before_a_failure_is_still_found() {
        let steps = vec![process("game.exe"), Step::Failed];
        assert_eq!(find("game.exe", steps), (Some(true), 1));
    }

    #[cfg(windows)]
    #[test]
    fn a_process_that_does_not_exist_is_not_running() {
        let name = "definitely-not-a-real-game-kestrel.exe";
        assert_eq!(game_listed(name), Some(false));
        assert!(!game_running(name));
    }

    #[cfg(windows)]
    #[test]
    fn this_test_process_is_found_by_name_in_any_case() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy().to_uppercase();
        assert_eq!(game_listed(&name), Some(true));
        assert!(game_running(&name));
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_game_runs_only_when_faked() {
        // Off Windows, `game_running` and `game_listed` only read KESTREL_FAKE_GAME, and this is the only test that
        // calls them there (the not-running test above is Windows-only), so setting the variable can't race another
        // test.
        std::env::remove_var("KESTREL_FAKE_GAME");
        assert!(!game_running("VALORANT-Win64-Shipping.exe"));
        assert_eq!(game_listed("VALORANT-Win64-Shipping.exe"), Some(false));
        std::env::set_var("KESTREL_FAKE_GAME", "1");
        assert!(game_running("VALORANT-Win64-Shipping.exe"));
        assert_eq!(game_listed("VALORANT-Win64-Shipping.exe"), Some(true));
        std::env::remove_var("KESTREL_FAKE_GAME");
        assert_eq!(crate::busy_now(), Some(Vec::new()));
    }
}
