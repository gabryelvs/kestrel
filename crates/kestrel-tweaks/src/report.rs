//! The Kestrel report: the journal's story as plain text, for the Tweaks tab's Copy report and for Copy diagnostics.
//!
//! It lists what is active, then everything that happened, newest first: each applied, undone and re-applied change
//! with its time, the value before and after and how it ended, and each restore point's result. It is made from a
//! [`Replay`] alone. Nothing here reads Windows, asks for the journal's lock or writes a line, so a report can be made at
//! any moment, even while another process is changing something.
//!
//! The journal is read the way the engine writes it, and not only line by line:
//!
//! - A change is an `Intent` and then its `Outcome`. The report tells it once, when the outcome comes: "Applied X: a →
//!   b", from the intent's prior and planned state. The line goes where the intent was and carries the intent's time, so
//!   the list stays in the order things began, and a record's time here is the one under Active.
//! - An intent for a record that is already open is another try at it (Apply again, Use it again). It has no values of
//!   its own to tell, so its outcome is "Applied X again", or "Couldn't apply X: why", and never a second "Applied X: a
//!   → b" with the value something else had left in the setting.
//! - An outcome with a `detail` is a change that failed, even when it says `applied`: that is how the engine records one
//!   it couldn't put back, which stays open because it may still be on the PC. It is told as a failure.
//! - An outcome written after a crash is told as an interruption, and never as a success the person chose.
//! - An intent that never got its outcome, and a record closed as never applied, have no line.
//!
//! Active is the open records of the `Replay` it is given, newest first, each with the state its first intent found, the
//! one it planned and that intent's time. It is not judged here: `text` knows nothing of Windows or of accounts, so what
//! counts as active is for whoever builds the `Replay`. `Engine::report` hands over the records that would be an Applied
//! row, which it can tell by reading the PC and without writing to it.
//!
//! Everything the journal holds is untrusted text. Any of it that reaches the report has its control characters
//! replaced with spaces, so no detail, name or reason can start a line of its own, and times are cut by characters.
use kestrel_core::tweak_id::TweakId;

use crate::catalog::info;
use crate::journal::{CloseReason, Entry, OpenRecord, Replay, RestoreOutcome, Saved};
use crate::targets;

const NO_CHANGES: &str = "No changes yet.";
const NOTHING_ACTIVE: &str = "Nothing is active.";
const NO_FINISHED_CHANGES: &str = "No finished changes yet.";
const UNREADABLE_CAUSE: &str =
    "An interruption while Kestrel was writing is the usual cause of an unreadable line.";
const UNREADABLE: &str = "Part of Kestrel's history couldn't be read.";

/// The report for a journal that was read. `now` is when it is made, spelled as the journal spells times.
/// `unreadable_note` adds the warning that part of the history couldn't be read, which is so when `replay.unreadable`
/// is above 0. The warning is the last line.
///
/// Active lists `replay.open` just as it is given, newest first, and does not look at `settled`. That is the caller's
/// to decide: a record whose outcome hasn't been written yet (a crash cut it off after the write) is what the next look at
/// the tab calls Applied if its setting holds what Kestrel wrote, and `Engine::report` passes it for that reason. The
/// History tells nothing of an intent that has no outcome, so such a record can be active before it has a line.
///
/// With neither anything active nor anything to tell, the report is the header and "No changes yet.". Otherwise both
/// headings are there, as in the spec's example, and an empty section says why under its heading: "Nothing is active."
/// when everything was undone, flagged or left alone, and "No finished changes yet." when only unfinished ones are
/// active.
pub fn text(
    replay: &Replay,
    kestrel_version: &str,
    build: u32,
    now: &str,
    unreadable_note: bool,
) -> String {
    let mut lines = vec![header(kestrel_version, build, now)];
    let active = active_lines(&replay.open);
    let history = history_lines(&replay.history);
    if active.is_empty() && history.is_empty() {
        lines.push(NO_CHANGES.to_string());
    } else {
        lines.push("Active".to_string());
        lines.extend(or_say(active, NOTHING_ACTIVE));
        lines.push("History".to_string());
        lines.extend(or_say(history, NO_FINISHED_CHANGES));
    }
    if unreadable_note {
        lines.push(UNREADABLE_CAUSE.to_string());
        lines.push(UNREADABLE.to_string());
    }
    lines.join("\n")
}

/// The report for a journal that couldn't be read at all: the header and a line saying so, with `reason`. It is not
/// "No changes yet.", which would say that nothing was ever done.
pub fn unavailable(kestrel_version: &str, build: u32, now: &str, reason: &str) -> String {
    let reason = clean(reason);
    let said = if reason.trim().is_empty() {
        "Kestrel's history couldn't be read.".to_string()
    } else {
        format!("Kestrel's history couldn't be read: {reason}")
    };
    format!("{}\n{said}", header(kestrel_version, build, now))
}

fn header(kestrel_version: &str, build: u32, now: &str) -> String {
    format!(
        "Kestrel report · Kestrel {kestrel_version} · Windows build {build} · {}",
        when(now)
    )
}

/// A section's lines, or the words for a section with none.
fn or_say(lines: Vec<String>, when_empty: &str) -> Vec<String> {
    if lines.is_empty() {
        vec![format!("  {when_empty}")]
    } else {
        lines
    }
}

/// The records it is given, newest first (`open` is oldest first), each with the state its first intent found and the one
/// it planned, and the time of that intent.
fn active_lines(open: &[OpenRecord]) -> Vec<String> {
    open.iter()
        .rev()
        .map(|record| {
            format!(
                "  {}: {} → {} ({})",
                info(record.tweak).title,
                words(record.tweak, &record.prior),
                words(record.tweak, &record.planned),
                when(&record.at)
            )
        })
        .collect()
}

/// The History's lines, newest first. Entries of the same minute keep the journal's order, reversed: the file's order
/// is the order things were written in, and it is the only one that doesn't depend on the clock or on a time being
/// well formed.
fn history_lines(entries: &[Entry]) -> Vec<String> {
    let mut walk = Walk::default();
    for entry in entries {
        walk.take(entry);
    }
    walk.lines.into_iter().flatten().rev().collect()
}

/// A tweak and an account: the journal's idea of one record.
type Key = (TweakId, Option<String>);

/// A record while the journal is read in order. Which records are open is what tells a change from another try at it,
/// so this follows `journal::replay`: an intent opens a record unless one is open; an outcome that didn't take drops the
/// record its intent opened and leaves one that was open before; `Undone` and `Closed` end a record.
struct Record {
    key: Key,
    /// The state the record's first intent found: what an undo goes back to.
    first_prior: Saved,
    /// The intent that no outcome has answered yet.
    waiting: Option<Attempt>,
}

/// An intent waiting for its outcome. It holds its place in the History, and its line is made when it is answered.
struct Attempt {
    place: usize,
    /// When it began, as the report shows times.
    time: String,
    /// The state it found and the one it planned, if it opened the record. A later try has none to tell.
    opened: Option<(Saved, Saved)>,
}

/// What an outcome answers.
enum Tried<'a> {
    /// No intent was waiting for it.
    Unknown,
    /// The intent that opened the record.
    First(&'a Saved, &'a Saved),
    /// Another try at a record that was already open.
    Again,
}

#[derive(Default)]
struct Walk {
    /// The records open at this point of the journal, in the order they opened.
    open: Vec<Record>,
    /// One place for each entry that has something to tell, in the journal's order. An intent keeps a place that its
    /// outcome fills; one that is never answered leaves it empty.
    lines: Vec<Option<String>>,
}

impl Walk {
    fn take(&mut self, entry: &Entry) {
        match entry {
            Entry::Intent {
                at,
                tweak,
                sid,
                prior,
                planned,
                ..
            } => {
                if let Some(key) = key_of(tweak, sid.as_deref()) {
                    self.begin(key, at, prior, planned);
                }
            }
            Entry::Outcome {
                at,
                tweak,
                sid,
                applied,
                detail,
                interrupted,
            } => {
                if let Some(key) = key_of(tweak, sid.as_deref()) {
                    self.answer(&key, at, *applied, detail.as_deref(), *interrupted);
                }
            }
            Entry::Reapplied { at, tweak, sid } => {
                if let Some(key) = key_of(tweak, sid.as_deref()) {
                    self.reapplied(&key, at);
                }
            }
            Entry::Undone {
                at,
                tweak,
                sid,
                detail,
            } => {
                if let Some(key) = key_of(tweak, sid.as_deref()) {
                    self.undone(&key, at, detail.as_deref());
                }
            }
            Entry::Closed {
                at,
                tweak,
                sid,
                reason,
            } => {
                if let Some(key) = key_of(tweak, sid.as_deref()) {
                    self.closed(&key, at, *reason);
                }
            }
            Entry::RestorePoint { at, outcome } => self.say(None, at, &restore_point_line(outcome)),
        }
    }

    fn find_mut(&mut self, key: &Key) -> Option<&mut Record> {
        self.open.iter_mut().find(|record| record.key == *key)
    }

    /// The record leaves the open ones. This is the state its first intent found, if it was open.
    fn close(&mut self, key: &Key) -> Option<Saved> {
        let index = self.open.iter().position(|record| record.key == *key)?;
        Some(self.open.remove(index).first_prior)
    }

    /// An intent. It tells nothing yet, but keeps its place for its outcome.
    fn begin(&mut self, key: Key, at: &str, prior: &Saved, planned: &Saved) {
        let place = self.lines.len();
        self.lines.push(None);
        let time = when(at);
        match self.find_mut(&key) {
            // The record is open, so this is another try at it, and the record keeps its first prior.
            Some(record) => {
                record.waiting = Some(Attempt {
                    place,
                    time,
                    opened: None,
                });
            }
            None => self.open.push(Record {
                key,
                first_prior: prior.clone(),
                waiting: Some(Attempt {
                    place,
                    time,
                    opened: Some((prior.clone(), planned.clone())),
                }),
            }),
        }
    }

    /// An outcome, which answers the intent waiting at its record.
    fn answer(
        &mut self,
        key: &Key,
        at: &str,
        applied: bool,
        detail: Option<&str>,
        interrupted: bool,
    ) {
        let attempt = self.find_mut(key).and_then(|record| record.waiting.take());
        let tried = match &attempt {
            None => Tried::Unknown,
            Some(Attempt {
                opened: Some((from, to)),
                ..
            }) => Tried::First(from, to),
            Some(_) => Tried::Again,
        };
        let said = outcome_line(key.0, applied, detail, interrupted, &tried);
        let opened_the_record = matches!(tried, Tried::First(..));
        // An attempt that didn't take is taken back, and that leaves no record if the attempt opened it.
        if !applied && opened_the_record {
            self.open.retain(|record| record.key != *key);
        }
        self.say(attempt, at, &said);
    }

    /// The person applied a tweak again. Its intent, if there is one, only had the value something else had left.
    fn reapplied(&mut self, key: &Key, at: &str) {
        let attempt = self.find_mut(key).and_then(|record| record.waiting.take());
        let said = format!("Applied {} again", info(key.0).title);
        self.say(attempt, at, &said);
    }

    fn undone(&mut self, key: &Key, at: &str, detail: Option<&str>) {
        let first_prior = self.close(key);
        let id = key.0;
        let title = info(id).title;
        let detail = detail.map(clean).filter(|detail| !detail.trim().is_empty());
        let said = match (detail, id, first_prior) {
            // The plan's undo says which plan it went back to, or that it fell back to Balanced.
            (Some(detail), _, _) => format!("Undid {title}: {detail}"),
            // Remove it deletes Kestrel's plan and leaves the plan in use, which may not be the one Kestrel found.
            (None, TweakId::PowerPlan, _) => {
                format!("Undid {title}: removed Kestrel's plan and left the plan in use alone")
            }
            (None, _, Some(prior)) => format!("Undid {title}: back to {}", words(id, &prior)),
            (None, _, None) => format!("Undid {title}"),
        };
        self.say(None, at, &said);
    }

    fn closed(&mut self, key: &Key, at: &str, reason: CloseReason) {
        self.close(key);
        let title = info(key.0).title;
        let said = match reason {
            CloseReason::WentBack => format!("{title} went back outside Kestrel"),
            CloseReason::PlanRemovedElsewhere => format!("{title} was removed outside Kestrel"),
            // A crash came before the change, so it never happened and there is nothing to tell.
            CloseReason::NotApplied => return,
        };
        self.say(None, at, &said);
    }

    /// Puts a line in the place an attempt kept for it, with the time the attempt began. With no attempt it goes last,
    /// with the time of the entry that tells it.
    fn say(&mut self, attempt: Option<Attempt>, at: &str, words: &str) {
        let (place, time) = match attempt {
            Some(attempt) => (Some(attempt.place), attempt.time),
            None => (None, when(at)),
        };
        let line = format!("  {time} {words}");
        match place.and_then(|place| self.lines.get_mut(place)) {
            Some(slot) => *slot = Some(line),
            None => self.lines.push(Some(line)),
        }
    }
}

/// The record an entry is about. The journal refuses a line that names a tweak it doesn't know, but a `Replay` is
/// public, so an entry like that is passed over here rather than trusted.
fn key_of(tweak: &str, sid: Option<&str>) -> Option<Key> {
    TweakId::parse(tweak).map(|id| (id, sid.map(str::to_string)))
}

/// What an outcome says, given the attempt it answers.
fn outcome_line(
    id: TweakId,
    applied: bool,
    detail: Option<&str>,
    interrupted: bool,
    tried: &Tried,
) -> String {
    let title = info(id).title;
    // A change that didn't take, or one that didn't and couldn't be put back, which the engine writes as applied with
    // the reason: either way it is no success.
    if !applied || detail.is_some() {
        return match detail.map(clean).filter(|detail| !detail.trim().is_empty()) {
            Some(detail) => format!("Couldn't apply {title}: {detail}"),
            None => format!("Couldn't apply {title}"),
        };
    }
    match (interrupted, tried) {
        (true, Tried::First(from, to)) => format!(
            "Kestrel was interrupted while applying {title}: {} → {}",
            words(id, from),
            words(id, to)
        ),
        (true, Tried::Again) => format!("Kestrel was interrupted while applying {title} again"),
        (true, Tried::Unknown) => format!("Kestrel was interrupted while applying {title}"),
        (false, Tried::First(from, to)) => {
            format!("Applied {title}: {} → {}", words(id, from), words(id, to))
        }
        (false, Tried::Again) => format!("Applied {title} again"),
        (false, Tried::Unknown) => format!("Applied {title}"),
    }
}

fn restore_point_line(outcome: &RestoreOutcome) -> String {
    match outcome {
        RestoreOutcome::Created { sequence } => format!("Restore point created (#{sequence})"),
        RestoreOutcome::Reused { sequence, time } => {
            let time = when(time);
            if time.trim().is_empty() {
                format!("Restore point reused (#{sequence})")
            } else {
                format!("Restore point reused from {time} (#{sequence})")
            }
        }
        RestoreOutcome::Skipped => "Restore point skipped".to_string(),
        RestoreOutcome::Failed { reason } => {
            let reason = clean(reason);
            if reason.trim().is_empty() {
                "Restore point failed".to_string()
            } else {
                format!("Restore point failed: {reason}")
            }
        }
    }
}

/// A saved state in words, safe to print.
fn words(tweak: TweakId, saved: &Saved) -> String {
    clean(&targets::describe(tweak, saved))
}

/// A journal time as the report shows it: `2026-11-02T18:12:05+01:00` is `2026-11-02 18:12`. It is the first 16
/// characters, counted as characters because the journal is a file anyone may have edited and a cut through the middle of
/// a character would panic, with the `T` between date and time as a space. Something that isn't a time is printed as it
/// is, up to that length.
fn when(at: &str) -> String {
    let first: String = at
        .chars()
        .take(16)
        .enumerate()
        .map(|(index, c)| if index == 10 && c == 'T' { ' ' } else { c })
        .collect();
    clean(&first)
}

/// Text from the journal as one line of the report: each control character, and each separator that a viewer would
/// break a line at, is a space.
fn clean(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::replay;
    use TweakId::{
        BackgroundRecording, GameMode, PowerPlan, SystemProtection, WindowedOptimizations,
    };

    const NOW: &str = "2026-11-02T18:20:00+00:00";
    const SID: &str = "S-1-5-21-1-2-3-1001";
    const OTHER_SID: &str = "S-1-5-21-1-2-3-1002";
    const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
    const EXM: &str = "0e1d2c3b-4a59-4687-b6a5-948372615049";
    const EXM_NAME: &str = "EXM Free Power Plan V8";
    const KESTREL: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";
    const UNREADABLE_TEXT: &str = "Part of Kestrel's history couldn't be read.";
    /// A string for the journal to hold that tries to start lines of the report: line breaks of every kind, a forged
    /// History heading and entry, an escape sequence, a NUL.
    const FORGERY: &str = "went wrong\n  2026-11-02 18:12 Applied Game Mode: off → on\r\nHistory\u{85}\u{2028}\u{2029}\u{b}\u{c}\0\u{1b}[2J\u{7f}end";

    // Entries, as the engine writes them.

    /// A time of day on the day of the spec's example, as the journal spells it.
    fn time(hours_and_minutes: &str) -> String {
        format!("2026-11-02T{hours_and_minutes}:00+00:00")
    }

    /// The account a tweak's record belongs to: none for the two machine-wide tweaks.
    fn owner(id: TweakId) -> Option<String> {
        match id {
            PowerPlan | SystemProtection => None,
            _ => Some(SID.to_string()),
        }
    }

    fn dword(value: u32) -> Saved {
        Saved::Dword { value: Some(value) }
    }

    fn plan(guid: &str, name: &str) -> Saved {
        Saved::Plan {
            guid: guid.to_string(),
            name: name.to_string(),
        }
    }

    fn protection(on: bool) -> Saved {
        Saved::Protection { on }
    }

    fn token(token: Option<&str>) -> Saved {
        Saved::Token {
            token: token.map(str::to_string),
            value_existed: false,
            key_existed: false,
        }
    }

    fn intent(id: TweakId, at: &str, prior: Saved, planned: Saved) -> Entry {
        Entry::Intent {
            at: at.to_string(),
            batch: at.to_string(),
            tweak: id.as_str().to_string(),
            sid: owner(id),
            value_name: None,
            prior,
            planned,
        }
    }

    fn outcome_with(
        id: TweakId,
        at: &str,
        applied: bool,
        detail: Option<&str>,
        interrupted: bool,
    ) -> Entry {
        Entry::Outcome {
            at: at.to_string(),
            tweak: id.as_str().to_string(),
            sid: owner(id),
            applied,
            detail: detail.map(str::to_string),
            interrupted,
        }
    }

    fn outcome(id: TweakId, at: &str) -> Entry {
        outcome_with(id, at, true, None, false)
    }

    fn reapplied(id: TweakId, at: &str) -> Entry {
        Entry::Reapplied {
            at: at.to_string(),
            tweak: id.as_str().to_string(),
            sid: owner(id),
        }
    }

    fn undone(id: TweakId, at: &str, detail: Option<&str>) -> Entry {
        Entry::Undone {
            at: at.to_string(),
            tweak: id.as_str().to_string(),
            sid: owner(id),
            detail: detail.map(str::to_string),
        }
    }

    fn closed(id: TweakId, at: &str, reason: CloseReason) -> Entry {
        Entry::Closed {
            at: at.to_string(),
            tweak: id.as_str().to_string(),
            sid: owner(id),
            reason,
        }
    }

    fn restore_point(at: &str, outcome: RestoreOutcome) -> Entry {
        Entry::RestorePoint {
            at: at.to_string(),
            outcome,
        }
    }

    /// The same entry, as written by another account.
    fn of_account(mut entry: Entry, account: &str) -> Entry {
        if let Entry::Intent { sid, .. }
        | Entry::Outcome { sid, .. }
        | Entry::Reapplied { sid, .. }
        | Entry::Undone { sid, .. }
        | Entry::Closed { sid, .. } = &mut entry
        {
            *sid = Some(account.to_string());
        }
        entry
    }

    /// Game Mode turned on, which Apply writes as an intent and an outcome.
    fn game_mode_on(at: &str) -> Vec<Entry> {
        vec![
            intent(GameMode, at, dword(0), dword(1)),
            outcome(GameMode, at),
        ]
    }

    /// Kestrel's plan made and switched to, over `prior`.
    fn kestrel_plan(at: &str, prior: Saved) -> Vec<Entry> {
        vec![
            intent(PowerPlan, at, prior, plan(KESTREL, "Kestrel")),
            outcome(PowerPlan, at),
        ]
    }

    // Reading them back.

    /// The entries as the journal's file holds them, a line each.
    fn file(entries: &[Entry]) -> String {
        entries
            .iter()
            .map(|entry| {
                let mut json = serde_json::to_value(entry).unwrap();
                json["v"] = 1.into();
                format!("{json}\n")
            })
            .collect()
    }

    /// What the journal makes of the entries, which must be all of them.
    fn replayed(entries: &[Entry]) -> Replay {
        let found = replay(&file(entries));
        assert_eq!(found.unreadable, 0, "the journal reads what the test wrote");
        found
    }

    fn report_of(entries: &[Entry]) -> String {
        text(&replayed(entries), "0.5.0", 26200, NOW, false)
    }

    /// The lines under a heading, up to the next line that isn't indented, without their indent.
    fn section(report: &str, heading: &str) -> Vec<String> {
        report
            .lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| line.starts_with("  "))
            .map(|line| line[2..].to_string())
            .collect()
    }

    fn history_of(entries: &[Entry]) -> Vec<String> {
        section(&report_of(entries), "History")
    }

    fn active_of(entries: &[Entry]) -> Vec<String> {
        section(&report_of(entries), "Active")
    }

    /// A History line for something that happened at 18:12.
    fn at_18_12(words: &str) -> String {
        format!("2026-11-02 18:12 {words}")
    }

    // What the spec shows.

    #[test]
    fn the_report_matches_the_spec() {
        let mut entries = vec![restore_point(
            &time("18:12"),
            RestoreOutcome::Created { sequence: 57 },
        )];
        entries.extend(kestrel_plan(&time("18:12"), plan(EXM, EXM_NAME)));
        entries.extend(game_mode_on(&time("18:12")));
        entries.push(undone(GameMode, &time("18:15"), None));

        let report = text(&replayed(&entries), "0.5.0", 26200, NOW, false);

        assert_eq!(
            report,
            "\
Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20
Active
  Kestrel power plan: EXM Free Power Plan V8 → Kestrel (2026-11-02 18:12)
History
  2026-11-02 18:15 Undid Game Mode: back to off
  2026-11-02 18:12 Applied Game Mode: off → on
  2026-11-02 18:12 Applied Kestrel power plan: EXM Free Power Plan V8 → Kestrel
  2026-11-02 18:12 Restore point created (#57)"
        );
    }

    #[test]
    fn no_changes_says_so() {
        let report = text(&Replay::default(), "0.5.0", 26200, NOW, false);

        assert_eq!(
            report,
            "Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20\nNo changes yet."
        );
    }

    #[test]
    fn every_entry_kind_has_a_line() {
        let t = time("18:12");
        let cases: Vec<(Vec<Entry>, Vec<String>)> = vec![
            // The four results of asking for a restore point.
            (
                vec![restore_point(&t, RestoreOutcome::Created { sequence: 57 })],
                vec![at_18_12("Restore point created (#57)")],
            ),
            (
                vec![restore_point(
                    &t,
                    RestoreOutcome::Reused {
                        sequence: 56,
                        time: "2026-11-02T09:00:00+00:00".to_string(),
                    },
                )],
                vec![at_18_12("Restore point reused from 2026-11-02 09:00 (#56)")],
            ),
            // A point Windows gave no time for.
            (
                vec![restore_point(
                    &t,
                    RestoreOutcome::Reused {
                        sequence: 56,
                        time: String::new(),
                    },
                )],
                vec![at_18_12("Restore point reused (#56)")],
            ),
            (
                vec![restore_point(
                    &t,
                    RestoreOutcome::Reused {
                        sequence: 56,
                        time: " \n ".to_string(),
                    },
                )],
                vec![at_18_12("Restore point reused (#56)")],
            ),
            (
                vec![restore_point(&t, RestoreOutcome::Skipped)],
                vec![at_18_12("Restore point skipped")],
            ),
            (
                vec![restore_point(
                    &t,
                    RestoreOutcome::Failed {
                        reason: "System Protection is off.".to_string(),
                    },
                )],
                vec![at_18_12("Restore point failed: System Protection is off.")],
            ),
            // A change.
            (
                game_mode_on(&t),
                vec![at_18_12("Applied Game Mode: off → on")],
            ),
            // Applied again.
            (
                [
                    game_mode_on(&t),
                    vec![
                        intent(GameMode, &t, dword(2), dword(1)),
                        reapplied(GameMode, &t),
                    ],
                ]
                .concat(),
                vec![
                    at_18_12("Applied Game Mode again"),
                    at_18_12("Applied Game Mode: off → on"),
                ],
            ),
            // Undone.
            (
                [game_mode_on(&t), vec![undone(GameMode, &t, None)]].concat(),
                vec![
                    at_18_12("Undid Game Mode: back to off"),
                    at_18_12("Applied Game Mode: off → on"),
                ],
            ),
            // A setting that went back, and a plan that was deleted, without Kestrel.
            (
                [
                    game_mode_on(&t),
                    vec![closed(GameMode, &t, CloseReason::WentBack)],
                ]
                .concat(),
                vec![
                    at_18_12("Game Mode went back outside Kestrel"),
                    at_18_12("Applied Game Mode: off → on"),
                ],
            ),
            (
                [
                    kestrel_plan(&t, plan(BALANCED, "Balanced")),
                    vec![closed(PowerPlan, &t, CloseReason::PlanRemovedElsewhere)],
                ]
                .concat(),
                vec![
                    at_18_12("Kestrel power plan was removed outside Kestrel"),
                    at_18_12("Applied Kestrel power plan: Balanced → Kestrel"),
                ],
            ),
            // A change that didn't take.
            (
                vec![
                    intent(GameMode, &t, dword(0), dword(1)),
                    outcome_with(
                        GameMode,
                        &t,
                        false,
                        Some("Windows refused the change: access denied"),
                        false,
                    ),
                ],
                vec![at_18_12(
                    "Couldn't apply Game Mode: Windows refused the change: access denied",
                )],
            ),
            // The record of System Protection, which Kestrel turned on.
            (
                vec![
                    intent(SystemProtection, &t, protection(false), protection(true)),
                    outcome(SystemProtection, &t),
                ],
                vec![at_18_12("Applied System Protection: off → on")],
            ),
            // A crash before the change leaves nothing to tell, and so does an intent still waiting for its outcome (the
            // History says so, since that record is in the Active list).
            (
                vec![
                    intent(GameMode, &t, dword(0), dword(1)),
                    closed(GameMode, &t, CloseReason::NotApplied),
                ],
                Vec::new(),
            ),
            (
                vec![intent(GameMode, &t, dword(0), dword(1))],
                vec!["No finished changes yet.".to_string()],
            ),
        ];
        for (entries, expected) in cases {
            assert_eq!(history_of(&entries), expected, "{entries:#?}");
        }
    }

    #[test]
    fn an_undo_says_where_it_went_back_to() {
        let t = time("18:12");
        let after = |entries: Vec<Entry>, last: Entry| {
            history_of(&[entries, vec![last]].concat())[0].clone()
        };

        // The plan: the sentence the undo gave, which also tells of the fallback to Balanced.
        let applied = kestrel_plan(&t, plan(EXM, EXM_NAME));
        for (detail, said) in [
            (
                "Back to EXM Free Power Plan V8.",
                "Undid Kestrel power plan: Back to EXM Free Power Plan V8.",
            ),
            (
                "Your previous plan is gone, so Kestrel switched to Balanced.",
                "Undid Kestrel power plan: Your previous plan is gone, so Kestrel switched to Balanced.",
            ),
        ] {
            assert_eq!(
                after(applied.clone(), undone(PowerPlan, &t, Some(detail))),
                at_18_12(said)
            );
        }
        // Remove it deletes the plan and leaves the one in use, so it isn't a way back to the one Kestrel found.
        assert_eq!(
            after(applied, undone(PowerPlan, &t, None)),
            at_18_12(
                "Undid Kestrel power plan: removed Kestrel's plan and left the plan in use alone"
            )
        );

        // System Protection goes back to off, and every other setting to the state its first intent found.
        let protected = vec![
            intent(SystemProtection, &t, protection(false), protection(true)),
            outcome(SystemProtection, &t),
        ];
        assert_eq!(
            after(protected, undone(SystemProtection, &t, None)),
            at_18_12("Undid System Protection: back to off")
        );
        let windowed = vec![
            intent(WindowedOptimizations, &t, token(None), token(Some("0"))),
            outcome(WindowedOptimizations, &t),
        ];
        assert_eq!(
            after(windowed, undone(WindowedOptimizations, &t, None)),
            at_18_12("Undid Optimizations for windowed games: back to not set")
        );

        // With no record to say what it was, the line says no more than the entry does.
        assert_eq!(
            history_of(&[undone(GameMode, &t, None)]),
            [at_18_12("Undid Game Mode")]
        );
        // And an empty detail is no detail.
        assert_eq!(
            after(game_mode_on(&t), undone(GameMode, &t, Some(" "))),
            at_18_12("Undid Game Mode: back to off")
        );
    }

    // Later intents.

    #[test]
    fn a_later_intent_is_not_a_fresh_apply() {
        let t = time("18:12");
        let later = time("18:30");
        // Apply again over a third value: its intent plans what the first one did, over the value it found.
        let again = intent(GameMode, &later, dword(2), dword(1));

        // It didn't take: that's all that is told of it, not "Applied Game Mode: on → on".
        let failed = outcome_with(
            GameMode,
            &later,
            false,
            Some("Windows refused the change: access denied"),
            false,
        );
        assert_eq!(
            history_of(&[game_mode_on(&t), vec![again.clone(), failed]].concat()),
            [
                "2026-11-02 18:30 Couldn't apply Game Mode: Windows refused the change: access denied".to_string(),
                at_18_12("Applied Game Mode: off → on"),
            ]
        );

        // An outcome that says it was applied, after a later intent, is the change being made again.
        assert_eq!(
            history_of(
                &[
                    game_mode_on(&t),
                    vec![again.clone(), outcome(GameMode, &later)]
                ]
                .concat()
            ),
            [
                "2026-11-02 18:30 Applied Game Mode again".to_string(),
                at_18_12("Applied Game Mode: off → on"),
            ]
        );

        // An attempt that didn't take, over a record that was already open, leaves that record where it was.
        let entries = [
            game_mode_on(&t),
            vec![
                again,
                outcome_with(GameMode, &later, false, Some("no"), false),
                undone(GameMode, &time("18:40"), None),
            ],
        ]
        .concat();
        assert_eq!(
            history_of(&entries)[0],
            "2026-11-02 18:40 Undid Game Mode: back to off"
        );

        // But one that opened the record takes it back, so the next attempt opens it again with its own values.
        let entries = vec![
            intent(GameMode, &t, dword(0), dword(1)),
            outcome_with(GameMode, &t, false, Some("no"), false),
            intent(GameMode, &later, dword(1), dword(0)),
            outcome(GameMode, &later),
        ];
        assert_eq!(
            history_of(&entries),
            [
                "2026-11-02 18:30 Applied Game Mode: on → off".to_string(),
                at_18_12("Couldn't apply Game Mode: no"),
            ]
        );
        assert_eq!(
            active_of(&entries),
            ["Game Mode: on → off (2026-11-02 18:30)"]
        );
    }

    #[test]
    fn a_closed_record_makes_the_next_apply_a_fresh_one() {
        let t = time("18:12");
        let later = time("18:30");
        let entries = [
            game_mode_on(&t),
            vec![undone(GameMode, &time("18:20"), None)],
            vec![
                intent(GameMode, &later, dword(1), dword(0)),
                outcome(GameMode, &later),
            ],
        ]
        .concat();

        assert_eq!(
            history_of(&entries),
            [
                "2026-11-02 18:30 Applied Game Mode: on → off".to_string(),
                "2026-11-02 18:20 Undid Game Mode: back to off".to_string(),
                at_18_12("Applied Game Mode: off → on"),
            ]
        );
    }

    #[test]
    fn a_change_that_failed_and_could_not_be_put_back_is_not_a_success() {
        let t = time("18:12");
        let entries = vec![
            intent(GameMode, &t, dword(0), dword(1)),
            outcome_with(
                GameMode,
                &t,
                true,
                Some("Windows didn't keep the change. Kestrel couldn't put the old value back: no"),
                false,
            ),
        ];

        assert_eq!(
            history_of(&entries),
            [at_18_12(
                "Couldn't apply Game Mode: Windows didn't keep the change. Kestrel couldn't put the old value back: no"
            )]
        );
        // The record stays open, since the setting may still be changed, so it is still in the Active list.
        assert_eq!(
            active_of(&entries),
            ["Game Mode: off → on (2026-11-02 18:12)"]
        );
    }

    #[test]
    fn an_interrupted_change_does_not_read_as_a_plain_success() {
        let t = time("18:12");
        // A crash cut the change short, and the next start settled it the next morning.
        let morning = "2026-11-03T09:00:00+00:00";

        // The first apply: the line says what Kestrel was doing, with the time it was interrupted.
        let entries = vec![
            intent(GameMode, &t, dword(0), dword(1)),
            outcome_with(GameMode, morning, true, None, true),
        ];
        assert_eq!(
            history_of(&entries),
            [at_18_12(
                "Kestrel was interrupted while applying Game Mode: off → on"
            )]
        );
        // The record is open, and settled by that outcome, so it is Active until something ends it.
        assert_eq!(
            active_of(&entries),
            ["Game Mode: off → on (2026-11-02 18:12)"]
        );

        // Apply again: it has no values of its own to show.
        let entries = [
            game_mode_on(&t),
            vec![
                intent(GameMode, &time("18:30"), dword(2), dword(1)),
                outcome_with(GameMode, morning, true, None, true),
            ],
        ]
        .concat();
        assert_eq!(
            history_of(&entries),
            [
                "2026-11-02 18:30 Kestrel was interrupted while applying Game Mode again"
                    .to_string(),
                at_18_12("Applied Game Mode: off → on"),
            ]
        );

        // The plan reads the same way.
        let entries = vec![
            intent(
                PowerPlan,
                &t,
                plan(BALANCED, "Balanced"),
                plan(KESTREL, "Kestrel"),
            ),
            outcome_with(PowerPlan, morning, true, None, true),
        ];
        assert_eq!(
            history_of(&entries),
            [at_18_12(
                "Kestrel was interrupted while applying Kestrel power plan: Balanced → Kestrel"
            )]
        );

        // An outcome with no intent in front of it is told as much as it says.
        assert_eq!(
            history_of(&[outcome_with(GameMode, &t, true, None, true)]),
            [at_18_12("Kestrel was interrupted while applying Game Mode")]
        );
        assert_eq!(
            history_of(&[outcome(GameMode, &t)]),
            [at_18_12("Applied Game Mode")]
        );
        assert_eq!(
            history_of(&[outcome_with(GameMode, &t, false, None, false)]),
            [at_18_12("Couldn't apply Game Mode")]
        );
    }

    #[test]
    fn a_line_goes_where_its_attempt_began() {
        // A crash left an intent, something else was written before the next start settled it, and the settling
        // outcome came last. The line is at the time of the intent, and in its place, so the list stays newest first.
        let entries = vec![
            intent(GameMode, &time("18:12"), dword(0), dword(1)),
            undone(
                PowerPlan,
                "2026-11-03T09:01:00+00:00",
                Some("Back to Balanced."),
            ),
            outcome_with(GameMode, "2026-11-03T09:02:00+00:00", true, None, true),
        ];

        assert_eq!(
            history_of(&entries),
            [
                "2026-11-03 09:01 Undid Kestrel power plan: Back to Balanced.".to_string(),
                at_18_12("Kestrel was interrupted while applying Game Mode: off → on"),
            ]
        );

        // And a change that took a moment is told at the moment it began.
        let entries = vec![
            intent(GameMode, "2026-11-02T18:12:59+00:00", dword(0), dword(1)),
            outcome(GameMode, "2026-11-02T18:13:01+00:00"),
        ];
        assert_eq!(
            history_of(&entries),
            [at_18_12("Applied Game Mode: off → on")]
        );
        assert_eq!(
            active_of(&entries),
            ["Game Mode: off → on (2026-11-02 18:12)"]
        );
    }

    // The Active list.

    #[test]
    fn active_lists_what_is_open_with_its_first_values_newest_first() {
        let entries = [
            kestrel_plan(&time("18:10"), plan(BALANCED, "Balanced")),
            game_mode_on(&time("18:12")),
            vec![
                intent(BackgroundRecording, &time("18:13"), dword(1), dword(0)),
                outcome(BackgroundRecording, &time("18:13")),
                // Game Mode was changed by something else and applied again: the record keeps its first values.
                intent(GameMode, &time("18:30"), dword(2), dword(1)),
                reapplied(GameMode, &time("18:30")),
                undone(BackgroundRecording, &time("18:40"), None),
            ],
        ]
        .concat();

        assert_eq!(
            active_of(&entries),
            [
                "Game Mode: off → on (2026-11-02 18:12)",
                "Kestrel power plan: Balanced → Kestrel (2026-11-02 18:10)",
            ]
        );
    }

    #[test]
    fn active_says_so_when_nothing_is() {
        let t = time("18:12");
        let entries = [game_mode_on(&t), vec![undone(GameMode, &t, None)]].concat();

        assert_eq!(active_of(&entries), ["Nothing is active."]);
        assert_eq!(history_of(&entries).len(), 2);
    }

    #[test]
    fn active_lists_every_record_it_is_given_and_judges_none() {
        // What counts as applied is for whoever builds the `Replay` to say, and `Engine::report` hands over the records
        // that hold. One still waiting for its outcome (a crash cut it off after the write) is what the next look at the
        // tab calls Applied, so it is listed like any other.
        let t = time("18:12");
        let entries = [
            kestrel_plan(&t, plan(BALANCED, "Balanced")),
            vec![intent(GameMode, &t, dword(0), dword(1))],
        ]
        .concat();
        assert_eq!(
            active_of(&entries),
            [
                "Game Mode: off → on (2026-11-02 18:12)",
                "Kestrel power plan: Balanced → Kestrel (2026-11-02 18:12)",
            ]
        );
    }

    #[test]
    fn a_change_with_nothing_finished_yet_does_not_say_that_nothing_changed() {
        // Active has a record and History has no line, because the record's outcome isn't written yet. "No changes yet."
        // is for a report with nothing in either.
        let t = time("18:12");
        let report = report_of(&[intent(GameMode, &t, dword(0), dword(1))]);

        assert_eq!(
            report,
            "\
Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20
Active
  Game Mode: off → on (2026-11-02 18:12)
History
  No finished changes yet."
        );
        assert!(!report.contains("No changes yet."));
    }

    #[test]
    fn each_account_has_a_record_of_its_own() {
        let t = time("18:12");
        let entries = [
            game_mode_on(&t),
            game_mode_on(&t)
                .into_iter()
                .map(|entry| of_account(entry, OTHER_SID))
                .collect(),
        ]
        .concat();

        // The report knows nothing of accounts: it lists the records it is given, and `Engine::report` gives it this
        // account's and the machine's.
        assert_eq!(
            active_of(&entries),
            [
                "Game Mode: off → on (2026-11-02 18:12)",
                "Game Mode: off → on (2026-11-02 18:12)",
            ]
        );
        assert_eq!(history_of(&entries).len(), 2);
        // The second account's intent is a fresh one, not a later one.
        assert!(history_of(&entries)
            .iter()
            .all(|line| line == &at_18_12("Applied Game Mode: off → on")));
    }

    // Order.

    #[test]
    fn history_is_newest_first_and_a_minute_keeps_the_journals_reverse_order() {
        let entries = [
            vec![restore_point(&time("18:10"), RestoreOutcome::Skipped)],
            game_mode_on(&time("18:12")),
            vec![
                intent(BackgroundRecording, &time("18:12"), dword(1), dword(0)),
                outcome(BackgroundRecording, &time("18:12")),
            ],
            kestrel_plan(&time("18:12"), plan(BALANCED, "Balanced")),
            vec![undone(BackgroundRecording, &time("18:13"), None)],
        ]
        .concat();

        assert_eq!(
            history_of(&entries),
            [
                "2026-11-02 18:13 Undid Background recording off: back to on",
                "2026-11-02 18:12 Applied Kestrel power plan: Balanced → Kestrel",
                "2026-11-02 18:12 Applied Background recording off: on → off",
                "2026-11-02 18:12 Applied Game Mode: off → on",
                "2026-11-02 18:10 Restore point skipped",
            ]
        );
    }

    // The unreadable lines.

    #[test]
    fn unreadable_lines_are_warned_about() {
        let report = text(&Replay::default(), "0.5.0", 26200, NOW, true);

        assert_eq!(
            report,
            "\
Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20
No changes yet.
An interruption while Kestrel was writing is the usual cause of an unreadable line.
Part of Kestrel's history couldn't be read."
        );
        // The warning is the trailing line, with history before it as well.
        let t = time("18:12");
        let report = text(&replayed(&game_mode_on(&t)), "0.5.0", 26200, NOW, true);
        assert_eq!(report.lines().last(), Some(UNREADABLE_TEXT));
        assert!(report.ends_with(&format!("\n{UNREADABLE_TEXT}")));
        assert!(report.contains("interruption"));
        // And there is none to give when every line was read.
        let report = text(&replayed(&game_mode_on(&t)), "0.5.0", 26200, NOW, false);
        assert!(!report.contains("couldn't be read"), "{report}");
        assert!(!report.contains("interruption"), "{report}");
    }

    #[test]
    fn a_journal_that_could_not_be_read_at_all_says_so_and_not_that_nothing_happened() {
        let report = unavailable("0.5.0", 26200, NOW, "Access is denied. (os error 5)");

        assert_eq!(
            report,
            "Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20\nKestrel's history couldn't be read: Access is denied. (os error 5)"
        );
        assert!(!report.contains("No changes yet."));
        // Whatever the reason holds, it stays on its line.
        let report = unavailable("0.5.0", 26200, NOW, "bad\nHistory");
        assert_eq!(report.lines().count(), 2, "{report}");
    }

    // Times.

    #[test]
    fn a_time_is_its_first_sixteen_characters_with_the_t_taken_out() {
        for (at, shown) in [
            ("2026-11-02T18:12:05+01:00", "2026-11-02 18:12"),
            ("2026-11-02T18:12:05.123456789+01:00", "2026-11-02 18:12"),
            ("2026-11-02 18:12", "2026-11-02 18:12"),
            ("2026-11-02T18:12", "2026-11-02 18:12"),
            // Not a time: whatever is there, no more than 16 characters of it.
            ("soon", "soon"),
            ("", ""),
            ("Tomorrow at teatime!!", "Tomorrow at teat"),
            // 16 characters are 16 characters whatever their size in bytes, and a cut never lands inside one.
            ("2026-11-02T18:1€€€€€€€€€", "2026-11-02 18:1€"),
            ("2026-11-02T18:12€€€", "2026-11-02 18:12"),
            // A line break is a space, whatever else there is.
            ("2026-11-02\n18:12:00", "2026-11-02 18:12"),
        ] {
            assert_eq!(when(at), shown, "{at:?}");
        }
        assert_eq!(when(&"日本語".repeat(6)), "日本語".repeat(5) + "日");
        assert_eq!(when(&"🦅".repeat(20)), "🦅".repeat(16));
    }

    #[test]
    fn a_time_that_is_not_one_is_printed_as_it_is_and_never_panics() {
        let report = text(&Replay::default(), "0.5.0", 26200, "soon", false);
        assert!(report.starts_with("Kestrel report · Kestrel 0.5.0 · Windows build 26200 · soon\n"));
        let report = text(&Replay::default(), "0.5.0", 26200, "", false);
        assert!(report.starts_with("Kestrel report · Kestrel 0.5.0 · Windows build 26200 · \n"));

        // In the entries too, in each place a time goes.
        let odd = "20€€-11-02T18:1€:00";
        let entries = [
            vec![
                restore_point(odd, RestoreOutcome::Skipped),
                restore_point(
                    odd,
                    RestoreOutcome::Reused {
                        sequence: 1,
                        time: odd.to_string(),
                    },
                ),
            ],
            vec![
                intent(GameMode, odd, dword(0), dword(1)),
                outcome(GameMode, odd),
                undone(GameMode, odd, None),
                intent(
                    PowerPlan,
                    odd,
                    plan(BALANCED, "Balanced"),
                    plan(KESTREL, "Kestrel"),
                ),
                outcome(PowerPlan, odd),
            ],
        ]
        .concat();
        let report = report_of(&entries);
        assert!(
            report.contains("  20€€-11-02 18:1€ Undid Game Mode: back to off"),
            "{report}"
        );
        assert!(report.contains("(20€€-11-02 18:1€)"), "{report}");
        assert!(
            report.contains("reused from 20€€-11-02 18:1€ (#1)"),
            "{report}"
        );
    }

    // Untrusted text.

    #[test]
    fn nothing_the_journal_holds_can_start_a_line_of_the_report() {
        let t = time("18:12");
        let entries = [
            // A reason, and a time, as the restore point's entry could hold them.
            vec![
                restore_point(
                    &t,
                    RestoreOutcome::Failed {
                        reason: FORGERY.to_string(),
                    },
                ),
                restore_point(
                    &t,
                    RestoreOutcome::Reused {
                        sequence: 56,
                        time: FORGERY.to_string(),
                    },
                ),
            ],
            // A token in a string value, which describes as itself, in a time of its own.
            vec![
                intent(
                    WindowedOptimizations,
                    FORGERY,
                    token(None),
                    token(Some(FORGERY)),
                ),
                outcome(WindowedOptimizations, &t),
            ],
            // A plan's name, applied and then undone with a detail.
            vec![
                intent(
                    PowerPlan,
                    &t,
                    plan(BALANCED, FORGERY),
                    plan(KESTREL, "Kestrel"),
                ),
                outcome(PowerPlan, &t),
                undone(PowerPlan, &t, Some(FORGERY)),
            ],
            // A detail of a change that didn't take.
            vec![
                intent(GameMode, &t, dword(0), dword(1)),
                outcome_with(GameMode, &t, false, Some(FORGERY), false),
            ],
        ]
        .concat();
        let replayed = replayed(&entries);
        // Open records say it too, in their own time.
        let mut open = replayed.open.clone();
        open.push(OpenRecord {
            tweak: GameMode,
            sid: Some(SID.to_string()),
            value_name: None,
            prior: plan(BALANCED, FORGERY),
            planned: token(Some(FORGERY)),
            at: FORGERY.to_string(),
            settled: true,
        });
        let replayed = Replay { open, ..replayed };

        let report = text(&replayed, "0.5.0", 26200, FORGERY, true);

        // The header, the two headings and what is under them, the two lines of the warning: nothing more.
        assert_eq!(report.lines().count(), 1 + 1 + 2 + 1 + 6 + 2, "{report}");
        assert_eq!(report.lines().filter(|line| *line == "History").count(), 1);
        assert_eq!(report.lines().filter(|line| *line == "Active").count(), 1);
        for line in report.lines() {
            assert!(
                line.starts_with("Kestrel report")
                    || line == "Active"
                    || line == "History"
                    || line.starts_with("  ")
                    || line == UNREADABLE_TEXT
                    || line.starts_with("An interruption"),
                "{line:?}"
            );
        }
        // No control character is left, and no separator that a viewer would break a line at.
        assert!(
            !report
                .chars()
                .any(|c| c != '\n' && (c.is_control() || c == '\u{2028}' || c == '\u{2029}')),
            "{report:?}"
        );
        // Each one is a space where it was, one for one.
        let spaces = " ".repeat(7);
        let cleaned = format!(
            "went wrong   2026-11-02 18:12 Applied Game Mode: off → on  History{spaces}[2J end"
        );
        assert!(
            report.contains(&format!(
                "2026-11-02 18:12 Couldn't apply Game Mode: {cleaned}"
            )),
            "{report}"
        );
        assert!(
            report.contains(&format!("Undid Kestrel power plan: {cleaned}")),
            "{report}"
        );
        assert!(
            report.contains(&format!("Restore point failed: {cleaned}")),
            "{report}"
        );
        assert!(
            report.contains(&format!("Applied Kestrel power plan: {cleaned} → Kestrel")),
            "{report}"
        );
    }

    // Journals that can't be trusted to agree with themselves.

    #[test]
    fn a_replay_that_disagrees_with_itself_is_still_read() {
        let t = time("18:12");
        let odd = |tweak: &str| Entry::Intent {
            at: t.clone(),
            batch: t.clone(),
            tweak: tweak.to_string(),
            sid: None,
            value_name: None,
            prior: dword(0),
            planned: dword(1),
        };
        let history = vec![
            // Entries about a tweak nobody has heard of, which the journal itself would have refused.
            odd("nonsense"),
            Entry::Outcome {
                at: t.clone(),
                tweak: "nonsense".to_string(),
                sid: None,
                applied: true,
                detail: None,
                interrupted: false,
            },
            Entry::Undone {
                at: t.clone(),
                tweak: String::new(),
                sid: None,
                detail: None,
            },
            // And ones about records that were never opened.
            reapplied(GameMode, &t),
            undone(GameMode, &t, None),
            closed(PowerPlan, &t, CloseReason::NotApplied),
            outcome_with(GameMode, &t, false, None, false),
        ];
        let open = vec![OpenRecord {
            tweak: GameMode,
            sid: Some(SID.to_string()),
            value_name: None,
            prior: dword(0),
            planned: dword(1),
            at: String::new(),
            settled: true,
        }];

        let report = text(
            &Replay {
                open,
                history,
                unreadable: 3,
            },
            "0.5.0",
            26200,
            NOW,
            true,
        );

        assert_eq!(
            section(&report, "History"),
            [
                at_18_12("Couldn't apply Game Mode"),
                at_18_12("Undid Game Mode"),
                at_18_12("Applied Game Mode again"),
            ]
        );
        assert_eq!(section(&report, "Active"), ["Game Mode: off → on ()"]);
    }

    #[test]
    fn the_report_follows_the_journals_idea_of_an_open_record() {
        // Every run of up to four entries about one setting. What the report takes to be open when they end, and what
        // that record's first prior is, are what `replay` says.
        let t = time("18:12");
        let alphabet = [
            intent(GameMode, &t, dword(0), dword(1)),
            intent(GameMode, &t, dword(2), dword(1)),
            outcome(GameMode, &t),
            outcome_with(GameMode, &t, false, Some("no"), false),
            outcome_with(GameMode, &t, true, Some("no"), false),
            outcome_with(GameMode, &t, true, None, true),
            reapplied(GameMode, &t),
            undone(GameMode, &t, None),
            closed(GameMode, &t, CloseReason::WentBack),
            closed(GameMode, &t, CloseReason::NotApplied),
        ];
        let base = alphabet.len();
        let mut runs = 0;
        for length in 0..=4u32 {
            for code in 0..base.pow(length) {
                let entries: Vec<Entry> = (0..length)
                    .map(|place| alphabet[(code / base.pow(place)) % base].clone())
                    .collect();
                let found = replayed(&entries);
                let mut walk = Walk::default();
                for entry in &entries {
                    walk.take(entry);
                }

                assert_eq!(walk.open.len(), found.open.len(), "{entries:#?}");
                if let (Some(mine), Some(theirs)) = (walk.open.first(), found.open.first()) {
                    assert_eq!(mine.first_prior, theirs.prior, "{entries:#?}");
                }
                text(&found, "0.5.0", 26200, NOW, false);
                runs += 1;
            }
        }
        assert_eq!(runs, 1 + 10 + 100 + 1000 + 10_000);
    }
}
