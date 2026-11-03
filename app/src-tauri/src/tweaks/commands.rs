//! The commands the Tweaks tab calls. Every one is `async`, so none runs on the UI thread, and everything that can
//! block runs through `blocking` on a thread of its own: the engine's calls, and the restore point's helper, which may
//! take minutes.
//!
//! A command that changes anything goes through `change`, which asks the gate first. The reads (the view, the Preview,
//! the report) don't: the page can always look. Commands return what changed and the page reads the view again, since
//! nothing changes in the background.
use std::io;
use std::sync::Arc;

use kestrel_bench::ValorantSettings;
use kestrel_core::tweak_id::TweakId;
use kestrel_core::Report;
use kestrel_tweaks::{
    run_helper, Engine, Gate, ItemOutcome, PlanInput, PreviewView, RestoreChoice, RestoreOutcome,
    TweaksView,
};
use tauri::{AppHandle, Manager};

use super::{blocking, game_running, gate, report_text, require_open};
use crate::bench::controller::Controller;
use crate::AppState;

fn engine_of(app: &AppHandle) -> Arc<Engine> {
    app.state::<Arc<Engine>>().inner().clone()
}

fn controller_of(app: &AppHandle) -> Controller {
    app.state::<Controller>().inner().clone()
}

/// What the rules need to know about this PC: the last scan, and the fullscreen mode VALORANT's own settings file says.
fn input_for(report: &Report, settings: Option<ValorantSettings>) -> PlanInput {
    PlanInput::from_snapshot(&report.snapshot, settings.and_then(|s| s.fullscreen_mode))
}

/// The rules' input now. The scan runs at start, so the tab can ask before it is done: then this runs the same
/// read-only collection the scan command does (off the runtime's thread) and keeps it as the last scan, rather than
/// failing. A probe that fails is the command's error.
async fn plan_input(app: &AppHandle) -> Result<PlanInput, String> {
    let report = crate::latest_report(app).await?;
    let controller = controller_of(app);
    // VALORANT's settings are read from disk.
    blocking(move || input_for(&report, controller.game_settings())).await
}

/// Whether tweaks may change Windows right now.
fn gate_now(controller: &Controller) -> Gate {
    gate(game_running(), controller.session_active())
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

/// Runs a call that changes Windows, or the journal's record of what was changed, once the gate is open. Every command
/// that changes anything comes through here.
async fn change<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(&Engine) -> io::Result<T> + Send + 'static,
) -> Result<T, String> {
    let engine = engine_of(app);
    let controller = controller_of(app);
    blocking(move || gated(gate_now(&controller), &engine, work)).await?
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
    let engine = engine_of(&app);
    let controller = controller_of(&app);
    blocking(move || {
        engine
            .view(&input, gate_now(&controller))
            .map_err(|why| why.to_string())
    })
    .await?
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
    let input = plan_input(&app).await?;
    change(&app, move |engine| engine.apply(&input, &ids)).await
}

/// Undoes one tweak, if it still holds what Kestrel wrote.
#[tauri::command(async)]
pub async fn tweaks_undo(app: AppHandle, id: TweakId) -> Result<ItemOutcome, String> {
    change(&app, move |engine| engine.undo(id)).await
}

/// Writes Kestrel's value over what something else changed a tweak to.
#[tauri::command(async)]
pub async fn tweaks_apply_again(app: AppHandle, id: TweakId) -> Result<ItemOutcome, String> {
    let input = plan_input(&app).await?;
    change(&app, move |engine| engine.apply_again(&input, id)).await
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
    use chrono::NaiveDate;
    use kestrel_core::FormFactor;
    use kestrel_tweaks::fake::{FakeSystem, PROTECTION};
    use kestrel_tweaks::journal::{Entry, JournalFile, PlainFolder};
    use kestrel_tweaks::{Readiness, System};
    use std::cell::Cell;
    use std::path::{Path, PathBuf};

    fn journal_in(dir: &Path) -> JournalFile {
        JournalFile::new(dir, Box::new(PlainFolder))
    }

    /// A fake PC on stock Balanced with Game Mode off, and an engine for it with a journal that doesn't exist yet.
    struct Rig {
        pc: Arc<FakeSystem>,
        dir: PathBuf,
        engine: Engine,
        _tmp: tempfile::TempDir,
    }

    impl Rig {
        fn new() -> Rig {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("Kestrel");
            let pc = Arc::new(FakeSystem::new());
            pc.set_dword(
                kestrel_tweaks::fake::SID,
                kestrel_tweaks::targets::GAME_BAR_KEY,
                kestrel_tweaks::targets::AUTO_GAME_MODE,
                0,
            );
            let engine = Engine::new(pc.clone(), journal_in(&dir));
            Rig {
                pc,
                dir,
                engine,
                _tmp: tmp,
            }
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
            rig.engine.applied_ids(),
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
        assert_eq!(rig.engine.applied_ids(), ["game-mode"]);
        // The engine's errors are the command's, in the engine's words.
        let held = journal_in(&rig.dir).lock().unwrap();
        let why = gated(Gate::Open, &rig.engine, |engine| {
            engine.undo(TweakId::GameMode)
        })
        .unwrap_err();
        assert_eq!(
            why,
            "Kestrel is already changing settings in another window."
        );
        drop(held);
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
        let report = kestrel_core::build_report(
            kestrel_core::owner_pc_fixture(),
            NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(),
            "test",
        );
        let windowed = ValorantSettings {
            fullscreen_mode: Some(2),
            ..Default::default()
        };
        assert_eq!(
            input_for(&report, Some(windowed)),
            PlanInput::from_snapshot(&report.snapshot, Some(2))
        );
        assert_eq!(input_for(&report, None).fullscreen_mode, None);
        let unreadable = ValorantSettings::default();
        assert_eq!(input_for(&report, Some(unreadable)).fullscreen_mode, None);
        assert_eq!(input_for(&report, None).build, 26200);
    }
}
