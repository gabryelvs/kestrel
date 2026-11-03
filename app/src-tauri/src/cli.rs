//! The command-line flags Kestrel answers without opening its window. `main` asks `handle` before Tauri and WebView2
//! start, so nothing has touched COM yet, and the restore point helper needs that: Microsoft's sample sets COM security
//! for the whole process, which can only be done once.
//!
//! Three flags are known:
//!
//! - `--restore-point` (`HELPER_FLAG`): the short-lived copy of Kestrel that asks Windows for a restore point. It
//!   prints one line of JSON and exits with the helper's code, which is 0 (the line is the answer).
//! - `--tweaks-has-changes`: does Kestrel have a change open, for the uninstaller to ask whether to undo it. Exit 0 for
//!   yes, 1 for none, 2 when the journal can't be read.
//! - `--tweaks-revert-all`: undoes what can be undone, for the uninstaller. Exit 0 when nothing is left open, 3 when
//!   something is, 2 when the journal can't be read or is being written by another process.
//!
//! Neither of the last two changes anything on a PC that never applied a tweak, or makes the journal's folder.
use std::sync::Arc;

use kestrel_tweaks::journal::JournalFile;
use kestrel_tweaks::restore::HELPER_FLAG;
use kestrel_tweaks::{helper_main, this_pc, Engine, System};

/// Asks whether any change of Kestrel's is open.
pub const HAS_CHANGES: &str = "--tweaks-has-changes";
/// Asks Kestrel to undo what it can.
pub const REVERT_ALL: &str = "--tweaks-revert-all";

/// `--tweaks-has-changes`: a change is open.
const CHANGES_OPEN: i32 = 0;
/// `--tweaks-has-changes`: none is.
const NO_CHANGES: i32 = 1;
/// Either: the journal couldn't be read, or the undo couldn't be started.
const FAILED: i32 = 2;
/// `--tweaks-revert-all`: nothing is open any more.
const ALL_UNDONE: i32 = 0;
/// `--tweaks-revert-all`: something is.
const SOMETHING_LEFT: i32 = 3;

/// What to do for `args` (the arguments after the program's name): the exit code if the first one is a flag handled
/// here, or None, which means the app starts as usual. `journal` makes this PC's journal; it touches nothing until
/// it is asked for something.
pub fn handle(args: &[String], journal: impl Fn() -> JournalFile) -> Option<i32> {
    answer(args, &journal, this_pc, helper_main)
}

/// `handle` with the PC and the restore point helper handed in, so a test can use a pretend PC and a helper that
/// prints nothing. Each is only asked for by the flag that needs it.
fn answer(
    args: &[String],
    journal: &dyn Fn() -> JournalFile,
    pc: impl Fn() -> Arc<dyn System>,
    helper: impl FnOnce() -> i32,
) -> Option<i32> {
    match args.first()?.as_str() {
        HELPER_FLAG => Some(helper()),
        HAS_CHANGES => Some(has_changes(journal, pc())),
        REVERT_ALL => Some(revert_all(journal, pc())),
        _ => None,
    }
}

fn has_changes(journal: &dyn Fn() -> JournalFile, pc: Arc<dyn System>) -> i32 {
    // `has_open_records` says yes for a journal it can't read, which is the safe side for the app and the wrong one
    // here: the uninstaller would ask a question whose answer it can't carry out. So the journal is read on its own
    // first.
    if journal().read().is_err() {
        return FAILED;
    }
    if Engine::new(pc, journal()).has_open_records() {
        CHANGES_OPEN
    } else {
        NO_CHANGES
    }
}

fn revert_all(journal: &dyn Fn() -> JournalFile, pc: Arc<dyn System>) -> i32 {
    if journal().read().is_err() {
        return FAILED;
    }
    let engine = Engine::new(pc, journal());
    // `Engine::revert_all` takes the lock and makes the folder even when there is nothing to undo, and a PC that never
    // applied anything keeps no folder.
    if !engine.has_open_records() {
        return ALL_UNDONE;
    }
    // This fails before undoing anything: the journal is being written by another process, or can't be read.
    if engine.revert_all().is_err() {
        return FAILED;
    }
    // What is left is whatever the journal still holds open, which is more than the items that failed: another
    // account's changes wait for that account, and System Protection stays on because turning it off deletes the
    // drive's restore points.
    if engine.has_open_records() {
        SOMETHING_LEFT
    } else {
        ALL_UNDONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_core::tweak_id::TweakId;
    use kestrel_core::FormFactor;
    use kestrel_probe::account::Account;
    use kestrel_tweaks::fake::{FakeSystem, SID};
    use kestrel_tweaks::journal::PlainFolder;
    use kestrel_tweaks::targets::{AUTO_GAME_MODE, GAME_BAR_KEY};
    use kestrel_tweaks::{PlanInput, Readiness};
    use std::path::PathBuf;

    fn flag(text: &str) -> Vec<String> {
        vec![text.to_string()]
    }

    fn input() -> PlanInput {
        PlanInput {
            build: 26200,
            form_factor: FormFactor::Desktop,
            hybrid_cpu: false,
            cpu_name: "Test CPU".into(),
            gpu_choice: false,
            valorant_exe: None,
            fullscreen_mode: None,
        }
    }

    /// A fake PC with Game Mode off, a journal folder that doesn't exist yet, and the flags run against both.
    struct Rig {
        pc: Arc<FakeSystem>,
        dir: PathBuf,
        _tmp: tempfile::TempDir,
    }

    impl Rig {
        fn new() -> Rig {
            let tmp = tempfile::tempdir().unwrap();
            let pc = Arc::new(FakeSystem::new());
            pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
            Rig {
                pc,
                dir: tmp.path().join("Kestrel"),
                _tmp: tmp,
            }
        }

        fn journal(&self) -> JournalFile {
            JournalFile::new(&self.dir, Box::new(PlainFolder))
        }

        /// What pressing Apply on Game Mode leaves: an open record.
        fn apply_game_mode(&self) {
            let engine = Engine::new(self.pc.clone(), self.journal());
            let items = engine.apply(&input(), &[TweakId::GameMode]).unwrap();
            assert!(items[0].ok, "{items:?}");
        }

        /// The flag as `main` would see it, with this PC under it and no restore point helper.
        fn run(&self, flag: &str) -> Option<i32> {
            let pc: Arc<dyn System> = self.pc.clone();
            answer(
                &[flag.to_string()],
                &|| self.journal(),
                || pc.clone(),
                || panic!("the restore point helper ran"),
            )
        }
    }

    #[test]
    fn has_changes_reads_the_journal() {
        let rig = Rig::new();
        assert_eq!(rig.run(HAS_CHANGES), Some(1), "no journal: nothing is open");
        rig.apply_game_mode();
        assert_eq!(rig.run(HAS_CHANGES), Some(0), "an open record");
    }

    #[test]
    fn handle_asks_this_pc_and_the_journal_it_is_given() {
        // `handle` itself, with this PC (a pretend one off Windows) and the journal it is given: nothing in the
        // journal means nothing open and nothing to undo.
        let tmp = tempfile::tempdir().unwrap();
        let journal = || JournalFile::new(tmp.path().join("Kestrel"), Box::new(PlainFolder));
        assert_eq!(handle(&flag(HAS_CHANGES), journal), Some(1));
        assert_eq!(handle(&flag(REVERT_ALL), journal), Some(0));
    }

    #[test]
    fn a_missing_folder_is_no_changes_and_stays_missing() {
        let rig = Rig::new();
        assert_eq!(rig.run(HAS_CHANGES), Some(1));
        assert_eq!(rig.run(REVERT_ALL), Some(0), "nothing to undo");
        assert!(!rig.dir.exists(), "asking made no folder");
        assert!(rig.pc.writes().is_empty());
    }

    #[test]
    fn a_journal_that_cant_be_read_is_an_error_for_both_questions() {
        let rig = Rig::new();
        // A directory where the journal file should be: not "missing", and not readable.
        std::fs::create_dir_all(rig.dir.join("journal.jsonl")).unwrap();
        assert_eq!(rig.run(HAS_CHANGES), Some(2));
        assert_eq!(rig.run(REVERT_ALL), Some(2));
        assert!(rig.pc.writes().is_empty());
    }

    #[test]
    fn revert_all_undoes_what_is_open_and_then_nothing_is() {
        let rig = Rig::new();
        let before = rig.pc.dump();
        rig.apply_game_mode();
        assert_ne!(rig.pc.dump(), before);

        assert_eq!(rig.run(REVERT_ALL), Some(0));

        assert_eq!(rig.pc.dump(), before, "the PC is as it was");
        assert_eq!(rig.run(HAS_CHANGES), Some(1));
    }

    #[test]
    fn revert_all_says_when_a_value_something_else_changed_was_left_alone() {
        let rig = Rig::new();
        rig.apply_game_mode();
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 5);
        let held = rig.pc.dump();

        assert_eq!(rig.run(REVERT_ALL), Some(3));

        assert_eq!(
            rig.pc.dump(),
            held,
            "nothing of the other tool's was touched"
        );
        assert_eq!(rig.run(HAS_CHANGES), Some(0), "the record is still open");
    }

    #[test]
    fn revert_all_leaves_system_protection_on_and_says_so() {
        let rig = Rig::new();
        rig.pc.set_protection(Readiness::ProtectionOff);
        Engine::new(rig.pc.clone(), rig.journal())
            .turn_protection_on()
            .unwrap();

        assert_eq!(rig.run(REVERT_ALL), Some(3));

        assert_eq!(rig.pc.protection(), Readiness::Ready);
        assert_eq!(rig.pc.writes(), ["protection on"]);
    }

    #[test]
    fn revert_all_says_when_what_is_left_belongs_to_another_account() {
        let rig = Rig::new();
        rig.apply_game_mode();
        let other = "S-1-5-21-1-2-3-1002".to_string();
        rig.pc.set_account(Account {
            signed_in: Some(other.clone()),
            this_process: Some(other),
        });
        let held = rig.pc.dump();

        assert_eq!(rig.run(REVERT_ALL), Some(3));

        assert_eq!(rig.pc.dump(), held, "that account's settings wait for it");
        assert_eq!(rig.run(HAS_CHANGES), Some(0));
    }

    #[test]
    fn a_journal_someone_else_is_writing_cant_be_undone_now_but_can_be_asked() {
        let rig = Rig::new();
        rig.apply_game_mode();
        let before = rig.pc.dump();
        let held = rig.journal().lock().unwrap();

        assert_eq!(rig.run(REVERT_ALL), Some(2));
        assert_eq!(rig.run(HAS_CHANGES), Some(0), "reading takes no lock");

        assert_eq!(rig.pc.dump(), before);
        drop(held);
        assert_eq!(rig.run(REVERT_ALL), Some(0), "and once it is free, it can");
    }

    #[test]
    fn the_restore_point_flag_is_the_helper_the_app_starts_and_its_code_is_the_exit_code() {
        let rig = Rig::new();
        let pc: Arc<dyn System> = rig.pc.clone();
        let code = answer(&flag(HELPER_FLAG), &|| rig.journal(), || pc.clone(), || 7);
        assert_eq!(code, Some(7));
        assert!(!rig.dir.exists(), "and it asks nothing of the journal");
    }

    #[test]
    fn the_flags_are_spelled_as_the_uninstaller_and_the_helper_start_them() {
        assert_eq!(HAS_CHANGES, "--tweaks-has-changes");
        assert_eq!(REVERT_ALL, "--tweaks-revert-all");
        assert_eq!(HELPER_FLAG, "--restore-point");
    }

    // The installer's hooks and the launch check are compiled and run only by GitHub's Windows job, so what can be known
    // here is pinned here: the tests below read the files that job is given.

    const HOOKS: &str = include_str!("../windows/hooks.nsh");

    /// The lines of one macro in the hooks file, without comments, blank lines or indentation.
    fn hook(name: &str) -> Vec<&'static str> {
        let header = format!("!macro {name}");
        HOOKS
            .lines()
            .map(str::trim)
            .skip_while(|line| *line != header)
            .skip(1)
            .take_while(|line| *line != "!macroend")
            .filter(|line| !line.is_empty() && !line.starts_with(';'))
            .collect()
    }

    #[test]
    fn the_installer_is_pointed_at_the_hooks_file() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let nsis = &config["bundle"]["windows"]["nsis"];
        assert_eq!(
            nsis["installMode"], "perMachine",
            "the uninstaller runs elevated, so Kestrel.exe starts from it without a prompt"
        );
        let hooks = nsis["installerHooks"]
            .as_str()
            .expect("tauri.conf.json names the hooks file");
        // Tauri resolves it against the folder that holds tauri.conf.json.
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(hooks);
        assert!(file.is_file(), "{} isn't there", file.display());
    }

    #[test]
    fn the_uninstaller_starts_the_flags_the_app_answers_and_checks_that_each_started() {
        let lines = hook("NSIS_HOOK_PREUNINSTALL");
        for flag in [HAS_CHANGES, REVERT_ALL] {
            let starts: Vec<usize> = (0..lines.len())
                .filter(|&at| lines[at].starts_with("ExecWait") && lines[at].contains(flag))
                .collect();
            assert_eq!(starts.len(), 1, "hooks.nsh starts {flag} once");
            // A program that can't be started leaves ExecWait's variable as it was, and NSIS's error flag, which stays
            // set from whatever ran before, is the only sign of it. So the flag is cleared right before each start...
            assert_eq!(
                lines[starts[0] - 1],
                "ClearErrors",
                "{flag} is started after ClearErrors"
            );
            // ...and looked at right after it, before the exit code is.
            assert!(
                lines[starts[0] + 1].contains("${Errors}"),
                "{flag}'s start is followed by a look at the error flag, not {}",
                lines[starts[0] + 1]
            );
        }
        // The words the owner chose (decision 88).
        for words in [
            "Undo Kestrel's changes before removing it?",
            "Kestrel couldn't undo everything. Check those settings in Windows.",
        ] {
            assert_eq!(HOOKS.matches(words).count(), 1, "hooks.nsh says: {words}");
        }
    }

    #[test]
    fn deleting_the_application_data_removes_the_journals_files_by_name_and_never_recurses() {
        // `RMDir /r` descends into anything marked as a folder, a junction included, and a standard user may have made
        // ProgramData\Kestrel one before Kestrel ever ran (the journal's folder check refuses it for that reason). So
        // the uninstaller deletes the files the journal keeps, by name, and removes the folder only if that leaves it
        // empty. What the journal keeps is what a real Apply leaves in its folder, so a third file fails this test.
        let rig = Rig::new();
        rig.apply_game_mode();
        let mut kept: Vec<String> = std::fs::read_dir(&rig.dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        kept.sort();
        assert!(
            !kept.is_empty(),
            "an Apply leaves its journal in the folder"
        );

        let after = hook("NSIS_HOOK_POSTUNINSTALL");
        let place = |line: &str| {
            after
                .iter()
                .position(|&each| each == line)
                .unwrap_or_else(|| panic!("hooks.nsh has no `{line}` for the application data"))
        };
        // `$APPDATA` under `all` is ProgramData as the shell names it, as the Rust side does, whatever the environment
        // says. Tauri's own block, under this same condition, has just switched to the current user for its folders,
        // and nothing after the hook uses a shell folder, so that is what is put back.
        let mut wanted = vec!["SetShellVarContext all".to_string()];
        wanted.extend(
            kept.iter()
                .map(|name| format!(r#"Delete "$APPDATA\Kestrel\{name}""#)),
        );
        wanted.push(r#"RMDir "$APPDATA\Kestrel""#.to_string());
        wanted.push("SetShellVarContext current".to_string());
        let places: Vec<usize> = wanted.iter().map(|line| place(line)).collect();
        assert!(
            places.windows(2).all(|pair| pair[0] < pair[1]),
            "in this order: {wanted:?}"
        );
        // And nothing more: no other delete, nothing recursive, nothing taken from the environment.
        let removals = after
            .iter()
            .filter(|line| line.starts_with("Delete") || line.starts_with("RMDir"))
            .count();
        assert_eq!(removals, kept.len() + 1, "{after:?}");
        assert!(
            after
                .iter()
                .all(|line| !line.contains("/r") && !line.contains('%')),
            "{after:?}"
        );
    }

    #[test]
    fn the_launch_check_gives_every_call_of_the_exe_a_command_after_it() {
        // PowerShell starts a GUI-subsystem program that ends a pipeline in the background, even inside @(): it doesn't
        // wait for it, read its output or set $LASTEXITCODE. So each call has a command after it. The first Windows run
        // read nothing from `@(& $exe --restore-point)`, and the exit code it then checked was the call before's.
        let ci = include_str!("../../../.github/workflows/ci.yml");
        for flag in [HAS_CHANGES, REVERT_ALL, HELPER_FLAG] {
            let call = format!("& $exe {flag}");
            let calls: Vec<&str> = ci.lines().filter(|line| line.contains(&call)).collect();
            assert_eq!(calls.len(), 1, "ci.yml calls the exe with {flag} once");
            let rest = calls[0].split(&call).nth(1).unwrap().trim_start();
            assert!(
                rest.starts_with('|'),
                "{flag} is followed by a command in: {}",
                calls[0].trim()
            );
        }
    }

    #[test]
    fn an_unrelated_flag_starts_the_app() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = || JournalFile::new(tmp.path().join("Kestrel"), Box::new(PlainFolder));
        assert_eq!(handle(&flag("--whatever"), journal), None);
        assert_eq!(handle(&[], journal), None);
        assert!(!tmp.path().join("Kestrel").exists());
    }
}
