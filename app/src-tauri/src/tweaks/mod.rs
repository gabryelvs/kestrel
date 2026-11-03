//! The Tweaks tab's side of the app. The engine in `kestrel-tweaks` changes Windows and keeps the journal, but it
//! enforces no gate of its own: the rule that nothing changes while VALORANT is open or a benchmark is armed is here,
//! and every command that changes anything asks it first (see `commands`).
//!
//! The engine is one `Arc<Engine>` in the app's state, built in `setup` before any command can run. Building it
//! touches nothing, on the PC or in the journal's folder; a refused or missing folder first shows when a command needs
//! the journal.
pub mod commands;

use std::sync::Arc;

use kestrel_tweaks::journal::JournalFile;
use kestrel_tweaks::{Engine, Gate, System};

/// Whether tweaks may change Windows now, and if not, what they wait for. A benchmark that is armed comes first: it
/// wants the PC as it is, whether or not the game is open yet. `game_running` is `None` when the list of processes
/// couldn't be read, and that is a closed gate too: not being able to see the game is not the game being closed.
pub fn gate(game_running: Option<bool>, session_active: bool) -> Gate {
    if session_active {
        return Gate::BenchmarkActive;
    }
    match game_running {
        Some(true) => Gate::GameRunning,
        Some(false) => Gate::Open,
        None => Gate::CantTell,
    }
}

/// Ok when the gate is open. Otherwise the words the page shows beside its disabled buttons, which is also what a
/// command refused by the gate comes back with.
pub fn require_open(state: Gate) -> Result<(), String> {
    match state {
        Gate::Open => Ok(()),
        Gate::GameRunning => Err("Close VALORANT first.".to_string()),
        Gate::BenchmarkActive => Err("Finish the benchmark first.".to_string()),
        Gate::CantTell => {
            Err("Kestrel couldn't check whether VALORANT is open. Try again.".to_string())
        }
    }
}

/// Whether VALORANT is running, found by process name the way the benchmark finds it, or None when the list of
/// processes couldn't be read (the benchmark takes that for "no"; the gate doesn't). Nothing opens its process.
pub(crate) fn game_listed() -> Option<bool> {
    kestrel_probe::process::game_listed(kestrel_bench::VALORANT.process_name)
}

/// The engine for a PC and its journal. It touches neither, so `setup` can build it before the window exists and a
/// PC that never presses Apply never gets a folder.
pub fn engine(system: Arc<dyn System>, journal: JournalFile) -> Arc<Engine> {
    Arc::new(Engine::new(system, journal))
}

/// Runs `work` on a thread meant for blocking, so a slow call never sits on one of the async runtime's workers. The
/// engine's calls read and write the journal and Windows, `turn_protection_on` waits up to 30 seconds for Windows to
/// say it worked, and the restore point's helper may take 3 minutes. A panic in `work` comes back as the error.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|why| why.to_string())
}

/// The Kestrel report as plain text, made now. `build` is the Windows build the last scan saw, or 0 before one.
pub(crate) fn report_text(engine: &Engine, build: u32) -> String {
    engine.report(crate::VERSION, build, &chrono::Local::now().to_rfc3339())
}

/// What Copy diagnostics adds under `"tweaks"`: the report, whether Kestrel runs as another account than the one at
/// the screen, and what System Protection reads (as `Readiness` is written: `"ready"`, `"protection-off"`,
/// `"blocked-by-policy"` or `"unknown"`), so a test on a real PC shows the state without a `reg query`. The engine
/// only reads for both.
pub fn diagnostics(engine: &Engine, build: u32) -> serde_json::Value {
    serde_json::json!({
        "report": report_text(engine, build),
        "accountsDiffer": engine.accounts_differ(),
        "systemProtection": engine.protection(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_core::FormFactor;
    use kestrel_probe::account::Account;
    use kestrel_tweaks::fake::{FakeSystem, SID};
    use kestrel_tweaks::journal::{Folder, JournalFile, PlainFolder};
    use kestrel_tweaks::{PlanInput, Readiness};
    use std::io;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    /// A journal folder that refuses everything and counts how often it was asked.
    struct Refusing(Arc<AtomicUsize>);

    impl Folder for Refusing {
        fn check(&self, dir: &Path) -> io::Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(io::Error::other(kestrel_tweaks::folder::not_trusted(dir)))
        }

        fn prepare(&self, dir: &Path) -> io::Result<()> {
            self.check(dir)
        }
    }

    #[test]
    fn the_gate_names_the_benchmark_first() {
        assert_eq!(gate(Some(true), true), Gate::BenchmarkActive);
        assert_eq!(gate(Some(true), false), Gate::GameRunning);
        assert_eq!(gate(Some(false), false), Gate::Open);
        assert_eq!(gate(Some(false), true), Gate::BenchmarkActive);
    }

    #[test]
    fn a_process_list_that_cant_be_read_is_not_an_open_gate() {
        assert_eq!(gate(None, false), Gate::CantTell);
        assert_eq!(
            gate(None, true),
            Gate::BenchmarkActive,
            "the benchmark is still named first"
        );
    }

    #[test]
    fn a_closed_gate_says_what_to_wait_for_and_an_open_one_says_nothing() {
        assert_eq!(require_open(Gate::Open), Ok(()));
        assert_eq!(
            require_open(Gate::GameRunning),
            Err("Close VALORANT first.".to_string())
        );
        assert_eq!(
            require_open(Gate::BenchmarkActive),
            Err("Finish the benchmark first.".to_string())
        );
        assert_eq!(
            require_open(Gate::CantTell),
            Err("Kestrel couldn't check whether VALORANT is open. Try again.".to_string())
        );
    }

    #[test]
    fn building_the_engine_touches_nothing_and_a_refused_folder_only_shows_in_a_command() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Kestrel");
        let pc = Arc::new(FakeSystem::new());
        let asked = Arc::new(AtomicUsize::new(0));

        let built = engine(
            pc.clone(),
            JournalFile::new(&dir, Box::new(Refusing(asked.clone()))),
        );

        assert_eq!(
            asked.load(Ordering::SeqCst),
            0,
            "the folder isn't looked at"
        );
        assert!(pc.writes().is_empty());
        assert!(!dir.exists());
        // The first thing that needs the journal is what says it was refused.
        let why = built.view(&input(), Gate::Open).unwrap_err();
        assert!(
            why.to_string()
                .ends_with("belongs to another account. Delete it, then try again."),
            "{why}"
        );
        assert!(asked.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn blocking_work_runs_on_a_thread_of_its_own() {
        let here = std::thread::current().id();
        let there =
            tauri::async_runtime::block_on(blocking(|| std::thread::current().id())).unwrap();
        assert_ne!(there, here);
    }

    #[test]
    fn diagnostics_carry_the_report_the_accounts_and_what_system_protection_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let pc = Arc::new(FakeSystem::new());
        let built = engine(
            pc.clone(),
            JournalFile::new(tmp.path().join("Kestrel"), Box::new(PlainFolder)),
        );

        let tweaks = diagnostics(&built, 26200);

        let header = format!(
            "Kestrel report · Kestrel {} · Windows build 26200 · ",
            env!("CARGO_PKG_VERSION")
        );
        assert!(
            tweaks["report"].as_str().unwrap().starts_with(&header),
            "{tweaks}"
        );
        assert_eq!(tweaks["accountsDiffer"], false);
        assert_eq!(tweaks["systemProtection"], "ready");
        let mut keys: Vec<_> = tweaks.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["accountsDiffer", "report", "systemProtection"]);

        for (readiness, name) in [
            (Readiness::ProtectionOff, "protection-off"),
            (Readiness::BlockedByPolicy, "blocked-by-policy"),
            (Readiness::Unknown, "unknown"),
            (Readiness::Ready, "ready"),
        ] {
            pc.set_protection(readiness);
            assert_eq!(diagnostics(&built, 26200)["systemProtection"], name);
        }
        pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: Some("S-1-5-21-1-2-3-1002".to_string()),
        });
        assert_eq!(diagnostics(&built, 26200)["accountsDiffer"], true);
        // Before a scan the build is 0, and the report still reads.
        assert!(diagnostics(&built, 0)["report"]
            .as_str()
            .unwrap()
            .contains("Windows build 0 · "));
    }
}
