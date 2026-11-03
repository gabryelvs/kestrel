//! The commands the Tweaks tab calls. Every one is `async`, so none runs on the UI thread, and everything that can
//! block runs through `blocking` on a thread of its own: the engine's calls, and the restore point's helper, which may
//! take minutes.
//!
//! A command that changes anything goes through `change` (or `change_with_input`, when it needs the rules' input),
//! which asks the gate before the engine is touched. The reads (the view, the Preview, the report) don't: the page can
//! always look. Commands return what changed and the page reads the view again, since nothing changes in the
//! background.
//!
//! The gate looks at the benchmark controller and at the process list. The `_with` functions take the controller, and
//! how to list the game, as arguments, so the tests can arm a real controller and make the list unreadable.
use std::future::Future;
use std::io;
use std::sync::Arc;

use kestrel_bench::ValorantSettings;
use kestrel_core::tweak_id::TweakId;
use kestrel_core::SystemSnapshot;
use kestrel_tweaks::{
    run_helper, Engine, Gate, ItemOutcome, PlanInput, PreviewView, RestoreChoice, RestoreOutcome,
    TweaksView,
};
use tauri::{AppHandle, Manager};

use super::{blocking, game_listed, gate, report_text, require_open};
use crate::bench::controller::Controller;
use crate::AppState;

fn engine_of(app: &AppHandle) -> Arc<Engine> {
    app.state::<Arc<Engine>>().inner().clone()
}

fn controller_of(app: &AppHandle) -> Controller {
    app.state::<Controller>().inner().clone()
}

/// What the rules need to know about this PC: the last scan, and the fullscreen mode VALORANT's own settings file says.
pub(crate) fn input_for(
    snapshot: &SystemSnapshot,
    settings: Option<ValorantSettings>,
) -> PlanInput {
    PlanInput::from_snapshot(snapshot, settings.and_then(|s| s.fullscreen_mode))
}

/// The rules' input now. The scan runs at start, so the tab can ask before it is done: then this runs the same
/// read-only collection the scan command does (off the runtime's thread) and keeps it as the last scan, rather than
/// failing. A probe that fails is the command's error. Only the scan's snapshot is used, so the saved benchmarks aren't
/// read and the findings aren't rebuilt for it, as `latest_report` would.
async fn plan_input(app: &AppHandle) -> Result<PlanInput, String> {
    let snapshot = crate::latest_snapshot(app).await?;
    let controller = controller_of(app);
    // VALORANT's settings are read from disk.
    blocking(move || input_for(&snapshot, controller.game_settings())).await
}

/// Whether tweaks may change Windows right now: a benchmark armed, the game open, or `listed` unable to say.
fn gate_now(controller: &Controller, listed: &impl Fn() -> Option<bool>) -> Gate {
    gate(listed(), controller.session_active())
}

/// Hands `engine` to `work` when the gate is open, and says in the engine's own words why `work` failed. A closed gate
/// returns before the engine is asked, so nothing is written, in the journal or to Windows.
fn gated<T>(
    open_or_not: Gate,
    engine: &Engine,
    work: impl FnOnce(&Engine) -> io::Result<T>,
) -> Result<T, String> {
    require_open(open_or_not)?;
    work(engine).map_err(|why| why.to_string())
}

/// The view, with the gate as it is now.
async fn view_with(
    engine: &Arc<Engine>,
    controller: &Controller,
    listed: impl Fn() -> Option<bool> + Send + 'static,
    input: PlanInput,
) -> Result<TweaksView, String> {
    let (engine, controller) = (engine.clone(), controller.clone());
    blocking(move || {
        engine
            .view(&input, gate_now(&controller, &listed))
            .map_err(|why| why.to_string())
    })
    .await?
}

/// Runs a call that changes Windows, or the journal's record of what was changed, once the gate is open.
async fn change_with<T: Send + 'static>(
    engine: &Arc<Engine>,
    controller: &Controller,
    listed: impl Fn() -> Option<bool> + Send + 'static,
    work: impl FnOnce(&Engine) -> io::Result<T> + Send + 'static,
) -> Result<T, String> {
    let (engine, controller) = (engine.clone(), controller.clone());
    blocking(move || gated(gate_now(&controller, &listed), &engine, work)).await?
}

/// `change_with` for a call that needs the rules' input, which can mean running the scan. The gate is asked before that,
/// so a scan that fails can't hide why nothing was done, and asked again right before the engine, which is the check
/// that counts: the game may have been started meanwhile.
async fn change_with_input<T: Send + 'static>(
    engine: &Arc<Engine>,
    controller: &Controller,
    listed: impl Fn() -> Option<bool> + Clone + Send + 'static,
    input: impl Future<Output = Result<PlanInput, String>>,
    work: impl FnOnce(&Engine, &PlanInput) -> io::Result<T> + Send + 'static,
) -> Result<T, String> {
    let early = (controller.clone(), listed.clone());
    blocking(move || require_open(gate_now(&early.0, &early.1))).await??;
    let input = input.await?;
    change_with(engine, controller, listed, move |engine| {
        work(engine, &input)
    })
    .await
}

/// `change_with` for this app: this app's engine and benchmark controller, and the game found in the process list.
async fn change<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(&Engine) -> io::Result<T> + Send + 'static,
) -> Result<T, String> {
    let (engine, controller) = (engine_of(app), controller_of(app));
    change_with(&engine, &controller, game_listed, work).await
}

/// The restore point step for what the person chose, and the record of how it went.
///
/// `Skip` is recorded as skipped. `TurnOnAndCreate` turns System Protection on first, as a change of its own; if that
/// fails, no point is asked for, and the failure is the outcome (recorded and returned) rather than this function's
/// error, so the page can ask whether to go on with Kestrel's undo only. `create` asks Windows for the point.
///
/// The error is the journal's. A journal that can't take the outcome won't take the Apply either, and a lock held for
/// a moment would otherwise lose the outcome without a word.
fn restore_point(
    engine: &Engine,
    choice: RestoreChoice,
    create: impl FnOnce() -> RestoreOutcome,
) -> io::Result<RestoreOutcome> {
    let outcome = match choice {
        RestoreChoice::Skip => RestoreOutcome::Skipped,
        RestoreChoice::Create => create(),
        RestoreChoice::TurnOnAndCreate => match engine.turn_protection_on() {
            Ok(()) => create(),
            Err(why) => RestoreOutcome::Failed {
                reason: why.to_string(),
            },
        },
    };
    engine.record_restore(&outcome)?;
    Ok(outcome)
}

/// Asks Windows for a restore point, in a short-lived copy of this program (already elevated, so nothing is asked of
/// the person) that waits up to 3 minutes. Off Windows nothing is started, and the answer is `Skipped`.
fn create_restore_point() -> RestoreOutcome {
    match std::env::current_exe() {
        Ok(exe) => run_helper(&exe),
        Err(why) => RestoreOutcome::Failed {
            reason: format!(
                "Kestrel couldn't find its own program to start the restore point helper: {why}"
            ),
        },
    }
}

/// The Tweaks tab: one row per tweak, and the gate as it is now.
#[tauri::command(async)]
pub async fn tweaks_view(app: AppHandle) -> Result<TweaksView, String> {
    let input = plan_input(&app).await?;
    view_with(&engine_of(&app), &controller_of(&app), game_listed, input).await
}

/// What Apply would change for `ids`, and whether a restore point can be made first. It changes nothing.
#[tauri::command(async)]
pub async fn tweaks_preview(app: AppHandle, ids: Vec<TweakId>) -> Result<PreviewView, String> {
    let input = plan_input(&app).await?;
    let engine = engine_of(&app);
    blocking(move || engine.preview(&input, &ids)).await
}

/// The restore point step of Apply. See `restore_point`.
#[tauri::command(async)]
pub async fn tweaks_restore_point(
    app: AppHandle,
    choice: RestoreChoice,
) -> Result<RestoreOutcome, String> {
    change(&app, move |engine| {
        restore_point(engine, choice, create_restore_point)
    })
    .await
}

/// Applies `ids`, each on its own: one failing doesn't stop the others.
#[tauri::command(async)]
pub async fn tweaks_apply(app: AppHandle, ids: Vec<TweakId>) -> Result<Vec<ItemOutcome>, String> {
    let (engine, controller) = (engine_of(&app), controller_of(&app));
    change_with_input(
        &engine,
        &controller,
        game_listed,
        plan_input(&app),
        move |engine, input| engine.apply(input, &ids),
    )
    .await
}

/// Undoes one tweak, if it still holds what Kestrel wrote.
#[tauri::command(async)]
pub async fn tweaks_undo(app: AppHandle, id: TweakId) -> Result<ItemOutcome, String> {
    change(&app, move |engine| engine.undo(id)).await
}

/// Writes Kestrel's value over what something else changed a tweak to.
#[tauri::command(async)]
pub async fn tweaks_apply_again(app: AppHandle, id: TweakId) -> Result<ItemOutcome, String> {
    let (engine, controller) = (engine_of(&app), controller_of(&app));
    change_with_input(
        &engine,
        &controller,
        game_listed,
        plan_input(&app),
        move |engine, input| engine.apply_again(input, id),
    )
    .await
}

/// Writes the old value over what something else changed a tweak to.
#[tauri::command(async)]
pub async fn tweaks_restore_anyway(app: AppHandle, id: TweakId) -> Result<ItemOutcome, String> {
    change(&app, move |engine| engine.restore_anyway(id)).await
}

/// Undoes everything this account and the machine have open, except System Protection.
#[tauri::command(async)]
pub async fn tweaks_revert_all(app: AppHandle) -> Result<Vec<ItemOutcome>, String> {
    change(&app, |engine| engine.revert_all()).await
}

/// Switches back to Kestrel's power plan after another was picked.
#[tauri::command(async)]
pub async fn tweaks_use_plan_again(app: AppHandle) -> Result<ItemOutcome, String> {
    change(&app, |engine| engine.use_plan_again()).await
}

/// Deletes Kestrel's power plan and keeps the plan in use.
#[tauri::command(async)]
pub async fn tweaks_remove_plan(app: AppHandle) -> Result<ItemOutcome, String> {
    change(&app, |engine| engine.remove_plan()).await
}

/// The Kestrel report as plain text. It names the Windows build of the last scan, or 0 before there has been one.
#[tauri::command(async)]
pub async fn tweaks_report(app: AppHandle) -> Result<String, String> {
    let build = app.state::<AppState>().build();
    let engine = engine_of(&app);
    blocking(move || report_text(&engine, build)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::beep::Cue;
    use crate::bench::capture::Simulated;
    use crate::bench::controller::{BenchState, Platform};
    use kestrel_bench::{Context, GpuLink, Store};
    use kestrel_core::FormFactor;
    use kestrel_tweaks::fake::{FakeSystem, PROTECTION, SID};
    use kestrel_tweaks::journal::{Entry, JournalFile, PlainFolder};
    use kestrel_tweaks::targets::{AUTO_GAME_MODE, GAME_BAR_KEY};
    use kestrel_tweaks::{Readiness, System};
    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;
    use tauri::async_runtime::block_on;

    fn journal_in(dir: &Path) -> JournalFile {
        JournalFile::new(dir, Box::new(PlainFolder))
    }

    /// A platform that does nothing: the start key is free, the game isn't open, and nothing is heard or shown.
    struct Quiet;

    impl Platform for Quiet {
        fn register_start_key(&self) -> bool {
            true
        }
        fn unregister_start_key(&self) {}
        fn game_running(&self, _process_name: &str) -> bool {
            false
        }
        fn beep(&self, _cue: Cue) {}
        fn busy_now(&self) -> Vec<(String, f32)> {
            Vec::new()
        }
        fn game_settings(&self) -> Option<ValorantSettings> {
            None
        }
        fn gpu_link(&self, _pnp_device_id: &str) -> Option<GpuLink> {
            None
        }
        fn emit(&self, _state: Option<&BenchState>) {}
    }

    /// A fake PC on stock Balanced with Game Mode off, an engine for it with a journal that doesn't exist yet, and a
    /// benchmark controller with nothing armed.
    struct Rig {
        pc: Arc<FakeSystem>,
        dir: PathBuf,
        engine: Arc<Engine>,
        controller: Controller,
        _tmp: tempfile::TempDir,
    }

    impl Rig {
        fn new() -> Rig {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("Kestrel");
            let pc = Arc::new(FakeSystem::new());
            pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
            let engine = Arc::new(Engine::new(pc.clone(), journal_in(&dir)));
            let controller = Controller::new(
                Arc::new(Quiet),
                Arc::new(Simulated {
                    seconds_per_capture: 0.0,
                }),
                Arc::new(Store::new(tmp.path().join("benchmarks"))),
                tmp.path().join("work"),
                "test",
            )
            .with_timing(Duration::from_millis(10), Duration::from_millis(1));
            Rig {
                pc,
                dir,
                engine,
                controller,
                _tmp: tmp,
            }
        }

        /// A benchmark is armed, waiting for the game and the start key.
        fn arm_a_benchmark(&self) {
            self.controller
                .start("valorant", Context::default())
                .unwrap();
        }

        /// What the journal says the restore points came to, oldest first.
        fn recorded(&self) -> Vec<RestoreOutcome> {
            journal_in(&self.dir)
                .read()
                .unwrap()
                .history
                .into_iter()
                .filter_map(|entry| match entry {
                    Entry::RestorePoint { outcome, .. } => Some(outcome),
                    _ => None,
                })
                .collect()
        }

        /// Applies Game Mode the way the Apply command does, with the game listed the way `listed` says.
        fn apply_game_mode(
            &self,
            listed: impl Fn() -> Option<bool> + Send + 'static,
        ) -> Result<Vec<ItemOutcome>, String> {
            block_on(change_with(
                &self.engine,
                &self.controller,
                listed,
                |engine| engine.apply(&input(), &[TweakId::GameMode]),
            ))
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            // An armed session keeps a thread watching for the game until it ends.
            self.controller.cancel();
        }
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

    /// The rules' input as a scan that fails; `asked` says whether anybody ran it.
    async fn scan_that_fails(asked: Arc<AtomicBool>) -> Result<PlanInput, String> {
        asked.store(true, Ordering::SeqCst);
        Err("The scan failed.".to_string())
    }

    #[test]
    fn skipping_records_that_it_was_skipped_and_asks_windows_for_nothing() {
        let rig = Rig::new();
        let outcome = restore_point(&rig.engine, RestoreChoice::Skip, || {
            panic!("a restore point was asked for")
        })
        .unwrap();
        assert_eq!(outcome, RestoreOutcome::Skipped);
        assert_eq!(rig.recorded(), [RestoreOutcome::Skipped]);
        assert!(rig.pc.writes().is_empty());
    }

    #[test]
    fn creating_records_whatever_the_helper_came_back_with() {
        for said in [
            RestoreOutcome::Created { sequence: 57 },
            RestoreOutcome::Reused {
                sequence: 56,
                time: "2026-11-02T09:00:00+00:00".into(),
            },
            RestoreOutcome::Failed {
                reason: "System Protection is off.".into(),
            },
        ] {
            let rig = Rig::new();
            let outcome =
                restore_point(&rig.engine, RestoreChoice::Create, || said.clone()).unwrap();
            assert_eq!(outcome, said);
            assert_eq!(rig.recorded(), [said]);
            assert!(
                rig.pc.writes().is_empty(),
                "a restore point changes no setting"
            );
        }
    }

    #[test]
    fn turning_protection_on_comes_before_the_point_and_is_a_change_of_its_own() {
        let rig = Rig::new();
        rig.pc.set_protection(Readiness::ProtectionOff);
        let pc = rig.pc.clone();
        let outcome = restore_point(&rig.engine, RestoreChoice::TurnOnAndCreate, move || {
            assert_eq!(pc.protection(), Readiness::Ready, "on before the point");
            RestoreOutcome::Created { sequence: 58 }
        })
        .unwrap();
        assert_eq!(outcome, RestoreOutcome::Created { sequence: 58 });
        assert_eq!(rig.recorded(), [outcome]);
        assert_eq!(rig.pc.writes(), ["protection on"]);
        assert_eq!(
            rig.engine.applied_ids().unwrap(),
            ["system-protection"],
            "it has a row of its own, and an undo"
        );
    }

    #[test]
    fn a_turn_on_that_fails_is_a_failed_restore_point_and_not_an_error() {
        // Two ways turning it on fails: Windows refuses the switch, or a policy decides System Protection.
        let cant_switch = || {
            let rig = Rig::new();
            rig.pc.set_protection(Readiness::ProtectionOff);
            rig.pc.fail_write(PROTECTION);
            rig
        };
        let blocked = || {
            let rig = Rig::new();
            rig.pc.set_protection(Readiness::BlockedByPolicy);
            rig
        };
        let ways: [&dyn Fn() -> Rig; 2] = [&cant_switch, &blocked];
        for make in ways {
            // The reason is the engine's own, whatever its words are.
            let reason = make().engine.turn_protection_on().unwrap_err().to_string();
            let rig = make();

            let outcome = restore_point(&rig.engine, RestoreChoice::TurnOnAndCreate, || {
                panic!("a restore point was asked for after protection wouldn't turn on")
            })
            .unwrap();

            let failed = RestoreOutcome::Failed { reason };
            assert_eq!(outcome, failed);
            assert_eq!(rig.recorded(), [failed], "the page and the report agree");
            assert!(rig.pc.writes().is_empty());
        }
    }

    #[test]
    fn a_journal_that_cant_take_the_outcome_is_an_error_for_the_page() {
        let rig = Rig::new();
        // Another process is changing settings, and holds the journal.
        let held = journal_in(&rig.dir).lock().unwrap();
        let asked = Cell::new(0);
        for choice in [RestoreChoice::Skip, RestoreChoice::Create] {
            let why = restore_point(&rig.engine, choice, || {
                asked.set(asked.get() + 1);
                RestoreOutcome::Created { sequence: 57 }
            })
            .unwrap_err();
            assert_eq!(why.kind(), io::ErrorKind::WouldBlock);
        }
        assert_eq!(asked.get(), 1, "only the creating choice asked Windows");
        drop(held);
        assert!(rig.recorded().is_empty(), "nothing was half written");
    }

    #[test]
    fn a_closed_gate_refuses_before_the_journal_or_windows_is_touched() {
        for (closed, said) in [
            (Gate::GameRunning, "Close VALORANT first."),
            (Gate::BenchmarkActive, "Finish the benchmark first."),
            (
                Gate::CantTell,
                "Kestrel couldn't check whether VALORANT is open. Try again.",
            ),
        ] {
            let rig = Rig::new();
            let why = gated(closed, &rig.engine, |engine| {
                engine.apply(&input(), &[TweakId::GameMode])
            })
            .unwrap_err();
            assert_eq!(why, said);
            assert!(rig.pc.writes().is_empty());
            assert!(!rig.dir.exists(), "not even the journal's folder");
        }
    }

    #[test]
    fn an_open_gate_hands_the_engine_its_work_and_says_why_it_failed() {
        let rig = Rig::new();
        let items = gated(Gate::Open, &rig.engine, |engine| {
            engine.apply(&input(), &[TweakId::GameMode])
        })
        .unwrap();
        assert!(items[0].ok, "{items:?}");
        assert_eq!(rig.engine.applied_ids().unwrap(), ["game-mode"]);
        // The engine's errors are the command's, in the engine's words.
        let held = journal_in(&rig.dir).lock().unwrap();
        let why = gated(Gate::Open, &rig.engine, |engine| {
            engine.undo(TweakId::GameMode)
        })
        .unwrap_err();
        assert_eq!(
            why,
            "Kestrel is already changing settings. Try again in a moment."
        );
        drop(held);
    }

    #[test]
    fn a_change_waits_for_the_benchmark_then_for_the_game_and_never_guesses() {
        for (armed, listed, said) in [
            (true, Some(false), "Finish the benchmark first."),
            (true, Some(true), "Finish the benchmark first."),
            (true, None, "Finish the benchmark first."),
            (false, Some(true), "Close VALORANT first."),
            (
                false,
                None,
                "Kestrel couldn't check whether VALORANT is open. Try again.",
            ),
        ] {
            let rig = Rig::new();
            if armed {
                rig.arm_a_benchmark();
            }
            let why = rig.apply_game_mode(move || listed).unwrap_err();
            assert_eq!(
                why, said,
                "benchmark armed: {armed}, game listed: {listed:?}"
            );
            assert!(rig.pc.writes().is_empty());
            assert!(!rig.dir.exists(), "not even the journal's folder");
        }
    }

    #[test]
    fn a_change_goes_ahead_with_no_benchmark_armed_and_the_game_not_listed() {
        let rig = Rig::new();
        let items = rig.apply_game_mode(|| Some(false)).unwrap();
        assert!(items[0].ok, "{items:?}");
        assert_eq!(rig.engine.applied_ids().unwrap(), ["game-mode"]);
    }

    #[test]
    fn the_gate_opens_again_when_the_benchmark_ends() {
        let rig = Rig::new();
        rig.arm_a_benchmark();
        assert_eq!(
            rig.apply_game_mode(|| Some(false)).unwrap_err(),
            "Finish the benchmark first."
        );
        rig.controller.cancel();
        assert!(rig.apply_game_mode(|| Some(false)).is_ok());
    }

    #[test]
    fn the_view_carries_the_gate_as_it_is_and_a_list_that_cant_be_read_is_not_open() {
        for (armed, listed, expected) in [
            (false, Some(false), Gate::Open),
            (false, Some(true), Gate::GameRunning),
            (true, Some(false), Gate::BenchmarkActive),
            (false, None, Gate::CantTell),
        ] {
            let rig = Rig::new();
            if armed {
                rig.arm_a_benchmark();
            }
            let view = block_on(view_with(
                &rig.engine,
                &rig.controller,
                move || listed,
                input(),
            ))
            .unwrap();
            assert_eq!(view.gate, expected);
        }
    }

    #[test]
    fn the_gate_is_asked_before_the_scan_so_a_scan_that_fails_cant_hide_it() {
        let rig = Rig::new();
        let asked = Arc::new(AtomicBool::new(false));
        let apply = |engine: &Engine, input: &PlanInput| engine.apply(input, &[TweakId::GameMode]);

        let why = block_on(change_with_input(
            &rig.engine,
            &rig.controller,
            || Some(true),
            scan_that_fails(asked.clone()),
            apply,
        ))
        .unwrap_err();
        assert_eq!(why, "Close VALORANT first.");
        assert!(!asked.load(Ordering::SeqCst), "the scan wasn't even run");

        // With the gate open the scan's own error comes back.
        let why = block_on(change_with_input(
            &rig.engine,
            &rig.controller,
            || Some(false),
            scan_that_fails(asked.clone()),
            apply,
        ))
        .unwrap_err();
        assert_eq!(why, "The scan failed.");
        assert!(asked.load(Ordering::SeqCst));
        assert!(rig.pc.writes().is_empty());
        assert!(!rig.dir.exists());
    }

    #[test]
    fn the_gate_is_asked_again_right_before_the_engine_and_that_is_the_one_that_counts() {
        // Open when the scan starts, and closed by the time it is done: the game was started meanwhile.
        let rig = Rig::new();
        let asked = Arc::new(AtomicUsize::new(0));
        let listed = {
            let asked = asked.clone();
            move || Some(asked.fetch_add(1, Ordering::SeqCst) >= 1)
        };

        let why = block_on(change_with_input(
            &rig.engine,
            &rig.controller,
            listed,
            async { Ok::<_, String>(input()) },
            |engine: &Engine, input: &PlanInput| engine.apply(input, &[TweakId::GameMode]),
        ))
        .unwrap_err();

        assert_eq!(why, "Close VALORANT first.");
        assert_eq!(asked.load(Ordering::SeqCst), 2);
        assert!(rig.pc.writes().is_empty());
        assert!(!rig.dir.exists());
    }

    #[test]
    fn with_the_gate_open_the_scans_input_goes_to_the_work() {
        let rig = Rig::new();
        let items = block_on(change_with_input(
            &rig.engine,
            &rig.controller,
            || Some(false),
            async { Ok::<_, String>(input()) },
            |engine: &Engine, input: &PlanInput| engine.apply(input, &[TweakId::GameMode]),
        ))
        .unwrap();
        assert!(items[0].ok, "{items:?}");
        assert_eq!(rig.engine.applied_ids().unwrap(), ["game-mode"]);
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_no_helper_is_started_and_the_point_is_skipped() {
        // On Windows this would start this program again with `--restore-point`; here, where there are no restore
        // points, it must not start anything (the program is the test runner).
        assert_eq!(create_restore_point(), RestoreOutcome::Skipped);
    }

    #[test]
    fn the_rules_see_the_scan_and_the_fullscreen_mode_valorant_is_set_to() {
        let snapshot = kestrel_core::owner_pc_fixture();
        let windowed = ValorantSettings {
            fullscreen_mode: Some(2),
            ..Default::default()
        };
        assert_eq!(
            input_for(&snapshot, Some(windowed)),
            PlanInput::from_snapshot(&snapshot, Some(2))
        );
        assert_eq!(input_for(&snapshot, None).fullscreen_mode, None);
        let unreadable = ValorantSettings::default();
        assert_eq!(input_for(&snapshot, Some(unreadable)).fullscreen_mode, None);
        assert_eq!(input_for(&snapshot, None).build, 26200);
    }
}
