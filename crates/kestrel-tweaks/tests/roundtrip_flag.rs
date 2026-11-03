//! The round trip changes the machine it runs on, so it must never start without being told to. No test here gives it
//! the flag on Windows.
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kestrel-tweak-roundtrip"))
        .args(args)
        .output()
        .expect("the round trip should start")
}

#[cfg(windows)]
#[test]
fn without_its_flag_alone_it_says_what_it_would_do_and_stops() {
    for args in [
        &[][..],
        &["--yes"],
        &["--yes-change-this-machine", "--and-more"],
        &["--and-more", "--yes-change-this-machine"],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let said = String::from_utf8_lossy(&output.stdout);
        assert!(
            said.contains("Nothing has been changed."),
            "{args:?}: {said}"
        );
        assert!(
            said.contains("--yes-change-this-machine"),
            "{args:?}: {said}"
        );
    }
}

/// The one thing it does without its flag: reads, in a process of its own, which is what a test can be.
#[cfg(windows)]
#[test]
fn its_copy_that_reads_for_the_restore_point_says_one_line_and_changes_nothing() {
    let output = run(&["--read-as-the-restore-point-helper"]);
    assert_eq!(output.status.code(), Some(0));
    let said = String::from_utf8_lossy(&output.stdout);
    assert_eq!(said.lines().count(), 1, "{said}");
    assert!(!said.contains("Nothing has been changed."), "{said}");
}

#[cfg(not(windows))]
#[test]
fn off_windows_it_only_says_so() {
    for args in [&[][..], &["--yes-change-this-machine"]] {
        let output = run(args);
        assert!(output.status.success(), "{args:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "Windows only\n");
    }
}
