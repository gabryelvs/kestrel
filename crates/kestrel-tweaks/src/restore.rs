//! The restore point: what asking Windows for one comes to, and the short-lived copy of Kestrel that asks.
//!
//! Microsoft's sample for `SRSetRestorePointW` sets COM security for the whole process, which can only be done once and
//! may clash with WebView2. So the app never asks from its own process: it starts itself again with `HELPER_FLAG`
//! (`run_helper`), and that copy asks (`helper_main`) and prints how it went as one line of JSON, whatever happened.
//!
//! The journal is the real undo. A restore point is a bonus that Windows may later remove, and Windows may say yes and
//! make none at all, so an outcome says only what can be shown (see `make_point`). What talks to Windows is in
//! `win::restore`; all of this is tested on any OS.
use std::io::{self, BufRead, BufReader, Write};
use std::panic::{catch_unwind, UnwindSafe};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::journal::RestoreOutcome;
use crate::system::Readiness;

/// What the restore point is called in Windows' list.
pub const DESCRIPTION: &str = "Kestrel: before applying tweaks";
/// The flag that makes a copy of Kestrel the helper.
pub const HELPER_FLAG: &str = "--restore-point";
/// How long the helper may take. Microsoft says the call takes a full snapshot of the registry and other system
/// databases; a clean VM took about 30 seconds, and hangs have been reported.
pub const HELPER_TIMEOUT: Duration = Duration::from_secs(180);
/// How long to leave Windows between two looks at something it takes a while to show.
pub const POLL_WAIT: Duration = Duration::from_secs(1);

/// How many looks to take at something Windows takes a while to show, `POLL_WAIT` apart.
const POLLS: u32 = 30;

const PROTECTION_OFF: &str = "System Protection is off.";
const NOT_LISTED: &str = "Windows didn't list the new restore point.";
#[cfg(any(windows, test))]
const TOOK_TOO_LONG: &str = "Windows took too long to create the restore point.";
const UNREADABLE_ANSWER: &str = "Kestrel's restore point helper gave an answer it can't read.";
const HELPER_PANICKED: &str = "Kestrel's restore point helper ran into a bug";

/// What the person chose when Apply came to the restore point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreChoice {
    Create,
    /// System Protection is off, and the person asked for it to be turned on first.
    TurnOnAndCreate,
    Skip,
}

/// A restore point in Windows' list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub sequence: u32,
    /// When it was made, as the journal spells times: see `parse::local_time`.
    pub time: String,
}

/// Why Windows wouldn't make a restore point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateError {
    ProtectionOff,
    /// Anything else, in a sentence for the person.
    Other(String),
}

/// What making a restore point needs from Windows.
pub trait RestorePoints {
    /// The point with the highest sequence number, or None when Windows lists none. The error is why it couldn't say,
    /// in a sentence for the person.
    fn newest(&self) -> Result<Option<Point>, String>;
    /// Asks Windows for a point, and returns the sequence number it answers with. If it skipped making one, that is an
    /// earlier point's number.
    fn create(&self, description: &str) -> Result<u32, CreateError>;
}

/// Asks for a restore point, and says what came of it.
///
/// Windows makes one point per 24 hours, and inside that window it says yes with an earlier point's number. Sequence
/// numbers only rise, so an answer at or below the newest listed number is an earlier point: `Reused`, with the listed
/// point's number and time, and nothing to wait for. Only a number above the one listed before the call is `Created`,
/// and only once Windows lists it, which can lag the call: `wait` runs before each of up to 30 looks, so each wait sits
/// between two reads of the list. If the first read fails nothing is asked for, because a point reused would then pass
/// for a new one.
pub fn make_point(
    points: &dyn RestorePoints,
    description: &str,
    wait: &mut dyn FnMut(),
) -> RestoreOutcome {
    let before = match points.newest() {
        Ok(before) => before,
        Err(reason) => return RestoreOutcome::Failed { reason },
    };
    let answer = match points.create(description) {
        Ok(answer) => answer,
        Err(CreateError::ProtectionOff) => return failed(PROTECTION_OFF),
        Err(CreateError::Other(reason)) => return RestoreOutcome::Failed { reason },
    };
    let floor = before.as_ref().map(|point| point.sequence);
    if let Some(point) = before.filter(|point| answer <= point.sequence) {
        return RestoreOutcome::Reused {
            sequence: point.sequence,
            time: point.time,
        };
    }
    let listed = poll(wait, || {
        // A look that fails is only one look. There are more, and it's Windows' list that says.
        let newest = points.newest().ok().flatten()?;
        floor
            .is_none_or(|floor| newest.sequence > floor)
            .then_some(newest.sequence)
    });
    match listed {
        Some(sequence) => RestoreOutcome::Created { sequence },
        None => failed(NOT_LISTED),
    }
}

/// Waits and looks, up to `POLLS` times, until a look finds something.
fn poll<T>(mut wait: impl FnMut(), mut look: impl FnMut() -> Option<T>) -> Option<T> {
    (0..POLLS).find_map(|_| {
        wait();
        look()
    })
}

fn failed(reason: &str) -> RestoreOutcome {
    RestoreOutcome::Failed {
        reason: reason.to_string(),
    }
}

/// The point with the highest number, whatever order Windows lists them in.
pub fn newest_of(points: impl IntoIterator<Item = Point>) -> Option<Point> {
    points.into_iter().max_by_key(|point| point.sequence)
}

/// Whether `SRSetRestorePointW` opened a new point, going by the list from just before it (`before`) and the number it
/// answered with (`sequence`). Sequence numbers only rise, so only a number above the newest listed one is new, and a
/// sequence that wasn't opened mustn't be ended. A list that couldn't be read is a no: leaving a new point not ended
/// does no harm, as Microsoft's sample says that call is only required of nested points.
fn opened_a_new_point(before: &Result<Option<Point>, String>, sequence: i64) -> bool {
    match before {
        Ok(Some(point)) => sequence > i64::from(point.sequence),
        Ok(None) => sequence > 0,
        Err(_) => false,
    }
}

/// What comes of the number `SRSetRestorePointW` answered `BEGIN_SYSTEM_CHANGE` with: the number, once it's known to be
/// one a point can have, which comes before anything else is sent. `end` sends `END_SYSTEM_CHANGE`, and it's called
/// only for a point opened here (see `opened_a_new_point`).
pub fn after_begin(
    before: &Result<Option<Point>, String>,
    sequence: i64,
    end: impl FnOnce(),
) -> Result<u32, CreateError> {
    let answer = u32::try_from(sequence).map_err(|_| {
        CreateError::Other(format!(
            "Windows answered with a restore point number no point can have: {sequence}."
        ))
    })?;
    if opened_a_new_point(before, sequence) {
        end();
    }
    Ok(answer)
}

/// What a failed `SRSetRestorePointW` means, from the status it leaves. Microsoft lists them on `STATEMGRSTATUS`.
pub fn create_error(status: u32) -> CreateError {
    const ERROR_SUCCESS: u32 = 0;
    const ERROR_BAD_ENVIRONMENT: u32 = 10;
    const ERROR_DISK_FULL: u32 = 112;
    const ERROR_SERVICE_DISABLED: u32 = 1058;
    const ERROR_TIMEOUT: u32 = 1460;
    let said = match status {
        ERROR_SERVICE_DISABLED => return CreateError::ProtectionOff,
        // As an error's text this would say the operation succeeded.
        ERROR_SUCCESS => "Windows refused the restore point and didn't say why.".to_string(),
        ERROR_BAD_ENVIRONMENT => "Windows doesn't make restore points in safe mode.".to_string(),
        ERROR_DISK_FULL => {
            "System Restore has paused because the disk is low on space.".to_string()
        }
        ERROR_TIMEOUT => {
            "Windows timed out waiting for its turn to make a restore point.".to_string()
        }
        other => format!(
            "Windows refused the restore point: {}",
            io::Error::from_raw_os_error(other as i32)
        ),
    };
    CreateError::Other(said)
}

/// Waits until System Protection reads `wanted`, or says what it reads instead. Windows' `Enable` returns before
/// monitoring is on, and the engine trusts `System::protection` at once: a read that still said off would close the
/// record as turned off outside Kestrel, and lose the undo.
pub fn protection_settles(
    wanted: Readiness,
    mut read: impl FnMut() -> Readiness,
    wait: impl FnMut(),
) -> io::Result<()> {
    let mut seen = wanted;
    let settled = poll(wait, || {
        seen = read();
        (seen == wanted).then_some(())
    });
    settled.ok_or_else(|| io::Error::other(not_settled(wanted, seen)))
}

fn not_settled(wanted: Readiness, seen: Readiness) -> String {
    let turned = if wanted == Readiness::Ready {
        "on"
    } else {
        "off"
    };
    let now = match seen {
        Readiness::Ready => "it's on",
        Readiness::ProtectionOff => "it's off",
        Readiness::BlockedByPolicy => "a policy blocks it",
        Readiness::Unknown => "it can't be read",
    };
    let seconds = u64::from(POLLS) * POLL_WAIT.as_secs();
    format!("Kestrel asked Windows to turn System Protection {turned}, and {seconds} seconds later {now}.")
}

/// The helper: makes the point and prints how it went as one line of `RestoreOutcome` JSON, whatever happened, and that
/// is its only output. Its exit code is 0, because the line is the answer. It needs a process of its own, where nothing
/// has touched COM yet. Off Windows there is nothing to make, and it says `Skipped`.
pub fn helper_main() -> i32 {
    say(&surviving(made_here), &mut io::stdout());
    0
}

#[cfg(windows)]
fn made_here() -> RestoreOutcome {
    crate::win::make_point_here()
}

#[cfg(not(windows))]
fn made_here() -> RestoreOutcome {
    RestoreOutcome::Skipped
}

/// A bug in the helper is one more thing that can go wrong with a restore point, and the app gets a line about it. The
/// helper's stderr goes nowhere, so the line carries the panic's message when it has one.
fn surviving(work: impl FnOnce() -> RestoreOutcome + UnwindSafe) -> RestoreOutcome {
    catch_unwind(work).unwrap_or_else(|panic| {
        let said = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str));
        match said {
            Some(said) => failed(&format!("{HELPER_PANICKED}: {said}")),
            None => failed(&format!("{HELPER_PANICKED}.")),
        }
    })
}

/// The helper's line. A stdout that is closed loses it, which `println!` would make a panic; the app then has no answer
/// to read, which is a failure too. An outcome always has a JSON form, and its text can't break the line.
fn say(outcome: &RestoreOutcome, out: &mut dyn Write) {
    let line = serde_json::to_string(outcome).unwrap_or_default();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// What the helper's line says. Anything else is a failure.
pub fn outcome_from_line(line: &str) -> RestoreOutcome {
    serde_json::from_str(line.trim()).unwrap_or_else(|_| failed(UNREADABLE_ANSWER))
}

/// Runs in the app: starts `exe` as the helper, waits up to `HELPER_TIMEOUT`, kills it past that, and reads its line.
/// It's already elevated, so this asks the person nothing. Off Windows it returns `Skipped` and starts nothing.
pub fn run_helper(exe: &Path) -> RestoreOutcome {
    #[cfg(windows)]
    {
        let mut command = Command::new(exe);
        command.arg(HELPER_FLAG);
        answer_from(command, HELPER_TIMEOUT)
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        RestoreOutcome::Skipped
    }
}

#[cfg(any(windows, test))]
fn answer_from(command: Command, timeout: Duration) -> RestoreOutcome {
    match first_line_of(command, timeout) {
        Ok(Some(line)) => outcome_from_line(&line),
        Ok(None) => failed(TOOK_TOO_LONG),
        Err(why) => RestoreOutcome::Failed {
            reason: format!("Kestrel couldn't start its restore point helper: {why}"),
        },
    }
}

/// Starts `command` with no input and no window, waits for the first line it prints, newline included, and ends it.
/// None means `timeout` passed first. A process that ends without a line gives what it printed, which may be nothing.
///
/// The line is read on a thread so that a process that never prints can't hold this past the timeout, and that thread
/// is left to end by itself, in case something else still holds the pipe. The process is waited for on a thread too:
/// one stuck in the kernel doesn't end when it's killed, and nothing here should wait for that. On Windows a
/// GUI-subsystem program gets this pipe as its standard output too, because the handles come through its STARTUPINFO.
pub fn first_line_of(mut command: Command, timeout: Duration) -> io::Result<Option<String>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let mut child = command.spawn()?;
    let output = child.stdout.take();
    let (answer, answered) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        if let Some(output) = output {
            // Bytes that aren't text leave `line` empty, which is no answer.
            let _ = BufReader::new(output).read_line(&mut line);
        }
        let _ = answer.send(line);
    });
    let line = answered.recv_timeout(timeout).ok();
    // The line is all it had to say, and if it's still going past the timeout this is what the timeout is for. Killing
    // one that has already ended changes nothing.
    let _ = child.kill();
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    const THEN: &str = "2026-11-02T09:00:00+00:00";

    fn point(sequence: u32) -> Option<Point> {
        Some(Point {
            sequence,
            time: THEN.to_string(),
        })
    }

    type Listing = Result<Option<Point>, String>;

    /// A Windows that answers as it is told. Each read of the list gives the next of `listings`, and the last goes on
    /// being the answer.
    struct Scripted {
        listings: RefCell<VecDeque<Listing>>,
        answer: Result<u32, CreateError>,
        reads: Cell<u32>,
        asked_for: RefCell<Vec<String>>,
    }

    impl Scripted {
        fn new(listings: Vec<Listing>, answer: Result<u32, CreateError>) -> Self {
            Scripted {
                listings: RefCell::new(listings.into()),
                answer,
                reads: Cell::new(0),
                asked_for: RefCell::new(Vec::new()),
            }
        }
    }

    impl RestorePoints for Scripted {
        fn newest(&self) -> Result<Option<Point>, String> {
            self.reads.set(self.reads.get() + 1);
            let mut listings = self.listings.borrow_mut();
            if listings.len() > 1 {
                listings.pop_front().unwrap()
            } else {
                listings[0].clone()
            }
        }

        fn create(&self, description: &str) -> Result<u32, CreateError> {
            self.asked_for.borrow_mut().push(description.to_string());
            self.answer.clone()
        }
    }

    /// What `make_point` comes to, and how many times it waited.
    fn made(points: &Scripted) -> (RestoreOutcome, u32) {
        let mut waits = 0;
        let outcome = make_point(points, DESCRIPTION, &mut || waits += 1);
        (outcome, waits)
    }

    #[test]
    fn a_higher_sequence_is_created() {
        // Windows lists it on the second look after the call.
        let points = Scripted::new(vec![Ok(point(56)), Ok(point(56)), Ok(point(57))], Ok(57));
        let (outcome, waits) = made(&points);
        assert_eq!(outcome, RestoreOutcome::Created { sequence: 57 });
        assert_eq!(points.reads.get(), 3);
        assert_eq!(waits, 2);
        assert_eq!(*points.asked_for.borrow(), [DESCRIPTION]);
    }

    #[test]
    fn each_wait_sits_between_two_reads_of_the_list() {
        // None comes after the last look, and the first gives Windows a moment after the call.
        let points = Scripted::new(vec![Ok(point(56)), Ok(point(56)), Ok(point(57))], Ok(57));
        let mut reads_at_each_wait = Vec::new();
        let outcome = make_point(&points, DESCRIPTION, &mut || {
            reads_at_each_wait.push(points.reads.get());
        });
        assert_eq!(outcome, RestoreOutcome::Created { sequence: 57 });
        assert_eq!(reads_at_each_wait, [1, 2]);
        assert_eq!(points.reads.get(), 3);
    }

    #[test]
    fn the_same_sequence_is_reused_with_its_time() {
        let points = Scripted::new(vec![Ok(point(56))], Ok(56));
        let (outcome, waits) = made(&points);
        assert_eq!(
            outcome,
            RestoreOutcome::Reused {
                sequence: 56,
                time: THEN.to_string()
            }
        );
        assert_eq!(points.reads.get(), 1, "no polling");
        assert_eq!(waits, 0);
    }

    #[test]
    fn an_answer_below_the_newest_is_reused_with_the_newest_ones_number_and_time() {
        // Sequence numbers only rise, so no new point is coming, and there is nothing to wait for.
        for answer in [56, 55, 1, 0] {
            let points = Scripted::new(vec![Ok(point(57))], Ok(answer));
            let (outcome, waits) = made(&points);
            assert_eq!(
                outcome,
                RestoreOutcome::Reused {
                    sequence: 57,
                    time: THEN.to_string()
                },
                "{answer}"
            );
            assert_eq!(points.reads.get(), 1, "no polling for {answer}");
            assert_eq!(waits, 0, "{answer}");
        }
    }

    #[test]
    fn success_without_a_listing_fails_after_thirty_polls() {
        let points = Scripted::new(vec![Ok(point(56))], Ok(57));
        let (outcome, waits) = made(&points);
        assert_eq!(
            outcome,
            failed("Windows didn't list the new restore point.")
        );
        assert_eq!(waits, 30);
        // The read before asking, and thirty looks after.
        assert_eq!(points.reads.get(), 31);
    }

    #[test]
    fn protection_off_is_reported() {
        let points = Scripted::new(vec![Ok(point(56))], Err(CreateError::ProtectionOff));
        let (outcome, waits) = made(&points);
        assert_eq!(outcome, failed("System Protection is off."));
        assert_eq!(waits, 0);
    }

    #[test]
    fn no_earlier_point_still_works() {
        let points = Scripted::new(vec![Ok(None), Ok(point(1))], Ok(1));
        let (outcome, waits) = made(&points);
        assert_eq!(outcome, RestoreOutcome::Created { sequence: 1 });
        assert_eq!(waits, 1);
    }

    #[test]
    fn with_no_earlier_point_an_empty_list_is_still_no_point() {
        let points = Scripted::new(vec![Ok(None)], Ok(1));
        let (outcome, waits) = made(&points);
        assert_eq!(
            outcome,
            failed("Windows didn't list the new restore point.")
        );
        assert_eq!(waits, 30);
    }

    #[test]
    fn any_other_refusal_says_windows_reason() {
        let points = Scripted::new(
            vec![Ok(point(56))],
            Err(CreateError::Other("The disk is full.".to_string())),
        );
        assert_eq!(made(&points).0, failed("The disk is full."));
    }

    #[test]
    fn a_list_that_cant_be_read_first_asks_for_nothing() {
        // Without the number from before, a point Windows reused would pass for a new one.
        let points = Scripted::new(vec![Err("No list.".to_string())], Ok(57));
        let (outcome, waits) = made(&points);
        assert_eq!(outcome, failed("No list."));
        assert_eq!(waits, 0);
        assert!(points.asked_for.borrow().is_empty());
    }

    #[test]
    fn a_look_that_fails_is_only_one_look() {
        let points = Scripted::new(
            vec![Ok(point(56)), Err("Not now.".to_string()), Ok(point(57))],
            Ok(57),
        );
        assert_eq!(made(&points).0, RestoreOutcome::Created { sequence: 57 });
    }

    #[test]
    fn a_lower_or_equal_number_in_the_list_is_not_a_new_point() {
        let points = Scripted::new(vec![Ok(point(56)), Ok(point(55)), Ok(point(56))], Ok(57));
        assert_eq!(
            made(&points).0,
            failed("Windows didn't list the new restore point.")
        );
    }

    #[test]
    fn the_newest_point_is_the_one_with_the_highest_number_whatever_the_order() {
        let listed = |sequences: &[u32]| {
            newest_of(sequences.iter().map(|&sequence| Point {
                sequence,
                time: sequence.to_string(),
            }))
            .map(|point| (point.sequence, point.time))
        };
        assert_eq!(listed(&[]), None);
        assert_eq!(listed(&[3]), Some((3, "3".to_string())));
        assert_eq!(listed(&[56, 58, 57]), Some((58, "58".to_string())));
    }

    #[test]
    fn only_a_number_above_the_newest_listed_was_opened_here() {
        let newest = |sequence| -> Listing { Ok(point(sequence)) };
        assert!(opened_a_new_point(&newest(56), 57));
        assert!(!opened_a_new_point(&newest(56), 56));
        // Sequence numbers only rise, so these are earlier points.
        assert!(!opened_a_new_point(&newest(56), 55));
        assert!(!opened_a_new_point(&newest(56), 1));
        // With nothing listed, only a number a point can have.
        assert!(opened_a_new_point(&Ok(None), 1));
        assert!(!opened_a_new_point(&Ok(None), 0));
        assert!(!opened_a_new_point(&Ok(None), -1));
        // Ending what wasn't opened here is what mustn't happen, and not ending a new one does no harm.
        assert!(!opened_a_new_point(&Err("No list.".to_string()), 57));
    }

    /// What `after_begin` comes to, and whether it sent `END_SYSTEM_CHANGE`.
    fn after_answer(before: &Listing, answer: i64) -> (Result<u32, CreateError>, bool) {
        let mut ended = false;
        let said = after_begin(before, answer, || ended = true);
        (said, ended)
    }

    #[test]
    fn a_new_number_is_ended_and_comes_back() {
        assert_eq!(after_answer(&Ok(point(56)), 57), (Ok(57), true));
        assert_eq!(after_answer(&Ok(None), 1), (Ok(1), true));
    }

    #[test]
    fn a_number_at_or_below_the_newest_comes_back_with_no_end() {
        for answer in [56, 55, 1, 0] {
            let said = u32::try_from(answer).unwrap();
            assert_eq!(
                after_answer(&Ok(point(56)), answer),
                (Ok(said), false),
                "{answer}"
            );
        }
        assert_eq!(after_answer(&Ok(None), 0), (Ok(0), false));
    }

    #[test]
    fn a_number_no_point_can_have_is_an_error_with_no_end() {
        // The number is checked before anything is sent, or the second of these would end a sequence nothing opened.
        let too_big = i64::from(u32::MAX) + 57;
        assert_eq!(too_big, 4_294_967_352);
        for answer in [-1, too_big, i64::MIN, i64::MAX] {
            for before in [Ok(point(56)), Ok(None)] {
                let (said, ended) = after_answer(&before, answer);
                assert_eq!(
                    said,
                    Err(CreateError::Other(format!(
                        "Windows answered with a restore point number no point can have: {answer}."
                    )))
                );
                assert!(!ended, "{answer}");
            }
        }
    }

    #[test]
    fn a_list_that_couldnt_be_read_is_no_end_and_the_number_still_comes_back() {
        let before = Err("No list.".to_string());
        assert_eq!(after_answer(&before, 57), (Ok(57), false));
    }

    #[test]
    fn windows_status_numbers_are_turned_into_reasons() {
        assert_eq!(create_error(1058), CreateError::ProtectionOff);
        for (status, words) in [
            (10, "safe mode"),
            (112, "low on space"),
            (1460, "timed out"),
        ] {
            let CreateError::Other(said) = create_error(status) else {
                panic!("{status} is not System Protection being off");
            };
            assert!(said.contains(words), "{status}: {said}");
            assert!(said.ends_with('.'), "{status}: {said}");
        }
        for status in [13, 1359, 5] {
            let CreateError::Other(said) = create_error(status) else {
                panic!("{status} is not System Protection being off");
            };
            assert!(
                said.starts_with("Windows refused the restore point: "),
                "{said}"
            );
            assert!(said.ends_with(&format!("(os error {status})")), "{said}");
        }
    }

    #[test]
    fn a_refusal_with_no_status_doesnt_read_as_success() {
        // Zero is "the operation completed successfully" to Windows, which is the last thing to tell someone here.
        assert_eq!(
            create_error(0),
            CreateError::Other("Windows refused the restore point and didn't say why.".to_string())
        );
    }

    // Waiting for System Protection.

    /// What waiting for `wanted` comes to, where the readings are `reads` and then the last of them for good, with how
    /// many reads and waits it took.
    fn settling(wanted: Readiness, reads: &[Readiness]) -> (io::Result<()>, usize, u32) {
        let mut done = 0;
        let mut waits = 0;
        let settled = protection_settles(
            wanted,
            || {
                let read = reads[done.min(reads.len() - 1)];
                done += 1;
                read
            },
            || waits += 1,
        );
        (settled, done, waits)
    }

    #[test]
    fn protection_that_reads_as_wanted_ends_the_wait() {
        use Readiness::{ProtectionOff, Ready};
        let (settled, reads, waits) = settling(Ready, &[Ready]);
        assert!(settled.is_ok());
        assert_eq!((reads, waits), (1, 1));

        let (settled, reads, waits) =
            settling(ProtectionOff, &[Ready, Ready, Ready, ProtectionOff]);
        assert!(settled.is_ok());
        assert_eq!((reads, waits), (4, 4));
    }

    #[test]
    fn protection_that_reads_as_wanted_on_the_last_look_counts() {
        use Readiness::{ProtectionOff, Ready};
        let mut reads = vec![ProtectionOff; 29];
        reads.push(Ready);
        let (settled, done, waits) = settling(Ready, &reads);
        assert!(settled.is_ok());
        assert_eq!((done, waits), (30, 30));
    }

    #[test]
    fn protection_that_never_reads_as_wanted_says_so_after_thirty_seconds() {
        use Readiness::{BlockedByPolicy, ProtectionOff, Ready, Unknown};
        for (wanted, stays, said) in [
            (
                Ready,
                ProtectionOff,
                "Kestrel asked Windows to turn System Protection on, and 30 seconds later it's off.",
            ),
            (
                ProtectionOff,
                Ready,
                "Kestrel asked Windows to turn System Protection off, and 30 seconds later it's on.",
            ),
            (
                Ready,
                Unknown,
                "Kestrel asked Windows to turn System Protection on, and 30 seconds later it can't be read.",
            ),
            (
                ProtectionOff,
                Unknown,
                "Kestrel asked Windows to turn System Protection off, and 30 seconds later it can't be read.",
            ),
            (
                Ready,
                BlockedByPolicy,
                "Kestrel asked Windows to turn System Protection on, and 30 seconds later a policy blocks it.",
            ),
        ] {
            let (settled, reads, waits) = settling(wanted, &[stays]);
            assert_eq!(settled.unwrap_err().to_string(), said);
            assert_eq!((reads, waits), (30, 30));
        }
    }

    // The helper's line.

    #[test]
    fn run_helper_fails_cleanly_on_garbage() {
        for garbage in [
            "not json",
            "",
            "   ",
            "{",
            "[]",
            "null",
            r#"{"result":"created"}"#,
            r#"{"result":"created","sequence":-1}"#,
            r#"{"result":"something-new"}"#,
            r#"{"result":"skipped"} and more"#,
        ] {
            assert_eq!(
                outcome_from_line(garbage),
                failed("Kestrel's restore point helper gave an answer it can't read."),
                "{garbage}"
            );
        }
    }

    #[test]
    fn every_outcome_comes_through_the_helpers_line_as_it_was() {
        for outcome in [
            RestoreOutcome::Created { sequence: 57 },
            RestoreOutcome::Reused {
                sequence: 56,
                time: THEN.to_string(),
            },
            RestoreOutcome::Skipped,
            // A reason can hold anything Windows says.
            failed("Windows said \"no\"\nin two lines."),
        ] {
            let mut said = Vec::new();
            say(&outcome, &mut said);
            let line = String::from_utf8(said).unwrap();
            assert_eq!(line.matches('\n').count(), 1, "{line}");
            assert!(line.ends_with('\n'), "{line}");
            assert_eq!(outcome_from_line(&line), outcome);
        }
    }

    #[test]
    fn the_helpers_line_is_spelled_as_the_journal_spells_an_outcome() {
        let mut said = Vec::new();
        say(&RestoreOutcome::Created { sequence: 57 }, &mut said);
        assert_eq!(said, b"{\"result\":\"created\",\"sequence\":57}\n");
    }

    #[test]
    fn a_stdout_that_is_closed_is_not_a_panic() {
        struct Closed;
        impl io::Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
        }
        say(&RestoreOutcome::Skipped, &mut Closed);
    }

    #[test]
    fn a_panic_in_the_helper_is_an_outcome_and_not_a_crash() {
        assert_eq!(
            surviving(|| RestoreOutcome::Skipped),
            RestoreOutcome::Skipped
        );
        // Stderr goes nowhere, so the reason is the only place its message can reach anyone.
        assert_eq!(
            surviving(|| panic!("WMI said something no one expected")),
            failed(
                "Kestrel's restore point helper ran into a bug: WMI said something no one expected"
            )
        );
        // A message with values in it comes as another type.
        let number = 7;
        assert_eq!(
            surviving(move || panic!("WMI said {number}")),
            failed("Kestrel's restore point helper ran into a bug: WMI said 7")
        );
        // And a panic needn't hold text at all.
        assert_eq!(
            surviving(|| std::panic::panic_any(7)),
            failed("Kestrel's restore point helper ran into a bug.")
        );
    }

    #[test]
    fn the_choices_are_spelled_in_kebab_case_the_way_the_page_sends_them() {
        for (choice, text) in [
            (RestoreChoice::Create, "create"),
            (RestoreChoice::TurnOnAndCreate, "turn-on-and-create"),
            (RestoreChoice::Skip, "skip"),
        ] {
            let json = serde_json::to_string(&choice).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<RestoreChoice>(&json).unwrap(),
                choice
            );
        }
    }

    #[test]
    fn the_values_the_app_and_windows_go_by() {
        assert_eq!(DESCRIPTION, "Kestrel: before applying tweaks");
        assert_eq!(HELPER_FLAG, "--restore-point");
        assert_eq!(HELPER_TIMEOUT, Duration::from_secs(180));
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_helper_is_skipped_and_nothing_is_started() {
        // Trying to start this would come back as a failure.
        let outcome = run_helper(Path::new("/no/such/kestrel"));
        assert_eq!(outcome, RestoreOutcome::Skipped);
    }

    // Running a helper. These need a shell, and Windows has its own tests in `win::restore`.

    #[cfg(unix)]
    mod processes {
        use super::*;
        use std::time::Instant;

        fn shell(script: &str) -> Command {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            command
        }

        const PLENTY: Duration = Duration::from_secs(60);

        #[test]
        fn a_helpers_answer_is_the_first_line_it_prints() {
            let line = first_line_of(shell("echo one; echo two"), PLENTY).unwrap();
            assert_eq!(line.as_deref(), Some("one\n"));
        }

        #[test]
        fn a_helpers_line_is_its_outcome() {
            let line = r#"echo '{"result":"created","sequence":57}'"#;
            assert_eq!(
                answer_from(shell(line), PLENTY),
                RestoreOutcome::Created { sequence: 57 }
            );
        }

        #[test]
        fn a_helper_that_says_nothing_or_garbage_has_failed() {
            let unreadable = failed("Kestrel's restore point helper gave an answer it can't read.");
            for script in ["exit 0", "exit 3", "echo garbage", "printf half-a-line"] {
                assert_eq!(answer_from(shell(script), PLENTY), unreadable, "{script}");
            }
        }

        #[test]
        fn a_helper_that_cant_start_has_failed_and_says_why() {
            let outcome = answer_from(Command::new("/no/such/kestrel"), PLENTY);
            let RestoreOutcome::Failed { reason } = outcome else {
                panic!("nothing could have run");
            };
            assert!(
                reason.starts_with("Kestrel couldn't start its restore point helper: "),
                "{reason}"
            );
        }

        #[test]
        fn a_helper_reads_no_input() {
            // With anything but an empty input, `cat` would never end.
            let line = first_line_of(shell("cat; echo finished"), Duration::from_secs(30));
            assert_eq!(line.unwrap().as_deref(), Some("finished\n"));
        }

        #[test]
        fn more_than_a_pipe_holds_still_comes_through() {
            // A pipe holds far less than this, so whoever isn't reading would leave the helper stuck.
            let script = "head -c 500000 /dev/zero | tr '\\0' x; echo";
            let line = first_line_of(shell(script), PLENTY).unwrap().unwrap();
            assert_eq!(line.len(), 500_001);
        }

        #[test]
        fn a_helper_past_its_time_is_killed_and_it_took_too_long() {
            let dir = tempfile::tempdir().unwrap();
            let pid_file = dir.path().join("pid");
            let script = format!("echo $$ > '{}'; exec sleep 60", pid_file.display());
            let started = Instant::now();

            let outcome = answer_from(shell(&script), Duration::from_secs(2));

            assert_eq!(
                outcome,
                failed("Windows took too long to create the restore point.")
            );
            assert!(started.elapsed() < Duration::from_secs(30));
            let pid = std::fs::read_to_string(pid_file).unwrap();
            let is_gone = || {
                !shell(&format!("kill -0 {} 2>/dev/null", pid.trim()))
                    .status()
                    .unwrap()
                    .success()
            };
            let gone = (0..100).any(|_| {
                std::thread::sleep(Duration::from_millis(100));
                is_gone()
            });
            assert!(gone, "process {} is still there", pid.trim());
        }

        #[test]
        fn a_helper_that_answers_and_doesnt_end_is_not_waited_for() {
            let script = r#"echo '{"result":"skipped"}'; exec sleep 60"#;
            let started = Instant::now();
            assert_eq!(answer_from(shell(script), PLENTY), RestoreOutcome::Skipped);
            assert!(started.elapsed() < Duration::from_secs(30));
        }
    }
}
