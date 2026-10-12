//! One benchmark session (spec "What the owner sees", 3): a warm-up pass, then runs 1–3, each started by the
//! owner's start key. A pure state machine: the app performs the actions it returns.
use crate::model::{Flag, RunRecord};
use serde::Serialize;

/// Runs that count (the warm-up comes before them).
pub const RUNS: u8 = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "kebab-case")]
pub enum Phase {
    /// Armed: waiting for the start key (which needs the game running).
    Waiting,
    Capturing,
    /// The last run came back flagged; the owner redoes or keeps it.
    Review {
        flags: Vec<Flag>,
    },
    Done,
    Failed {
        message: String,
    },
    Cancelled,
}

/// What the app must do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// One long beep: a capture starts.
    BeepStart,
    /// Two short beeps: a capture ended.
    BeepEnd,
    /// Capture pass `pass` (0 = warm-up, 1–3 = runs).
    StartCapture { pass: u8 },
    /// Stop the running capture (cancelled).
    StopCapture,
    /// All runs are in: combine and save them.
    Finish,
}

/// The session as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub profile_id: String,
    /// 0 = warm-up, 1–3 = runs.
    pub pass: u8,
    #[serde(flatten)]
    pub phase: Phase,
    pub game_running: bool,
    /// Runs 1–3 as accepted so far.
    pub runs: Vec<Option<RunRecord>>,
    pub result_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Session {
    profile_id: String,
    pass: u8,
    phase: Phase,
    game_running: bool,
    runs: Vec<Option<RunRecord>>,
    /// A flagged run waiting for redo or keep.
    pending: Option<RunRecord>,
    result_id: Option<String>,
}

impl Session {
    pub fn new(profile_id: &str) -> Self {
        Self {
            profile_id: profile_id.to_string(),
            pass: 0,
            phase: Phase::Waiting,
            game_running: false,
            runs: vec![None; usize::from(RUNS)],
            pending: None,
            result_id: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn pass(&self) -> u8 {
        self.pass
    }

    pub fn is_over(&self) -> bool {
        matches!(
            self.phase,
            Phase::Done | Phase::Failed { .. } | Phase::Cancelled
        )
    }

    pub fn set_game_running(&mut self, running: bool) {
        self.game_running = running;
    }

    pub fn start_pressed(&mut self) -> Vec<Action> {
        if self.phase != Phase::Waiting || !self.game_running {
            return Vec::new();
        }
        self.phase = Phase::Capturing;
        vec![Action::BeepStart, Action::StartCapture { pass: self.pass }]
    }

    pub fn capture_finished(&mut self, outcome: Result<RunRecord, String>) -> Vec<Action> {
        if self.phase != Phase::Capturing {
            return Vec::new();
        }
        let run = match outcome {
            Ok(run) => run,
            Err(message) => {
                self.phase = Phase::Failed { message };
                return vec![Action::BeepEnd];
            }
        };
        let mut actions = vec![Action::BeepEnd];
        if self.pass == 0 {
            // The warm-up only warms the game up; its numbers and flags aren't kept.
            self.pass = 1;
            self.phase = Phase::Waiting;
        } else if run.flags.is_empty() {
            actions.extend(self.accept(run));
        } else {
            self.phase = Phase::Review {
                flags: run.flags.clone(),
            };
            self.pending = Some(run);
        }
        actions
    }

    /// Throws the flagged run away and waits to capture it again.
    pub fn redo(&mut self) -> Vec<Action> {
        if matches!(self.phase, Phase::Review { .. }) {
            self.pending = None;
            self.phase = Phase::Waiting;
        }
        Vec::new()
    }

    /// Keeps the flagged run, marked as kept.
    pub fn keep(&mut self) -> Vec<Action> {
        if !matches!(self.phase, Phase::Review { .. }) {
            return Vec::new();
        }
        let Some(mut run) = self.pending.take() else {
            return Vec::new();
        };
        run.kept = true;
        self.accept(run)
    }

    pub fn cancel(&mut self) -> Vec<Action> {
        if self.is_over() {
            return Vec::new();
        }
        let was_capturing = self.phase == Phase::Capturing;
        self.phase = Phase::Cancelled;
        self.pending = None;
        if was_capturing {
            vec![Action::StopCapture]
        } else {
            Vec::new()
        }
    }

    /// Ends the session with an error (for example, saving the result failed).
    pub fn fail(&mut self, message: String) {
        if self.phase != Phase::Cancelled {
            self.phase = Phase::Failed { message };
        }
    }

    pub fn set_result_id(&mut self, id: String) {
        self.result_id = Some(id);
    }

    /// The accepted runs, in order.
    pub fn runs(&self) -> Vec<RunRecord> {
        self.runs.iter().flatten().cloned().collect()
    }

    pub fn view(&self) -> SessionView {
        SessionView {
            profile_id: self.profile_id.clone(),
            pass: self.pass,
            phase: self.phase.clone(),
            game_running: self.game_running,
            runs: self.runs.clone(),
            result_id: self.result_id.clone(),
        }
    }

    fn accept(&mut self, run: RunRecord) -> Vec<Action> {
        self.runs[usize::from(self.pass) - 1] = Some(run);
        if self.pass >= RUNS {
            self.phase = Phase::Done;
            vec![Action::Finish]
        } else {
            self.pass += 1;
            self.phase = Phase::Waiting;
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RunMetrics;
    use serde_json::json;

    fn clean(avg: f64) -> RunRecord {
        RunRecord {
            metrics: RunMetrics {
                avg_fps: avg,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn flagged() -> RunRecord {
        RunRecord {
            flags: vec![Flag::TooShort],
            ..clean(100.0)
        }
    }

    fn armed() -> Session {
        let mut s = Session::new("valorant");
        s.set_game_running(true);
        s
    }

    #[test]
    fn warm_up_then_three_runs_then_finish() {
        let mut s = armed();
        assert_eq!(
            s.start_pressed(),
            vec![Action::BeepStart, Action::StartCapture { pass: 0 }]
        );
        assert_eq!(
            s.capture_finished(Ok(flagged())),
            vec![Action::BeepEnd],
            "warm-up flags don't matter"
        );
        assert_eq!((s.pass(), s.phase().clone()), (1, Phase::Waiting));
        for pass in 1..=3u8 {
            assert_eq!(
                s.start_pressed(),
                vec![Action::BeepStart, Action::StartCapture { pass }]
            );
            let actions = s.capture_finished(Ok(clean(400.0 + f64::from(pass))));
            if pass < 3 {
                assert_eq!(actions, vec![Action::BeepEnd]);
                assert_eq!((s.pass(), s.phase().clone()), (pass + 1, Phase::Waiting));
            } else {
                assert_eq!(actions, vec![Action::BeepEnd, Action::Finish]);
                assert_eq!(s.phase(), &Phase::Done);
            }
        }
        assert_eq!(
            s.runs()
                .iter()
                .map(|r| r.metrics.avg_fps)
                .collect::<Vec<_>>(),
            vec![401.0, 402.0, 403.0]
        );
        assert!(s.is_over());
    }

    #[test]
    fn a_flagged_run_is_redone_or_kept() {
        let mut s = armed();
        s.start_pressed();
        s.capture_finished(Ok(clean(1.0)));
        s.start_pressed();
        assert_eq!(s.capture_finished(Ok(flagged())), vec![Action::BeepEnd]);
        assert_eq!(
            s.phase(),
            &Phase::Review {
                flags: vec![Flag::TooShort]
            }
        );
        assert!(s.redo().is_empty());
        assert_eq!(
            (s.pass(), s.phase().clone()),
            (1, Phase::Waiting),
            "redo repeats run 1"
        );
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        assert!(s.keep().is_empty());
        assert_eq!((s.pass(), s.phase().clone()), (2, Phase::Waiting));
        assert!(s.runs()[0].kept);
        s.start_pressed();
        s.capture_finished(Ok(clean(2.0)));
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        assert_eq!(
            s.keep(),
            vec![Action::Finish],
            "keeping the third run finishes"
        );
        assert_eq!(s.phase(), &Phase::Done);
    }

    #[test]
    fn start_key_is_ignored_unless_waiting_with_the_game() {
        let mut s = Session::new("valorant");
        assert!(s.start_pressed().is_empty(), "the game isn't running");
        assert_eq!(s.phase(), &Phase::Waiting);
        s.set_game_running(true);
        assert_eq!(s.start_pressed().len(), 2);
        assert!(
            s.start_pressed().is_empty(),
            "a second press during the capture does nothing"
        );
        s.capture_finished(Ok(clean(1.0)));
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        assert!(
            s.start_pressed().is_empty(),
            "nothing starts while a flagged run waits for a decision"
        );
        assert!(
            s.capture_finished(Ok(clean(1.0))).is_empty(),
            "a stray capture result is ignored"
        );
    }

    #[test]
    fn cancel_stops_a_capture_and_ends_the_session() {
        let mut s = armed();
        s.start_pressed();
        assert_eq!(s.cancel(), vec![Action::StopCapture]);
        assert_eq!(s.phase(), &Phase::Cancelled);
        assert!(s.capture_finished(Ok(clean(1.0))).is_empty());
        assert!(s.start_pressed().is_empty() && s.cancel().is_empty());
        let mut waiting = armed();
        assert!(waiting.cancel().is_empty());
        assert_eq!(waiting.phase(), &Phase::Cancelled);
    }

    #[test]
    fn a_capture_error_fails_the_session() {
        let mut s = armed();
        s.start_pressed();
        assert_eq!(
            s.capture_finished(Err("PresentMon failed".into())),
            vec![Action::BeepEnd]
        );
        assert_eq!(
            s.phase(),
            &Phase::Failed {
                message: "PresentMon failed".into()
            }
        );
        assert!(s.start_pressed().is_empty() && s.is_over());
        let mut done = armed();
        done.fail("Couldn't save the result".into());
        assert_eq!(
            done.phase(),
            &Phase::Failed {
                message: "Couldn't save the result".into()
            }
        );
    }

    #[test]
    fn the_view_is_flat_camel_case_json() {
        let mut s = armed();
        s.start_pressed();
        s.capture_finished(Ok(clean(1.0)));
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        let v = serde_json::to_value(s.view()).unwrap();
        assert_eq!(v["profileId"], json!("valorant"));
        assert_eq!(v["pass"], json!(1));
        assert_eq!(v["phase"], json!("review"));
        assert_eq!(v["flags"], json!([{"kind": "too-short"}]));
        assert_eq!(v["gameRunning"], json!(true));
        assert_eq!(v["runs"], json!([null, null, null]));
        assert_eq!(v["resultId"], json!(null));
    }
}
