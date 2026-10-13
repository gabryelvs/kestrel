//! Runs a benchmark session (decisions 37–49): the start key, the beeps, one capture at a time, and turning each
//! capture into a run. The rules live in kestrel_bench::session; this file performs the actions they return. The
//! outside world (start key, beeps, probes, events) sits behind `Platform`, so whole sessions run in tests with fakes.
//!
//! The platform is never called while the controller's lock is held: claiming or releasing the start key runs on the
//! UI thread, which may be waiting for that lock to handle a key press.
use super::beep::Cue;
use super::capture::{Capture, CaptureOutput, CANCELLED};
use kestrel_bench::flags::{flags, RunInputs};
use kestrel_bench::session::{Action, Phase, Session, SessionView};
use kestrel_bench::{
    csv, metrics, result, Benchmark, Context, GameProfile, RunRecord, Store, ValorantSettings,
    PRESENTMON_VERSION,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The key that starts each pass (decision 37).
pub const START_KEY: &str = "F10";
/// Exists in the work folder while PresentMon runs; found at start-up, it means a crash mid-capture.
pub const ACTIVE_MARKER: &str = "capture.active";
const COUNTDOWN: u32 = 10;
const BUSY: &str = "A benchmark is already running.";

pub trait Platform: Send + Sync + 'static {
    /// Claims the start key; false when another app already holds it.
    fn register_start_key(&self) -> bool;
    /// Gives the start key back. Called whenever a session is over or dismissed and before each new claim, so it must
    /// do nothing when the key isn't held.
    fn unregister_start_key(&self);
    fn game_running(&self, process_name: &str) -> bool;
    fn beep(&self, cue: Cue);
    /// Background apps right now: name and share of the whole CPU, in %.
    fn busy_now(&self) -> Vec<(String, f32)>;
    fn game_settings(&self) -> Option<ValorantSettings>;
    /// Tells the UI the session changed; None means there's no session.
    fn emit(&self, state: Option<&BenchState>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartMode {
    /// The start key works.
    Hotkey,
    /// Another app holds the key: the UI's "Start run" button starts a 10-second countdown instead.
    Countdown,
}

/// What the UI shows about the session.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchState {
    #[serde(flatten)]
    pub session: SessionView,
    pub start_mode: StartMode,
    pub start_key: &'static str,
    pub capture_seconds: u32,
    pub switch_at_seconds: u32,
    /// Seconds left in the running capture.
    pub seconds_left: Option<u32>,
    /// Seconds left in the fallback countdown.
    pub countdown: Option<u32>,
}

struct Live {
    session: Session,
    profile: &'static GameProfile,
    start_mode: StartMode,
    seconds_left: Option<u32>,
    countdown: Option<u32>,
    /// Raised by Cancel. Also identifies this session, so a late result from an old capture can't reach a new one.
    cancel: Arc<AtomicBool>,
    /// Frame times of accepted runs (index = run − 1).
    frames: Vec<Option<Vec<f32>>>,
    /// Frame times of a flagged run waiting for redo or keep.
    pending_frames: Option<Vec<f32>>,
    context: Context,
}

impl Live {
    /// Runs one step of the session and says which run slot it filled, if any: the first slot that was empty before
    /// and isn't now. A run's frame times go to the slot the session put the run in, not to one a copy of the
    /// session's rules picks.
    fn accepting(
        &mut self,
        step: impl FnOnce(&mut Session) -> Vec<Action>,
    ) -> (Vec<Action>, Option<usize>) {
        let before = self.session.view().runs;
        let actions = step(&mut self.session);
        let filled = self
            .session
            .view()
            .runs
            .iter()
            .zip(&before)
            .position(|(now, was)| now.is_some() && was.is_none());
        (actions, filled)
    }
}

/// A finished session as the save needs it, taken in one look at the session: once `perform` has it, a dismiss (or a
/// dismiss and a new start) can't take the session away from the save.
struct Finished {
    /// The benchmark the session's runs add up to.
    bench: Benchmark,
    /// Each run's frame times, or the first thing that's missing.
    frames: Result<Vec<Vec<f32>>, String>,
    /// Identifies the session the result belongs to (see `Live::cancel`).
    cancel: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct Controller {
    live: Arc<Mutex<Option<Live>>>,
    platform: Arc<dyn Platform>,
    capture: Arc<dyn Capture>,
    store: Arc<Store>,
    work_dir: PathBuf,
    kestrel_version: String,
    /// How often the game check repeats while a session waits.
    poll: Duration,
    /// One step of the fallback countdown.
    tick: Duration,
}

impl Controller {
    pub fn new(
        platform: Arc<dyn Platform>,
        capture: Arc<dyn Capture>,
        store: Arc<Store>,
        work_dir: PathBuf,
        kestrel_version: &str,
    ) -> Self {
        Self {
            live: Arc::new(Mutex::new(None)),
            platform,
            capture,
            store,
            work_dir,
            kestrel_version: kestrel_version.to_string(),
            poll: Duration::from_secs(2),
            tick: Duration::from_secs(1),
        }
    }

    pub fn with_timing(mut self, poll: Duration, tick: Duration) -> Self {
        self.poll = poll;
        self.tick = tick;
        self
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn game_settings(&self) -> Option<ValorantSettings> {
        self.platform.game_settings()
    }

    pub fn state(&self) -> Option<BenchState> {
        self.lock().as_ref().map(|live| BenchState {
            session: live.session.view(),
            start_mode: live.start_mode,
            start_key: START_KEY,
            capture_seconds: live.profile.route.capture_seconds,
            switch_at_seconds: live.profile.route.switch_at_seconds,
            seconds_left: live.seconds_left,
            countdown: live.countdown,
        })
    }

    /// Arms a session for `profile_id`. `context` describes the PC (from the last scan).
    pub fn start(&self, profile_id: &str, context: Context) -> Result<BenchState, String> {
        if self.busy() {
            return Err(BUSY.into());
        }
        let profile =
            kestrel_bench::profile(profile_id).ok_or("Kestrel doesn't know that game.")?;
        // Never call the platform with the lock held (see the module docs).
        self.platform.unregister_start_key();
        let start_mode = if self.platform.register_start_key() {
            StartMode::Hotkey
        } else {
            StartMode::Countdown
        };
        let mut session = Session::new(profile.id);
        session.set_game_running(self.platform.game_running(profile.process_name));
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut live = self.lock();
            if live.as_ref().is_some_and(|l| !l.session.is_over()) {
                return Err(BUSY.into());
            }
            *live = Some(Live {
                session,
                profile,
                start_mode,
                seconds_left: None,
                countdown: None,
                cancel: cancel.clone(),
                frames: vec![None; usize::from(kestrel_bench::session::RUNS)],
                pending_frames: None,
                context,
            });
        }
        self.watch_game(profile, cancel);
        self.emit();
        self.state()
            .ok_or_else(|| "The benchmark didn't start.".to_string())
    }

    /// The start key was pressed (or the countdown ended).
    pub fn start_pressed(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) if live.countdown.is_none() => live.session.start_pressed(),
            _ => Vec::new(),
        };
        self.perform(actions);
    }

    /// The fallback start, offered in either start mode (another app may hold the start key, or a game may swallow it):
    /// beeps for 10 seconds, then starts the pass. The start key is ignored while it counts down (`start_pressed`).
    pub fn start_countdown(&self) -> Result<(), String> {
        let cancel = {
            let mut guard = self.lock();
            let live = guard.as_mut().ok_or("No benchmark is running.")?;
            if live.session.phase() != &Phase::Waiting {
                return Err("Wait for the current step to finish.".into());
            }
            if !live.session.view().game_running {
                return Err("Start the game first.".into());
            }
            if live.countdown.is_some() {
                return Ok(());
            }
            live.countdown = Some(COUNTDOWN);
            live.cancel.clone()
        };
        self.emit();
        let me = self.clone();
        std::thread::spawn(move || {
            for left in (1..=COUNTDOWN).rev() {
                if cancel.load(Ordering::SeqCst) {
                    return;
                }
                me.set_countdown(&cancel, Some(left));
                me.platform.beep(Cue::Tick);
                std::thread::sleep(me.tick);
            }
            me.set_countdown(&cancel, None);
            if !cancel.load(Ordering::SeqCst) {
                me.start_pressed();
            }
        });
        Ok(())
    }

    /// Throws away a flagged run; the next press captures it again.
    pub fn redo(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) => {
                let actions = live.session.redo();
                // A flagged run's frame times only matter while the owner decides about it.
                if !matches!(live.session.phase(), Phase::Review { .. }) {
                    live.pending_frames = None;
                }
                actions
            }
            None => Vec::new(),
        };
        self.perform(actions);
    }

    /// Keeps a flagged run.
    pub fn keep(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) => {
                let (actions, accepted) = live.accepting(Session::keep);
                // The flagged run's frame times go with the run, if the session took it.
                let pending = live.pending_frames.take();
                if let Some(slot) = accepted {
                    live.frames[slot] = pending;
                }
                actions
            }
            None => Vec::new(),
        };
        self.perform(actions);
    }

    pub fn cancel(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) => {
                live.cancel.store(true, Ordering::SeqCst);
                live.countdown = None;
                live.session.cancel()
            }
            None => Vec::new(),
        };
        self.perform(actions);
    }

    /// Clears a finished session, so the page shows the results list again.
    pub fn dismiss(&self) {
        let cleared = {
            let mut guard = self.lock();
            if guard.as_ref().is_some_and(|l| l.session.is_over()) {
                if let Some(live) = guard.take() {
                    live.cancel.store(true, Ordering::SeqCst);
                }
                true
            } else {
                false
            }
        };
        if cleared {
            // F10 must never outlive its session. `perform` gives the key back for whichever session is live when it
            // looks, so a dismiss that lands between a session ending and that look would leave the key claimed.
            self.platform.unregister_start_key();
            self.emit();
        }
    }

    fn busy(&self) -> bool {
        self.lock().as_ref().is_some_and(|l| !l.session.is_over())
    }

    fn lock(&self) -> MutexGuard<'_, Option<Live>> {
        // A panic elsewhere must not lock the benchmark forever.
        self.live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn emit(&self) {
        let state = self.state();
        self.platform.emit(state.as_ref());
    }

    /// Gives the key back if the session is over, carries out the session's actions, and updates the UI. The key goes
    /// first: `Finish` blocks while it saves, and F10 must not stay claimed through that. What `Finish` will save is
    /// taken in the same look at the session that decides whether it's over, before anything blocks, so a dismiss
    /// (or a dismiss and a new start) while the key is given back or the result saved can't take the session away.
    fn perform(&self, actions: Vec<Action>) {
        let (over, mut finished) = {
            let guard = self.lock();
            let over = guard.as_ref().is_some_and(|l| l.session.is_over());
            let finished = guard
                .as_ref()
                .filter(|_| actions.contains(&Action::Finish))
                .and_then(|live| self.finished_from(live));
            (over, finished)
        };
        if over {
            self.platform.unregister_start_key();
        }
        for action in actions {
            match action {
                Action::BeepStart => self.platform.beep(Cue::Start),
                // Already played by the capture thread the moment PresentMon stopped (see `spawn_capture`).
                Action::BeepEnd => {}
                Action::StartCapture { pass } => self.spawn_capture(pass),
                Action::StopCapture => {} // cancel() already raised the flag the capture watches
                Action::Finish => {
                    if let Some(finished) = finished.take() {
                        self.save(finished);
                    }
                }
            }
        }
        self.emit();
    }

    fn spawn_capture(&self, pass: u8) {
        let Some((profile, cancel)) = self.lock().as_ref().map(|l| (l.profile, l.cancel.clone()))
        else {
            return;
        };
        let me = self.clone();
        std::thread::spawn(move || {
            let done = Arc::new(AtomicBool::new(false));
            let pacer = {
                let (me, done, cancel) = (me.clone(), done.clone(), cancel.clone());
                std::thread::spawn(move || me.pace(profile, &done, &cancel))
            };
            let _ = std::fs::create_dir_all(&me.work_dir);
            let marker = me.work_dir.join(ACTIVE_MARKER);
            let _ = std::fs::write(&marker, b"capturing");
            let csv_path = me.work_dir.join(format!("capture-{pass}.csv"));
            let outcome = me.capture.run(
                profile.process_name,
                profile.route.capture_seconds,
                &csv_path,
                &cancel,
            );
            let _ = std::fs::remove_file(&marker);
            done.store(true, Ordering::SeqCst);
            let _ = pacer.join();
            // The double beep marks the end of the run as soon as PresentMon stops (spec: at 90 s), not after the
            // analysis below, which samples the CPU for 1.5 s. It sounds in the cases where the session answers with
            // `BeepEnd` (a capture that wasn't cancelled, for the session still capturing it), so `perform` skips that.
            let cancelled = matches!(&outcome, Err(message) if message == CANCELLED);
            if !cancelled && me.capturing(&cancel) {
                me.platform.beep(Cue::End);
            }
            let outcome = outcome.and_then(|output| me.analyse(&output));
            me.capture_finished(&cancel, outcome);
        });
    }

    /// Whether the session `cancel` belongs to is still the live one and still capturing.
    fn capturing(&self, cancel: &Arc<AtomicBool>) -> bool {
        self.lock().as_ref().is_some_and(|l| {
            Arc::ptr_eq(&l.cancel, cancel) && l.session.phase() == &Phase::Capturing
        })
    }

    /// Counts the capture down for the UI and beeps when it's time to switch to the bots.
    fn pace(&self, profile: &GameProfile, done: &AtomicBool, cancel: &Arc<AtomicBool>) {
        let started = Instant::now();
        let mut switched = false;
        while !done.load(Ordering::SeqCst) && !cancel.load(Ordering::SeqCst) {
            let elapsed = started.elapsed().as_secs() as u32;
            if !switched && elapsed >= profile.route.switch_at_seconds {
                switched = true;
                self.platform.beep(Cue::Switch);
            }
            self.set_seconds_left(
                cancel,
                Some(profile.route.capture_seconds.saturating_sub(elapsed)),
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        self.set_seconds_left(cancel, None);
    }

    fn analyse(&self, output: &CaptureOutput) -> Result<(RunRecord, Vec<f32>), String> {
        // PresentMon writes no CSV when the game showed no frames (minimized, never in front): that's a run too
        // short to use, which the owner can redo, not a broken session.
        let frames = match csv::parse(&output.csv) {
            Ok(frames) => frames,
            Err(csv::CsvError::Empty) => Vec::new(),
            Err(e) => return Err(format!("Couldn't read PresentMon's results ({e:?}).")),
        };
        let (metrics, times) = metrics::measure(&frames);
        let busy = self.platform.busy_now();
        let settings = self.platform.game_settings();
        let flags = flags(&RunInputs {
            frames: &frames,
            metrics: &metrics,
            stderr: &output.stderr,
            settings: settings.as_ref(),
            busy: &busy,
        });
        Ok((
            RunRecord {
                metrics,
                flags,
                kept: false,
            },
            times,
        ))
    }

    fn capture_finished(
        &self,
        cancel: &Arc<AtomicBool>,
        outcome: Result<(RunRecord, Vec<f32>), String>,
    ) {
        let actions = {
            let mut guard = self.lock();
            let Some(live) = guard.as_mut().filter(|l| Arc::ptr_eq(&l.cancel, cancel)) else {
                return;
            };
            match outcome {
                Err(message) if message == CANCELLED => Vec::new(),
                Err(message) => live.session.capture_finished(Err(message)),
                Ok((run, times)) => {
                    let (actions, accepted) =
                        live.accepting(|session| session.capture_finished(Ok(run)));
                    // The frame times go where the session put the run: held while the owner decides about a flagged
                    // run, stored with the run the session accepted, or dropped (the warm-up's).
                    if matches!(live.session.phase(), Phase::Review { .. }) {
                        live.pending_frames = Some(times);
                    } else if let Some(slot) = accepted {
                        live.frames[slot] = Some(times);
                    }
                    actions
                }
            }
        };
        self.perform(actions);
    }

    /// What saving needs from a done session that has no result yet (None for any other): the benchmark its runs add
    /// up to, each run's frame times, and the handle that identifies the session. It makes no platform call, so
    /// `perform` can take it with the lock held.
    fn finished_from(&self, live: &Live) -> Option<Finished> {
        if live.session.phase() != &Phase::Done || live.session.view().result_id.is_some() {
            return None;
        }
        let runs = live.session.runs();
        let bench = Benchmark {
            name: format!(
                "Benchmark {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            ),
            created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            profile_id: live.profile.id.to_string(),
            route_version: live.profile.route.version,
            kestrel_version: self.kestrel_version.clone(),
            presentmon_version: PRESENTMON_VERSION.to_string(),
            result: result::combine(&runs),
            runs,
            context: live.context.clone(),
            ..Default::default()
        };
        // Every accepted run has its frame times. One without would be saved as an empty file that passes for a real
        // run, so it fails the session instead.
        let frames = live
            .frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                f.clone()
                    .ok_or_else(|| format!("run {} has no frame times.", i + 1))
            })
            .collect();
        Some(Finished {
            bench,
            frames,
            cancel: live.cancel.clone(),
        })
    }

    /// Saves a finished session's benchmark (the first one becomes the starting point). The id (or a save error) goes
    /// back only to the session it came from, even if that one was dismissed and another started meanwhile; the
    /// benchmark is saved either way.
    fn save(&self, finished: Finished) {
        let Finished {
            mut bench,
            frames,
            cancel,
        } = finished;
        let saved = frames.and_then(|frames| {
            bench.context.game_settings = self.platform.game_settings();
            self.store
                .save(&mut bench, &frames)
                .map_err(|e| e.to_string())
        });
        if let Some(live) = self
            .lock()
            .as_mut()
            .filter(|l| Arc::ptr_eq(&l.cancel, &cancel))
        {
            match saved {
                Ok(id) => live.session.set_result_id(id),
                Err(e) => live.session.fail(format!("Couldn't save the result: {e}")),
            }
        }
    }

    /// Re-checks whether the game runs until the session ends.
    fn watch_game(&self, profile: &'static GameProfile, cancel: Arc<AtomicBool>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(me.poll);
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            let running = me.platform.game_running(profile.process_name);
            let changed = {
                let mut guard = me.lock();
                match guard.as_mut().filter(|l| Arc::ptr_eq(&l.cancel, &cancel)) {
                    None => return,
                    Some(live) if live.session.is_over() => return,
                    Some(live) if live.session.view().game_running != running => {
                        live.session.set_game_running(running);
                        true
                    }
                    Some(_) => false,
                }
            };
            if changed {
                me.emit();
            }
        });
    }

    fn set_seconds_left(&self, cancel: &Arc<AtomicBool>, value: Option<u32>) {
        let changed = match self
            .lock()
            .as_mut()
            .filter(|l| Arc::ptr_eq(&l.cancel, cancel))
        {
            Some(live) if live.seconds_left != value => {
                live.seconds_left = value;
                true
            }
            _ => false,
        };
        if changed {
            self.emit();
        }
    }

    fn set_countdown(&self, cancel: &Arc<AtomicBool>, value: Option<u32>) {
        if let Some(live) = self
            .lock()
            .as_mut()
            .filter(|l| Arc::ptr_eq(&l.cancel, cancel))
        {
            live.countdown = value;
        }
        self.emit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::capture::Simulated;
    use std::path::Path;
    use std::sync::atomic::AtomicUsize;

    #[derive(Default)]
    struct Fake {
        key_free: AtomicBool,
        key_held: AtomicBool,
        game: AtomicBool,
        beeps: Mutex<Vec<Cue>>,
        busy: Mutex<Vec<(String, f32)>>,
        emitted: AtomicUsize,
        /// Counts the reads of the game's settings. The read numbered `hold_read` (from 1; 0 holds none) waits until
        /// `release_read` is raised, so a test can act while a finished session is being saved.
        reads: AtomicUsize,
        hold_read: AtomicUsize,
        read_held: AtomicBool,
        release_read: AtomicBool,
        /// Counts the calls that give the start key back. The call numbered `hold_unregister` (from 1; 0 holds none)
        /// takes effect, then doesn't return until `release_unregister` is raised: a platform call that takes a
        /// while, during which a test can act.
        unregisters: AtomicUsize,
        hold_unregister: AtomicUsize,
        unregister_held: AtomicBool,
        release_unregister: AtomicBool,
    }

    impl Platform for Fake {
        fn register_start_key(&self) -> bool {
            let free = self.key_free.load(Ordering::SeqCst);
            if free {
                self.key_held.store(true, Ordering::SeqCst);
            }
            free
        }
        fn unregister_start_key(&self) {
            self.key_held.store(false, Ordering::SeqCst);
            let call = self.unregisters.fetch_add(1, Ordering::SeqCst) + 1;
            if call == self.hold_unregister.load(Ordering::SeqCst) {
                self.unregister_held.store(true, Ordering::SeqCst);
                while !self.release_unregister.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
        fn game_running(&self, _process_name: &str) -> bool {
            self.game.load(Ordering::SeqCst)
        }
        fn beep(&self, cue: Cue) {
            self.beeps.lock().unwrap().push(cue);
        }
        fn busy_now(&self) -> Vec<(String, f32)> {
            self.busy.lock().unwrap().clone()
        }
        fn game_settings(&self) -> Option<ValorantSettings> {
            let read = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            if read == self.hold_read.load(Ordering::SeqCst) {
                self.read_held.store(true, Ordering::SeqCst);
                while !self.release_read.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            Some(ValorantSettings {
                resolution: Some([1920, 1080]),
                ..Default::default()
            })
        }
        fn emit(&self, _state: Option<&BenchState>) {
            self.emitted.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Never finishes on its own: returns only when cancelled.
    #[derive(Default)]
    struct Blocking {
        started: AtomicBool,
    }

    impl Capture for Blocking {
        fn run(
            &self,
            _p: &str,
            _s: u32,
            _c: &Path,
            cancel: &AtomicBool,
        ) -> Result<CaptureOutput, String> {
            self.started.store(true, Ordering::SeqCst);
            while !cancel.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(CANCELLED.into())
        }
        fn clean_up_stale_session(&self) {}
    }

    /// Fails the way a broken PresentMon does: an error that isn't a cancel.
    struct Failing;

    impl Capture for Failing {
        fn run(
            &self,
            _p: &str,
            _s: u32,
            _c: &Path,
            _cancel: &AtomicBool,
        ) -> Result<CaptureOutput, String> {
            Err("PresentMon failed".into())
        }
        fn clean_up_stale_session(&self) {}
    }

    fn setup(capture: Arc<dyn Capture>) -> (Controller, Arc<Fake>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::default());
        fake.key_free.store(true, Ordering::SeqCst);
        fake.game.store(true, Ordering::SeqCst);
        let store = Arc::new(Store::new(dir.path().join("benchmarks")));
        let controller = Controller::new(
            fake.clone(),
            capture,
            store,
            dir.path().join("work"),
            "0.2.0",
        )
        .with_timing(Duration::from_millis(10), Duration::from_millis(1));
        (controller, fake, dir)
    }

    fn fast() -> Arc<dyn Capture> {
        Arc::new(Simulated {
            seconds_per_capture: 0.0,
        })
    }

    fn wait_for(c: &Controller, what: &str, check: impl Fn(&BenchState) -> bool) -> BenchState {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(state) = c.state() {
                if check(&state) {
                    return state;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {:?}",
                c.state()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn eventually(what: &str, check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !check() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn waiting_at(pass: u8) -> impl Fn(&BenchState) -> bool {
        move |s| s.session.phase == Phase::Waiting && s.session.pass == pass
    }

    #[test]
    fn a_whole_session_saves_the_starting_point() {
        let (c, fake, _dir) = setup(fast());
        let state = c
            .start(
                "valorant",
                Context {
                    windows_build: 26200,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            (
                state.session.pass,
                state.start_mode,
                state.session.game_running
            ),
            (0, StartMode::Hotkey, true)
        );
        assert_eq!(
            (
                state.start_key,
                state.capture_seconds,
                state.switch_at_seconds
            ),
            ("F10", 90, 30)
        );
        assert!(fake.key_held.load(Ordering::SeqCst));
        for pass in 0..3u8 {
            wait_for(&c, "the next pass", waiting_at(pass));
            c.start_pressed();
        }
        wait_for(&c, "run 3", waiting_at(3));
        c.start_pressed();
        let done = wait_for(&c, "the saved result", |s| {
            s.session.phase == Phase::Done && s.session.result_id.is_some()
        });
        let id = done.session.result_id.unwrap();
        let saved = c.store().get(&id).unwrap();
        assert_eq!(saved.name, kestrel_bench::STARTING_POINT);
        assert_eq!(saved.runs.len(), 3);
        assert!(saved.runs.iter().all(|r| r.flags.is_empty() && !r.kept));
        assert_eq!(saved.context.windows_build, 26200);
        assert_eq!(
            saved.context.game_settings.unwrap().resolution,
            Some([1920, 1080])
        );
        assert_eq!(
            (
                saved.kestrel_version.as_str(),
                saved.presentmon_version.as_str()
            ),
            ("0.2.0", "2.6.0")
        );
        assert!(c.store().frame_times(&id, 3).unwrap().len() > 30_000);
        eventually("the start key to be given back", || {
            !fake.key_held.load(Ordering::SeqCst)
        });
        let beeps = fake.beeps.lock().unwrap();
        assert_eq!(beeps.iter().filter(|&&b| b == Cue::Start).count(), 4);
        assert_eq!(beeps.iter().filter(|&&b| b == Cue::End).count(), 4);
        assert!(fake.emitted.load(Ordering::SeqCst) > 8);
    }

    #[test]
    fn a_flagged_run_waits_and_can_be_kept_or_redone() {
        let (c, fake, _dir) = setup(fast());
        *fake.busy.lock().unwrap() = vec![("chrome.exe".into(), 30.0)];
        c.start("valorant", Context::default()).unwrap();
        c.start_pressed();
        wait_for(&c, "run 1", waiting_at(1));
        c.start_pressed();
        let review = wait_for(&c, "review", |s| {
            matches!(s.session.phase, Phase::Review { .. })
        });
        assert_eq!(
            review.session.phase,
            Phase::Review {
                flags: vec![kestrel_bench::Flag::BusyBackground {
                    name: "chrome.exe".into()
                }]
            }
        );
        c.redo();
        wait_for(&c, "run 1 again", waiting_at(1));
        c.start_pressed();
        wait_for(&c, "review again", |s| {
            matches!(s.session.phase, Phase::Review { .. })
        });
        c.keep();
        wait_for(&c, "run 2", waiting_at(2));
        fake.busy.lock().unwrap().clear();
        c.start_pressed();
        wait_for(&c, "run 3", waiting_at(3));
        c.start_pressed();
        let done = wait_for(&c, "done", |s| s.session.result_id.is_some());
        let id = done.session.result_id.unwrap();
        let saved = c.store().get(&id).unwrap();
        assert!(saved.runs[0].kept && !saved.runs[1].kept);
        assert!(
            !c.store().frame_times(&id, 1).unwrap().is_empty(),
            "the kept run's frames were saved"
        );
    }

    #[test]
    fn cancel_during_capture_stops_it() {
        let blocking = Arc::new(Blocking::default());
        let (c, fake, dir) = setup(blocking.clone());
        c.start("valorant", Context::default()).unwrap();
        c.start_pressed();
        wait_for(&c, "the capture", |s| s.session.phase == Phase::Capturing);
        eventually("the capture to start", || {
            blocking.started.load(Ordering::SeqCst)
        });
        assert!(
            dir.path().join("work").join(ACTIVE_MARKER).exists(),
            "a marker exists while capturing"
        );
        c.cancel();
        wait_for(&c, "cancelled", |s| s.session.phase == Phase::Cancelled);
        eventually("the marker to go", || {
            !dir.path().join("work").join(ACTIVE_MARKER).exists()
        });
        eventually("the key to be given back", || {
            !fake.key_held.load(Ordering::SeqCst)
        });
        assert!(c.store().list().is_empty());
        c.dismiss();
        assert!(c.state().is_none());
    }

    #[test]
    fn nothing_starts_until_the_game_runs() {
        let (c, fake, _dir) = setup(fast());
        fake.game.store(false, Ordering::SeqCst);
        assert!(
            !c.start("valorant", Context::default())
                .unwrap()
                .session
                .game_running
        );
        c.start_pressed();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(c.state().unwrap().session.phase, Phase::Waiting);
        assert!(fake.beeps.lock().unwrap().is_empty());
        fake.game.store(true, Ordering::SeqCst);
        wait_for(&c, "the game to be seen", |s| s.session.game_running);
        c.start_pressed();
        wait_for(&c, "run 1", waiting_at(1));
        c.cancel(); // the session is still armed: stop its game check
    }

    #[test]
    fn a_taken_key_falls_back_to_a_countdown() {
        let (c, fake, _dir) = setup(fast());
        fake.key_free.store(false, Ordering::SeqCst);
        assert_eq!(
            c.start("valorant", Context::default()).unwrap().start_mode,
            StartMode::Countdown
        );
        c.start_countdown().unwrap();
        wait_for(&c, "run 1 after the countdown", waiting_at(1));
        assert_eq!(
            fake.beeps
                .lock()
                .unwrap()
                .iter()
                .filter(|&&b| b == Cue::Tick)
                .count(),
            10
        );
        assert!(c.state().unwrap().countdown.is_none());
        c.cancel(); // the session is still armed: stop its game check
    }

    #[test]
    fn one_session_at_a_time() {
        let (c, _fake, _dir) = setup(fast());
        c.start("valorant", Context::default()).unwrap();
        assert_eq!(
            c.start("valorant", Context::default()).unwrap_err(),
            "A benchmark is already running."
        );
        assert_eq!(
            c.start("fivem", Context::default()).unwrap_err(),
            "A benchmark is already running."
        );
        c.cancel();
        assert!(c
            .start("fivem", Context::default())
            .unwrap_err()
            .contains("doesn't know"));
        assert!(
            c.start("valorant", Context::default()).is_ok(),
            "a cancelled session can be replaced"
        );
        c.cancel(); // the replacement is still armed: stop its game check
    }

    #[test]
    fn a_capture_without_frames_is_a_short_run() {
        let (c, _fake, _dir) = setup(fast());
        let empty = CaptureOutput {
            csv: String::new(),
            stderr: String::new(),
        };
        let (record, times) = c.analyse(&empty).unwrap();
        assert_eq!(
            record.flags,
            vec![kestrel_bench::Flag::TooShort],
            "the owner can redo it"
        );
        assert!(times.is_empty());
        let no_frame_times = CaptureOutput {
            csv: "Application,PresentMode\nx,y\n".into(),
            stderr: String::new(),
        };
        assert!(
            c.analyse(&no_frame_times).is_err(),
            "a CSV without frame times is an error"
        );
    }

    #[test]
    fn a_failed_capture_ends_the_session() {
        let (c, fake, dir) = setup(Arc::new(Failing));
        c.start("valorant", Context::default()).unwrap();
        c.start_pressed();
        let failed = wait_for(&c, "the failure", |s| {
            matches!(s.session.phase, Phase::Failed { .. })
        });
        assert_eq!(
            failed.session.phase,
            Phase::Failed {
                message: "PresentMon failed".into()
            }
        );
        eventually("the start key to be given back", || {
            !fake.key_held.load(Ordering::SeqCst)
        });
        assert!(
            !dir.path().join("work").join(ACTIVE_MARKER).exists(),
            "the marker goes when the capture ends"
        );
        std::thread::sleep(Duration::from_millis(50)); // a second end beep would have sounded by now
        assert_eq!(
            fake.beeps
                .lock()
                .unwrap()
                .iter()
                .filter(|&&b| b == Cue::End)
                .count(),
            1
        );
        assert!(c.store().list().is_empty(), "nothing is saved");
        // "Copy diagnostics" embeds this state as `benchSession` (lib.rs, get_diagnostics): the failure has to be in it.
        let diagnostics = serde_json::json!({ "benchSession": c.state() });
        assert_eq!(diagnostics["benchSession"]["phase"], "failed");
        assert_eq!(diagnostics["benchSession"]["message"], "PresentMon failed");
    }

    #[test]
    fn dismissing_while_the_result_saves_leaves_nothing_claimed() {
        let (c, fake, _dir) = setup(fast());
        // The warm-up and the three runs each read the game's settings once while analysing: the fifth read is the
        // save's, and it waits until the test lets it go.
        fake.hold_read.store(5, Ordering::SeqCst);
        c.start("valorant", Context::default()).unwrap();
        for pass in 0..4u8 {
            wait_for(&c, "the next pass", waiting_at(pass));
            c.start_pressed();
        }
        eventually("the save to start", || {
            fake.read_held.load(Ordering::SeqCst)
        });
        let saving = c.state().unwrap();
        assert_eq!(
            (saving.session.phase, saving.session.result_id),
            (Phase::Done, None)
        );
        assert!(
            !fake.key_held.load(Ordering::SeqCst),
            "the key is given back before the save, not after it"
        );
        // A dismiss can land before the capture thread has given the key back: it gives the key back itself.
        fake.key_held.store(true, Ordering::SeqCst);
        c.dismiss();
        assert!(c.state().is_none());
        assert!(
            !fake.key_held.load(Ordering::SeqCst),
            "nothing is armed, so F10 is free"
        );
        // The save goes on without a session: it brings neither the session nor the key back.
        let emitted = fake.emitted.load(Ordering::SeqCst);
        fake.release_read.store(true, Ordering::SeqCst);
        eventually("the save to land", || c.store().list().len() == 1);
        eventually("the capture thread to finish", || {
            fake.emitted.load(Ordering::SeqCst) > emitted
        });
        assert!(c.state().is_none());
        assert!(!fake.key_held.load(Ordering::SeqCst));
    }

    #[test]
    fn a_quick_dismiss_and_restart_never_loses_the_saved_result() {
        let (c, fake, _dir) = setup(fast());
        // The first release is start()'s. The second is the one made when the last run puts the session in Done: it
        // doesn't return until the test lets it, like a call that waits for the UI thread.
        fake.hold_unregister.store(2, Ordering::SeqCst);
        c.start("valorant", Context::default()).unwrap();
        for pass in 0..4u8 {
            wait_for(&c, "the next pass", waiting_at(pass));
            c.start_pressed();
        }
        eventually("the release to be under way", || {
            fake.unregister_held.load(Ordering::SeqCst)
        });
        let finished = c.state().unwrap();
        assert_eq!(
            (finished.session.phase, finished.session.result_id),
            (Phase::Done, None)
        );
        // Meanwhile the owner dismisses the finished session and arms another one.
        c.dismiss();
        c.start("valorant", Context::default()).unwrap();
        let emitted = fake.emitted.load(Ordering::SeqCst);
        fake.release_unregister.store(true, Ordering::SeqCst);
        eventually("the capture thread to finish", || {
            fake.emitted.load(Ordering::SeqCst) > emitted
        });
        assert_eq!(
            c.store().list().len(),
            1,
            "the dismissed session's result is saved"
        );
        let armed = c.state().unwrap();
        assert_eq!(
            (
                armed.session.phase,
                armed.session.pass,
                armed.session.result_id
            ),
            (Phase::Waiting, 0, None),
            "the new session is untouched by the old one's result"
        );
        assert!(
            fake.key_held.load(Ordering::SeqCst),
            "and keeps its start key"
        );
        c.cancel(); // the new session is still armed: stop its game check
    }
}
