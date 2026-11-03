//! The journal: the crash-safe record of every change Kestrel makes, and the only thing undo trusts.
//!
//! It is an append-only file of JSON lines, `journal.jsonl`, in a folder only administrators may write to. Each line is
//! `{"v":1, ...}` followed by the fields of one [`Entry`]. Before a change Kestrel appends and flushes an `Intent`
//! holding the setting's `prior` state and the one it is about to write; afterwards, an `Outcome`. A record stays open
//! from its intent until an `Undone` or a `Closed`, so a crash between any two steps leaves enough on disk to put the
//! setting back. [`replay`] reads the lines back into the open records.
//!
//! A record belongs to one tweak and one account: its key is the tweak and the account's SID, and the two machine-wide
//! changes (the power plan, System Protection) have no SID.
//!
//! The file is treated as untrusted even so. A line is believed only when it names a known tweak, its SID looks like
//! a SID, a saved plan is a GUID, and a value name (only the game's GPU preference has one) is a path to
//! `VALORANT-Win64-Shipping.exe`. A line that fails any of that, or isn't an entry this version wrote, is counted in
//! `unreadable` and affects nothing. The one exception is a last line that is not JSON and has no newline: that is a
//! write cut short by a crash, and it is ignored without a word. Beyond that the journal only reports what its lines
//! say: whether a saved state suits its tweak is for whoever acts on the record to check.
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read as _, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use kestrel_core::tweak_id::TweakId;
use serde::{Deserialize, Serialize};

/// The format's version, written at the start of every line. A line with any other is not read.
const VERSION: u32 = 1;
const JOURNAL: &str = "journal.jsonl";
const LOCK: &str = "journal.lock";
const GAME_EXE: &str = "VALORANT-Win64-Shipping.exe";
const ALREADY_CHANGING: &str = "Kestrel is already changing settings in another window.";

/// A setting's state as the journal keeps it: enough to put the setting back, and to tell whether it still is what
/// Kestrel left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Saved {
    /// A DWORD value; `None` when the value wasn't there.
    Dword { value: Option<u32> },
    /// One `Name=Value;` token inside a string value (the DirectX settings string, or the game's own GPU-preference
    /// value); `token` is `None` when the token wasn't there. `value_existed` and `key_existed` say whether the string
    /// value and its key were there before Kestrel's first change, so an undo removes only what Kestrel made.
    Token {
        token: Option<String>,
        value_existed: bool,
        key_existed: bool,
    },
    /// The three strings under `Control Panel\Mouse` (`None` when absent) and the setting in force, as
    /// [threshold 1, threshold 2, acceleration].
    Mouse {
        speed: Option<String>,
        threshold1: Option<String>,
        threshold2: Option<String>,
        live: [i32; 3],
    },
    /// A power plan: its GUID and the name Windows showed for it.
    Plan { guid: String, name: String },
    /// Whether System Protection was on.
    Protection { on: bool },
}

/// How the request for a restore point went. The report says the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum RestoreOutcome {
    /// A new point was made; `sequence` is its number.
    Created {
        sequence: u32,
    },
    /// Windows answered with an earlier point's number, because it makes one per 24 hours; `time` is when that was.
    Reused {
        sequence: u32,
        time: String,
    },
    /// The person went on without one.
    Skipped,
    Failed {
        reason: String,
    },
}

/// Why a record ended without Kestrel having to change anything back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloseReason {
    /// The setting is back at its old value, put there by something other than Kestrel.
    WentBack,
    /// Kestrel's power plan was deleted outside Kestrel.
    PlanRemovedElsewhere,
    /// After a crash: the setting was never changed.
    NotApplied,
}

/// One line of the journal. `tweak` and `sid` name the record it is about (`sid` is `None` for a machine-wide change)
/// and `at` is when it was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Entry {
    /// About to change a setting; written and flushed before the change. `batch` ties together the items of one
    /// Apply. `value_name` is the game's executable path as the name of the registry value it lives in, for the GPU
    /// preference only.
    Intent {
        at: String,
        batch: String,
        tweak: String,
        sid: Option<String>,
        value_name: Option<String>,
        prior: Saved,
        planned: Saved,
    },
    /// How the change went, after reading the setting back. `applied: false` means it didn't take and the old value
    /// was put back. `interrupted` marks an outcome written after a crash, when Kestrel looked at what was left.
    Outcome {
        at: String,
        tweak: String,
        sid: Option<String>,
        applied: bool,
        detail: Option<String>,
        interrupted: bool,
    },
    /// The person applied a tweak again over a value something else had changed. The record keeps its first prior.
    Reapplied {
        at: String,
        tweak: String,
        sid: Option<String>,
    },
    /// The change was undone, and the record ends. `detail` says anything the person should know about how.
    Undone {
        at: String,
        tweak: String,
        sid: Option<String>,
        detail: Option<String>,
    },
    /// The record ended without Kestrel putting anything back.
    Closed {
        at: String,
        tweak: String,
        sid: Option<String>,
        reason: CloseReason,
    },
    /// The result of asking Windows for a restore point. It belongs to no record.
    RestorePoint { at: String, outcome: RestoreOutcome },
}

/// A change Kestrel made that is still on the PC, as the journal's lines add up. Records are keyed by tweak and
/// account; `value_name`, `prior`, `planned` and `at` are those of the record's first intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRecord {
    pub tweak: TweakId,
    pub sid: Option<String>,
    pub value_name: Option<String>,
    pub prior: Saved,
    pub planned: Saved,
    pub at: String,
    /// False from an intent until its outcome. A record that stays false was cut off by a crash.
    pub settled: bool,
}

/// What a journal's lines add up to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replay {
    /// The records still open, oldest first.
    pub open: Vec<OpenRecord>,
    /// Every readable entry, in file order.
    pub history: Vec<Entry>,
    /// Lines that were refused or not JSON, apart from a last line that was cut short. They are never acted on.
    pub unreadable: u32,
}

/// Reads a journal's text into its open records.
///
/// A record opens with its first `Intent` and keeps that intent's prior: a later intent for the same tweak and
/// account only marks it unsettled until its outcome. An `Outcome` with `applied: true` (or a `Reapplied`) settles it;
/// one with `applied: false` takes back the intent it answers, which drops a record that intent opened and leaves a
/// record that was open before it as it was. `Undone` and `Closed` end it. An entry about a record that isn't open
/// changes nothing but the history.
pub fn replay(text: &str) -> Replay {
    // Everything up to the last newline is whole lines. What follows it has no newline of its own: it is the last
    // line, and the only one that may be a write that was cut short.
    let (whole, last) = text
        .rfind('\n')
        .map_or(("", text), |end| text.split_at(end + 1));
    let lines = whole
        .lines()
        .map(|line| (line, false))
        .chain([(last, true)]);
    let mut records = Records::default();
    let mut history = Vec::new();
    let mut unreadable = 0u32;
    for (line, may_be_cut_short) in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse(line) {
            Ok((entry, key)) => {
                if let Some(key) = key {
                    records.apply(&entry, &key);
                }
                history.push(entry);
            }
            Err(Unread::Garbled) if may_be_cut_short => {}
            Err(Unread::Garbled | Unread::Refused) => unreadable = unreadable.saturating_add(1),
        }
    }
    Replay {
        open: records.0.into_iter().map(|slot| slot.record).collect(),
        history,
        unreadable,
    }
}

/// The tweak and account a record belongs to.
type Key = (TweakId, Option<String>);

/// Why a line is not an entry to believe.
enum Unread {
    /// Not JSON.
    Garbled,
    /// JSON, but not an entry of this version, or not one to believe.
    Refused,
}

/// A line as an entry to believe, and the record it is about (none for a restore point).
fn parse(line: &str) -> Result<(Entry, Option<Key>), Unread> {
    let json = serde_json::from_str::<serde_json::Value>(line).map_err(|_| Unread::Garbled)?;
    if json.get("v") != Some(&serde_json::Value::from(VERSION)) {
        return Err(Unread::Refused);
    }
    let entry = serde_json::from_value::<Entry>(json).map_err(|_| Unread::Refused)?;
    let key = record_of(&entry).map_err(|Untrusted| Unread::Refused)?;
    Ok((entry, key))
}

/// A line whose fields can't be trusted to be acted on.
struct Untrusted;

/// The record an entry is about, once every field that undo will act on is known to be safe to act on.
fn record_of(entry: &Entry) -> Result<Option<Key>, Untrusted> {
    match entry {
        Entry::Intent {
            tweak,
            sid,
            value_name,
            prior,
            planned,
            ..
        } => {
            let key = key_of(tweak, sid.as_deref())?;
            // Only the GPU preference has a value named by a path, and it is only ever the game's.
            if let Some(name) = value_name {
                if key.0 != TweakId::GpuPreference || !is_game_value_name(name) {
                    return Err(Untrusted);
                }
            }
            if is_sound(prior) && is_sound(planned) {
                Ok(Some(key))
            } else {
                Err(Untrusted)
            }
        }
        Entry::Outcome { tweak, sid, .. }
        | Entry::Reapplied { tweak, sid, .. }
        | Entry::Undone { tweak, sid, .. }
        | Entry::Closed { tweak, sid, .. } => key_of(tweak, sid.as_deref()).map(Some),
        Entry::RestorePoint { .. } => Ok(None),
    }
}

fn key_of(tweak: &str, sid: Option<&str>) -> Result<Key, Untrusted> {
    let tweak = TweakId::parse(tweak).ok_or(Untrusted)?;
    match sid {
        Some(sid) if !is_sid(sid) => Err(Untrusted),
        _ => Ok((tweak, sid.map(str::to_string))),
    }
}

/// A saved state is safe to act on when a plan in it is a GUID. Nothing else in it is ever a name or a path.
fn is_sound(saved: &Saved) -> bool {
    !matches!(saved, Saved::Plan { guid, .. } if !is_guid(guid))
}

/// `S-` and then numbers joined by dashes, like `S-1-5-21-1-2-3-1001`. It goes into a registry path, so it can hold
/// nothing else.
fn is_sid(text: &str) -> bool {
    text.strip_prefix("S-").is_some_and(|rest| {
        rest.split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// The text form of a GUID, `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, with no braces.
pub(crate) fn is_guid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// A path to the game's executable, to be used as the name of a registry value. Windows keeps a value's name in the
/// spelling it was first given, and Kestrel updates the value as it is spelled there, so the executable's name may be in
/// any case and the last slash either way. A control character is refused: a NUL in a name would reach Windows cut
/// short, as a different name.
fn is_game_value_name(name: &str) -> bool {
    if name.chars().any(char::is_control) {
        return false;
    }
    let name = name.as_bytes();
    let Some(split) = name.len().checked_sub(GAME_EXE.len() + 1) else {
        return false;
    };
    matches!(name[split], b'\\' | b'/')
        && name[split + 1..].eq_ignore_ascii_case(GAME_EXE.as_bytes())
}

/// An open record while the journal is being read, with what it takes to take an outcome back.
struct Slot {
    record: OpenRecord,
    /// The record exists only because of the intent that is still waiting for its outcome.
    fresh: bool,
    /// Whether the record was settled before that intent.
    settled_before: bool,
}

#[derive(Default)]
struct Records(Vec<Slot>);

impl Records {
    /// Applies an entry about the record `key`.
    fn apply(&mut self, entry: &Entry, key: &Key) {
        let found = self
            .0
            .iter()
            .position(|slot| slot.record.tweak == key.0 && slot.record.sid == key.1);
        match (entry, found) {
            (
                Entry::Intent {
                    at,
                    value_name,
                    prior,
                    planned,
                    ..
                },
                None,
            ) => self.0.push(Slot {
                record: OpenRecord {
                    tweak: key.0,
                    sid: key.1.clone(),
                    value_name: value_name.clone(),
                    prior: prior.clone(),
                    planned: planned.clone(),
                    at: at.clone(),
                    settled: false,
                },
                fresh: true,
                settled_before: false,
            }),
            // The first prior is kept: a later intent only unsettles the record until its outcome.
            (Entry::Intent { .. }, Some(index)) => {
                let slot = &mut self.0[index];
                slot.settled_before = slot.record.settled;
                slot.record.settled = false;
                slot.fresh = false;
            }
            (Entry::Outcome { applied: true, .. } | Entry::Reapplied { .. }, Some(index)) => {
                let slot = &mut self.0[index];
                slot.record.settled = true;
                slot.fresh = false;
            }
            (Entry::Outcome { applied: false, .. }, Some(index)) => {
                if self.0[index].fresh {
                    self.0.remove(index);
                } else {
                    let slot = &mut self.0[index];
                    slot.record.settled = slot.settled_before;
                }
            }
            (Entry::Undone { .. } | Entry::Closed { .. }, Some(index)) => {
                self.0.remove(index);
            }
            _ => {}
        }
    }
}

/// Where the journal may live. On Windows that is a folder only administrators and SYSTEM can write to, because an
/// elevated Kestrel must not act on a file any user can edit.
pub trait Folder: Send + Sync {
    /// Refuses a folder that exists with an owner that isn't trusted. A folder that doesn't exist is fine.
    fn check(&self, dir: &Path) -> io::Result<()>;
    /// Creates the folder if it's missing and locks it down so that only administrators can write to it. It refuses
    /// what `check` refuses.
    fn prepare(&self, dir: &Path) -> io::Result<()>;
}

/// A folder that is trusted as it is and created as it is, for tests and for running off Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlainFolder;

impl Folder for PlainFolder {
    fn check(&self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }

    fn prepare(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)
    }
}

/// What `JournalFile::append` writes: the entry, led by the version.
#[derive(Serialize)]
struct OnDisk<'a> {
    v: u32,
    #[serde(flatten)]
    entry: &'a Entry,
}

/// An entry as its line in the file, without the newline.
fn line(entry: &Entry) -> io::Result<String> {
    Ok(serde_json::to_string(&OnDisk { v: VERSION, entry })?)
}

/// The journal file in its folder, and the lock that lets one process at a time write to it.
pub struct JournalFile {
    dir: PathBuf,
    folder: Box<dyn Folder>,
}

impl JournalFile {
    /// Touches nothing, so a PC that never pressed Apply has no folder.
    pub fn new(dir: impl Into<PathBuf>, folder: Box<dyn Folder>) -> Self {
        Self {
            dir: dir.into(),
            folder,
        }
    }

    /// Takes the right to write the journal; only one process at a time can hold it (the app, or the uninstaller's
    /// undo). The folder is checked and then prepared, so this is also what creates it on a PC's first Apply. It is
    /// refused with `ErrorKind::WouldBlock` when someone else holds it. Whoever changes anything takes it first and
    /// holds it until done; `append` doesn't take it, and reads never do.
    pub fn lock(&self) -> io::Result<JournalLock> {
        self.folder.check(&self.dir)?;
        self.folder.prepare(&self.dir)?;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join(LOCK))?;
        match file.try_lock() {
            Ok(()) => Ok(JournalLock { file }),
            Err(TryLockError::WouldBlock) => {
                Err(io::Error::new(io::ErrorKind::WouldBlock, ALREADY_CHANGING))
            }
            Err(TryLockError::Error(error)) => Err(error),
        }
    }

    /// Adds one entry as a line of its own, and returns only once the file system has it on disk. The folder is
    /// checked again first: an entry is never added to a file in a folder that isn't trusted.
    pub fn append(&self, entry: &Entry) -> io::Result<()> {
        self.folder.check(&self.dir)?;
        let mut text = line(entry)?;
        text.push('\n');
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(self.dir.join(JOURNAL))?;
        // A crash can leave the start of a line with no newline after it. The next entry must begin a line of its own,
        // or it would be glued to the fragment and lost with it.
        if ends_mid_line(&mut file)? {
            text.insert(0, '\n');
        }
        file.write_all(text.as_bytes())?;
        file.sync_data()
    }

    /// The journal as it stands now. A PC with no journal has an empty one, and nothing is created to find out.
    pub fn read(&self) -> io::Result<Replay> {
        self.folder.check(&self.dir)?;
        match std::fs::read(self.dir.join(JOURNAL)) {
            // Lossy, so one damaged byte costs the line it is in and not the whole journal.
            Ok(bytes) => Ok(replay(&String::from_utf8_lossy(&bytes))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Replay::default()),
            Err(error) => Err(error),
        }
    }
}

/// Whether the file ends part-way through a line.
fn ends_mid_line(file: &mut File) -> io::Result<bool> {
    if file.metadata()?.len() == 0 {
        return Ok(false);
    }
    let mut last = [0u8; 1];
    file.seek(SeekFrom::End(-1))?;
    file.read_exact(&mut last)?;
    Ok(last[0] != b'\n')
}

/// The right to write the journal, held until this is dropped.
#[derive(Debug)]
#[must_use = "the lock is released as soon as this is dropped"]
pub struct JournalLock {
    file: File,
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        // Closing the file would release the lock too, but Windows says that can take a while, and the next lock
        // should not have to wait for it.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{BALANCED, SID};
    use std::sync::{Arc, Mutex};

    const OTHER_SID: &str = "S-1-5-21-1-2-3-1002";
    const NOW: &str = "2026-11-02T21:30:05+01:00";
    const LATER: &str = "2026-11-02T21:30:06+01:00";
    const GAME: &str =
        r"D:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe";
    const KESTREL_PLAN: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";

    // Entries, built the way the engine will build them. Intents are stamped NOW and everything after them LATER, so a
    // test can tell which of the two a record's time came from.

    fn owned(sid: Option<&str>) -> Option<String> {
        sid.map(str::to_string)
    }

    fn dword(value: u32) -> Saved {
        Saved::Dword { value: Some(value) }
    }

    fn absent() -> Saved {
        Saved::Dword { value: None }
    }

    fn token(token: Option<&str>) -> Saved {
        Saved::Token {
            token: token.map(str::to_string),
            value_existed: false,
            key_existed: false,
        }
    }

    fn plan(guid: &str) -> Saved {
        Saved::Plan {
            guid: guid.to_string(),
            name: "Kestrel".to_string(),
        }
    }

    fn intent(tweak: &str, sid: Option<&str>, prior: Saved, planned: Saved) -> Entry {
        Entry::Intent {
            at: NOW.to_string(),
            batch: "batch-1".to_string(),
            tweak: tweak.to_string(),
            sid: owned(sid),
            value_name: None,
            prior,
            planned,
        }
    }

    fn gpu_intent(tweak: &str, value_name: &str) -> Entry {
        Entry::Intent {
            at: NOW.to_string(),
            batch: "batch-1".to_string(),
            tweak: tweak.to_string(),
            sid: owned(Some(SID)),
            value_name: Some(value_name.to_string()),
            prior: token(None),
            planned: token(Some("2")),
        }
    }

    fn outcome(tweak: &str, sid: Option<&str>, applied: bool) -> Entry {
        Entry::Outcome {
            at: LATER.to_string(),
            tweak: tweak.to_string(),
            sid: owned(sid),
            applied,
            detail: None,
            interrupted: false,
        }
    }

    fn reapplied(tweak: &str, sid: Option<&str>) -> Entry {
        Entry::Reapplied {
            at: LATER.to_string(),
            tweak: tweak.to_string(),
            sid: owned(sid),
        }
    }

    fn undone(tweak: &str, sid: Option<&str>) -> Entry {
        Entry::Undone {
            at: LATER.to_string(),
            tweak: tweak.to_string(),
            sid: owned(sid),
            detail: None,
        }
    }

    fn closed(tweak: &str, sid: Option<&str>, reason: CloseReason) -> Entry {
        Entry::Closed {
            at: LATER.to_string(),
            tweak: tweak.to_string(),
            sid: owned(sid),
            reason,
        }
    }

    fn later(mut entry: Entry) -> Entry {
        if let Entry::Intent { at, .. } = &mut entry {
            *at = LATER.to_string();
        }
        entry
    }

    /// One entry of every kind that names a record, about the same tweak and account.
    fn every_kind(tweak: &str, sid: Option<&str>) -> Vec<Entry> {
        vec![
            intent(tweak, sid, absent(), dword(1)),
            outcome(tweak, sid, true),
            reapplied(tweak, sid),
            undone(tweak, sid),
            closed(tweak, sid, CloseReason::WentBack),
        ]
    }

    /// The journal as its file holds it: one line per entry, each ending in a newline.
    fn file(entries: &[Entry]) -> String {
        entries
            .iter()
            .map(|entry| format!("{}\n", line(entry).unwrap()))
            .collect()
    }

    /// True when the journal reads the entry back as the one line it wrote.
    fn is_read(entry: &Entry) -> bool {
        let found = replay(&file(std::slice::from_ref(entry)));
        found.unreadable == 0 && found.history == [entry.clone()]
    }

    /// True when the journal counts the entry's line as unreadable and does nothing with it.
    fn is_refused(entry: &Entry) -> bool {
        let found = replay(&file(std::slice::from_ref(entry)));
        found.unreadable == 1 && found.history.is_empty() && found.open.is_empty()
    }

    fn journal_in(dir: &Path) -> JournalFile {
        JournalFile::new(dir, Box::new(PlainFolder))
    }

    // What a journal makes of its lines.

    #[test]
    fn intent_and_applied_outcome_open_a_settled_record() {
        let entries = [
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
        ];
        let found = replay(&file(&entries));
        assert_eq!(found.unreadable, 0);
        assert_eq!(
            found.open,
            vec![OpenRecord {
                tweak: TweakId::GameMode,
                sid: Some(SID.to_string()),
                value_name: None,
                prior: absent(),
                planned: dword(1),
                at: NOW.to_string(),
                settled: true,
            }]
        );
        assert_eq!(found.history, entries);
    }

    #[test]
    fn intent_without_outcome_is_unsettled() {
        let found = replay(&file(&[intent("game-mode", Some(SID), absent(), dword(1))]));
        assert_eq!(found.open.len(), 1);
        assert!(!found.open[0].settled);
        assert_eq!(found.open[0].prior, absent());
        assert_eq!(found.open[0].planned, dword(1));
    }

    #[test]
    fn failed_outcome_leaves_no_open_record() {
        let entries = [
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), false),
        ];
        let found = replay(&file(&entries));
        assert!(found.open.is_empty());
        assert_eq!(found.history, entries, "history keeps both");
    }

    #[test]
    fn a_second_intent_keeps_the_first_prior() {
        // Applied once; something else changed the value; applied again.
        let first = intent("game-mode", Some(SID), absent(), dword(1));
        let second = later(intent("game-mode", Some(SID), dword(5), dword(1)));
        let waiting = replay(&file(&[
            first.clone(),
            outcome("game-mode", Some(SID), true),
            second.clone(),
        ]));
        assert_eq!(waiting.open.len(), 1);
        assert_eq!(waiting.open[0].prior, absent(), "the first prior");
        assert_eq!(
            waiting.open[0].at, NOW,
            "the record is as old as the first change"
        );
        assert!(!waiting.open[0].settled, "until the second has its outcome");

        let done = replay(&file(&[
            first,
            outcome("game-mode", Some(SID), true),
            second,
            outcome("game-mode", Some(SID), true),
        ]));
        assert_eq!(done.open.len(), 1);
        assert_eq!(done.open[0].prior, absent());
        assert!(done.open[0].settled);
    }

    #[test]
    fn a_failed_second_attempt_leaves_the_first_record_standing() {
        let entries = [
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
            later(intent("game-mode", Some(SID), dword(5), dword(1))),
            outcome("game-mode", Some(SID), false),
        ];
        let found = replay(&file(&entries));
        assert_eq!(found.open.len(), 1, "the first change still needs its undo");
        assert_eq!(found.open[0].prior, absent());
        assert!(found.open[0].settled, "as it was before the failed attempt");
        assert_eq!(found.history, entries);
    }

    #[test]
    fn a_failed_attempt_never_drops_a_change_that_may_have_happened() {
        // The first attempt has no outcome (a crash after its write, maybe), so the second one's failure can't
        // be taken to mean that nothing was ever changed.
        let found = replay(&file(&[
            intent("game-mode", Some(SID), absent(), dword(1)),
            later(intent("game-mode", Some(SID), dword(1), dword(1))),
            outcome("game-mode", Some(SID), false),
        ]));
        assert_eq!(found.open.len(), 1);
        assert_eq!(found.open[0].prior, absent());
        assert!(!found.open[0].settled, "still waiting to be judged");
    }

    #[test]
    fn reapplied_settles_an_open_record_and_only_that() {
        let waiting = intent("game-mode", Some(SID), absent(), dword(1));
        let found = replay(&file(&[waiting.clone(), reapplied("game-mode", Some(SID))]));
        assert_eq!(found.open.len(), 1);
        assert!(found.open[0].settled);
        assert_eq!(found.open[0].prior, absent());

        // A different account's record is not the one that was applied again.
        let other = replay(&file(&[waiting, reapplied("game-mode", Some(OTHER_SID))]));
        assert!(!other.open[0].settled);
    }

    #[test]
    fn entries_about_nothing_open_change_nothing() {
        let entries = [
            outcome("game-mode", Some(SID), true),
            reapplied("game-mode", Some(SID)),
            undone("game-mode", Some(SID)),
            closed("game-mode", Some(SID), CloseReason::NotApplied),
        ];
        let found = replay(&file(&entries));
        assert!(found.open.is_empty());
        assert_eq!(found.history, entries, "they are still part of the story");
        assert_eq!(found.unreadable, 0);
    }

    #[test]
    fn undone_and_closed_end_the_record() {
        let endings = [
            undone("game-mode", Some(SID)),
            closed("game-mode", Some(SID), CloseReason::WentBack),
            closed("game-mode", Some(SID), CloseReason::PlanRemovedElsewhere),
            closed("game-mode", Some(SID), CloseReason::NotApplied),
        ];
        for ending in endings {
            let entries = [
                intent("game-mode", Some(SID), absent(), dword(1)),
                outcome("game-mode", Some(SID), true),
                ending,
            ];
            let found = replay(&file(&entries));
            assert!(found.open.is_empty(), "{:?}", entries[2]);
            assert_eq!(found.history, entries, "history keeps it: {:?}", entries[2]);
        }
    }

    #[test]
    fn records_are_per_tweak_and_account() {
        let entries = [
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
            intent("game-mode", Some(OTHER_SID), dword(0), dword(1)),
            outcome("game-mode", Some(OTHER_SID), true),
            intent("background-recording", Some(SID), dword(1), dword(0)),
            outcome("background-recording", Some(SID), true),
            // Machine-wide: no account.
            intent("power-plan", None, plan(BALANCED), plan(KESTREL_PLAN)),
            outcome("power-plan", None, true),
        ];
        let keys = |found: &Replay| -> Vec<(TweakId, Option<String>)> {
            found
                .open
                .iter()
                .map(|r| (r.tweak, r.sid.clone()))
                .collect()
        };
        let found = replay(&file(&entries));
        assert_eq!(
            keys(&found),
            vec![
                (TweakId::GameMode, Some(SID.to_string())),
                (TweakId::GameMode, Some(OTHER_SID.to_string())),
                (TweakId::BackgroundRecording, Some(SID.to_string())),
                (TweakId::PowerPlan, None),
            ]
        );
        assert_eq!(found.open[0].prior, absent());
        assert_eq!(found.open[1].prior, dword(0));
        assert!(found.open.iter().all(|r| r.settled));

        // Ending one account's record leaves the same tweak's record for the other account.
        let mut ended = entries.to_vec();
        ended.push(undone("game-mode", Some(OTHER_SID)));
        let found = replay(&file(&ended));
        assert_eq!(
            keys(&found),
            vec![
                (TweakId::GameMode, Some(SID.to_string())),
                (TweakId::BackgroundRecording, Some(SID.to_string())),
                (TweakId::PowerPlan, None),
            ]
        );
    }

    #[test]
    fn open_records_are_oldest_first_and_a_new_start_goes_last() {
        let mut entries = vec![
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
            intent("background-recording", Some(SID), dword(1), dword(0)),
            outcome("background-recording", Some(SID), true),
            intent("pointer-precision", Some(SID), absent(), dword(1)),
            outcome("pointer-precision", Some(SID), true),
        ];
        let tweaks = |entries: &[Entry]| -> Vec<TweakId> {
            replay(&file(entries))
                .open
                .iter()
                .map(|r| r.tweak)
                .collect()
        };
        assert_eq!(
            tweaks(&entries),
            [
                TweakId::GameMode,
                TweakId::BackgroundRecording,
                TweakId::PointerPrecision
            ]
        );
        // Undone, then applied again: a fresh record with its own prior, and the newest.
        entries.push(undone("game-mode", Some(SID)));
        entries.push(intent("game-mode", Some(SID), dword(0), dword(1)));
        entries.push(outcome("game-mode", Some(SID), true));
        assert_eq!(
            tweaks(&entries),
            [
                TweakId::BackgroundRecording,
                TweakId::PointerPrecision,
                TweakId::GameMode
            ]
        );
        assert_eq!(replay(&file(&entries)).open[2].prior, dword(0));
    }

    #[test]
    fn restore_points_are_history_and_nothing_else() {
        let outcomes = [
            RestoreOutcome::Created { sequence: 12 },
            RestoreOutcome::Reused {
                sequence: 11,
                time: "2026-11-02 18:00".to_string(),
            },
            RestoreOutcome::Skipped,
            RestoreOutcome::Failed {
                reason: "Windows said no".to_string(),
            },
        ];
        let mut entries = vec![
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
        ];
        entries.extend(outcomes.into_iter().map(|result| Entry::RestorePoint {
            at: NOW.to_string(),
            outcome: result,
        }));
        let found = replay(&file(&entries));
        assert_eq!(found.history, entries);
        assert_eq!(found.open.len(), 1);
        assert!(found.open[0].settled);
        assert_eq!(found.unreadable, 0);
    }

    // Lines that can't be read.

    #[test]
    fn a_truncated_last_line_is_ignored_silently() {
        let whole = file(&[
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
        ]);
        let complete = replay(&whole);
        let found = replay(&format!(r#"{whole}{{"v":1,"kind":"inte"#));
        assert_eq!(found.unreadable, 0);
        assert_eq!(found.open, complete.open);
        assert_eq!(found.history, complete.history);
        // Alone, it is just as silent.
        assert_eq!(replay(r#"{"v":1,"kind":"inte"#), Replay::default());
    }

    #[test]
    fn a_last_line_that_is_whole_json_is_read_even_without_its_newline() {
        // The writer was cut off between the closing brace and the newline.
        let text = file(&[intent("game-mode", Some(SID), absent(), dword(1))]);
        let found = replay(text.trim_end());
        assert_eq!(found.unreadable, 0);
        assert_eq!(found.open.len(), 1);
        // If it is whole JSON but not an entry Kestrel could trust, it is counted: it isn't a torn write.
        for last in ["[]", "123", "null", r#"{"v":1,"kind":"format-c"}"#] {
            assert_eq!(replay(last).unreadable, 1, "{last}");
        }
    }

    #[test]
    fn a_garbled_last_line_that_ends_in_a_newline_is_counted() {
        let text = format!(
            "{}{{\"v\":1,\"kind\":\"inte\n",
            file(&[intent("game-mode", Some(SID), absent(), dword(1))])
        );
        let found = replay(&text);
        assert_eq!(
            found.unreadable, 1,
            "it was finished, so it is not a torn write"
        );
        assert_eq!(found.open.len(), 1);
    }

    #[test]
    fn an_unreadable_middle_line_is_counted() {
        let first = intent("game-mode", Some(SID), absent(), dword(1));
        let last = outcome("game-mode", Some(SID), true);
        let text = format!(
            "{}\nthis is not json\n{}\n",
            line(&first).unwrap(),
            line(&last).unwrap()
        );
        let found = replay(&text);
        assert_eq!(found.unreadable, 1);
        assert_eq!(
            found.history,
            [first, last],
            "the lines either side were read"
        );
        assert!(found.open[0].settled);
    }

    #[test]
    fn blank_lines_and_windows_line_endings_are_fine() {
        let entries = [
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
        ];
        let plain = replay(&file(&entries));
        let text = format!("\n  \n{}\n\t\n", file(&entries).replace('\n', "\r\n"));
        let found = replay(&text);
        assert_eq!(found.unreadable, 0);
        assert_eq!(found.history, plain.history);
        assert_eq!(found.open, plain.open);
    }

    #[test]
    fn an_unknown_version_is_refused() {
        let valid = line(&closed("game-mode", Some(SID), CloseReason::WentBack)).unwrap();
        assert!(valid.starts_with(r#"{"v":1,"#), "{valid}");
        for v in [
            r#""v":2"#,
            r#""v":0"#,
            r#""v":"1""#,
            r#""v":1.0"#,
            r#""v":null"#,
            r#""v":true"#,
            r#""w":1"#,
        ] {
            let line = valid.replacen(r#""v":1"#, v, 1);
            assert_eq!(replay(&format!("{line}\n")).unreadable, 1, "{line}");
            assert!(replay(&format!("{line}\n")).history.is_empty(), "{line}");
        }
        // No version at all.
        let bare = valid.replacen(r#""v":1,"#, "", 1);
        assert_eq!(replay(&format!("{bare}\n")).unreadable, 1, "{bare}");
    }

    #[test]
    fn an_unknown_kind_is_refused() {
        let valid = line(&closed("game-mode", Some(SID), CloseReason::WentBack)).unwrap();
        for kind in [
            r#""kind":"format""#,
            r#""kind":"Closed""#,
            r#""kind":3"#,
            r#""kind":null"#,
        ] {
            let line = valid.replacen(r#""kind":"closed""#, kind, 1);
            assert_eq!(replay(&format!("{line}\n")).unreadable, 1, "{line}");
        }
        let bare = valid.replacen(r#""kind":"closed","#, "", 1);
        assert_eq!(replay(&format!("{bare}\n")).unreadable, 1, "{bare}");
        // And a reason that isn't one.
        let reason = valid.replacen("went-back", "format-c", 1);
        assert_eq!(replay(&format!("{reason}\n")).unreadable, 1, "{reason}");
    }

    #[test]
    fn extra_fields_are_ignored() {
        // A later Kestrel may add a field without a new version; this one still reads the line.
        let valid = line(&closed("game-mode", Some(SID), CloseReason::WentBack)).unwrap();
        let extended = valid.replacen('{', r#"{"note":"added later","#, 1);
        let found = replay(&format!("{extended}\n"));
        assert_eq!(found.unreadable, 0);
        assert_eq!(found.history.len(), 1);
    }

    #[test]
    fn an_unknown_tweak_is_refused() {
        for tweak in ["format-c", "Game-Mode", "game_mode", "game-mode ", ""] {
            for entry in every_kind(tweak, Some(SID)) {
                assert!(is_refused(&entry), "{tweak:?}: {entry:?}");
            }
        }
        // Every tweak is known by the name it has everywhere else.
        for id in TweakId::ESSENTIAL
            .into_iter()
            .chain([TweakId::SystemProtection])
        {
            assert!(is_read(&outcome(id.as_str(), Some(SID), true)), "{id:?}");
        }
    }

    #[test]
    fn an_odd_sid_is_refused() {
        for odd in [
            r"S-1-5-21-1\..\Software",
            "S-1-5-21-1/../x",
            "s-1-5-21-1",
            "S-1-5-21-1 ",
            " S-1-5-21-1",
            "S-1-5-21-1\n",
            "S-",
            "S-1-",
            "S--1",
            "S-1--5",
            "S-1-5-21-x",
            "S-1-5-21-0x10",
            "S-1-5-21-１",
            "S-١-٥",
            "1-5-21-1",
            "S1-5-21-1",
            ".DEFAULT",
            "",
        ] {
            for entry in every_kind("game-mode", Some(odd)) {
                assert!(is_refused(&entry), "{odd:?}: {entry:?}");
            }
        }
        for fine in [
            SID,
            OTHER_SID,
            "S-1-5-18",
            "S-1-5-32-544",
            "S-1-12-1-1-2-3-4",
        ] {
            for entry in every_kind("game-mode", Some(fine)) {
                assert!(is_read(&entry), "{fine}: {entry:?}");
            }
        }
        // No account is how a machine-wide change is written.
        for entry in [
            intent("power-plan", None, plan(BALANCED), plan(KESTREL_PLAN)),
            outcome("power-plan", None, true),
            reapplied("power-plan", None),
            undone("power-plan", None),
            closed("power-plan", None, CloseReason::PlanRemovedElsewhere),
        ] {
            assert!(is_read(&entry), "{entry:?}");
        }
    }

    #[test]
    fn a_gpu_value_name_must_be_the_game() {
        let lower = GAME.to_lowercase();
        let slashes = GAME.replace('\\', "/");
        for fine in [GAME, lower.as_str(), slashes.as_str()] {
            assert!(is_read(&gpu_intent("gpu-preference", fine)), "{fine}");
        }
        let smuggled = "DirectXUserGlobalSettings\0\\VALORANT-Win64-Shipping.exe";
        let newline = "D:\\x\n\\VALORANT-Win64-Shipping.exe";
        for odd in [
            r"C:\evil.exe",
            "DirectXUserGlobalSettings",
            "VALORANT-Win64-Shipping.exe",
            r"D:\Riot Games\VALORANT-Win64-Shipping.exe.bak",
            r"D:\Riot Games\XVALORANT-Win64-Shipping.exe",
            r"D:\Riot Games\VALORANT-Win64-Shipping.exe\..\..\x",
            r"D:\Riot Games\VALORANT-Win64-Shipping.exe ",
            smuggled,
            newline,
            "",
        ] {
            assert!(is_refused(&gpu_intent("gpu-preference", odd)), "{odd:?}");
        }
        // Only the GPU preference has a value named by a path.
        let others = TweakId::ESSENTIAL
            .into_iter()
            .chain([TweakId::SystemProtection])
            .filter(|id| *id != TweakId::GpuPreference);
        for id in others {
            assert!(is_refused(&gpu_intent(id.as_str(), GAME)), "{id:?}");
        }
    }

    #[test]
    fn a_saved_plan_guid_must_be_a_guid() {
        let no_dashes = KESTREL_PLAN.replace('-', "");
        let braces = format!("{{{KESTREL_PLAN}}}");
        let not_hex = KESTREL_PLAN.replace('f', "g");
        let longer = format!("{KESTREL_PLAN}0");
        let shorter = &KESTREL_PLAN[1..];
        let moved = "4b4553540-24c-4e50-a1b2-c3d4e5f60705";
        for odd in [
            r"..\x",
            "",
            "Kestrel",
            no_dashes.as_str(),
            braces.as_str(),
            not_hex.as_str(),
            longer.as_str(),
            shorter,
            moved,
        ] {
            assert!(
                is_refused(&intent("power-plan", None, plan(odd), plan(KESTREL_PLAN))),
                "prior {odd:?}"
            );
            assert!(
                is_refused(&intent("power-plan", None, plan(BALANCED), plan(odd))),
                "planned {odd:?}"
            );
        }
        let upper = KESTREL_PLAN.to_uppercase();
        for fine in [KESTREL_PLAN, BALANCED, upper.as_str()] {
            assert!(
                is_read(&intent("power-plan", None, plan(fine), plan(KESTREL_PLAN))),
                "{fine}"
            );
        }
    }

    #[test]
    fn a_refused_line_changes_nothing() {
        let good = [
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
        ];
        // Each would end, settle or drop the record if it were believed.
        let sneaky = [
            undone("game-mode", Some(r"S-1-5-21-1\..\x")),
            closed("format-c", Some(SID), CloseReason::WentBack),
            outcome("game-mode", Some("S-1-5-21-1 "), false),
            gpu_intent("game-mode", GAME),
        ];
        let mut entries = good.to_vec();
        entries.extend(sneaky.iter().cloned());
        let found = replay(&file(&entries));
        assert_eq!(found.unreadable, 4);
        assert_eq!(found.history, good);
        assert_eq!(found.open.len(), 1);
        assert!(found.open[0].settled);
        assert_eq!(found.open[0].prior, absent());
    }

    #[test]
    fn a_bad_byte_costs_one_line_not_the_journal() {
        let root = tempfile::tempdir().unwrap();
        let first = line(&intent("game-mode", Some(SID), absent(), dword(1))).unwrap();
        let last = line(&outcome("game-mode", Some(SID), true)).unwrap();
        let mut bytes = format!("{first}\n").into_bytes();
        bytes.extend_from_slice(b"{\"v\":1,\"kind\":\"closed\",\xFF\xFE}\n");
        bytes.extend_from_slice(format!("{last}\n").as_bytes());
        std::fs::write(root.path().join("journal.jsonl"), bytes).unwrap();
        let found = journal_in(root.path()).read().unwrap();
        assert_eq!(found.unreadable, 1);
        assert_eq!(found.history.len(), 2);
        assert!(found.open[0].settled);
    }

    // What is written.

    #[test]
    fn every_kind_has_the_shape_the_journal_documents() {
        let golden = |text: &str| {
            text.replace("@NOW", NOW)
                .replace("@LATER", LATER)
                .replace("@SID", SID)
        };
        let mouse =
            |speed: &str, threshold1: &str, threshold2: &str, live: [i32; 3]| Saved::Mouse {
                speed: Some(speed.to_string()),
                threshold1: Some(threshold1.to_string()),
                threshold2: Some(threshold2.to_string()),
                live,
            };
        let cases = [
            (
                Entry::Intent {
                    at: NOW.to_string(),
                    batch: "batch-1".to_string(),
                    tweak: "gpu-preference".to_string(),
                    sid: Some(SID.to_string()),
                    value_name: Some(r"C:\Games\VALORANT-Win64-Shipping.exe".to_string()),
                    prior: token(None),
                    planned: token(Some("2")),
                },
                r#"{"v":1,"kind":"intent","at":"@NOW","batch":"batch-1","tweak":"gpu-preference","sid":"@SID","value_name":"C:\\Games\\VALORANT-Win64-Shipping.exe","prior":{"type":"token","token":null,"value_existed":false,"key_existed":false},"planned":{"type":"token","token":"2","value_existed":false,"key_existed":false}}"#,
            ),
            (
                intent(
                    "power-plan",
                    None,
                    Saved::Plan {
                        guid: BALANCED.to_string(),
                        name: "Balanced".to_string(),
                    },
                    plan(KESTREL_PLAN),
                ),
                r#"{"v":1,"kind":"intent","at":"@NOW","batch":"batch-1","tweak":"power-plan","sid":null,"value_name":null,"prior":{"type":"plan","guid":"381b4222-f694-41f0-9685-ff5bb260df2e","name":"Balanced"},"planned":{"type":"plan","guid":"4b455354-524c-4e50-a1b2-c3d4e5f60705","name":"Kestrel"}}"#,
            ),
            (
                intent(
                    "pointer-precision",
                    Some(SID),
                    mouse("1", "6", "10", [6, 10, 1]),
                    mouse("0", "0", "0", [0, 0, 0]),
                ),
                r#"{"v":1,"kind":"intent","at":"@NOW","batch":"batch-1","tweak":"pointer-precision","sid":"@SID","value_name":null,"prior":{"type":"mouse","speed":"1","threshold1":"6","threshold2":"10","live":[6,10,1]},"planned":{"type":"mouse","speed":"0","threshold1":"0","threshold2":"0","live":[0,0,0]}}"#,
            ),
            (
                intent("game-mode", Some(SID), absent(), dword(1)),
                r#"{"v":1,"kind":"intent","at":"@NOW","batch":"batch-1","tweak":"game-mode","sid":"@SID","value_name":null,"prior":{"type":"dword","value":null},"planned":{"type":"dword","value":1}}"#,
            ),
            (
                intent(
                    "system-protection",
                    None,
                    Saved::Protection { on: false },
                    Saved::Protection { on: true },
                ),
                r#"{"v":1,"kind":"intent","at":"@NOW","batch":"batch-1","tweak":"system-protection","sid":null,"value_name":null,"prior":{"type":"protection","on":false},"planned":{"type":"protection","on":true}}"#,
            ),
            (
                outcome("game-mode", Some(SID), true),
                r#"{"v":1,"kind":"outcome","at":"@LATER","tweak":"game-mode","sid":"@SID","applied":true,"detail":null,"interrupted":false}"#,
            ),
            (
                Entry::Outcome {
                    at: LATER.to_string(),
                    tweak: "game-mode".to_string(),
                    sid: Some(SID.to_string()),
                    applied: false,
                    detail: Some("Windows refused.".to_string()),
                    interrupted: true,
                },
                r#"{"v":1,"kind":"outcome","at":"@LATER","tweak":"game-mode","sid":"@SID","applied":false,"detail":"Windows refused.","interrupted":true}"#,
            ),
            (
                reapplied("game-mode", Some(SID)),
                r#"{"v":1,"kind":"reapplied","at":"@LATER","tweak":"game-mode","sid":"@SID"}"#,
            ),
            (
                undone("game-mode", Some(SID)),
                r#"{"v":1,"kind":"undone","at":"@LATER","tweak":"game-mode","sid":"@SID","detail":null}"#,
            ),
            (
                closed("game-mode", Some(SID), CloseReason::WentBack),
                r#"{"v":1,"kind":"closed","at":"@LATER","tweak":"game-mode","sid":"@SID","reason":"went-back"}"#,
            ),
            (
                closed("power-plan", None, CloseReason::PlanRemovedElsewhere),
                r#"{"v":1,"kind":"closed","at":"@LATER","tweak":"power-plan","sid":null,"reason":"plan-removed-elsewhere"}"#,
            ),
            (
                closed("game-mode", Some(SID), CloseReason::NotApplied),
                r#"{"v":1,"kind":"closed","at":"@LATER","tweak":"game-mode","sid":"@SID","reason":"not-applied"}"#,
            ),
            (
                Entry::RestorePoint {
                    at: NOW.to_string(),
                    outcome: RestoreOutcome::Created { sequence: 7 },
                },
                r#"{"v":1,"kind":"restore-point","at":"@NOW","outcome":{"result":"created","sequence":7}}"#,
            ),
            (
                Entry::RestorePoint {
                    at: NOW.to_string(),
                    outcome: RestoreOutcome::Reused {
                        sequence: 7,
                        time: "2026-11-02 18:00".to_string(),
                    },
                },
                r#"{"v":1,"kind":"restore-point","at":"@NOW","outcome":{"result":"reused","sequence":7,"time":"2026-11-02 18:00"}}"#,
            ),
            (
                Entry::RestorePoint {
                    at: NOW.to_string(),
                    outcome: RestoreOutcome::Skipped,
                },
                r#"{"v":1,"kind":"restore-point","at":"@NOW","outcome":{"result":"skipped"}}"#,
            ),
            (
                Entry::RestorePoint {
                    at: NOW.to_string(),
                    outcome: RestoreOutcome::Failed {
                        reason: "Windows said no".to_string(),
                    },
                },
                r#"{"v":1,"kind":"restore-point","at":"@NOW","outcome":{"result":"failed","reason":"Windows said no"}}"#,
            ),
        ];
        for (entry, text) in cases {
            let text = golden(text);
            assert_eq!(line(&entry).unwrap(), text);
            let found = replay(&text);
            assert_eq!(found.unreadable, 0, "{text}");
            assert_eq!(found.history, [entry], "{text}");
        }
    }

    // The file.

    #[test]
    fn append_then_read_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Kestrel");
        let journal = JournalFile::new(&dir, Box::new(PlainFolder));
        let entries = [
            intent("game-mode", Some(SID), absent(), dword(1)),
            outcome("game-mode", Some(SID), true),
            intent("power-plan", None, plan(BALANCED), plan(KESTREL_PLAN)),
            Entry::RestorePoint {
                at: NOW.to_string(),
                outcome: RestoreOutcome::Skipped,
            },
            outcome("power-plan", None, true),
        ];
        let _lock = journal.lock().unwrap();
        for entry in &entries {
            journal.append(entry).unwrap();
        }
        let found = journal.read().unwrap();
        assert_eq!(found.unreadable, 0);
        assert_eq!(found.history, entries);
        assert_eq!(found.open.len(), 2);
        assert!(found.open.iter().all(|record| record.settled));
        // On disk it is one line per entry, each ending in a newline, in the folder's journal.jsonl.
        let text = std::fs::read_to_string(dir.join("journal.jsonl")).unwrap();
        assert_eq!(text, file(&entries));
        assert_eq!(text.lines().count(), entries.len());
    }

    #[test]
    fn a_new_entry_is_never_glued_to_a_torn_line() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.jsonl");
        let first = intent("game-mode", Some(SID), absent(), dword(1));
        let next = outcome("game-mode", Some(SID), true);
        let journal = journal_in(root.path());

        // A crash left the start of a line.
        let whole = line(&next).unwrap();
        std::fs::write(
            &path,
            format!("{}\n{}", line(&first).unwrap(), &whole[..20]),
        )
        .unwrap();
        journal.append(&next).unwrap();
        let found = journal.read().unwrap();
        assert!(found.open[0].settled, "the new entry reads");
        assert_eq!(found.history, [first.clone(), next.clone()]);
        assert_eq!(
            found.unreadable, 1,
            "the fragment is a line of its own now, and counted"
        );

        // A crash left a whole line without its newline: nothing is lost and nothing is counted.
        std::fs::write(&path, line(&first).unwrap()).unwrap();
        journal.append(&next).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            file(&[first.clone(), next.clone()])
        );
        let found = journal.read().unwrap();
        assert_eq!(found.unreadable, 0);
        assert_eq!(found.history, [first, next]);
    }

    #[test]
    fn reading_a_missing_journal_creates_nothing() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Kestrel");
        let journal = JournalFile::new(&dir, Box::new(PlainFolder));
        assert!(
            !dir.exists(),
            "new touches nothing: a PC that never pressed Apply has no folder"
        );
        assert_eq!(journal.read().unwrap(), Replay::default());
        assert!(!dir.exists(), "and reading doesn't make one");

        // A folder with no journal in it stays as empty as it was.
        std::fs::create_dir(&dir).unwrap();
        assert_eq!(journal.read().unwrap(), Replay::default());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn a_second_lock_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let first = journal_in(root.path());
        let second = journal_in(root.path());
        let held = first.lock().unwrap();
        for refused in [second.lock().unwrap_err(), first.lock().unwrap_err()] {
            assert_eq!(refused.kind(), io::ErrorKind::WouldBlock);
            assert_eq!(
                refused.to_string(),
                "Kestrel is already changing settings in another window."
            );
        }
        drop(held);
        let taken = second.lock().unwrap();
        assert_eq!(first.lock().unwrap_err().kind(), io::ErrorKind::WouldBlock);
        drop(taken);
        drop(first.lock().unwrap());
        assert!(root.path().join("journal.lock").is_file());
    }

    #[test]
    fn a_lock_file_left_by_a_crash_does_not_hold_the_lock() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("journal.lock"), "left behind").unwrap();
        drop(journal_in(root.path()).lock().unwrap());
    }

    #[test]
    fn reading_takes_no_lock_and_waits_for_none() {
        let root = tempfile::tempdir().unwrap();
        let writer = journal_in(root.path());
        let reader = journal_in(root.path());
        let held = writer.lock().unwrap();
        writer
            .append(&intent("game-mode", Some(SID), absent(), dword(1)))
            .unwrap();
        assert_eq!(reader.read().unwrap().open.len(), 1);
        assert_eq!(reader.lock().unwrap_err().kind(), io::ErrorKind::WouldBlock);
        drop(held);
        drop(reader.lock().unwrap());
    }

    /// A folder that refuses to be trusted, as one a standard user created beforehand would be.
    struct Untrusted;

    impl Folder for Untrusted {
        fn check(&self, _dir: &Path) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "C:\\ProgramData\\Kestrel belongs to another account.",
            ))
        }

        fn prepare(&self, dir: &Path) -> io::Result<()> {
            std::fs::create_dir_all(dir)
        }
    }

    #[test]
    fn an_untrusted_folder_refuses_reads_and_locks() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Kestrel");
        let journal = JournalFile::new(&dir, Box::new(Untrusted));
        for err in [journal.read().unwrap_err(), journal.lock().unwrap_err()] {
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(
                err.to_string(),
                "C:\\ProgramData\\Kestrel belongs to another account."
            );
        }
        assert!(!dir.exists(), "refused before anything was prepared");

        // With a journal already in it, the folder is still not believed.
        std::fs::create_dir(&dir).unwrap();
        let entry = intent("game-mode", Some(SID), absent(), dword(1));
        let path = dir.join("journal.jsonl");
        std::fs::write(&path, file(std::slice::from_ref(&entry))).unwrap();
        assert!(journal.read().is_err());
        assert!(journal.lock().is_err());
        assert!(!dir.join("journal.lock").exists());
        // And nothing is added to it.
        assert!(journal
            .append(&outcome("game-mode", Some(SID), true))
            .is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            file(std::slice::from_ref(&entry))
        );
    }

    /// A folder that remembers what it was asked, in order.
    #[derive(Clone, Default)]
    struct Recording(Arc<Mutex<Vec<&'static str>>>);

    impl Recording {
        fn calls(&self) -> Vec<&'static str> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    impl Folder for Recording {
        fn check(&self, _dir: &Path) -> io::Result<()> {
            self.0.lock().unwrap().push("check");
            Ok(())
        }

        fn prepare(&self, dir: &Path) -> io::Result<()> {
            self.0.lock().unwrap().push("prepare");
            std::fs::create_dir_all(dir)
        }
    }

    #[test]
    fn the_folder_is_checked_before_it_is_prepared_and_only_a_lock_prepares_it() {
        let root = tempfile::tempdir().unwrap();
        let folder = Recording::default();
        let journal = JournalFile::new(root.path().join("Kestrel"), Box::new(folder.clone()));
        assert_eq!(folder.calls(), Vec::<&str>::new(), "new asks nothing");
        journal.read().unwrap();
        assert_eq!(folder.calls(), ["check"], "a read only checks");
        let held = journal.lock().unwrap();
        assert_eq!(folder.calls(), ["check", "prepare"]);
        drop(held);
        drop(journal.lock().unwrap());
        assert_eq!(
            folder.calls(),
            ["check", "prepare"],
            "each lock prepares once"
        );
    }

    #[test]
    fn the_plain_folder_trusts_what_is_there_and_creates_what_is_not() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("a").join("b");
        PlainFolder.check(&nested).unwrap();
        assert!(!nested.exists(), "checking creates nothing");
        PlainFolder.prepare(&nested).unwrap();
        assert!(nested.is_dir());
        PlainFolder.check(&nested).unwrap();
        PlainFolder.prepare(&nested).unwrap();
    }

    #[test]
    fn the_journal_can_be_shared_between_threads() {
        fn shared<T: Send + Sync>() {}
        fn sendable<T: Send>() {}
        shared::<JournalFile>();
        sendable::<JournalLock>();
    }
}
