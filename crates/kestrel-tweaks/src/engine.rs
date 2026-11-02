//! The engine: what Apply and the undo buttons do, in the order that keeps every change undoable, and what is made of
//! the journal afterwards, crash or no crash.
//!
//! Every change goes the same way, under the journal's lock: read what holds now, and stop there if the read fails;
//! append and flush an intent saying what was found and what is about to be written; write to Windows; read it back;
//! add the outcome. The intent is the only entry that has to reach the file before anything else happens, and if it
//! can't, nothing is changed. Whatever comes after it can be lost to a crash or a failing disk at no cost, because the
//! intent says what to look for: the next look at the journal compares the setting with it and settles the record.
//!
//! Every open record is judged in one way, by what its setting holds now against what Kestrel wrote and what it found
//! (see `Standing`). `view`, `applied_ids` and `has_open_records` start by doing that, and Undo decides by it. A look
//! at the journal can add to the journal and never touches Windows, so nothing changes before someone presses a
//! button, and Kestrel never re-applies by itself.
//!
//! A record belongs to the signed-in account or, with no SID, to the whole machine (the power plan, System Protection).
//! Records of any other account wait until that account is at the screen: they are counted, and never judged or
//! touched.
//!
//! The journal is a file, so a record can say anything its lines' checks let through. Whatever doesn't fit its tweak is
//! refused further down (`targets::restore`, `power::undo`) or here, and comes back as an item that failed.
use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kestrel_core::tweak_id::TweakId;
use serde::{Deserialize, Serialize};

use crate::catalog::{info, Timing};
use crate::journal::{CloseReason, Entry, JournalFile, OpenRecord, Replay, Saved};
use crate::plan::{self, PlanInput, PreviewItem, Row, RowState, ValueChange};
use crate::power::{self, KESTREL_PLAN_GUID, KESTREL_PLAN_NAME};
use crate::report;
use crate::system::{Read, Readiness, System};
use crate::targets;

const APPLIED: &str = "Applied.";
const UNDONE: &str = "Undone.";
const ALREADY_SET: &str = "Already set.";
const LEFT_ALONE: &str = "Something else changed this after Kestrel did. Kestrel left it alone.";
const ALREADY_BACK: &str = "This was already back as it was, so there was nothing to undo.";
const PLAN_ALREADY_GONE: &str = "Kestrel's plan was already gone, so there was nothing to undo.";
const NOT_ON_THIS_PC: &str = "Kestrel doesn't change this on this PC.";
const NOTHING_TO_UNDO: &str = "Kestrel hasn't changed this, so there is nothing to undo.";
const NOTHING_TO_APPLY_AGAIN: &str =
    "Kestrel hasn't changed this, so there is nothing to apply again.";
const ONLY_THE_SETTINGS: &str =
    "Restore anyway is only for a setting something else changed. Kestrel changed nothing.";
const NOT_KEPT: &str = "Windows didn't keep the change.";
const OLD_VALUE_NOT_KEPT: &str = "Windows didn't keep the old value.";
const PLAN_NOT_SWITCHED: &str = "Windows didn't switch to Kestrel's plan.";
const NO_OWNER: &str = "The journal doesn't say whose setting this is, so Kestrel left it alone.";
const PROTECTION_ON: &str = "System Protection is on because Kestrel turned it on. Turning it off deletes this drive's restore points.";
const PROTECTION_STAYS: &str =
    "System Protection stays on: turning it off deletes this drive's restore points.";
const PROTECTION_UNKNOWN: &str =
    "Couldn't tell whether System Protection is on, so Kestrel left it alone.";
const PROTECTION_NOT_KESTRELS: &str =
    "The journal doesn't say System Protection was off before Kestrel, so Kestrel left it on.";
const PROTECTION_STILL_ON: &str =
    "Windows said it turned System Protection off, and it's still on.";
const PROTECTION_NOT_CONFIRMED: &str =
    "Kestrel asked Windows to turn System Protection off, and couldn't tell whether it did.";

/// Whether Apply and Undo may run now, and if not, what they wait for. The app decides; the view only carries it to the
/// page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gate {
    Open,
    GameRunning,
    BenchmarkActive,
}

/// What the Tweaks tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TweaksView {
    /// The Essential rows in display order, and System Protection's while its record is open.
    pub rows: Vec<Row>,
    pub gate: Gate,
    /// Kestrel runs as another account than the one at the screen.
    pub other_account: bool,
    /// Open records of accounts that aren't at the screen. They wait.
    pub pending_other_accounts: u32,
    /// What happened to each change a crash cut short, as this run of the app has found out.
    pub interrupted: Vec<String>,
    /// Lines of the journal that couldn't be believed.
    pub unreadable_lines: u32,
    /// There is something for Revert everything to do: an open record of this account or the machine, other than
    /// System Protection, which it leaves alone.
    pub any_active: bool,
}

/// What the Preview shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewView {
    /// What Apply would change, leaving out any row that isn't Available.
    pub items: Vec<PreviewItem>,
    /// Whether Kestrel can ask Windows for a restore point first.
    pub restore: Readiness,
}

/// How one item went. `message` is what to tell the person: on a failure, the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemOutcome {
    pub id: TweakId,
    pub title: String,
    pub ok: bool,
    pub message: String,
}

/// Where a record's setting stands now, which decides everything done with the record.
enum Standing {
    /// It holds what Kestrel wrote. For the plan: Kestrel's plan is the one in use.
    Holds,
    /// It is back at what Kestrel found, so nothing of Kestrel's is left (see `targets::is_back_at`). For the plan:
    /// Kestrel's plan is gone.
    Back,
    /// Neither of those. For the plan: Kestrel's plan is on the PC and another is in use.
    Elsewhere,
    /// It couldn't be read, and this says why in a sentence. Nothing is ever decided on a failed read.
    Unknown(String),
}

/// Which button asked for a change.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asked {
    Apply,
    /// The only one that goes over a value something else has put there.
    ApplyAgain,
}

/// One change Apply is about to make, as its intent will say it.
struct Change {
    id: TweakId,
    sid: Option<String>,
    value_name: Option<String>,
    /// What holds now. For Apply again that isn't the record's prior, which stays the first one.
    prior: Saved,
    planned: Saved,
    /// A record is already open for it, so this is Apply again.
    again: bool,
}

impl Change {
    fn intent(&self, batch: &str) -> Entry {
        Entry::Intent {
            at: now(),
            batch: batch.to_string(),
            tweak: self.id.as_str().to_string(),
            sid: self.sid.clone(),
            value_name: self.value_name.clone(),
            prior: self.prior.clone(),
            planned: self.planned.clone(),
        }
    }

    fn outcome(&self, applied: bool, detail: Option<String>) -> Entry {
        Entry::Outcome {
            at: now(),
            tweak: self.id.as_str().to_string(),
            sid: self.sid.clone(),
            applied,
            detail,
            interrupted: false,
        }
    }
}

/// What a look at an open record adds to the journal.
struct Settling {
    entry: Entry,
    /// The entry ends the record.
    closes: bool,
    /// What to tell the person, when a crash cut the change short.
    line: Option<String>,
}

/// Applies and undoes changes for one PC and one journal.
pub struct Engine {
    system: Arc<dyn System>,
    journal: JournalFile,
    /// The lines for `TweaksView::interrupted`. They last as long as the app.
    interrupted: Mutex<Vec<String>>,
}

impl Engine {
    /// Touches nothing, on the PC or in the journal's folder.
    pub fn new(system: Arc<dyn System>, journal: JournalFile) -> Self {
        Self {
            system,
            journal,
            interrupted: Mutex::new(Vec::new()),
        }
    }

    /// What the Tweaks tab shows, after settling the journal. A journal that can't be read is the error; one that can't
    /// be written to only means the next look tries again.
    pub fn view(&self, input: &PlanInput, gate: Gate) -> io::Result<TweaksView> {
        let replay = self.reconcile()?;
        let account = self.system.account();
        let signed_in = account.signed_in.as_deref();
        let mut rows: Vec<Row> = TweakId::ESSENTIAL
            .into_iter()
            .filter_map(|id| self.row(input, id, signed_in, record_of(&replay.open, id, signed_in)))
            .collect();
        if record_of(&replay.open, TweakId::SystemProtection, signed_in).is_some() {
            rows.push(protection_row());
        }
        let others = replay
            .open
            .iter()
            .filter(|record| !is_ours(record, signed_in))
            .count();
        Ok(TweaksView {
            rows,
            gate,
            other_account: account.differs(),
            pending_other_accounts: u32::try_from(others).unwrap_or(u32::MAX),
            interrupted: self.lines().clone(),
            unreadable_lines: replay.unreadable,
            any_active: replay.open.iter().any(|record| {
                is_ours(record, signed_in) && record.tweak != TweakId::SystemProtection
            }),
        })
    }

    /// What Apply would change, without changing anything, in the journal either. A journal that can't be read leaves
    /// Apply nothing it could do, so the Preview has nothing to show.
    pub fn preview(&self, input: &PlanInput, ids: &[TweakId]) -> PreviewView {
        let restore = self.system.protection();
        let Ok(replay) = self.journal.read() else {
            return PreviewView {
                items: Vec::new(),
                restore,
            };
        };
        let (replay, _) = self.settle(replay, false);
        let signed_in = self.system.account().signed_in;
        let signed_in = signed_in.as_deref();
        let items = ids
            .iter()
            .filter_map(|&id| {
                self.preview_item(input, id, signed_in, record_of(&replay.open, id, signed_in))
            })
            .collect();
        PreviewView { items, restore }
    }

    /// Changes each item in turn, under one lock, starting from a settled journal: one item failing doesn't stop the
    /// others, and each says how it went. A flagged row is left alone and says why: what something else has put there
    /// is only written over by Apply again, which is the person being asked.
    ///
    /// If the journal can't be locked or read or the first intent can't be written, this returns the error and nothing
    /// has been changed. If it stops working part-way, the items that have no intent on disk fail and are not changed.
    pub fn apply(&self, input: &PlanInput, ids: &[TweakId]) -> io::Result<Vec<ItemOutcome>> {
        let _lock = self.journal.lock()?;
        let (replay, written) = self.settle(self.journal.read()?, true);
        written?;
        let signed_in = self.system.account().signed_in;
        let signed_in = signed_in.as_deref();
        let batch = now();
        let mut changed_anything = false;
        let mut items = Vec::with_capacity(ids.len());
        for &id in ids {
            let record = record_of(&replay.open, id, signed_in);
            let change = match self.change_for(input, id, signed_in, record, Asked::Apply) {
                Ok(change) => change,
                Err(skipped) => {
                    items.push(skipped);
                    continue;
                }
            };
            if let Err(why) = self.journal.append(&change.intent(&batch)) {
                if !changed_anything {
                    return Err(why);
                }
                items.push(failure(
                    id,
                    format!("Couldn't write to Kestrel's journal, so this wasn't changed: {why}"),
                ));
                continue;
            }
            changed_anything = true;
            items.push(self.make(input, &change));
        }
        Ok(items)
    }

    /// Undoes one tweak, if what holds now is what Kestrel wrote. If it is back at what Kestrel found, the record
    /// closes and that is a success with nothing written. If something else has changed it since, it is left alone. A
    /// failed undo keeps the record, so it can be tried again.
    pub fn undo(&self, id: TweakId) -> io::Result<ItemOutcome> {
        let _lock = self.journal.lock()?;
        let replay = self.journal.read()?;
        let signed_in = self.system.account().signed_in;
        Ok(match record_of(&replay.open, id, signed_in.as_deref()) {
            Some(record) => self.undo_record(record),
            None => failure(id, NOTHING_TO_UNDO),
        })
    }

    /// Undoes what this account and the machine have open, newest first, the way `undo` does each. An inactive Kestrel
    /// plan is removed. Other accounts' records are not part of it. What is left alone comes back with `ok: false`:
    /// rows something else has changed, what couldn't be read, and System Protection, which stays on.
    pub fn revert_all(&self) -> io::Result<Vec<ItemOutcome>> {
        let _lock = self.journal.lock()?;
        let replay = self.journal.read()?;
        let signed_in = self.system.account().signed_in;
        Ok(replay
            .open
            .iter()
            .rev()
            .filter(|record| is_ours(record, signed_in.as_deref()))
            .map(|record| {
                if record.tweak == TweakId::SystemProtection {
                    failure(record.tweak, PROTECTION_STAYS)
                } else {
                    self.undo_record(record)
                }
            })
            .collect())
    }

    /// Writes Kestrel's value over what something else has changed a setting to. The record keeps its first prior, so
    /// an undo still goes back to what was there before Kestrel. If it doesn't take, the other value is put back.
    pub fn apply_again(&self, input: &PlanInput, id: TweakId) -> io::Result<ItemOutcome> {
        let _lock = self.journal.lock()?;
        let (replay, written) = self.settle(self.journal.read()?, true);
        written?;
        let signed_in = self.system.account().signed_in;
        let signed_in = signed_in.as_deref();
        let Some(record) = record_of(&replay.open, id, signed_in) else {
            return Ok(failure(id, NOTHING_TO_APPLY_AGAIN));
        };
        let change = match self.change_for(input, id, signed_in, Some(record), Asked::ApplyAgain) {
            Ok(change) => change,
            Err(skipped) => return Ok(skipped),
        };
        self.journal.append(&change.intent(&now()))?;
        Ok(self.make(input, &change))
    }

    /// Writes the old value over whatever something else has put there, because the person said to, and ends the
    /// record. Otherwise the record is judged as Undo judges it: a value already back at the old one closes it with
    /// nothing written, and one that can't be read is left alone, because there's no telling what holds now. It isn't
    /// for the plan or System Protection, whose rows have buttons of their own.
    pub fn restore_anyway(&self, id: TweakId) -> io::Result<ItemOutcome> {
        let _lock = self.journal.lock()?;
        let replay = self.journal.read()?;
        let signed_in = self.system.account().signed_in;
        let Some(record) = record_of(&replay.open, id, signed_in.as_deref()) else {
            return Ok(failure(id, NOTHING_TO_UNDO));
        };
        if matches!(id, TweakId::PowerPlan | TweakId::SystemProtection) {
            return Ok(failure(id, ONLY_THE_SETTINGS));
        }
        Ok(match self.standing(record) {
            Standing::Elsewhere => self.put_back(record),
            _ => self.undo_record(record),
        })
    }

    /// Switches back to Kestrel's plan when another has been picked since. The record keeps the plan that was in use
    /// before Kestrel, for an undo to go back to.
    pub fn use_plan_again(&self) -> io::Result<ItemOutcome> {
        let id = TweakId::PowerPlan;
        let _lock = self.journal.lock()?;
        let replay = self.journal.read()?;
        let Some(record) = record_of(&replay.open, id, None) else {
            return Ok(failure(id, NOTHING_TO_APPLY_AGAIN));
        };
        let sys = &*self.system;
        let prior = match known(power::prior(sys)) {
            Ok(prior) => prior,
            Err(why) => return Ok(failure(id, why)),
        };
        let planned = kestrels_plan();
        if targets::same_meaning(id, &prior, &planned) {
            return Ok(failure(id, ALREADY_SET));
        }
        let change = Change {
            id,
            sid: record.sid.clone(),
            value_name: None,
            prior,
            planned,
            again: true,
        };
        self.journal.append(&change.intent(&now()))?;
        Ok(match power::use_again(sys) {
            Err(why) => self.failed(&change, false, why),
            Ok(()) if self.plan_holds() => self.made(&change),
            Ok(()) => self.failed(&change, false, PLAN_NOT_SWITCHED.to_string()),
        })
    }

    /// Deletes Kestrel's plan and leaves the plan in use as it is. It refuses while Kestrel's plan is the one in use:
    /// that's what Undo is for.
    pub fn remove_plan(&self) -> io::Result<ItemOutcome> {
        let id = TweakId::PowerPlan;
        let _lock = self.journal.lock()?;
        let replay = self.journal.read()?;
        let Some(record) = record_of(&replay.open, id, None) else {
            return Ok(failure(id, NOTHING_TO_UNDO));
        };
        Ok(match self.standing(record) {
            Standing::Holds => self.remove_recorded_plan(record),
            _ => self.undo_record(record),
        })
    }

    /// The ids of the rows that say Applied, for this account and the machine, after settling the journal. A journal
    /// that can't be read has nothing known to hold.
    pub fn applied_ids(&self) -> Vec<String> {
        let Ok(replay) = self.reconcile() else {
            return Vec::new();
        };
        let signed_in = self.system.account().signed_in;
        replay
            .open
            .iter()
            .filter(|record| is_ours(record, signed_in.as_deref()))
            .filter(|record| {
                // System Protection's row says Applied for as long as its record is open.
                record.tweak == TweakId::SystemProtection
                    || matches!(self.standing(record), Standing::Holds)
            })
            .map(|record| record.tweak.as_str().to_string())
            .collect()
    }

    /// Whether any account has a change open, after settling the journal. The answer is the file's, not the settling's:
    /// a record whose closing entry couldn't be written, because someone else holds the lock or the file refused it, is
    /// still open. It says yes for a journal it can't read. The answer decides whether anyone is asked about undoing,
    /// and that is the safe side to be wrong on.
    pub fn has_open_records(&self) -> bool {
        // If this can't read the journal, neither can the line below, and that one says so.
        let _ = self.reconcile();
        self.journal
            .read()
            .map_or(true, |replay| !replay.open.is_empty())
    }

    /// The Kestrel report as plain text (see `report`), for the Tweaks tab and for Copy diagnostics. `now` is when it is
    /// made, as the journal spells times.
    ///
    /// The History is the whole journal's story. The Active list is what would be an Applied row: this account's and the
    /// machine's open records whose setting holds what Kestrel wrote (see `applied_records`). Anything else stays in the
    /// History and is not listed as active: a value something else changed, one that went back, an inactive plan, one
    /// that can't be read, another account's.
    ///
    /// It only reads, both the PC and the journal. It doesn't settle what a crash left, so it takes no lock and writes nothing
    /// to the journal or to Windows, and it makes no folder on a PC that never applied; it can be asked for at any
    /// moment, even while another process is changing something, which then shows up half done or not at all. A journal
    /// that can't be read says so, and is not taken for an empty one.
    pub fn report(&self, kestrel_version: &str, build: u32, now: &str) -> String {
        match self.journal.read() {
            Ok(replay) => {
                let unreadable_note = replay.unreadable > 0;
                let open = self.applied_records(&replay);
                let replay = Replay { open, ..replay };
                report::text(&replay, kestrel_version, build, now, unreadable_note)
            }
            Err(why) => report::unavailable(kestrel_version, build, now, &why.to_string()),
        }
    }

    // Looking at the journal.

    /// Reads the journal and settles what it can. The lock is only asked for once there is something to write, which
    /// leaves a PC that never pressed Apply without a folder. Whoever holds the lock may be in the middle of a change,
    /// and its record would only look cut off by a crash, so without the lock nothing is written and no one is told.
    /// The rows are judged all the same. Under the lock the journal is read again, because whoever held it a moment
    /// ago may have finished.
    fn reconcile(&self) -> io::Result<Replay> {
        let replay = self.journal.read()?;
        let signed_in = self.system.account().signed_in;
        let anything_to_write = replay
            .open
            .iter()
            .any(|record| self.settling(record, signed_in.as_deref()).is_some());
        if !anything_to_write {
            return Ok(replay);
        }
        match self.journal.lock() {
            // A journal that won't take the entries only means the next look tries again.
            Ok(_lock) => Ok(self.settle(self.journal.read()?, true).0),
            Err(_) => Ok(self.settle(replay, false).0),
        }
    }

    /// Judges the open records, and returns the journal as that leaves it, without the records that closed. With
    /// `may_write`, for a caller holding the lock, what a look comes to goes in the file too, and the second result is
    /// an error if something couldn't be written. A line for the person only goes with an entry that reached the file,
    /// because otherwise the next look finds the record and says it again.
    fn settle(&self, replay: Replay, may_write: bool) -> (Replay, io::Result<()>) {
        let signed_in = self.system.account().signed_in;
        let mut written = Ok(());
        let mut open = Vec::with_capacity(replay.open.len());
        for record in replay.open {
            if let Some(settling) = self.settling(&record, signed_in.as_deref()) {
                if may_write {
                    match self.journal.append(&settling.entry) {
                        Ok(()) => self.lines().extend(settling.line),
                        Err(why) => written = Err(why),
                    }
                }
                if settling.closes {
                    continue;
                }
            }
            open.push(record);
        }
        (Replay { open, ..replay }, written)
    }

    /// What a look at an open record comes to, if anything. Another account's record comes to nothing: it waits, and
    /// its settings aren't even read.
    ///
    /// A settled record closes when its setting is back at what Kestrel found (for the plan, when Kestrel's plan is
    /// gone), and otherwise stays as it is, with the row saying how things stand. A record with no outcome was cut off
    /// by a crash: at the old value it never happened, and anything else gets the outcome it was missing, written as
    /// applied because `applied: false` would drop a record that has to stay open. A failed read decides nothing, so
    /// the next look tries again.
    fn settling(&self, record: &OpenRecord, signed_in: Option<&str>) -> Option<Settling> {
        if !is_ours(record, signed_in) {
            return None;
        }
        let ending = match self.standing(record) {
            Standing::Unknown(_) => return None,
            Standing::Back => {
                return Some(Settling {
                    entry: closed(record),
                    closes: true,
                    line: (!record.settled).then(|| interrupted_line(record, "It didn't change.")),
                })
            }
            Standing::Holds => "It finished.",
            Standing::Elsewhere if record.tweak == TweakId::PowerPlan => "Its plan isn't active.",
            Standing::Elsewhere => "Something else changed it.",
        };
        (!record.settled).then(|| Settling {
            entry: Entry::Outcome {
                at: now(),
                tweak: record.tweak.as_str().to_string(),
                sid: record.sid.clone(),
                applied: true,
                detail: None,
                interrupted: true,
            },
            closes: false,
            line: Some(interrupted_line(record, ending)),
        })
    }

    /// Compares what a record's setting holds now with what Kestrel wrote, and only then with what it found: a value
    /// that means both still holds, and closing its record would lose the undo.
    fn standing(&self, record: &OpenRecord) -> Standing {
        let sys = &*self.system;
        let id = record.tweak;
        let now = match id {
            TweakId::PowerPlan => return self.plan_standing(),
            TweakId::SystemProtection => match sys.protection() {
                Readiness::Ready => Saved::Protection { on: true },
                Readiness::ProtectionOff => Saved::Protection { on: false },
                Readiness::BlockedByPolicy | Readiness::Unknown => {
                    return Standing::Unknown(PROTECTION_UNKNOWN.to_string())
                }
            },
            _ => {
                let Some(sid) = record.sid.as_deref() else {
                    return Standing::Unknown(NO_OWNER.to_string());
                };
                match known(targets::current(sys, id, sid, record.value_name.as_deref())) {
                    Ok(now) => now,
                    Err(why) => {
                        return Standing::Unknown(format!(
                            "Couldn't read this setting, so Kestrel left it alone: {why}"
                        ))
                    }
                }
            }
        };
        if targets::same_meaning(id, &now, &record.planned) {
            Standing::Holds
        } else if targets::is_back_at(id, &now, &record.prior, sys.account().differs()) {
            Standing::Back
        } else {
            Standing::Elsewhere
        }
    }

    /// The plan's rules: its record stays open for as long as Kestrel's plan is on the PC, whichever plan is in use,
    /// even the one Kestrel found. Only the fixed GUID is ever looked for; whatever the record names isn't.
    fn plan_standing(&self) -> Standing {
        let sys = &*self.system;
        match known(power::prior(sys)) {
            Err(why) => Standing::Unknown(joined(&why, "Kestrel left it alone.")),
            Ok(now) if targets::same_meaning(TweakId::PowerPlan, &now, &kestrels_plan()) => {
                Standing::Holds
            }
            Ok(_) if sys.plan_exists(KESTREL_PLAN_GUID) => Standing::Elsewhere,
            Ok(_) => Standing::Back,
        }
    }

    fn plan_holds(&self) -> bool {
        matches!(self.plan_standing(), Standing::Holds)
    }

    /// The open records whose rows would say Applied: this account's and the machine's whose setting holds what Kestrel
    /// wrote, whether or not their outcome has been written (the next look would call one Applied that a crash cut off
    /// after the write). A flagged value, one back at its prior, an inactive plan, a setting that can't be read, and
    /// another account's record are not. Another account's settings aren't even read.
    ///
    /// This only reads, and settles nothing, so a record that the next look would close is left out here and not closed.
    /// For the same reason System Protection has to hold like the rest, where `applied_ids` has its row say Applied for
    /// as long as its record is open: protection that reads off, or can't be read, is not listed as active.
    fn applied_records(&self, replay: &Replay) -> Vec<OpenRecord> {
        let signed_in = self.system.account().signed_in;
        replay
            .open
            .iter()
            .filter(|record| {
                is_ours(record, signed_in.as_deref())
                    && matches!(self.standing(record), Standing::Holds)
            })
            .cloned()
            .collect()
    }

    fn lines(&self) -> MutexGuard<'_, Vec<String>> {
        self.interrupted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    // Deciding what Apply would do.

    /// The row for a tweak, given its open record; None for one with no row, as in `setting_row`.
    fn row(
        &self,
        input: &PlanInput,
        id: TweakId,
        signed_in: Option<&str>,
        record: Option<&OpenRecord>,
    ) -> Option<Row> {
        let sys = &*self.system;
        if id == TweakId::PowerPlan {
            Some(power::row(sys, input, record))
        } else {
            plan::setting_row(sys, input, id, signed_in, record)
        }
    }

    /// The change Apply would make for this tweak, or the item's answer if it would make none. Reading only.
    ///
    /// A row goes ahead when it's Available, and a flagged one when Apply again is what's asking. A failed read stops
    /// it here, which is what keeps a value of the wrong type from being written over: the writes for DWORDs and the
    /// mouse don't read first.
    fn change_for(
        &self,
        input: &PlanInput,
        id: TweakId,
        signed_in: Option<&str>,
        record: Option<&OpenRecord>,
        asked: Asked,
    ) -> Result<Change, ItemOutcome> {
        let sys = &*self.system;
        let Some(row) = self.row(input, id, signed_in, record) else {
            return Err(failure(id, NOT_ON_THIS_PC));
        };
        let (sid, value_name, read) = if id == TweakId::PowerPlan {
            (None, None, known(power::prior(sys)))
        } else {
            let Some(sid) = signed_in else {
                return Err(failure(id, row.why));
            };
            let (value_name, read) = self.read_setting(input, id, sid, record);
            (Some(sid.to_string()), value_name, read)
        };
        let prior = match (row.state, read) {
            (RowState::AlreadySet | RowState::Applied, _) => return Err(failure(id, ALREADY_SET)),
            (RowState::ChangedElsewhere, _) if asked == Asked::Apply => {
                return Err(failure(id, row.why))
            }
            (RowState::Available | RowState::ChangedElsewhere, Ok(prior)) => prior,
            (
                RowState::Available | RowState::ChangedElsewhere | RowState::CouldntRead,
                Err(why),
            ) => return Err(failure(id, why)),
            // A policy decides it, there's nothing here for it to do, or another button is the one for it.
            _ => return Err(failure(id, row.why)),
        };
        let planned = match (id, record) {
            (TweakId::PowerPlan, _) => kestrels_plan(),
            // Worked out from the first prior, never from whatever holds now.
            (_, Some(record)) => targets::target(id, &record.prior),
            (_, None) => targets::target(id, &prior),
        };
        Ok(Change {
            id,
            sid,
            value_name,
            prior,
            planned,
            again: record.is_some(),
        })
    }

    /// What one of the five settings holds now, and the name of the value it was read from if it has its own. The
    /// game's value is named by its record while one is open, because that's the value that was changed. Otherwise it
    /// is named as the PC already spells it. The error is a sentence.
    fn read_setting(
        &self,
        input: &PlanInput,
        id: TweakId,
        sid: &str,
        record: Option<&OpenRecord>,
    ) -> (Option<String>, Result<Saved, String>) {
        let sys = &*self.system;
        let named = match (id, record, input.valorant_exe.as_deref()) {
            (TweakId::GpuPreference, Some(record), _) => Ok(record.value_name.clone()),
            (TweakId::GpuPreference, None, Some(exe)) => {
                targets::found(targets::gpu_value_name(sys, sid, exe))
            }
            _ => Ok(None),
        };
        let unreadable = |why: String| format!("Couldn't read this setting: {why}");
        match named {
            Err(why) => (None, Err(unreadable(why))),
            Ok(name) => {
                let read = known(targets::current(sys, id, sid, name.as_deref()));
                (name, read.map_err(unreadable))
            }
        }
    }

    fn preview_item(
        &self,
        input: &PlanInput,
        id: TweakId,
        signed_in: Option<&str>,
        record: Option<&OpenRecord>,
    ) -> Option<PreviewItem> {
        let sys = &*self.system;
        let change = self
            .change_for(input, id, signed_in, record, Asked::Apply)
            .ok()?;
        if id == TweakId::PowerPlan {
            return Some(power::preview(sys, input));
        }
        let tags = info(id);
        Some(PreviewItem {
            id,
            title: tags.title.to_string(),
            location: location(id, change.value_name.as_deref()),
            changes: vec![ValueChange {
                label: tags.title.to_string(),
                now: targets::describe(id, &change.prior),
                after: targets::describe(id, &change.planned),
            }],
            // When the accounts differ only the signed-in account's strings are written, and Windows reads those at
            // sign-in.
            timing: if id == TweakId::PointerPrecision && sys.account().differs() {
                Timing::NextSignIn
            } else {
                tags.timing
            },
            note: None,
        })
    }

    // Making a change, once its intent is on disk.

    fn make(&self, input: &PlanInput, change: &Change) -> ItemOutcome {
        match change.sid.as_deref() {
            Some(sid) => self.make_setting(sid, change),
            None => self.make_plan(input, change),
        }
    }

    /// Writes one of the five settings and reads it back. If it doesn't hold, it is put back as it was.
    fn make_setting(&self, sid: &str, change: &Change) -> ItemOutcome {
        let sys = &*self.system;
        let (id, name) = (change.id, change.value_name.as_deref());
        let differs = sys.account().differs();
        let why = match targets::write(sys, id, sid, name, &change.planned, differs) {
            Err(why) => format!("Windows refused the change: {why}"),
            Ok(()) => {
                let held = |now: &Saved| targets::same_meaning(id, now, &change.planned);
                match self.reads_back(sid, id, name, held, NOT_KEPT) {
                    Ok(()) => return self.made(change),
                    Err(why) => why,
                }
            }
        };
        match self.take_back(sid, change, differs) {
            Ok(false) => self.failed(change, false, why),
            Ok(true) => self.failed(
                change,
                false,
                joined(&why, "Kestrel put the old value back."),
            ),
            Err(gone) => {
                let told = format!("Kestrel couldn't put the old value back: {gone}");
                self.failed(change, true, joined(&why, &told))
            }
        }
    }

    /// Puts a setting back as `change` found it, after a change that didn't take. True if that took a write, false if
    /// it had never left. It only counts as back if it reads back.
    ///
    /// Whether it ever left goes by the whole state and not by what it means. What the mouse means is in the strings,
    /// and Windows can have set the mouse in force and saved nothing.
    fn take_back(&self, sid: &str, change: &Change, differs: bool) -> Result<bool, String> {
        let sys = &*self.system;
        let (id, name) = (change.id, change.value_name.as_deref());
        if known(targets::current(sys, id, sid, name)).is_ok_and(|now| now == change.prior) {
            return Ok(false);
        }
        targets::restore(sys, id, sid, name, &change.prior, differs)
            .map_err(|why| why.to_string())?;
        self.reads_as_found(sid, id, name, &change.prior, differs)?;
        Ok(true)
    }

    /// Reads one of the five settings back: `held` says whether what it finds is right, or this says why not, in
    /// `not_kept`'s words if it holds something else.
    fn reads_back(
        &self,
        sid: &str,
        id: TweakId,
        name: Option<&str>,
        held: impl Fn(&Saved) -> bool,
        not_kept: &str,
    ) -> Result<(), String> {
        match known(targets::current(&*self.system, id, sid, name)) {
            Ok(now) if held(&now) => Ok(()),
            Ok(_) => Err(not_kept.to_string()),
            Err(why) => Err(format!("Couldn't read this setting back: {why}")),
        }
    }

    /// Reads one of the five settings back after Kestrel put it back.
    fn reads_as_found(
        &self,
        sid: &str,
        id: TweakId,
        name: Option<&str>,
        prior: &Saved,
        differs: bool,
    ) -> Result<(), String> {
        let back = |now: &Saved| targets::is_back_at(id, now, prior, differs);
        self.reads_back(sid, id, name, back, OLD_VALUE_NOT_KEPT)
    }

    /// Makes Kestrel's plan and switches to it, then reads back that it did. `create_and_activate` already takes back
    /// a plan it couldn't finish, and this takes back one Windows said it switched to and didn't.
    fn make_plan(&self, input: &PlanInput, change: &Change) -> ItemOutcome {
        let sys = &*self.system;
        let why = match power::create_and_activate(sys, input) {
            Err(why) => why,
            Ok(()) if self.plan_holds() => return self.made(change),
            Ok(()) => match power::remove(sys) {
                Ok(()) => joined(PLAN_NOT_SWITCHED, "Kestrel took its plan out again."),
                Err(why) => joined(PLAN_NOT_SWITCHED, &why),
            },
        };
        self.failed(change, sys.plan_exists(KESTREL_PLAN_GUID), why)
    }

    /// Finishes a change that held: its outcome, or `Reapplied` for a record that was already open.
    fn made(&self, change: &Change) -> ItemOutcome {
        self.note(&if change.again {
            Entry::Reapplied {
                at: now(),
                tweak: change.id.as_str().to_string(),
                sid: change.sid.clone(),
            }
        } else {
            change.outcome(true, None)
        });
        success(change.id, APPLIED)
    }

    /// Finishes a change that didn't take. `left_changed` says it couldn't be put back as it was. An `applied: false`
    /// takes back the intent, and that drops a record the intent opened, so a change still on the PC is written down as
    /// applied, with the reason, or it could never be undone. A record that was open before the intent stands either
    /// way, so its failure is written as what it was.
    fn failed(&self, change: &Change, left_changed: bool, why: String) -> ItemOutcome {
        let applied = left_changed && !change.again;
        self.note(&change.outcome(applied, Some(why.clone())));
        failure(change.id, why)
    }

    /// Adds an entry that comes after the change it is about. If it can't be written, nothing is lost: the intent or
    /// the record is on disk, and the next look at the journal finds the setting and settles it.
    fn note(&self, entry: &Entry) {
        let _ = self.journal.append(entry);
    }

    // Undoing.

    /// The three-way check, for one record.
    fn undo_record(&self, record: &OpenRecord) -> ItemOutcome {
        let id = record.tweak;
        match self.standing(record) {
            Standing::Unknown(why) => failure(id, why),
            Standing::Back => {
                self.note(&closed(record));
                let said = if id == TweakId::PowerPlan {
                    PLAN_ALREADY_GONE
                } else {
                    ALREADY_BACK
                };
                success(id, said)
            }
            Standing::Elsewhere if id == TweakId::PowerPlan => self.remove_recorded_plan(record),
            Standing::Elsewhere => failure(id, LEFT_ALONE),
            Standing::Holds => match id {
                TweakId::PowerPlan => match power::undo(&*self.system, &record.prior) {
                    Ok(said) => {
                        self.note(&undone(record, Some(said.clone())));
                        success(id, said)
                    }
                    Err(why) => failure(id, why),
                },
                TweakId::SystemProtection => self.protection_off(record),
                _ => self.put_back(record),
            },
        }
    }

    /// Writes one of the five settings back to what Kestrel found, and ends the record once that reads back.
    fn put_back(&self, record: &OpenRecord) -> ItemOutcome {
        let sys = &*self.system;
        let (id, name) = (record.tweak, record.value_name.as_deref());
        let Some(sid) = record.sid.as_deref() else {
            return failure(id, NO_OWNER);
        };
        let differs = sys.account().differs();
        if let Err(why) = targets::restore(sys, id, sid, name, &record.prior, differs) {
            return failure(id, format!("Couldn't put this back: {why}"));
        }
        match self.reads_as_found(sid, id, name, &record.prior, differs) {
            Ok(()) => {
                self.note(&undone(record, None));
                success(id, UNDONE)
            }
            Err(why) => failure(id, why),
        }
    }

    /// Deletes Kestrel's plan, or says why not.
    fn remove_recorded_plan(&self, record: &OpenRecord) -> ItemOutcome {
        match power::remove(&*self.system) {
            Ok(()) => {
                self.note(&undone(record, None));
                success(record.tweak, UNDONE)
            }
            Err(why) => failure(record.tweak, why),
        }
    }

    /// Turning System Protection off deletes the drive's restore points, so it takes a record saying that it was off
    /// before Kestrel. Like any undo it counts once it reads back: while Windows says it's still on, or won't say, the
    /// record stays and the row with it.
    fn protection_off(&self, record: &OpenRecord) -> ItemOutcome {
        let id = record.tweak;
        if record.prior != (Saved::Protection { on: false }) {
            return failure(id, PROTECTION_NOT_KESTRELS);
        }
        if let Err(why) = self.system.turn_protection_off() {
            return failure(id, format!("Couldn't turn System Protection off: {why}"));
        }
        match self.standing(record) {
            Standing::Back => {
                self.note(&undone(record, None));
                success(id, UNDONE)
            }
            Standing::Holds => failure(id, PROTECTION_STILL_ON),
            Standing::Elsewhere | Standing::Unknown(_) => failure(id, PROTECTION_NOT_CONFIRMED),
        }
    }
}

/// When something happened, as the journal says it.
fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

fn item(id: TweakId, ok: bool, message: impl Into<String>) -> ItemOutcome {
    ItemOutcome {
        id,
        title: info(id).title.to_string(),
        ok,
        message: message.into(),
    }
}

fn success(id: TweakId, message: impl Into<String>) -> ItemOutcome {
    item(id, true, message)
}

fn failure(id: TweakId, message: impl Into<String>) -> ItemOutcome {
    item(id, false, message)
}

/// Two sentences, where the first may or may not have its full stop yet.
fn joined(first: &str, second: &str) -> String {
    format!("{}. {second}", first.trim_end_matches('.'))
}

/// A read of a whole setting: its state, or why there isn't one. A setting that isn't set is a state too (a `Dword`
/// holding `None`, say), so `targets::current` and `power::prior` don't answer `Absent`; if one ever did, it wouldn't
/// have said what is there, and that is no reason to write over it.
fn known(read: Read<Saved>) -> Result<Saved, String> {
    match read {
        Read::Value(value) => Ok(value),
        Read::Absent => Err("Windows didn't say.".to_string()),
        Read::Failed(why) => Err(why),
    }
}

fn kestrels_plan() -> Saved {
    Saved::Plan {
        guid: KESTREL_PLAN_GUID.to_string(),
        name: KESTREL_PLAN_NAME.to_string(),
    }
}

/// Whether a record is this account's or the machine's to judge and act on.
fn is_ours(record: &OpenRecord, signed_in: Option<&str>) -> bool {
    record.sid.is_none() || record.sid.as_deref() == signed_in
}

/// The record a tweak's row and buttons go by: for a machine-wide tweak the one with no account, and otherwise the
/// signed-in account's.
fn record_of<'a>(
    open: &'a [OpenRecord],
    id: TweakId,
    signed_in: Option<&str>,
) -> Option<&'a OpenRecord> {
    let owner = match id {
        TweakId::PowerPlan | TweakId::SystemProtection => None,
        _ => signed_in,
    };
    open.iter()
        .find(|record| record.tweak == id && record.sid.as_deref() == owner)
}

/// The entry that ends a record when there is nothing to put back. Cut off by a crash, it never happened; otherwise it
/// went back, or for the plan, the plan went.
fn closed(record: &OpenRecord) -> Entry {
    let reason = if !record.settled {
        CloseReason::NotApplied
    } else if record.tweak == TweakId::PowerPlan {
        CloseReason::PlanRemovedElsewhere
    } else {
        CloseReason::WentBack
    };
    Entry::Closed {
        at: now(),
        tweak: record.tweak.as_str().to_string(),
        sid: record.sid.clone(),
        reason,
    }
}

fn undone(record: &OpenRecord, detail: Option<String>) -> Entry {
    Entry::Undone {
        at: now(),
        tweak: record.tweak.as_str().to_string(),
        sid: record.sid.clone(),
        detail,
    }
}

fn interrupted_line(record: &OpenRecord, ending: &str) -> String {
    format!(
        "Kestrel was interrupted while changing {}. {ending}",
        info(record.tweak).title
    )
}

/// System Protection's row, which has no rules: it is there because Kestrel turned it on.
fn protection_row() -> Row {
    let tags = info(TweakId::SystemProtection);
    Row {
        id: tags.id,
        title: tags.title.to_string(),
        state: RowState::Applied,
        ticked: false,
        why: PROTECTION_ON.to_string(),
        impact: tags.impact,
        timing: tags.timing,
    }
}

/// Where a Preview says one of the five settings lives. `HKCU` is shorthand: it is always the signed-in account's hive.
fn location(id: TweakId, value_name: Option<&str>) -> String {
    let (key, names) = match id {
        TweakId::GameMode => (targets::GAME_BAR_KEY, targets::AUTO_GAME_MODE.to_string()),
        TweakId::BackgroundRecording => (
            targets::GAME_DVR_KEY,
            targets::HISTORICAL_CAPTURE.to_string(),
        ),
        TweakId::WindowedOptimizations => {
            (targets::DIRECTX_KEY, targets::DIRECTX_SETTINGS.to_string())
        }
        TweakId::GpuPreference => (
            targets::DIRECTX_KEY,
            value_name.unwrap_or_default().to_string(),
        ),
        TweakId::PointerPrecision => (
            targets::MOUSE_KEY,
            [
                targets::MOUSE_SPEED,
                targets::MOUSE_THRESHOLD_1,
                targets::MOUSE_THRESHOLD_2,
            ]
            .join(", "),
        ),
        // Not registry values. The plan's Preview says its own, and System Protection isn't in a Preview.
        TweakId::PowerPlan | TweakId::SystemProtection => return info(id).title.to_string(),
    };
    format!(r"HKCU\{key}, {names}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{
        FakeSystem, ACTIVE_PLAN, BALANCED, HIGH_PERFORMANCE, MOUSE_LIVE, PROTECTION, SID,
    };
    use crate::journal::{Folder, PlainFolder, RestoreOutcome};
    use crate::targets::{
        ALLOW_AUTO_GAME_MODE, ALLOW_GAME_DVR, APP_CAPTURE, AUTO_GAME_MODE, DIRECTX_KEY,
        DIRECTX_SETTINGS, GAME_BAR_KEY, GAME_DVR_KEY, GAME_DVR_POLICY_KEY, GPU_PREFERENCE_TOKEN,
        HISTORICAL_CAPTURE, MOUSE_KEY, MOUSE_SPEED, MOUSE_THRESHOLD_1, MOUSE_THRESHOLD_2,
        SWAP_EFFECT_TOKEN,
    };
    use crate::tokens;
    use kestrel_core::snapshot::FormFactor;
    use kestrel_probe::account::Account;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;
    use RowState::{Applied, Available, ChangedElsewhere, CouldntRead, PlanInactive};
    use TweakId::{
        BackgroundRecording, GameMode, GpuPreference, PointerPrecision, PowerPlan,
        SystemProtection, WindowedOptimizations,
    };

    const OTHER_SID: &str = "S-1-5-21-1-2-3-1002";
    const EXE: &str =
        r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe";
    const EXE_LOWER: &str =
        r"c:\riot games\valorant\live\shootergame\binaries\win64\valorant-win64-shipping.exe";
    const MOVED_EXE: &str = r"D:\Games\Riot\VALORANT-Win64-Shipping.exe";
    const EXM: &str = "0e1d2c3b-4a59-4687-b6a5-948372615049";
    const EXM_NAME: &str = "EXM Free Power Plan V8";
    const KESTREL: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";
    const POWER_POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Power\PowerSettings";
    const POWER_POLICY_VALUE: &str = "ActivePowerScheme";
    const WHEN: &str = "2026-11-02T18:12:00+00:00";

    /// The five settings, in the order of the Tweaks tab.
    const SETTINGS: [TweakId; 5] = [
        GameMode,
        BackgroundRecording,
        WindowedOptimizations,
        GpuPreference,
        PointerPrecision,
    ];

    // The words the brief gives.
    const ALREADY_SET_TEXT: &str = "Already set.";
    const LEFT_ALONE_TEXT: &str =
        "Something else changed this after Kestrel did. Kestrel left it alone.";
    const PROTECTION_WHY_TEXT: &str = "System Protection is on because Kestrel turned it on. Turning it off deletes this drive's restore points.";

    /// A desktop on Windows 11 25H2 with two graphics chips, VALORANT where Riot puts it, and windowed fullscreen.
    fn input() -> PlanInput {
        PlanInput {
            build: 26200,
            form_factor: FormFactor::Desktop,
            hybrid_cpu: false,
            cpu_name: "Intel(R) Core(TM) i9-11900K".to_string(),
            gpu_choice: true,
            valorant_exe: Some(EXE.to_string()),
            fullscreen_mode: Some(1),
        }
    }

    /// A PC where each of the six has something to change: Game Mode is off, background recording is on, the windowed
    /// token isn't set (the settings value holds another token only), the game's graphics preference is left to
    /// Windows, the mouse is as it ships, and Balanced is the plan in use.
    fn arranged() -> FakeSystem {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1);
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=0;");
        pc
    }

    fn with_differing_accounts(pc: FakeSystem) -> FakeSystem {
        pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: Some(OTHER_SID.to_string()),
        });
        pc
    }

    /// Every way a tweak's row can be Available that differs in what undoing it has to put back.
    fn ways_to_be_available(id: TweakId) -> Vec<FakeSystem> {
        let pc = || {
            let pc = arranged();
            pc.remove_value(SID, DIRECTX_KEY, DIRECTX_SETTINGS);
            pc.remove_value(SID, DIRECTX_KEY, EXE);
            pc
        };
        let with_settings = |text: &str| {
            let pc = pc();
            pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, text);
            pc
        };
        let with_game = |text: &str| {
            let pc = pc();
            pc.set_string(SID, DIRECTX_KEY, EXE, text);
            pc
        };
        // The key and everything in it are gone, as on a PC that never opened Graphics settings.
        let no_key = || {
            let pc = pc();
            pc.delete_key_if_empty(SID, DIRECTX_KEY).unwrap();
            assert_eq!(pc.value_names(SID, DIRECTX_KEY), Read::Absent);
            pc
        };
        match id {
            PowerPlan => {
                let exm = arranged();
                exm.add_plan(EXM, EXM_NAME);
                exm.set_active(EXM);
                let fast = arranged();
                fast.set_active(HIGH_PERFORMANCE);
                vec![arranged(), exm, fast]
            }
            GameMode | BackgroundRecording => vec![arranged()],
            WindowedOptimizations => vec![
                with_settings("VRROptimizeEnable=1;"),
                with_settings("SwapEffectUpgradeEnable=0;VRROptimizeEnable=1;"),
                with_game("GpuPreference=1;"),
                pc(),
                no_key(),
            ],
            GpuPreference => vec![
                with_game("GpuPreference=0;"),
                with_game("GpuPreference=1;"),
                with_game("AutoHDREnable=1;"),
                with_settings("VRROptimizeEnable=1;"),
                pc(),
                no_key(),
            ],
            PointerPrecision => {
                let no_strings = arranged();
                no_strings.remove_value(SID, MOUSE_KEY, MOUSE_SPEED);
                no_strings.remove_value(SID, MOUSE_KEY, MOUSE_THRESHOLD_1);
                no_strings.remove_value(SID, MOUSE_KEY, MOUSE_THRESHOLD_2);
                vec![arranged(), no_strings]
            }
            SystemProtection => unreachable!("not an Essential row"),
        }
    }

    /// Changes a setting to something that is neither what it was before Kestrel, in any of `ways_to_be_available`, nor
    /// what Kestrel left, and leaves every other token in its string alone.
    fn change_elsewhere(pc: &FakeSystem, id: TweakId) {
        let set_token = |value_name: &str, token: &str, value: &str| {
            let text = pc.string(SID, DIRECTX_KEY, value_name).unwrap_or_default();
            let text = tokens::set(&text, token, value);
            pc.set_string(SID, DIRECTX_KEY, value_name, &text);
        };
        match id {
            GameMode => pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 2),
            BackgroundRecording => pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 2),
            WindowedOptimizations => set_token(DIRECTX_SETTINGS, SWAP_EFFECT_TOKEN, "2"),
            GpuPreference => set_token(EXE, GPU_PREFERENCE_TOKEN, "3"),
            PointerPrecision => {
                pc.set_string(SID, MOUSE_KEY, MOUSE_SPEED, "2");
                pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_1, "4");
                pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_2, "12");
            }
            PowerPlan | SystemProtection => unreachable!("not one of the five"),
        }
    }

    /// Puts a setting back to what it holds on `arranged`, without Kestrel.
    fn put_back_by_hand(pc: &FakeSystem, id: TweakId) {
        match id {
            GameMode => pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0),
            BackgroundRecording => pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1),
            WindowedOptimizations => {
                pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
            }
            GpuPreference => pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=0;"),
            PointerPrecision => {
                pc.set_string(SID, MOUSE_KEY, MOUSE_SPEED, "1");
                pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_1, "6");
                pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_2, "10");
                pc.set_mouse([6, 10, 1]);
            }
            PowerPlan | SystemProtection => unreachable!("not one of the five"),
        }
    }

    /// The registry value a tweak's data lives in.
    fn value_of(id: TweakId) -> &'static str {
        match id {
            GameMode => AUTO_GAME_MODE,
            BackgroundRecording => HISTORICAL_CAPTURE,
            WindowedOptimizations => DIRECTX_SETTINGS,
            GpuPreference => EXE,
            PointerPrecision => MOUSE_SPEED,
            PowerPlan | SystemProtection => unreachable!("not one of the five"),
        }
    }

    fn journal_in(dir: &Path) -> JournalFile {
        JournalFile::new(dir, Box::new(PlainFolder))
    }

    /// A folder that calls `hook` each time the journal looks at it, which is before every read, lock and entry, with
    /// the number of looks that came before. If the hook says no, so does the folder.
    struct Hooked<F> {
        looks: AtomicUsize,
        hook: F,
    }

    impl<F: Fn(usize) -> io::Result<()> + Send + Sync> Folder for Hooked<F> {
        fn check(&self, _dir: &Path) -> io::Result<()> {
            (self.hook)(self.looks.fetch_add(1, Ordering::SeqCst))
        }

        fn prepare(&self, dir: &Path) -> io::Result<()> {
            std::fs::create_dir_all(dir)
        }
    }

    fn hooked_journal(
        dir: &Path,
        hook: impl Fn(usize) -> io::Result<()> + Send + Sync + 'static,
    ) -> JournalFile {
        let folder = Hooked {
            looks: AtomicUsize::new(0),
            hook,
        };
        JournalFile::new(dir, Box::new(folder))
    }

    /// A pretend PC and an engine, with the journal in a folder that isn't there until something makes it.
    struct Rig {
        pc: Arc<FakeSystem>,
        engine: Engine,
        dir: TempDir,
    }

    impl Rig {
        fn on(pc: FakeSystem) -> Rig {
            let pc = Arc::new(pc);
            let dir = tempfile::tempdir().unwrap();
            let engine = Engine::new(pc.clone(), journal_in(&dir.path().join("Kestrel")));
            Rig { pc, engine, dir }
        }

        fn folder(&self) -> PathBuf {
            self.dir.path().join("Kestrel")
        }

        /// The same journal, as another process sees it.
        fn journal(&self) -> JournalFile {
            journal_in(&self.folder())
        }

        /// The app started again: the same PC and journal, and nothing remembered.
        fn restarted(&self) -> Engine {
            Engine::new(self.pc.clone(), self.journal())
        }

        fn story(&self) -> Vec<String> {
            story(&self.journal().read().unwrap().history)
        }

        fn open(&self) -> Vec<OpenRecord> {
            self.journal().read().unwrap().open
        }

        /// Writes the entries as an earlier run of Kestrel would have left them.
        fn left_by_an_earlier_run(&self, entries: &[Entry]) {
            let journal = self.journal();
            let lock = journal.lock().unwrap();
            for entry in entries {
                journal.append(entry).unwrap();
            }
            drop(lock);
        }

        /// Cuts the journal to its first `lines` lines, as a crash right after writing them leaves it.
        fn cut_journal(&self, lines: usize) {
            let path = self.folder().join("journal.jsonl");
            let text = std::fs::read_to_string(&path).unwrap();
            let kept: String = text.split_inclusive('\n').take(lines).collect();
            std::fs::write(&path, kept).unwrap();
        }

        fn view(&self) -> TweaksView {
            self.engine.view(&input(), Gate::Open).unwrap()
        }

        fn row(&self, id: TweakId) -> Row {
            row_in(&self.view(), id)
        }

        fn state(&self, id: TweakId) -> RowState {
            self.row(id).state
        }

        fn apply(&self, id: TweakId) -> ItemOutcome {
            let mut items = self.engine.apply(&input(), &[id]).unwrap();
            assert_eq!(items.len(), 1, "one item for one id");
            items.remove(0)
        }

        fn undo(&self, id: TweakId) -> ItemOutcome {
            self.engine.undo(id).unwrap()
        }
    }

    fn row_in(view: &TweaksView, id: TweakId) -> Row {
        view.rows
            .iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("no row for {id:?}"))
            .clone()
    }

    /// The journal's entries in short, in file order.
    fn story(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Intent { tweak, .. } => format!("intent {tweak}"),
                Entry::Outcome {
                    tweak,
                    applied: true,
                    interrupted: true,
                    ..
                } => format!("interrupted {tweak}"),
                Entry::Outcome {
                    tweak,
                    applied: true,
                    ..
                } => format!("applied {tweak}"),
                Entry::Outcome {
                    tweak,
                    applied: false,
                    ..
                } => format!("failed {tweak}"),
                Entry::Reapplied { tweak, .. } => format!("reapplied {tweak}"),
                Entry::Undone { tweak, .. } => format!("undone {tweak}"),
                Entry::Closed { tweak, reason, .. } => format!("closed {tweak} {reason:?}"),
                Entry::RestorePoint { .. } => "restore point".to_string(),
            })
            .collect()
    }

    fn says(pairs: &[(&str, TweakId)]) -> Vec<String> {
        pairs
            .iter()
            .map(|(what, id)| format!("{what} {}", id.as_str()))
            .collect()
    }

    fn interrupted(id: TweakId, ending: &str) -> String {
        format!(
            "Kestrel was interrupted while changing {}. {ending}",
            info(id).title
        )
    }

    fn dword(value: u32) -> Saved {
        Saved::Dword { value: Some(value) }
    }

    fn saved_plan(guid: &str, name: &str) -> Saved {
        Saved::Plan {
            guid: guid.to_string(),
            name: name.to_string(),
        }
    }

    fn intent(id: TweakId, sid: Option<&str>, prior: Saved, planned: Saved) -> Entry {
        Entry::Intent {
            at: WHEN.to_string(),
            batch: WHEN.to_string(),
            tweak: id.as_str().to_string(),
            sid: sid.map(str::to_string),
            value_name: None,
            prior,
            planned,
        }
    }

    fn outcome(id: TweakId, sid: Option<&str>) -> Entry {
        Entry::Outcome {
            at: WHEN.to_string(),
            tweak: id.as_str().to_string(),
            sid: sid.map(str::to_string),
            applied: true,
            detail: None,
            interrupted: false,
        }
    }

    /// Game Mode's intent as Apply writes it on `arranged`.
    fn game_mode_intent() -> Entry {
        intent(GameMode, Some(SID), dword(0), dword(1))
    }

    fn plan_intent(prior: Saved) -> Entry {
        intent(PowerPlan, None, prior, saved_plan(KESTREL, "Kestrel"))
    }

    fn balanced() -> Saved {
        saved_plan(BALANCED, "Balanced")
    }

    fn protection_record() -> [Entry; 2] {
        [
            intent(
                SystemProtection,
                None,
                Saved::Protection { on: false },
                Saved::Protection { on: true },
            ),
            outcome(SystemProtection, None),
        ]
    }

    // Apply and Undo, for each tweak.

    #[test]
    fn apply_then_undo_restores_everything_for_each_tweak() {
        for accounts_differ in [false, true] {
            for id in TweakId::ESSENTIAL {
                for (way, pc) in ways_to_be_available(id).into_iter().enumerate() {
                    let pc = if accounts_differ {
                        with_differing_accounts(pc)
                    } else {
                        pc
                    };
                    let rig = Rig::on(pc);
                    let context = format!("{id:?}, way {way}, accounts differ: {accounts_differ}");
                    assert_eq!(rig.state(id), Available, "{context}");
                    let before = rig.pc.dump();

                    let applied = rig.apply(id);
                    assert!(applied.ok, "{context}: {}", applied.message);
                    assert_eq!(applied.message, "Applied.", "{context}");
                    assert_eq!(applied.id, id);
                    assert_eq!(applied.title, info(id).title);
                    assert_ne!(rig.pc.dump(), before, "{context}");
                    assert_eq!(rig.state(id), Applied, "{context}");

                    let undone = rig.undo(id);
                    assert!(undone.ok, "{context}: {}", undone.message);
                    assert_eq!(rig.pc.dump(), before, "{context}");
                    assert_eq!(rig.state(id), Available, "{context}");
                    assert!(rig.open().is_empty(), "{context}");
                    assert_eq!(
                        rig.story(),
                        says(&[("intent", id), ("applied", id), ("undone", id)]),
                        "{context}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_intent_is_on_disk_before_the_write() {
        let rig = Rig::on(arranged());
        rig.pc.fail_write(AUTO_GAME_MODE);
        let before = rig.pc.dump();

        let item = rig.apply(GameMode);

        assert!(!item.ok);
        assert_eq!(
            item.message,
            "Windows refused the change: the fake was told that writing AutoGameModeEnabled fails",
            "the reason, and no word of putting back what never changed"
        );
        assert_eq!(
            rig.story(),
            says(&[("intent", GameMode), ("failed", GameMode)])
        );
        let history = rig.journal().read().unwrap().history;
        let Entry::Intent {
            at,
            batch,
            tweak,
            sid,
            value_name,
            prior,
            planned,
        } = &history[0]
        else {
            panic!("the first entry is the intent: {history:?}");
        };
        assert_eq!(tweak, "game-mode");
        assert_eq!(sid.as_deref(), Some(SID));
        assert_eq!(*value_name, None);
        assert_eq!(*prior, dword(0));
        assert_eq!(*planned, dword(1));
        assert!(chrono::DateTime::parse_from_rfc3339(at).is_ok(), "{at}");
        assert!(
            chrono::DateTime::parse_from_rfc3339(batch).is_ok(),
            "{batch}"
        );
        assert!(matches!(
            &history[1],
            Entry::Outcome { applied: false, detail: Some(detail), interrupted: false, .. }
                if detail.contains("told that writing")
        ));
        assert!(rig.open().is_empty(), "nothing to undo");
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn no_change_reaches_windows_before_its_intent_is_on_disk() {
        for id in TweakId::ESSENTIAL {
            let pc = Arc::new(arranged());
            let dir = tempfile::tempdir().unwrap();
            // Each time the journal looks at its folder, how many entries the file has, and how many changes the PC has
            // been through by then.
            let seen = Arc::new(Mutex::new(Vec::new()));
            let journal = {
                let (pc, seen, file) = (pc.clone(), seen.clone(), dir.path().join("journal.jsonl"));
                hooked_journal(dir.path(), move |_| {
                    let lines =
                        std::fs::read_to_string(&file).map_or(0, |text| text.lines().count());
                    let mut seen = seen.lock().unwrap();
                    seen.push((lines, pc.writes().len()));
                    Ok(())
                })
            };
            let engine = Engine::new(pc.clone(), journal);

            let items = engine.apply(&input(), &[id]).unwrap();

            assert!(items[0].ok, "{id:?}: {}", items[0].message);
            assert!(!pc.writes().is_empty(), "{id:?}");
            let seen = seen.lock().unwrap();
            assert!(
                seen.iter()
                    .all(|(lines, writes)| *writes == 0 || *lines > 0),
                "{id:?}: a change came before its intent: {seen:?}"
            );
            assert!(
                seen.iter()
                    .any(|(lines, writes)| *writes > 0 && *lines == 1),
                "{id:?}: the outcome comes after the change: {seen:?}"
            );
        }
    }

    #[test]
    fn a_lying_readback_puts_the_prior_back() {
        // The two DWORDs, the two strings, and the mouse both ways: through Windows, which saves the strings itself,
        // when the accounts are the same, and the strings alone when they differ.
        let cases = [
            (GameMode, false),
            (BackgroundRecording, false),
            (WindowedOptimizations, false),
            (GpuPreference, false),
            (PointerPrecision, false),
            (PointerPrecision, true),
        ];
        for (id, accounts_differ) in cases {
            for pc in ways_to_be_available(id) {
                let pc = if accounts_differ {
                    with_differing_accounts(pc)
                } else {
                    pc
                };
                let rig = Rig::on(pc);
                rig.pc.lie_on_readback(value_of(id));
                let before = rig.pc.dump();

                let item = rig.apply(id);

                assert!(!item.ok, "{id:?}");
                assert_eq!(
                    item.message, "Windows didn't keep the change. Kestrel put the old value back.",
                    "{id:?}"
                );
                assert_eq!(rig.pc.dump(), before, "{id:?}: the value is the prior");
                assert_eq!(rig.story(), says(&[("intent", id), ("failed", id)]));
                assert!(rig.open().is_empty(), "{id:?}");
                assert_eq!(rig.state(id), Available, "{id:?}");
            }
        }
    }

    #[test]
    fn a_mouse_change_the_strings_never_saw_is_still_put_back() {
        // Windows set the mouse in force and saved nothing, so the strings still mean what Kestrel found. That doesn't
        // make it a change that never happened.
        let pc = arranged();
        pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: None,
        });
        let rig = Rig::on(pc);
        let before = rig.pc.dump();

        let item = rig.apply(PointerPrecision);

        assert!(!item.ok);
        assert_eq!(
            item.message,
            "Windows didn't keep the change. Kestrel put the old value back."
        );
        assert_eq!(rig.pc.mouse_live(), Read::Value([6, 10, 1]));
        assert_eq!(rig.pc.dump(), before);
        assert_eq!(
            rig.story(),
            says(&[("intent", PointerPrecision), ("failed", PointerPrecision)])
        );
        assert!(rig.open().is_empty());
        assert_eq!(rig.state(PointerPrecision), Available);
    }

    #[test]
    fn a_change_that_cant_be_taken_back_keeps_its_record() {
        // The read-back lies, and the key Kestrel made can't be deleted again, so the setting isn't as it was.
        let pc = FakeSystem::new();
        pc.fail_write(DIRECTX_KEY);
        let rig = Rig::on(pc);
        rig.pc.lie_on_readback(DIRECTX_SETTINGS);

        let item = rig.apply(WindowedOptimizations);

        assert!(!item.ok);
        assert!(item.message.contains("didn't keep"), "{}", item.message);
        assert!(item.message.contains("couldn't put"), "{}", item.message);
        assert_eq!(
            rig.story(),
            says(&[
                ("intent", WindowedOptimizations),
                ("applied", WindowedOptimizations)
            ]),
            "an applied outcome, because a failed one would drop the record its intent opened"
        );
        assert_eq!(rig.open().len(), 1);
    }

    #[test]
    fn a_change_that_cant_be_read_back_keeps_its_record() {
        let rig = Rig::on(arranged());
        rig.pc.fail_read_after_write(AUTO_GAME_MODE);

        let item = rig.apply(GameMode);

        assert!(!item.ok);
        assert!(
            item.message.contains("read this setting back"),
            "{}",
            item.message
        );
        assert_eq!(rig.story(), ["intent game-mode", "applied game-mode"]);
        assert_eq!(rig.open().len(), 1, "Kestrel can't tell that it's back");
        assert_eq!(rig.state(GameMode), CouldntRead);

        // It did go back, and the first look that can read it says so.
        rig.pc.clear_failures();
        assert_eq!(rig.pc.dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE), Some(0));
        assert_eq!(rig.state(GameMode), Available);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn a_value_that_means_both_what_kestrel_wrote_and_what_it_found_still_holds() {
        // The strings already say acceleration is off, and the mouse in force says it's on.
        let pc = arranged();
        pc.set_string(SID, MOUSE_KEY, MOUSE_SPEED, "0");
        pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_1, "0");
        pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_2, "0");
        let rig = Rig::on(pc);
        let before = rig.pc.dump();
        assert_eq!(rig.state(PointerPrecision), Available);

        assert!(rig.apply(PointerPrecision).ok);

        assert_eq!(rig.state(PointerPrecision), Applied);
        assert_eq!(rig.open().len(), 1, "closing it would lose the undo");
        assert_eq!(rig.engine.applied_ids(), ["pointer-precision"]);
        assert!(rig.undo(PointerPrecision).ok);
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_setting_a_failed_write_half_changed_is_put_back() {
        // Windows saved the first two mouse strings and refused the third, and refuses it again on the way back.
        let rig = Rig::on(with_differing_accounts(arranged()));
        rig.pc.fail_write(MOUSE_THRESHOLD_2);
        let before = rig.pc.dump();

        let item = rig.apply(PointerPrecision);

        assert!(!item.ok);
        assert!(
            item.message.contains("told that writing"),
            "{}",
            item.message
        );
        assert_eq!(rig.pc.dump(), before, "the two that went through are back");
        // Kestrel can't be sure of that with an error in hand, so the record stays until a look at the setting says so.
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.state(PointerPrecision), Available);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn one_failure_doesnt_stop_the_others() {
        let rig = Rig::on(arranged());
        rig.pc.fail_write(AUTO_GAME_MODE);

        let items = rig
            .engine
            .apply(&input(), &[GameMode, BackgroundRecording])
            .unwrap();

        assert_eq!(items[0].id, GameMode);
        assert!(!items[0].ok);
        assert_eq!(items[1].id, BackgroundRecording);
        assert!(items[1].ok, "{}", items[1].message);
        assert_eq!(rig.pc.dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE), Some(0));
        assert_eq!(rig.pc.dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE), Some(0));
        assert_eq!(rig.state(GameMode), Available);
        assert_eq!(rig.state(BackgroundRecording), Applied);
    }

    #[test]
    fn already_set_writes_nothing_and_records_nothing() {
        let rig = Rig::on(FakeSystem::new());
        let before = rig.pc.dump();

        let item = rig.apply(GameMode);

        assert!(!item.ok);
        assert_eq!(item.message, ALREADY_SET_TEXT);
        assert_eq!(item.title, "Game Mode");
        assert!(rig.story().is_empty());
        assert_eq!(rig.pc.dump(), before);
        assert!(rig.pc.writes().is_empty());
    }

    #[test]
    fn a_row_that_cant_be_applied_says_why_and_is_left_alone() {
        // An organisation's policy.
        let rig = Rig::on(arranged());
        rig.pc
            .set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_AUTO_GAME_MODE, 0);
        let item = rig.apply(GameMode);
        assert!(!item.ok);
        assert_eq!(item.message, "An organisation policy turns Game Mode off.");
        // Nothing to do on this PC.
        let one_gpu = PlanInput {
            gpu_choice: false,
            ..input()
        };
        let items = rig.engine.apply(&one_gpu, &[GpuPreference]).unwrap();
        assert!(!items[0].ok);
        assert_eq!(items[0].message, "One graphics card: nothing to choose.");
        let no_game = PlanInput {
            valorant_exe: None,
            ..input()
        };
        let items = rig.engine.apply(&no_game, &[GpuPreference]).unwrap();
        assert_eq!(items[0].message, "VALORANT not found.");
        // Hidden on this build, and a setting Apply never handles.
        let old = PlanInput {
            build: 19045,
            ..input()
        };
        let items = rig
            .engine
            .apply(&old, &[WindowedOptimizations, SystemProtection])
            .unwrap();
        assert!(items
            .iter()
            .all(|item| !item.ok && !item.message.is_empty()));

        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());
    }

    #[test]
    fn a_failed_read_writes_nothing() {
        for id in SETTINGS {
            let rig = Rig::on(arranged());
            rig.pc.fail_read(value_of(id));
            let before = rig.pc.dump();

            let item = rig.apply(id);

            assert!(!item.ok, "{id:?}");
            assert!(
                item.message.contains("told that reading"),
                "{id:?} says why: {}",
                item.message
            );
            assert!(rig.pc.writes().is_empty(), "{id:?}");
            assert!(rig.story().is_empty(), "{id:?}");
            assert_eq!(rig.pc.dump(), before, "{id:?}");
        }
        // The mouse in force, and the plan in use. And what the row depends on besides the value, which reads fine:
        // not knowing what a policy says, or whether capture is on, is no reason to change anything either.
        for (id, read) in [
            (PointerPrecision, MOUSE_LIVE),
            (PowerPlan, ACTIVE_PLAN),
            (GameMode, ALLOW_AUTO_GAME_MODE),
            (BackgroundRecording, ALLOW_GAME_DVR),
            (BackgroundRecording, APP_CAPTURE),
            (PowerPlan, POWER_POLICY_VALUE),
        ] {
            let rig = Rig::on(arranged());
            rig.pc.fail_read(read);
            assert_eq!(rig.state(id), CouldntRead, "{id:?}, {read}");
            let item = rig.apply(id);
            assert!(!item.ok, "{id:?}, {read}");
            assert!(rig.pc.writes().is_empty(), "{id:?}, {read}");
            assert!(rig.story().is_empty(), "{id:?}, {read}");
        }
    }

    #[test]
    fn a_wrong_typed_value_is_left_in_place_by_apply() {
        // The Review Focus: a value of another type isn't absent, and writing over it would lose it.
        let rig = Rig::on(arranged());
        rig.pc.set_string(SID, GAME_BAR_KEY, AUTO_GAME_MODE, "1");
        assert_eq!(rig.state(GameMode), CouldntRead);

        let item = rig.apply(GameMode);

        assert!(!item.ok);
        assert!(item.message.contains("not a DWORD"), "{}", item.message);
        assert_eq!(
            rig.pc.string(SID, GAME_BAR_KEY, AUTO_GAME_MODE).as_deref(),
            Some("1")
        );
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());

        // The same for the other settings.
        let rig = Rig::on(arranged());
        rig.pc
            .set_string(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, "1");
        rig.pc.set_dword(SID, DIRECTX_KEY, DIRECTX_SETTINGS, 7);
        rig.pc.set_dword(SID, DIRECTX_KEY, EXE, 7);
        rig.pc.set_dword(SID, MOUSE_KEY, MOUSE_SPEED, 1);
        let before = rig.pc.dump();
        for id in [
            BackgroundRecording,
            WindowedOptimizations,
            GpuPreference,
            PointerPrecision,
        ] {
            assert_eq!(rig.state(id), CouldntRead, "{id:?}");
            assert!(!rig.apply(id).ok, "{id:?}");
        }
        assert_eq!(rig.pc.dump(), before);
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());
    }

    #[test]
    fn a_wrong_typed_value_is_left_in_place_by_every_undo() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        rig.pc.set_string(SID, GAME_BAR_KEY, AUTO_GAME_MODE, "1");
        let writes = rig.pc.writes().len();
        assert_eq!(rig.state(GameMode), CouldntRead);

        let undone = rig.undo(GameMode);
        assert!(!undone.ok);
        assert!(undone.message.contains("not a DWORD"), "{}", undone.message);
        let restored = rig.engine.restore_anyway(GameMode).unwrap();
        assert!(!restored.ok);
        assert!(
            restored.message.contains("not a DWORD"),
            "{}",
            restored.message
        );
        let items = rig.engine.revert_all().unwrap();
        assert!(!items[0].ok);
        let again = rig.engine.apply_again(&input(), GameMode).unwrap();
        assert!(!again.ok);

        assert_eq!(rig.pc.writes().len(), writes, "nothing was written");
        assert_eq!(
            rig.pc.string(SID, GAME_BAR_KEY, AUTO_GAME_MODE).as_deref(),
            Some("1")
        );
        assert_eq!(
            rig.open().len(),
            1,
            "the record waits for a value it can read"
        );
    }

    #[test]
    fn a_refused_journal_means_no_change() {
        let rig = Rig::on(arranged());
        let other = rig.journal();
        let held = other.lock().unwrap();
        let before = rig.pc.dump();

        let refused = rig.engine.apply(&input(), &TweakId::ESSENTIAL).unwrap_err();

        assert_eq!(refused.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(rig.pc.dump(), before);
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());

        drop(held);
        assert!(rig.apply(GameMode).ok, "free again");
    }

    #[test]
    fn every_undo_button_is_refused_while_the_journal_is_held() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        assert!(rig.apply(PowerPlan).ok);
        let other = rig.journal();
        let held = other.lock().unwrap();
        let before = rig.pc.dump();
        let entries = rig.story();

        let engine = &rig.engine;
        let results = [
            engine.undo(GameMode).map(|_| ()),
            engine.revert_all().map(|_| ()),
            engine.apply_again(&input(), GameMode).map(|_| ()),
            engine.restore_anyway(GameMode).map(|_| ()),
            engine.use_plan_again().map(|_| ()),
            engine.remove_plan().map(|_| ()),
        ];

        for result in results {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
        }
        assert_eq!(rig.pc.dump(), before);
        assert_eq!(rig.story(), entries);
        drop(held);
    }

    #[test]
    fn a_journal_that_cant_be_read_stops_apply() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[]);
        std::fs::create_dir(rig.folder().join("journal.jsonl")).unwrap();
        let before = rig.pc.dump();

        assert!(rig.engine.apply(&input(), &[GameMode]).is_err());

        assert_eq!(rig.pc.dump(), before);
    }

    /// A journal that lets the first looks at its folder through and refuses the rest: it stops working part-way.
    fn failing_journal(dir: &Path, checks_left: usize) -> JournalFile {
        hooked_journal(dir, move |look| {
            if look < checks_left {
                Ok(())
            } else {
                Err(io::Error::other("the journal stopped working"))
            }
        })
    }

    #[test]
    fn a_journal_that_stops_working_during_apply_costs_no_undo() {
        // Lock, read, then two entries for each of six items: more than Apply can ask for.
        for checks_left in 0..16 {
            let rig = Rig::on(arranged());
            let before = rig.pc.dump();
            let dying = Engine::new(rig.pc.clone(), failing_journal(&rig.folder(), checks_left));

            match dying.apply(&input(), &TweakId::ESSENTIAL) {
                Err(why) => {
                    assert!(why.to_string().contains("stopped working"), "{why}");
                    assert_eq!(rig.pc.dump(), before, "an error means no change");
                }
                Ok(items) => {
                    assert_ne!(rig.pc.dump(), before, "no change would have been an error");
                    assert_eq!(items.len(), 6, "after {checks_left} checks");
                    for item in items.iter().filter(|item| !item.ok) {
                        assert!(
                            item.message.contains("stopped working"),
                            "after {checks_left} checks: {}",
                            item.message
                        );
                    }
                }
            }

            // With the journal working again, everything that was changed can still be undone.
            let engine = rig.restarted();
            engine.view(&input(), Gate::Open).unwrap();
            let items = engine.revert_all().unwrap();
            assert!(
                items.iter().all(|item| item.ok),
                "after {checks_left} checks: {items:?}"
            );
            assert_eq!(rig.pc.dump(), before, "after {checks_left} checks");
            assert!(!engine.has_open_records(), "after {checks_left} checks");
        }
    }

    #[test]
    fn a_record_that_cant_be_closed_stops_apply_before_it_changes_anything() {
        // Kestrel's plan went outside Kestrel, and the record saying so can't be closed. If Apply went on, the plan it
        // makes would count as that old record's, and an undo would go back to the plan that record names.
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(HIGH_PERFORMANCE);
        rig.pc.delete_plan(KESTREL).unwrap();
        let before = rig.pc.dump();
        // The journal refuses just the one entry: the lock and the read are the looks before it.
        let refusing_the_first_entry = |dir: &Path| {
            hooked_journal(dir, |look| {
                if look == 2 {
                    Err(io::Error::other("the journal refused that one"))
                } else {
                    Ok(())
                }
            })
        };
        let engine = Engine::new(rig.pc.clone(), refusing_the_first_entry(&rig.folder()));

        let refused = engine.apply(&input(), &[PowerPlan]).unwrap_err();

        assert_eq!(refused.to_string(), "the journal refused that one");
        assert_eq!(rig.pc.dump(), before);
        assert_eq!(rig.story().len(), 2);

        // The same for Apply again, which also starts from a settled journal.
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        put_back_by_hand(&rig.pc, GameMode);
        let engine = Engine::new(rig.pc.clone(), refusing_the_first_entry(&rig.folder()));

        assert!(engine.apply_again(&input(), GameMode).is_err());
    }

    #[test]
    fn a_journal_that_wont_take_the_intent_stops_the_other_buttons_that_change_things() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        assert!(rig.apply(PowerPlan).ok);
        change_elsewhere(&rig.pc, GameMode);
        rig.pc.set_active(BALANCED);
        let held = rig.pc.dump();
        let writes = rig.pc.writes().len();
        // The lock and the read go through.
        let engine = Engine::new(rig.pc.clone(), failing_journal(&rig.folder(), 2));
        assert!(engine.apply_again(&input(), GameMode).is_err());
        let engine = Engine::new(rig.pc.clone(), failing_journal(&rig.folder(), 2));
        assert!(engine.use_plan_again().is_err());

        assert_eq!(rig.pc.dump(), held);
        assert_eq!(rig.pc.writes().len(), writes);
    }

    #[test]
    fn a_journal_that_stops_working_during_undo_heals_on_the_next_read() {
        for id in TweakId::ESSENTIAL {
            for checks_left in 0..3 {
                let rig = Rig::on(arranged());
                let before = rig.pc.dump();
                assert!(rig.apply(id).ok);
                let dying =
                    Engine::new(rig.pc.clone(), failing_journal(&rig.folder(), checks_left));

                let result = dying.undo(id);

                let engine = rig.restarted();
                if result.is_ok_and(|item| item.ok) {
                    assert_eq!(rig.pc.dump(), before, "{id:?}, {checks_left} checks");
                    engine.view(&input(), Gate::Open).unwrap();
                } else {
                    assert_ne!(rig.pc.dump(), before, "{id:?}, {checks_left} checks");
                    assert!(engine.undo(id).unwrap().ok);
                }
                assert!(rig.open().is_empty(), "{id:?}, {checks_left} checks");
                assert_eq!(rig.pc.dump(), before, "{id:?}, {checks_left} checks");
            }
        }
    }

    // What a look at the journal settles.

    #[test]
    fn a_value_back_at_the_prior_closes_quietly() {
        for id in SETTINGS {
            let rig = Rig::on(arranged());
            assert!(rig.apply(id).ok);
            put_back_by_hand(&rig.pc, id);
            let held = rig.pc.dump();
            let writes = rig.pc.writes().len();

            let view = rig.view();

            assert_eq!(row_in(&view, id).state, Available, "{id:?}");
            assert!(view.interrupted.is_empty(), "{id:?}: no crash here");
            assert!(!view.any_active, "{id:?}");
            assert_eq!(rig.pc.dump(), held, "{id:?}: the view wrote to Windows");
            assert_eq!(rig.pc.writes().len(), writes, "{id:?}");
            assert_eq!(
                rig.story().last().unwrap(),
                &format!("closed {} WentBack", id.as_str()),
                "{id:?}"
            );
            assert!(rig.open().is_empty(), "{id:?}");
        }
    }

    #[test]
    fn a_third_value_flags_the_row_and_undo_leaves_it() {
        // The windowed token, absent before Kestrel, now says 0.
        let rig = Rig::on(arranged());
        assert!(rig.apply(WindowedOptimizations).ok);
        let third = "VRROptimizeEnable=1;SwapEffectUpgradeEnable=0;";
        rig.pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, third);
        assert_eq!(rig.state(WindowedOptimizations), ChangedElsewhere);
        let writes = rig.pc.writes().len();

        let item = rig.undo(WindowedOptimizations);

        assert!(!item.ok);
        assert_eq!(item.message, LEFT_ALONE_TEXT);
        assert_eq!(
            rig.pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some(third)
        );
        assert_eq!(rig.pc.writes().len(), writes);
        assert_eq!(rig.state(WindowedOptimizations), ChangedElsewhere);
        assert_eq!(rig.open().len(), 1, "the row still has its buttons");

        // Each setting's third value is flagged the same way, and it never closes on its own.
        for id in SETTINGS {
            let rig = Rig::on(arranged());
            assert!(rig.apply(id).ok);
            change_elsewhere(&rig.pc, id);
            let view = rig.view();
            assert_eq!(row_in(&view, id).state, ChangedElsewhere, "{id:?}");
            assert!(view.any_active, "{id:?}");
            let undone = rig.undo(id);
            assert_eq!(undone.message, LEFT_ALONE_TEXT, "{id:?}");
            assert!(!undone.ok, "{id:?}");
            assert_eq!(rig.story(), says(&[("intent", id), ("applied", id)]));
        }
    }

    #[test]
    fn undo_finding_the_value_back_closes_without_writing() {
        for id in SETTINGS {
            let rig = Rig::on(arranged());
            assert!(rig.apply(id).ok);
            put_back_by_hand(&rig.pc, id);
            let writes = rig.pc.writes().len();

            let item = rig.undo(id);

            assert!(item.ok, "{id:?}: {}", item.message);
            assert_eq!(rig.pc.writes().len(), writes, "{id:?}");
            assert_eq!(
                rig.story().last().unwrap(),
                &format!("closed {} WentBack", id.as_str())
            );
            assert!(rig.open().is_empty(), "{id:?}");
        }
    }

    #[test]
    fn undo_with_nothing_open_changes_nothing() {
        let rig = Rig::on(arranged());
        for id in TweakId::ESSENTIAL {
            let item = rig.undo(id);
            assert!(!item.ok, "{id:?}");
            assert!(!item.message.is_empty(), "{id:?}");
        }
        assert!(!rig.undo(SystemProtection).ok);
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());
    }

    #[test]
    fn a_failed_undo_keeps_the_record() {
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(GameMode).ok);
        rig.pc.fail_write(AUTO_GAME_MODE);

        let item = rig.undo(GameMode);

        assert!(!item.ok);
        assert!(
            item.message.contains("told that writing"),
            "{}",
            item.message
        );
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.pc.dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE), Some(1));

        rig.pc.clear_failures();
        assert!(rig.undo(GameMode).ok, "it can be tried again");
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_mouse_undo_windows_refuses_keeps_its_record_until_it_works() {
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(PointerPrecision).ok);
        rig.pc.fail_write(MOUSE_LIVE);

        let item = rig.undo(PointerPrecision);

        assert!(!item.ok);
        assert!(
            item.message.contains("told that writing"),
            "{}",
            item.message
        );
        assert_eq!(rig.pc.mouse_live(), Read::Value([0, 0, 0]));

        // Looking doesn't lose it, whether or not Windows has come round.
        assert_eq!(rig.state(PointerPrecision), Applied);
        rig.pc.clear_failures();
        assert_eq!(rig.state(PointerPrecision), Applied);
        assert_eq!(
            rig.story(),
            says(&[("intent", PointerPrecision), ("applied", PointerPrecision)])
        );

        let item = rig.undo(PointerPrecision);

        assert!(item.ok, "{}", item.message);
        assert_eq!(rig.pc.dump(), before);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn a_mouse_windows_says_it_set_and_didnt_is_read_back() {
        // Applying it: nothing changed, so nothing is put back and nothing says it was.
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        rig.pc.ignore_mouse();

        let item = rig.apply(PointerPrecision);

        assert!(!item.ok);
        assert_eq!(item.message, "Windows didn't keep the change.");
        assert_eq!(rig.pc.dump(), before);
        assert!(rig.open().is_empty());

        // Undoing it: the strings went back and the mouse in force is still Kestrel's, so it isn't undone.
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(PointerPrecision).ok);
        rig.pc.ignore_mouse();

        let item = rig.undo(PointerPrecision);

        assert!(!item.ok);
        assert_eq!(item.message, "Windows didn't keep the old value.");
        assert_eq!(rig.pc.mouse_live(), Read::Value([0, 0, 0]));
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.state(PointerPrecision), ChangedElsewhere);

        rig.pc.clear_failures();
        assert!(rig.engine.restore_anyway(PointerPrecision).unwrap().ok);
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_crash_between_a_mouse_undos_two_writes_leaves_the_undo_to_try_again() {
        // The mouse in force is back, and the strings are still what Kestrel wrote.
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(PointerPrecision).ok);
        rig.pc.set_mouse([6, 10, 1]);

        let engine = rig.restarted();
        let view = engine.view(&input(), Gate::Open).unwrap();

        assert_eq!(row_in(&view, PointerPrecision).state, Applied);
        assert_eq!(rig.open().len(), 1);
        assert!(engine.undo(PointerPrecision).unwrap().ok);
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_record_stays_open_while_the_mouse_in_force_is_still_kestrels() {
        // The strings are back at what Kestrel found and the mouse in force isn't: a tool wrote the registry, say.
        let strings_back = |pc: &FakeSystem| {
            put_back_by_hand(pc, PointerPrecision);
            pc.set_mouse([0, 0, 0]);
        };
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(PointerPrecision).ok);
        strings_back(&rig.pc);
        let entries = rig.story();
        let held = rig.pc.dump();

        assert_eq!(rig.state(PointerPrecision), ChangedElsewhere);
        assert_eq!(rig.story(), entries, "the record stays");
        assert!(rig.engine.has_open_records());
        assert!(rig.engine.applied_ids().is_empty());
        // Undo leaves it alone and nothing has been written, as with any third value.
        let item = rig.undo(PointerPrecision);
        assert!(!item.ok);
        assert_eq!(item.message, LEFT_ALONE_TEXT);
        assert_eq!(rig.pc.dump(), held);
        // Restore anyway is what puts the mouse back.
        assert!(rig.engine.restore_anyway(PointerPrecision).unwrap().ok);
        assert_eq!(rig.pc.dump(), before);
        assert!(rig.open().is_empty());

        // After a crash it isn't a change that never happened either.
        let rig = Rig::on(arranged());
        strings_back(&rig.pc);
        let found = Saved::Mouse {
            speed: Some("1".to_string()),
            threshold1: Some("6".to_string()),
            threshold2: Some("10".to_string()),
            live: [6, 10, 1],
        };
        let planned = targets::target(PointerPrecision, &found);
        rig.left_by_an_earlier_run(&[intent(PointerPrecision, Some(SID), found, planned)]);

        let view = rig.view();

        assert_eq!(row_in(&view, PointerPrecision).state, ChangedElsewhere);
        assert_eq!(
            view.interrupted,
            [interrupted(PointerPrecision, "Something else changed it.")]
        );
        assert_eq!(rig.open().len(), 1);

        // When the accounts differ Kestrel never touched the mouse in force, so it says nothing.
        let rig = Rig::on(with_differing_accounts(arranged()));
        assert!(rig.apply(PointerPrecision).ok);
        put_back_by_hand(&rig.pc, PointerPrecision);
        rig.pc.set_mouse([0, 0, 0]);

        assert_eq!(rig.state(PointerPrecision), Available);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn an_undo_windows_didnt_keep_keeps_the_record() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        rig.pc.lie_on_readback(AUTO_GAME_MODE);

        let item = rig.undo(GameMode);

        assert!(!item.ok);
        assert!(item.message.contains("didn't keep"), "{}", item.message);
        assert_eq!(rig.open().len(), 1);
        // It did go back, and the next look says so.
        assert_eq!(rig.state(GameMode), Available);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn an_undo_that_cant_be_read_back_keeps_the_record() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        rig.pc.fail_read_after_write(AUTO_GAME_MODE);

        let item = rig.undo(GameMode);

        assert!(!item.ok);
        assert!(
            item.message.contains("read this setting back"),
            "{}",
            item.message
        );
        assert_eq!(rig.open().len(), 1);

        rig.pc.clear_failures();
        assert_eq!(rig.state(GameMode), Available);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn later_token_changes_survive_undo() {
        // Kestrel made the value and the key, and something else has put a token of its own in since.
        let rig = Rig::on(FakeSystem::new());
        assert!(rig.apply(WindowedOptimizations).ok);
        assert_eq!(
            rig.pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("SwapEffectUpgradeEnable=1;")
        );
        rig.pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "SwapEffectUpgradeEnable=1;VRROptimizeEnable=1;",
        );
        assert_eq!(rig.state(WindowedOptimizations), Applied);

        let item = rig.undo(WindowedOptimizations);

        assert!(item.ok, "{}", item.message);
        assert_eq!(
            rig.pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("VRROptimizeEnable=1;")
        );
    }

    #[test]
    fn reads_only_ever_touch_the_journal() {
        // Everything reading can settle, on a PC where each record is in a different state.
        let rig = Rig::on(arranged());
        for id in TweakId::ESSENTIAL {
            assert!(rig.apply(id).ok, "{id:?}");
        }
        put_back_by_hand(&rig.pc, GameMode);
        change_elsewhere(&rig.pc, BackgroundRecording);
        rig.pc.set_active(BALANCED);
        let held = rig.pc.dump();
        let writes = rig.pc.writes().len();

        rig.view();
        rig.engine.applied_ids();
        rig.engine.has_open_records();
        rig.engine.preview(&input(), &TweakId::ESSENTIAL);

        assert_eq!(rig.pc.dump(), held);
        assert_eq!(rig.pc.writes().len(), writes);
    }

    #[test]
    fn reading_on_a_pc_that_never_applied_makes_no_folder() {
        let rig = Rig::on(arranged());

        let view = rig.view();
        rig.engine.preview(&input(), &TweakId::ESSENTIAL);
        assert!(rig.engine.applied_ids().is_empty());
        assert!(!rig.engine.has_open_records());

        assert!(!view.any_active);
        assert!(!rig.folder().exists());
        assert!(rig.pc.writes().is_empty());
    }

    #[test]
    fn a_held_lock_only_puts_off_what_reconcile_writes() {
        // A crash's leftovers, a record to close, and a process that holds the journal, which may be the one that wrote
        // that intent and is about to change the value: nothing can be written, and no one is told of an interruption.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[
            game_mode_intent(),
            intent(BackgroundRecording, Some(SID), dword(1), dword(0)),
            outcome(BackgroundRecording, Some(SID)),
        ]);
        let entries = rig.story();
        let other = rig.journal();
        let held = other.lock().unwrap();

        let view = rig.view();

        assert_eq!(rig.story(), entries, "the journal is the other process's");
        assert!(view.interrupted.is_empty());
        // The rows are judged all the same: both settings are back at what they were.
        assert_eq!(row_in(&view, GameMode).state, Available);
        assert_eq!(row_in(&view, BackgroundRecording).state, Available);
        assert!(rig.engine.applied_ids().is_empty());
        assert!(
            rig.engine.has_open_records(),
            "the file still holds them, and that's what this counts"
        );
        assert_eq!(rig.story(), entries);

        drop(held);
        let view = rig.view();
        assert_eq!(
            view.interrupted,
            [interrupted(GameMode, "It didn't change.")]
        );
        assert_eq!(
            rig.story()[3..],
            [
                "closed game-mode NotApplied",
                "closed background-recording WentBack"
            ]
        );
        assert!(rig.open().is_empty());
        assert!(!rig.engine.has_open_records());
    }

    #[test]
    fn a_change_another_process_finishes_isnt_taken_for_a_crash() {
        // The first read finds an intent with no outcome. By the time Kestrel has the lock, the process that wrote the
        // intent has written its outcome and let go.
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        let other = rig.journal();
        let journal = hooked_journal(&rig.folder(), move |look| {
            if look == 1 {
                other.append(&outcome(GameMode, Some(SID)))?;
            }
            Ok(())
        });
        let engine = Engine::new(rig.pc.clone(), journal);

        let view = engine.view(&input(), Gate::Open).unwrap();

        assert!(view.interrupted.is_empty(), "{:?}", view.interrupted);
        assert_eq!(row_in(&view, GameMode).state, Applied);
        assert_eq!(rig.story(), ["intent game-mode", "applied game-mode"]);
    }

    #[test]
    fn a_journal_that_wont_take_a_write_during_reconcile_is_ignored() {
        // There is no one to tell: the look goes on, and the next one tries again.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        // Enough for the read, the lock and the read under the lock, and not for the entry.
        let dying = Engine::new(rig.pc.clone(), failing_journal(&rig.folder(), 3));

        let view = dying.view(&input(), Gate::Open).unwrap();

        assert_eq!(row_in(&view, GameMode).state, Available);
        assert!(view.interrupted.is_empty(), "nothing was settled yet");
        assert_eq!(rig.story(), ["intent game-mode"]);

        let view = rig.view();
        assert_eq!(
            view.interrupted,
            [interrupted(GameMode, "It didn't change.")]
        );
        assert_eq!(
            rig.story(),
            ["intent game-mode", "closed game-mode NotApplied"]
        );

        // A record whose closing entry didn't reach the file is still in the file, and still counts.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        // The read, the lock, the read under the lock, and then the entry, which is refused. The answer's own read
        // isn't.
        let refusing_the_entry = hooked_journal(&rig.folder(), |look| {
            if look == 3 {
                Err(io::Error::other("the journal refused that one"))
            } else {
                Ok(())
            }
        });
        let engine = Engine::new(rig.pc.clone(), refusing_the_entry);

        assert!(engine.has_open_records());
        assert_eq!(rig.story(), ["intent game-mode"]);
    }

    #[test]
    fn a_journal_that_cant_be_read_is_the_views_error() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[]);
        std::fs::create_dir(rig.folder().join("journal.jsonl")).unwrap();

        assert!(rig.engine.view(&input(), Gate::Open).is_err());
        assert!(
            rig.engine.applied_ids().is_empty(),
            "nothing is known to hold"
        );
        assert!(rig.engine.has_open_records(), "and something may be open");
        assert!(rig.engine.preview(&input(), &[GameMode]).items.is_empty());
    }

    // Restore anyway and Apply again.

    #[test]
    fn restore_anyway_writes_the_prior() {
        for id in SETTINGS {
            let rig = Rig::on(arranged());
            let before = rig.pc.dump();
            assert!(rig.apply(id).ok);
            change_elsewhere(&rig.pc, id);

            let item = rig.engine.restore_anyway(id).unwrap();

            assert!(item.ok, "{id:?}: {}", item.message);
            assert_eq!(item.message, "Undone.");
            assert_eq!(rig.pc.dump(), before, "{id:?}");
            assert_eq!(
                rig.story().last().unwrap(),
                &format!("undone {}", id.as_str())
            );
            assert_eq!(rig.state(id), Available, "{id:?}");
        }
    }

    #[test]
    fn restore_anyway_judges_the_record_as_undo_does() {
        // Back at what it was by the time the button is pressed: nothing to write.
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        put_back_by_hand(&rig.pc, GameMode);
        let writes = rig.pc.writes().len();

        let item = rig.engine.restore_anyway(GameMode).unwrap();

        assert!(item.ok, "{}", item.message);
        assert_eq!(rig.pc.writes().len(), writes);
        assert_eq!(rig.story().last().unwrap(), "closed game-mode WentBack");

        // Still what Kestrel wrote: an ordinary undo.
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(GameMode).ok);

        let item = rig.engine.restore_anyway(GameMode).unwrap();

        assert!(item.ok, "{}", item.message);
        assert_eq!(rig.pc.dump(), before);
        assert_eq!(rig.story().last().unwrap(), "undone game-mode");
    }

    #[test]
    fn restore_anyway_removes_what_kestrel_made_and_keeps_what_came_later() {
        let rig = Rig::on(FakeSystem::new());
        let before = rig.pc.dump();
        assert!(rig.apply(WindowedOptimizations).ok);
        rig.pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "SwapEffectUpgradeEnable=0;",
        );

        assert!(rig.engine.restore_anyway(WindowedOptimizations).unwrap().ok);

        assert_eq!(rig.pc.dump(), before, "the value and key were Kestrel's");

        let rig = Rig::on(FakeSystem::new());
        assert!(rig.apply(WindowedOptimizations).ok);
        rig.pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "SwapEffectUpgradeEnable=0;AutoHDREnable=1;",
        );

        assert!(rig.engine.restore_anyway(WindowedOptimizations).unwrap().ok);

        assert_eq!(
            rig.pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("AutoHDREnable=1;")
        );
    }

    #[test]
    fn restore_anyway_that_fails_keeps_the_record() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        change_elsewhere(&rig.pc, GameMode);
        rig.pc.fail_write(AUTO_GAME_MODE);

        let item = rig.engine.restore_anyway(GameMode).unwrap();

        assert!(!item.ok);
        assert!(
            item.message.contains("told that writing"),
            "{}",
            item.message
        );
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.pc.dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE), Some(2));
    }

    #[test]
    fn a_mouse_restore_anyway_windows_refuses_keeps_its_record_until_it_works() {
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(PointerPrecision).ok);
        change_elsewhere(&rig.pc, PointerPrecision);
        rig.pc.fail_write(MOUSE_LIVE);

        let item = rig.engine.restore_anyway(PointerPrecision).unwrap();

        assert!(!item.ok);
        assert!(
            item.message.contains("told that writing"),
            "{}",
            item.message
        );
        assert_eq!(rig.state(PointerPrecision), ChangedElsewhere);
        rig.pc.clear_failures();
        assert_eq!(rig.state(PointerPrecision), ChangedElsewhere);
        assert_eq!(rig.open().len(), 1);

        let item = rig.engine.restore_anyway(PointerPrecision).unwrap();

        assert!(item.ok, "{}", item.message);
        assert_eq!(rig.pc.dump(), before);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn restore_anyway_is_only_for_the_five_settings() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.left_by_an_earlier_run(&protection_record());
        let writes = rig.pc.writes().len();

        for id in [PowerPlan, SystemProtection] {
            let item = rig.engine.restore_anyway(id).unwrap();
            assert!(!item.ok, "{id:?}");
            assert_eq!(rig.open().len(), 2, "{id:?}");
        }
        assert!(rig
            .engine
            .restore_anyway(PowerPlan)
            .unwrap()
            .message
            .contains("only for"));
        // And there has to be something to restore.
        assert!(!rig.engine.restore_anyway(GameMode).unwrap().ok);
        assert_eq!(rig.pc.writes().len(), writes);
    }

    #[test]
    fn apply_again_keeps_the_first_prior() {
        for id in SETTINGS {
            for pc in ways_to_be_available(id) {
                let rig = Rig::on(pc);
                let before = rig.pc.dump();
                assert!(rig.apply(id).ok, "{id:?}");
                let first = rig.open()[0].clone();
                change_elsewhere(&rig.pc, id);
                assert_eq!(rig.state(id), ChangedElsewhere, "{id:?}");

                let found = rig.pc.dump();

                let item = rig.engine.apply_again(&input(), id).unwrap();

                assert!(item.ok, "{id:?}: {}", item.message);
                assert_eq!(item.message, "Applied.");
                assert_ne!(rig.pc.dump(), found, "{id:?}");
                assert_eq!(rig.state(id), Applied, "{id:?}");
                assert_eq!(
                    rig.story(),
                    says(&[
                        ("intent", id),
                        ("applied", id),
                        ("intent", id),
                        ("reapplied", id)
                    ])
                );
                // The second intent says what it wrote over, and plans what the first one did.
                let history = rig.journal().read().unwrap().history;
                let Entry::Intent { prior, planned, .. } = &history[2] else {
                    panic!("{history:?}");
                };
                assert_ne!(*prior, first.prior, "{id:?}");
                assert_eq!(*planned, first.planned, "{id:?}");
                assert_eq!(rig.open(), [first], "{id:?}: the record is as it was");

                let undone = rig.undo(id);
                assert!(undone.ok, "{id:?}: {}", undone.message);
                assert_eq!(rig.pc.dump(), before, "{id:?}: back to the first prior");
            }
        }
    }

    #[test]
    fn apply_leaves_a_flagged_row_alone_and_apply_again_is_the_asking() {
        for id in SETTINGS {
            let rig = Rig::on(arranged());
            let before = rig.pc.dump();
            assert!(rig.apply(id).ok, "{id:?}");
            change_elsewhere(&rig.pc, id);
            let held = rig.pc.dump();
            let writes = rig.pc.writes().len();
            let entries = rig.story();

            let item = rig.apply(id);

            assert!(!item.ok, "{id:?}");
            assert_eq!(
                item.message, "Something else changed this after Kestrel did.",
                "{id:?}"
            );
            assert_eq!(rig.pc.dump(), held, "{id:?}: the third value stays");
            assert_eq!(rig.pc.writes().len(), writes, "{id:?}");
            assert_eq!(rig.story(), entries, "{id:?}");
            assert_eq!(rig.state(id), ChangedElsewhere, "{id:?}");

            let item = rig.engine.apply_again(&input(), id).unwrap();

            assert!(item.ok, "{id:?}: {}", item.message);
            assert_eq!(rig.state(id), Applied, "{id:?}");
            assert!(rig.undo(id).ok, "{id:?}");
            assert_eq!(rig.pc.dump(), before, "{id:?}: back to the first prior");
        }
    }

    #[test]
    fn apply_again_acts_only_on_a_flagged_row() {
        let rig = Rig::on(arranged());
        // Nothing open: there's no record to apply again.
        let item = rig.engine.apply_again(&input(), GameMode).unwrap();
        assert!(!item.ok);
        assert!(rig.pc.writes().is_empty());

        // Still as Kestrel left it.
        assert!(rig.apply(GameMode).ok);
        let writes = rig.pc.writes().len();
        let item = rig.engine.apply_again(&input(), GameMode).unwrap();
        assert!(!item.ok);
        assert_eq!(item.message, ALREADY_SET_TEXT);
        assert_eq!(rig.pc.writes().len(), writes);

        // The plan and System Protection have buttons of their own.
        assert!(rig.apply(PowerPlan).ok);
        rig.left_by_an_earlier_run(&protection_record());
        let writes = rig.pc.writes().len();
        rig.pc.set_active(BALANCED);
        assert!(!rig.engine.apply_again(&input(), PowerPlan).unwrap().ok);
        assert!(
            !rig.engine
                .apply_again(&input(), SystemProtection)
                .unwrap()
                .ok
        );
        assert_eq!(rig.pc.writes().len(), writes);

        // Back at what it was by the time the button is pressed: there's no record left to apply again.
        put_back_by_hand(&rig.pc, GameMode);
        let item = rig.engine.apply_again(&input(), GameMode).unwrap();
        assert!(!item.ok);
        assert_eq!(rig.pc.writes().len(), writes);
        assert_eq!(rig.state(GameMode), Available);
    }

    #[test]
    fn a_failed_apply_again_leaves_the_record_and_the_third_value() {
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(GameMode).ok);
        change_elsewhere(&rig.pc, GameMode);
        rig.pc.fail_write(AUTO_GAME_MODE);

        let item = rig.engine.apply_again(&input(), GameMode).unwrap();

        assert!(!item.ok);
        assert!(
            item.message.contains("told that writing"),
            "{}",
            item.message
        );
        assert_eq!(rig.pc.dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE), Some(2));
        assert_eq!(rig.state(GameMode), ChangedElsewhere);
        assert_eq!(rig.story()[2..], ["intent game-mode", "failed game-mode"]);
        assert!(rig.open()[0].settled, "as it was before the attempt");

        rig.pc.clear_failures();
        assert!(rig.engine.restore_anyway(GameMode).unwrap().ok);
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_lying_readback_on_apply_again_puts_the_third_value_back() {
        for id in [GameMode, GpuPreference] {
            let rig = Rig::on(arranged());
            assert!(rig.apply(id).ok);
            change_elsewhere(&rig.pc, id);
            let held = rig.pc.dump();
            rig.pc.lie_on_readback(value_of(id));

            let item = rig.engine.apply_again(&input(), id).unwrap();

            assert!(!item.ok, "{id:?}");
            assert!(item.message.contains("didn't keep"), "{}", item.message);
            assert_eq!(rig.pc.dump(), held, "{id:?}");
            assert_eq!(rig.state(id), ChangedElsewhere, "{id:?}");
            assert_eq!(rig.open().len(), 1, "{id:?}");
        }
    }

    #[test]
    fn an_apply_again_that_cant_be_read_back_leaves_the_record_as_it_was() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        change_elsewhere(&rig.pc, GameMode);
        rig.pc.fail_read_after_write(AUTO_GAME_MODE);

        let item = rig.engine.apply_again(&input(), GameMode).unwrap();

        assert!(!item.ok);
        assert!(
            item.message.contains("read this setting back"),
            "{}",
            item.message
        );
        assert_eq!(rig.story().last().unwrap(), "failed game-mode");
        assert_eq!(rig.open()[0].prior, dword(0));
        assert!(rig.open()[0].settled);
    }

    // After a crash.

    #[test]
    fn crash_after_the_write_is_settled_as_applied() {
        let rig = Rig::on(FakeSystem::new());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&[game_mode_intent()]);

        let view = rig.view();

        assert_eq!(row_in(&view, GameMode).state, Applied);
        assert_eq!(view.interrupted, [interrupted(GameMode, "It finished.")]);
        assert_eq!(
            view.interrupted[0],
            "Kestrel was interrupted while changing Game Mode. It finished."
        );
        assert_eq!(rig.story(), ["intent game-mode", "interrupted game-mode"]);
        assert!(rig.open()[0].settled);
        // Settled once: looking again adds nothing.
        assert_eq!(rig.view().interrupted.len(), 1);
        assert_eq!(rig.story().len(), 2);
    }

    #[test]
    fn crash_before_the_write_closes_as_not_applied() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        let held = rig.pc.dump();

        let view = rig.view();

        assert_eq!(row_in(&view, GameMode).state, Available);
        assert_eq!(
            view.interrupted,
            ["Kestrel was interrupted while changing Game Mode. It didn't change."]
        );
        assert_eq!(
            rig.story(),
            ["intent game-mode", "closed game-mode NotApplied"]
        );
        assert!(rig.open().is_empty());
        assert_eq!(rig.pc.dump(), held);
        assert_eq!(rig.view().interrupted.len(), 1);
    }

    #[test]
    fn crash_with_a_third_value_is_flagged() {
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 2);
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        let held = rig.pc.dump();

        let view = rig.view();

        assert_eq!(row_in(&view, GameMode).state, ChangedElsewhere);
        assert_eq!(
            view.interrupted,
            ["Kestrel was interrupted while changing Game Mode. Something else changed it."]
        );
        assert_eq!(rig.story(), ["intent game-mode", "interrupted game-mode"]);
        assert_eq!(rig.pc.dump(), held);
        assert!(rig.open()[0].settled);
    }

    #[test]
    fn crash_with_a_value_that_cant_be_read_waits() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        rig.pc.fail_read(AUTO_GAME_MODE);

        let view = rig.view();

        assert_eq!(row_in(&view, GameMode).state, CouldntRead);
        assert!(view.interrupted.is_empty());
        assert_eq!(rig.story(), ["intent game-mode"]);

        rig.pc.clear_failures();
        assert_eq!(rig.view().interrupted.len(), 1);
    }

    #[test]
    fn a_crash_after_any_line_leaves_every_tweak_undoable() {
        for id in TweakId::ESSENTIAL {
            // With the intent on disk and the change made.
            let rig = Rig::on(arranged());
            let before = rig.pc.dump();
            assert!(rig.apply(id).ok, "{id:?}");
            let first_line = rig.journal().read().unwrap().history[0].clone();
            rig.cut_journal(1);

            let engine = rig.restarted();
            let view = engine.view(&input(), Gate::Open).unwrap();

            assert_eq!(row_in(&view, id).state, Applied, "{id:?}");
            assert_eq!(
                view.interrupted,
                [interrupted(id, "It finished.")],
                "{id:?}"
            );
            assert_eq!(rig.story(), says(&[("intent", id), ("interrupted", id)]));
            assert!(engine.undo(id).unwrap().ok, "{id:?}");
            assert_eq!(rig.pc.dump(), before, "{id:?}");

            // With the intent on disk and nothing changed.
            let rig = Rig::on(arranged());
            rig.left_by_an_earlier_run(&[first_line]);
            let before = rig.pc.dump();

            let engine = rig.restarted();
            let view = engine.view(&input(), Gate::Open).unwrap();

            assert_eq!(row_in(&view, id).state, Available, "{id:?}");
            assert_eq!(
                view.interrupted,
                [interrupted(id, "It didn't change.")],
                "{id:?}"
            );
            assert_eq!(
                rig.story()[1],
                format!("closed {} NotApplied", id.as_str()),
                "{id:?}"
            );
            assert_eq!(rig.pc.dump(), before, "{id:?}");

            // With the outcome written too, the record is simply applied, and no one is told of a crash.
            let rig = Rig::on(arranged());
            assert!(rig.apply(id).ok, "{id:?}");
            let engine = rig.restarted();
            let view = engine.view(&input(), Gate::Open).unwrap();
            assert_eq!(row_in(&view, id).state, Applied, "{id:?}");
            assert!(view.interrupted.is_empty(), "{id:?}");
        }
    }

    #[test]
    fn a_crash_between_the_undo_and_its_entry_closes_as_went_back() {
        for id in SETTINGS {
            let rig = Rig::on(arranged());
            let before = rig.pc.dump();
            assert!(rig.apply(id).ok, "{id:?}");
            assert!(rig.undo(id).ok, "{id:?}");
            rig.cut_journal(2);

            let engine = rig.restarted();
            let view = engine.view(&input(), Gate::Open).unwrap();

            assert_eq!(row_in(&view, id).state, Available, "{id:?}");
            assert!(view.interrupted.is_empty(), "{id:?}");
            assert_eq!(
                rig.story().last().unwrap(),
                &format!("closed {} WentBack", id.as_str())
            );
            assert_eq!(rig.pc.dump(), before, "{id:?}");
        }

        // For the plan that reads as its having gone.
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        assert!(rig.apply(PowerPlan).ok);
        assert!(rig.undo(PowerPlan).ok);
        rig.cut_journal(2);

        let engine = rig.restarted();
        let view = engine.view(&input(), Gate::Open).unwrap();

        assert_eq!(row_in(&view, PowerPlan).state, Available);
        assert_eq!(
            rig.story().last().unwrap(),
            "closed power-plan PlanRemovedElsewhere"
        );
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_torn_line_is_ignored_and_the_next_entry_starts_its_own() {
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        let path = rig.folder().join("journal.jsonl");
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str(r#"{"v":1,"kind":"outc"#);
        std::fs::write(&path, text).unwrap();

        let view = rig.view();

        assert_eq!(view.unreadable_lines, 0, "a cut line is no cause for alarm");
        assert_eq!(row_in(&view, GameMode).state, Applied);
        assert_eq!(view.interrupted.len(), 1);
        assert_eq!(rig.story(), ["intent game-mode", "interrupted game-mode"]);
    }

    #[test]
    fn a_crash_during_apply_again_is_judged_like_any_other() {
        // The second intent is on disk and the value is still the third one.
        let earlier = [
            game_mode_intent(),
            outcome(GameMode, Some(SID)),
            intent(GameMode, Some(SID), dword(2), dword(1)),
        ];
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 2);
        rig.left_by_an_earlier_run(&earlier);

        let view = rig.view();

        assert_eq!(row_in(&view, GameMode).state, ChangedElsewhere);
        assert_eq!(
            view.interrupted,
            [interrupted(GameMode, "Something else changed it.")]
        );
        assert_eq!(rig.open()[0].prior, dword(0), "the first prior");

        // Or the value got written.
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&earlier);

        let view = rig.view();

        assert_eq!(row_in(&view, GameMode).state, Applied);
        assert_eq!(view.interrupted, [interrupted(GameMode, "It finished.")]);
    }

    #[test]
    fn the_interrupted_lines_stay_until_the_app_closes() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);

        assert_eq!(rig.view().interrupted.len(), 1);
        assert!(rig.apply(GameMode).ok);
        assert!(rig.undo(GameMode).ok);
        assert_eq!(rig.view().interrupted.len(), 1, "still in this run");

        let next_run = rig.restarted();
        let view = next_run.view(&input(), Gate::Open).unwrap();
        assert!(view.interrupted.is_empty(), "gone with the app");
    }

    #[test]
    fn apply_settles_the_journal_before_it_starts() {
        // Pressed with no look at the journal first: the crash is settled, and said, before anything is changed.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);

        assert!(rig.apply(GameMode).ok);

        assert_eq!(
            rig.story(),
            [
                "intent game-mode",
                "closed game-mode NotApplied",
                "intent game-mode",
                "applied game-mode"
            ]
        );
        assert_eq!(rig.view().interrupted.len(), 1);
    }

    #[test]
    fn a_batch_is_when_the_apply_started() {
        let rig = Rig::on(arranged());

        let items = rig.engine.apply(&input(), &TweakId::ESSENTIAL).unwrap();

        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            TweakId::ESSENTIAL,
            "in the order asked for"
        );
        let history = rig.journal().read().unwrap().history;
        let batches: Vec<&String> = history
            .iter()
            .filter_map(|entry| match entry {
                Entry::Intent { batch, .. } => Some(batch),
                _ => None,
            })
            .collect();
        assert_eq!(batches.len(), 6);
        assert!(batches.iter().all(|batch| *batch == batches[0]));
        assert!(chrono::DateTime::parse_from_rfc3339(batches[0]).is_ok());
    }

    // Revert everything.

    #[test]
    fn revert_everything_goes_newest_first_and_leaves_flagged_rows() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        assert!(rig.apply(BackgroundRecording).ok);
        assert!(rig.apply(WindowedOptimizations).ok);
        change_elsewhere(&rig.pc, WindowedOptimizations);
        let settings = rig.pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS);
        let writes_before = rig.pc.writes().len();

        let items = rig.engine.revert_all().unwrap();

        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            [WindowedOptimizations, BackgroundRecording, GameMode],
            "newest first"
        );
        assert!(!items[0].ok);
        assert_eq!(items[0].message, LEFT_ALONE_TEXT);
        assert!(items[1].ok, "{}", items[1].message);
        assert!(items[2].ok, "{}", items[2].message);
        assert_eq!(
            rig.pc.writes()[writes_before..],
            [
                format!(r"dword {SID}\{GAME_DVR_KEY}\{HISTORICAL_CAPTURE} = 1"),
                format!(r"dword {SID}\{GAME_BAR_KEY}\{AUTO_GAME_MODE} = 0"),
            ]
        );
        assert_eq!(rig.pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS), settings);
        assert_eq!(rig.open().len(), 1, "the flagged row is still there");
        assert_eq!(rig.state(WindowedOptimizations), ChangedElsewhere);
    }

    #[test]
    fn revert_everything_puts_the_whole_tier_back() {
        for pc in ways_to_be_available(WindowedOptimizations) {
            let rig = Rig::on(pc);
            rig.pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=0;");
            let before = rig.pc.dump();
            let items = rig.engine.apply(&input(), &TweakId::ESSENTIAL).unwrap();
            assert!(items.iter().all(|item| item.ok), "{items:?}");
            assert!(rig.view().any_active);

            let items = rig.engine.revert_all().unwrap();

            assert_eq!(items.len(), 6);
            assert!(items.iter().all(|item| item.ok), "{items:?}");
            assert_eq!(rig.pc.dump(), before);
            assert!(!rig.view().any_active);
            assert!(!rig.engine.has_open_records());
        }

        // And a PC with no DirectX key: the two tokens made it between them, and the key goes with the last of them.
        let rig = Rig::on(ways_to_be_available(GpuPreference).pop().unwrap());
        let before = rig.pc.dump();
        let items = rig.engine.apply(&input(), &TweakId::ESSENTIAL).unwrap();
        assert!(items.iter().all(|item| item.ok), "{items:?}");
        assert!(rig.engine.revert_all().unwrap().iter().all(|item| item.ok));
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn revert_everything_leaves_system_protection_on() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&protection_record());
        assert!(rig.apply(GameMode).ok);

        let items = rig.engine.revert_all().unwrap();

        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            [GameMode, SystemProtection]
        );
        assert!(items[0].ok);
        assert!(!items[1].ok, "a row left alone comes back with ok false");
        assert!(
            items[1].message.contains("restore points"),
            "{}",
            items[1].message
        );
        assert_eq!(rig.pc.protection(), Readiness::Ready);
        assert!(!rig
            .pc
            .writes()
            .iter()
            .any(|line| line.contains("protection")));
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.open()[0].tweak, SystemProtection);
    }

    #[test]
    fn revert_everything_removes_an_inactive_plan() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(HIGH_PERFORMANCE);

        let items = rig.engine.revert_all().unwrap();

        assert!(items[0].ok, "{}", items[0].message);
        assert!(!rig.pc.plan_exists(KESTREL));
        assert_eq!(
            rig.pc.active_plan(),
            Read::Value(HIGH_PERFORMANCE.to_string()),
            "the person's choice stays"
        );
        assert!(rig.open().is_empty());
    }

    #[test]
    fn revert_everything_says_what_it_couldnt_do_and_goes_on() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        assert!(rig.apply(BackgroundRecording).ok);
        assert!(rig.apply(PointerPrecision).ok);
        rig.pc.fail_read(HISTORICAL_CAPTURE);

        let items = rig.engine.revert_all().unwrap();

        assert_eq!(
            items
                .iter()
                .map(|item| (item.id, item.ok))
                .collect::<Vec<_>>(),
            [
                (PointerPrecision, true),
                (BackgroundRecording, false),
                (GameMode, true)
            ]
        );
        assert!(
            items[1].message.contains("told that reading"),
            "{}",
            items[1].message
        );
        assert_eq!(rig.open().len(), 1);
    }

    #[test]
    fn revert_everything_settles_a_crash_it_finds() {
        // The uninstaller's undo starts with no view: whatever a crash left is judged the same way.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);

        let items = rig.engine.revert_all().unwrap();

        assert!(items[0].ok, "{}", items[0].message);
        assert!(rig.open().is_empty());
        assert_eq!(rig.story()[1], "closed game-mode NotApplied");
    }

    // The power plan.

    #[test]
    fn apply_records_the_plan_it_found_and_the_one_it_makes() {
        let rig = Rig::on(arranged());
        rig.pc.add_plan(EXM, EXM_NAME);
        rig.pc.set_active(EXM);

        let item = rig.apply(PowerPlan);

        assert!(item.ok, "{}", item.message);
        assert_eq!(rig.pc.active_plan(), Read::Value(KESTREL.to_string()));
        let record = &rig.open()[0];
        assert_eq!(record.tweak, PowerPlan);
        assert_eq!(record.sid, None, "machine-wide");
        assert_eq!(record.value_name, None);
        assert_eq!(record.prior, saved_plan(EXM, EXM_NAME));
        assert_eq!(record.planned, saved_plan(KESTREL, "Kestrel"));
        assert!(record.settled);
        assert_eq!(rig.state(PowerPlan), Applied);

        let undone = rig.undo(PowerPlan);
        assert!(undone.ok);
        assert_eq!(undone.message, format!("Back to {EXM_NAME}."));
        assert_eq!(rig.pc.active_plan(), Read::Value(EXM.to_string()));
        assert!(!rig.pc.plan_exists(KESTREL));
        let last = rig.journal().read().unwrap().history.pop().unwrap();
        assert!(
            matches!(&last, Entry::Undone { detail: Some(detail), sid: None, .. } if *detail == undone.message),
            "{last:?}"
        );
    }

    #[test]
    fn only_an_available_plan_row_is_applied() {
        // A policy picks the plan: `create_and_activate` would make it anyway, so the row has to stop it.
        let rig = Rig::on(arranged());
        rig.pc
            .set_policy_string(POWER_POLICY_KEY, POWER_POLICY_VALUE, BALANCED);
        let item = rig.apply(PowerPlan);
        assert!(!item.ok);
        assert_eq!(item.message, "An organisation policy picks the power plan.");
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());

        // AMD's guidance.
        let rig = Rig::on(arranged());
        let x3d = PlanInput {
            cpu_name: "AMD Ryzen 9 9950X3D".to_string(),
            ..input()
        };
        let items = rig.engine.apply(&x3d, &[PowerPlan]).unwrap();
        assert!(!items[0].ok);
        assert!(items[0].message.contains("AMD"), "{}", items[0].message);
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());

        // Kestrel's plan is already what's in use, with no record: nothing to record or undo.
        let rig = Rig::on(arranged());
        rig.pc.add_plan(KESTREL, "Kestrel");
        rig.pc.set_active(KESTREL);
        let item = rig.apply(PowerPlan);
        assert!(!item.ok);
        assert_eq!(item.message, ALREADY_SET_TEXT);
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());

        // Kestrel's plan already applied.
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        let writes = rig.pc.writes().len();
        let item = rig.apply(PowerPlan);
        assert!(!item.ok);
        assert_eq!(item.message, ALREADY_SET_TEXT);
        assert_eq!(rig.pc.writes().len(), writes);
        assert_eq!(rig.story().len(), 2);

        // Inactive: Use it again is the button for that.
        rig.pc.set_active(BALANCED);
        assert!(!rig.apply(PowerPlan).ok);
        assert_eq!(rig.pc.writes().len(), writes);
        assert_eq!(rig.story().len(), 2);
    }

    #[test]
    fn a_plan_that_wont_switch_on_leaves_nothing() {
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        rig.pc.fail_activate(KESTREL);

        let item = rig.apply(PowerPlan);

        assert!(!item.ok);
        assert!(
            item.message.contains("told that activating"),
            "{}",
            item.message
        );
        assert_eq!(rig.pc.dump(), before);
        assert_eq!(
            rig.story(),
            says(&[("intent", PowerPlan), ("failed", PowerPlan)])
        );
        assert!(rig.open().is_empty());
        assert_eq!(rig.state(PowerPlan), Available);
    }

    #[test]
    fn a_plan_that_cant_be_taken_out_again_keeps_its_record() {
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        rig.pc.fail_activate(KESTREL);
        rig.pc.fail_delete(KESTREL);

        let item = rig.apply(PowerPlan);

        assert!(!item.ok);
        assert!(rig.pc.plan_exists(KESTREL));
        assert_eq!(
            rig.story(),
            says(&[("intent", PowerPlan), ("applied", PowerPlan)]),
            "the plan on the PC needs a record that can remove it"
        );
        assert_eq!(rig.state(PowerPlan), PlanInactive);

        rig.pc.clear_failures();
        assert!(rig.engine.remove_plan().unwrap().ok);
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_plan_windows_says_it_switched_to_is_read_back() {
        let rig = Rig::on(arranged());
        let before = rig.pc.dump();
        rig.pc.ignore_activate(KESTREL);

        let item = rig.apply(PowerPlan);

        assert!(!item.ok);
        assert!(item.message.contains("didn't switch"), "{}", item.message);
        assert_eq!(rig.pc.dump(), before, "the plan is taken out again");
        assert_eq!(
            rig.story(),
            says(&[("intent", PowerPlan), ("failed", PowerPlan)])
        );
        assert!(rig.open().is_empty());

        // And if it can't be taken out, it stays on record.
        let rig = Rig::on(arranged());
        rig.pc.ignore_activate(KESTREL);
        rig.pc.fail_delete(KESTREL);

        let item = rig.apply(PowerPlan);

        assert!(!item.ok);
        assert_eq!(rig.state(PowerPlan), PlanInactive);
        assert_eq!(rig.open().len(), 1);
    }

    #[test]
    fn inactive_kestrel_plan_row_and_remove() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(BALANCED);

        let view = rig.view();
        assert_eq!(row_in(&view, PowerPlan).state, PlanInactive);
        assert!(view.any_active, "Revert everything still has it to remove");
        assert!(rig.engine.applied_ids().is_empty());
        assert_eq!(rig.open().len(), 1, "open while the plan exists");

        let removed = rig.engine.remove_plan().unwrap();

        assert!(removed.ok, "{}", removed.message);
        assert_eq!(rig.pc.active_plan(), Read::Value(BALANCED.to_string()));
        assert!(!rig.pc.plan_exists(KESTREL));
        assert!(rig.open().is_empty());
        assert_eq!(rig.story().last().unwrap(), "undone power-plan");
        assert_eq!(rig.state(PowerPlan), Available);

        // The other button.
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(BALANCED);
        assert_eq!(rig.state(PowerPlan), PlanInactive);

        let again = rig.engine.use_plan_again().unwrap();

        assert!(again.ok, "{}", again.message);
        assert_eq!(rig.pc.active_plan(), Read::Value(KESTREL.to_string()));
        assert_eq!(rig.state(PowerPlan), Applied);
        assert_eq!(rig.story().last().unwrap(), "reapplied power-plan");
        assert_eq!(rig.open()[0].prior, balanced(), "the first prior is kept");

        assert!(rig.undo(PowerPlan).ok);
        assert_eq!(rig.pc.active_plan(), Read::Value(BALANCED.to_string()));
    }

    #[test]
    fn undo_of_an_inactive_plan_removes_it_and_keeps_the_plan_in_use() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(HIGH_PERFORMANCE);

        let item = rig.undo(PowerPlan);

        assert!(item.ok, "{}", item.message);
        assert_eq!(
            rig.pc.active_plan(),
            Read::Value(HIGH_PERFORMANCE.to_string())
        );
        assert!(!rig.pc.plan_exists(KESTREL));
        assert!(rig.open().is_empty());
    }

    #[test]
    fn a_failed_plan_undo_keeps_the_record() {
        // Nothing can be switched to.
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.fail_activate(BALANCED);
        let item = rig.undo(PowerPlan);
        assert!(!item.ok);
        assert_eq!(rig.pc.active_plan(), Read::Value(KESTREL.to_string()));
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.story().len(), 2);

        // Switched back, but Kestrel's plan won't go: the row says it isn't active, and Remove it finishes the job.
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.fail_delete(KESTREL);
        let item = rig.undo(PowerPlan);
        assert!(!item.ok);
        assert!(
            item.message.starts_with("Back to Balanced."),
            "{}",
            item.message
        );
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.state(PowerPlan), PlanInactive);

        rig.pc.clear_failures();
        assert!(rig.engine.remove_plan().unwrap().ok);
        assert!(!rig.pc.plan_exists(KESTREL));
        assert!(rig.open().is_empty());
    }

    #[test]
    fn undo_with_the_old_plan_gone_says_balanced() {
        let rig = Rig::on(arranged());
        rig.pc.add_plan(EXM, EXM_NAME);
        rig.pc.set_active(EXM);
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.delete_plan(EXM).unwrap();

        let item = rig.undo(PowerPlan);

        assert!(item.ok);
        assert_eq!(
            item.message,
            "Your previous plan is gone, so Kestrel switched to Balanced."
        );
        assert_eq!(rig.pc.active_plan(), Read::Value(BALANCED.to_string()));
        // The report says so too.
        let last = rig.journal().read().unwrap().history.pop().unwrap();
        assert!(
            matches!(&last, Entry::Undone { detail: Some(detail), .. } if *detail == item.message),
            "{last:?}"
        );
    }

    #[test]
    fn a_settled_plan_record_stays_open_until_the_plan_is_gone() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        // Even with the very plan Kestrel found active again.
        rig.pc.set_active(BALANCED);
        assert_eq!(rig.state(PowerPlan), PlanInactive);
        assert_eq!(
            rig.story().len(),
            2,
            "the old plan being back closes nothing"
        );

        // Deleted outside Kestrel.
        rig.pc.delete_plan(KESTREL).unwrap();
        let view = rig.view();

        assert_eq!(row_in(&view, PowerPlan).state, Available);
        assert!(view.interrupted.is_empty());
        assert!(!view.any_active);
        assert_eq!(
            rig.story().last().unwrap(),
            "closed power-plan PlanRemovedElsewhere"
        );
        assert!(rig.open().is_empty());
    }

    #[test]
    fn crash_with_no_plan_on_disk_closes_as_not_applied() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[plan_intent(balanced())]);
        let held = rig.pc.dump();

        let view = rig.view();

        assert_eq!(row_in(&view, PowerPlan).state, Available);
        assert_eq!(
            view.interrupted,
            ["Kestrel was interrupted while changing Kestrel power plan. It didn't change."]
        );
        assert_eq!(rig.story()[1], "closed power-plan NotApplied");
        assert!(rig.open().is_empty());
        assert_eq!(rig.pc.dump(), held);
    }

    #[test]
    fn crash_with_the_plan_on_disk_stays_open_whichever_plan_is_active() {
        // Made but not switched on.
        let rig = Rig::on(arranged());
        rig.pc.add_plan(KESTREL, "Kestrel");
        rig.left_by_an_earlier_run(&[plan_intent(balanced())]);

        let view = rig.view();

        assert_eq!(row_in(&view, PowerPlan).state, PlanInactive);
        assert_eq!(
            view.interrupted,
            ["Kestrel was interrupted while changing Kestrel power plan. Its plan isn't active."]
        );
        assert_eq!(rig.story()[1], "interrupted power-plan");
        assert!(rig.open()[0].settled);

        // Switched on.
        let rig = Rig::on(arranged());
        rig.pc.add_plan(KESTREL, "Kestrel");
        rig.pc.set_active(KESTREL);
        rig.left_by_an_earlier_run(&[plan_intent(balanced())]);

        let view = rig.view();

        assert_eq!(row_in(&view, PowerPlan).state, Applied);
        assert_eq!(
            view.interrupted,
            ["Kestrel was interrupted while changing Kestrel power plan. It finished."]
        );
        assert_eq!(rig.story()[1], "interrupted power-plan");
        assert!(rig.undo(PowerPlan).ok);
        assert_eq!(rig.pc.active_plan(), Read::Value(BALANCED.to_string()));
    }

    #[test]
    fn crash_with_a_plan_that_cant_be_read_waits() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[plan_intent(balanced())]);
        rig.pc.fail_read(ACTIVE_PLAN);

        let view = rig.view();

        assert_eq!(row_in(&view, PowerPlan).state, CouldntRead);
        assert!(view.interrupted.is_empty());
        assert_eq!(rig.story().len(), 1);
    }

    #[test]
    fn a_plan_that_cant_be_read_isnt_taken_for_gone() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.fail_read(ACTIVE_PLAN);
        let writes = rig.pc.writes().len();

        assert_eq!(rig.state(PowerPlan), CouldntRead);
        assert_eq!(rig.story().len(), 2);
        let item = rig.undo(PowerPlan);
        assert!(!item.ok);
        assert!(
            item.message.contains("told that reading"),
            "{}",
            item.message
        );
        assert!(!rig.engine.remove_plan().unwrap().ok);
        assert!(!rig.engine.use_plan_again().unwrap().ok);
        assert!(rig.engine.applied_ids().is_empty());
        assert_eq!(rig.pc.writes().len(), writes);
        assert_eq!(rig.open().len(), 1);

        // Nor is one that really has gone, until the plan in use can be read: Windows isn't answering about plans.
        rig.pc.clear_failures();
        rig.pc.set_active(BALANCED);
        rig.pc.delete_plan(KESTREL).unwrap();
        rig.pc.fail_read(ACTIVE_PLAN);
        assert_eq!(rig.state(PowerPlan), CouldntRead);
        assert_eq!(rig.open().len(), 1);
        rig.pc.clear_failures();
        assert_eq!(rig.state(PowerPlan), Available);
        assert!(rig.open().is_empty());
    }

    #[test]
    fn a_plan_deleted_outside_is_closed_before_apply_records_a_new_prior() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(HIGH_PERFORMANCE);
        rig.pc.delete_plan(KESTREL).unwrap();

        // With no look at the journal in between, so it is Apply that has to notice.
        assert!(rig.apply(PowerPlan).ok);

        assert_eq!(
            rig.open()[0].prior,
            saved_plan(HIGH_PERFORMANCE, "High performance"),
            "not the plan the old record names"
        );
        assert!(rig.undo(PowerPlan).ok);
        assert_eq!(
            rig.pc.active_plan(),
            Read::Value(HIGH_PERFORMANCE.to_string())
        );
    }

    #[test]
    fn use_plan_again_needs_a_plan_that_isnt_in_use() {
        // Nothing recorded.
        let rig = Rig::on(arranged());
        assert!(!rig.engine.use_plan_again().unwrap().ok);
        assert!(!rig.engine.remove_plan().unwrap().ok);
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());

        // Already in use.
        assert!(rig.apply(PowerPlan).ok);
        let writes = rig.pc.writes().len();
        let item = rig.engine.use_plan_again().unwrap();
        assert!(!item.ok);
        assert_eq!(item.message, ALREADY_SET_TEXT);
        assert_eq!(rig.pc.writes().len(), writes);
        assert_eq!(rig.story().len(), 2);

        // Remove it won't take the plan in use.
        let item = rig.engine.remove_plan().unwrap();
        assert!(!item.ok);
        assert!(item.message.contains("in use"), "{}", item.message);
        assert!(rig.pc.plan_exists(KESTREL));
        assert_eq!(rig.open().len(), 1);
    }

    #[test]
    fn use_plan_again_that_fails_keeps_the_record() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(BALANCED);
        rig.pc.fail_activate(KESTREL);

        let item = rig.engine.use_plan_again().unwrap();

        assert!(!item.ok);
        assert!(
            item.message.contains("told that activating"),
            "{}",
            item.message
        );
        assert_eq!(rig.state(PowerPlan), PlanInactive);
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.story().last().unwrap(), "failed power-plan");

        // It said it switched and didn't.
        rig.pc.clear_failures();
        rig.pc.ignore_activate(KESTREL);
        let item = rig.engine.use_plan_again().unwrap();
        assert!(!item.ok);
        assert!(item.message.contains("didn't switch"), "{}", item.message);
        assert_eq!(rig.state(PowerPlan), PlanInactive);
        assert_eq!(rig.open().len(), 1);

        // The plan went away after the row was shown.
        rig.pc.clear_failures();
        rig.pc.delete_plan(KESTREL).unwrap();
        let item = rig.engine.use_plan_again().unwrap();
        assert!(!item.ok);
        assert!(item.message.contains("gone"), "{}", item.message);
    }

    #[test]
    fn a_plan_gone_when_a_button_is_pressed_closes_the_record() {
        for press in [
            (|engine: &Engine| engine.remove_plan()) as fn(&Engine) -> io::Result<ItemOutcome>,
            |engine: &Engine| engine.undo(PowerPlan),
        ] {
            let rig = Rig::on(arranged());
            assert!(rig.apply(PowerPlan).ok);
            rig.pc.set_active(HIGH_PERFORMANCE);
            rig.pc.delete_plan(KESTREL).unwrap();

            let item = press(&rig.engine).unwrap();

            assert!(item.ok, "{}", item.message);
            assert_eq!(
                rig.story().last().unwrap(),
                "closed power-plan PlanRemovedElsewhere"
            );
            assert!(rig.open().is_empty());
        }
    }

    // System Protection.

    #[test]
    fn the_system_protection_row_shows_only_while_its_record_is_open() {
        let rig = Rig::on(arranged());
        assert!(!rig.view().rows.iter().any(|row| row.id == SystemProtection));

        rig.left_by_an_earlier_run(&protection_record());
        let view = rig.view();

        let row = view.rows.last().unwrap();
        assert_eq!(row.id, SystemProtection);
        assert_eq!(row.state, Applied);
        assert_eq!(row.why, PROTECTION_WHY_TEXT);
        assert_eq!(row.title, "System Protection");
        assert!(!row.ticked);
        assert!(!view.any_active, "it isn't part of Revert everything");
        assert_eq!(rig.engine.applied_ids(), ["system-protection"]);

        // With the rest of the tier, it still doesn't count, and they do.
        assert!(rig.apply(GameMode).ok);
        assert!(rig.view().any_active);
    }

    #[test]
    fn undo_turns_system_protection_off() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&protection_record());

        let item = rig.undo(SystemProtection);

        assert!(item.ok, "{}", item.message);
        assert_eq!(item.message, "Undone.");
        assert_eq!(rig.pc.writes(), ["protection off"]);
        assert_eq!(rig.pc.protection(), Readiness::ProtectionOff);
        assert_eq!(rig.story().last().unwrap(), "undone system-protection");
        assert!(!rig.view().rows.iter().any(|row| row.id == SystemProtection));
    }

    #[test]
    fn a_failed_turn_off_keeps_the_record() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&protection_record());
        rig.pc.fail_write(PROTECTION);

        let item = rig.undo(SystemProtection);

        assert!(!item.ok);
        assert!(
            item.message.contains("told that writing"),
            "{}",
            item.message
        );
        assert!(rig.pc.writes().is_empty());
        assert_eq!(rig.pc.protection(), Readiness::Ready);
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.state(SystemProtection), Applied);
    }

    #[test]
    fn a_turn_off_that_doesnt_take_keeps_the_record() {
        // Windows says yes, and it stays on.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&protection_record());
        rig.pc.protection_lands_on(Readiness::Ready);

        let item = rig.undo(SystemProtection);

        assert!(!item.ok);
        assert!(item.message.contains("still on"), "{}", item.message);
        assert_eq!(rig.state(SystemProtection), Applied);
        assert_eq!(rig.open().len(), 1);
        assert_eq!(rig.story().len(), 2);

        // It can be tried again.
        rig.pc.clear_failures();
        assert!(rig.undo(SystemProtection).ok);
        assert_eq!(rig.pc.protection(), Readiness::ProtectionOff);
        assert!(rig.open().is_empty());

        // Or it says yes and then won't say at all: Kestrel can't claim it's undone, and the first look that can tell
        // does.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&protection_record());
        rig.pc.protection_lands_on(Readiness::Unknown);

        let item = rig.undo(SystemProtection);

        assert!(!item.ok);
        assert!(item.message.contains("couldn't tell"), "{}", item.message);
        assert_eq!(rig.open().len(), 1);

        rig.pc.clear_failures();
        rig.pc.set_protection(Readiness::ProtectionOff);
        assert!(!rig.view().rows.iter().any(|row| row.id == SystemProtection));
        assert!(rig.open().is_empty());
    }

    #[test]
    fn system_protection_off_outside_kestrel_closes_its_record() {
        // Settled.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&protection_record());
        rig.pc.set_protection(Readiness::ProtectionOff);
        let view = rig.view();
        assert!(!view.rows.iter().any(|row| row.id == SystemProtection));
        assert_eq!(
            rig.story().last().unwrap(),
            "closed system-protection WentBack"
        );

        // Cut off by a crash, before and after the switch.
        let [first, _] = protection_record();
        let rig = Rig::on(arranged());
        rig.pc.set_protection(Readiness::ProtectionOff);
        rig.left_by_an_earlier_run(std::slice::from_ref(&first));
        let view = rig.view();
        assert_eq!(
            view.interrupted,
            [interrupted(SystemProtection, "It didn't change.")]
        );
        assert_eq!(
            rig.story().last().unwrap(),
            "closed system-protection NotApplied"
        );

        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[first]);
        let view = rig.view();
        assert_eq!(
            view.interrupted,
            [interrupted(SystemProtection, "It finished.")]
        );
        assert_eq!(row_in(&view, SystemProtection).state, Applied);
    }

    #[test]
    fn windows_not_saying_about_system_protection_changes_nothing() {
        for readiness in [Readiness::Unknown, Readiness::BlockedByPolicy] {
            let rig = Rig::on(arranged());
            rig.left_by_an_earlier_run(&protection_record());
            rig.pc.set_protection(readiness);

            assert_eq!(rig.state(SystemProtection), Applied, "{readiness:?}");
            assert_eq!(rig.engine.applied_ids(), ["system-protection"]);
            assert_eq!(rig.story().len(), 2, "{readiness:?}");

            let item = rig.undo(SystemProtection);
            assert!(!item.ok, "{readiness:?}");
            assert!(rig.pc.writes().is_empty(), "{readiness:?}");
            assert_eq!(rig.open().len(), 1, "{readiness:?}");
        }
    }

    #[test]
    fn system_protection_is_only_turned_off_if_kestrel_turned_it_on() {
        // Turning it off deletes restore points, so a record that doesn't say it was off is not acted on.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[
            intent(
                SystemProtection,
                None,
                Saved::Protection { on: true },
                Saved::Protection { on: true },
            ),
            outcome(SystemProtection, None),
        ]);

        let item = rig.undo(SystemProtection);

        assert!(!item.ok);
        assert!(rig.pc.writes().is_empty());
        assert_eq!(rig.open().len(), 1);
    }

    // Accounts.

    #[test]
    fn another_accounts_records_wait() {
        // One record whose value is back at what it found, and one cut off by a crash: neither is this account's to
        // judge.
        let rig = Rig::on(arranged());
        rig.pc.set_dword(OTHER_SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        rig.left_by_an_earlier_run(&[
            intent(GameMode, Some(OTHER_SID), dword(0), dword(1)),
            outcome(GameMode, Some(OTHER_SID)),
            intent(BackgroundRecording, Some(OTHER_SID), dword(1), dword(0)),
        ]);
        let entries = rig.story();
        let held = rig.pc.dump();

        let view = rig.view();

        assert_eq!(view.pending_other_accounts, 2);
        assert_eq!(row_in(&view, GameMode).state, Available, "this account's");
        assert!(view.interrupted.is_empty());
        assert!(!view.any_active);
        assert!(!view.other_account, "this process is the signed-in account");
        assert!(rig.engine.applied_ids().is_empty());
        assert!(rig.engine.has_open_records());
        assert!(rig.engine.revert_all().unwrap().is_empty());
        assert!(!rig.undo(GameMode).ok, "this account has nothing to undo");
        assert!(!rig.engine.restore_anyway(GameMode).unwrap().ok);
        assert_eq!(rig.story(), entries);
        assert_eq!(rig.pc.dump(), held);
        assert!(rig.pc.writes().is_empty());

        // This account's own change is a record of its own.
        assert!(rig.apply(GameMode).ok);
        assert_eq!(
            rig.pc.dword(OTHER_SID, GAME_BAR_KEY, AUTO_GAME_MODE),
            Some(0)
        );
        assert_eq!(rig.open().len(), 3);
        assert!(rig.undo(GameMode).ok);
        assert_eq!(rig.view().pending_other_accounts, 2);
        assert_eq!(
            rig.pc.dword(OTHER_SID, GAME_BAR_KEY, AUTO_GAME_MODE),
            Some(0)
        );
    }

    #[test]
    fn the_signed_in_account_gets_its_records_back_when_it_is_at_the_screen() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        rig.pc.set_account(Account {
            signed_in: Some(OTHER_SID.to_string()),
            this_process: Some(OTHER_SID.to_string()),
        });
        rig.pc.set_dword(OTHER_SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);

        let view = rig.view();

        assert_eq!(view.pending_other_accounts, 1);
        assert_eq!(row_in(&view, GameMode).state, Available);

        rig.pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: Some(SID.to_string()),
        });
        let view = rig.view();
        assert_eq!(view.pending_other_accounts, 0);
        assert_eq!(row_in(&view, GameMode).state, Applied);
    }

    #[test]
    fn no_signed_in_account_still_allows_the_plan() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        rig.pc.set_account(Account {
            signed_in: None,
            this_process: Some(SID.to_string()),
        });
        let writes = rig.pc.writes().len();

        let view = rig.view();

        for id in SETTINGS {
            let row = row_in(&view, id);
            assert_eq!(row.state, CouldntRead, "{id:?}");
            assert_eq!(
                row.why, "Couldn't tell which account is signed in.",
                "{id:?}"
            );
        }
        assert_eq!(row_in(&view, PowerPlan).state, Available);
        assert_eq!(
            view.pending_other_accounts, 1,
            "whose it is can't be told yet"
        );
        assert!(!view.other_account);

        for id in SETTINGS {
            let item = rig.apply(id);
            assert!(!item.ok, "{id:?}");
            assert_eq!(item.message, "Couldn't tell which account is signed in.");
        }
        assert_eq!(rig.pc.writes().len(), writes);
        assert!(rig.engine.preview(&input(), &SETTINGS).items.is_empty());

        assert!(rig.apply(PowerPlan).ok);
        assert_eq!(rig.state(PowerPlan), Applied);
        assert_eq!(rig.engine.applied_ids(), ["power-plan"]);
        assert!(rig.undo(PowerPlan).ok);
        assert_eq!(rig.pc.active_plan(), Read::Value(BALANCED.to_string()));
    }

    #[test]
    fn differing_accounts_are_said_and_only_the_signed_in_account_is_changed() {
        let rig = Rig::on(with_differing_accounts(arranged()));
        let before = rig.pc.dump();

        let view = rig.view();
        assert!(view.other_account);
        assert_eq!(view.pending_other_accounts, 0);

        assert!(rig.apply(PointerPrecision).ok);
        assert_eq!(
            rig.pc.string(SID, MOUSE_KEY, MOUSE_SPEED).as_deref(),
            Some("0")
        );
        assert_eq!(rig.pc.string(OTHER_SID, MOUSE_KEY, MOUSE_SPEED), None);
        assert_eq!(
            rig.pc.mouse_live(),
            Read::Value([6, 10, 1]),
            "until next sign-in"
        );
        assert!(
            !rig.pc.writes().iter().any(|line| line.starts_with("mouse")),
            "the elevating account's profile is left out of it"
        );

        assert!(rig.undo(PointerPrecision).ok);
        assert_eq!(rig.pc.dump(), before);
        assert!(
            !rig.pc.writes().iter().any(|line| line.starts_with("mouse")),
            "nor does the undo call Windows' mouse setting"
        );
    }

    // The GPU value's name.

    #[test]
    fn the_gpu_value_keeps_the_spelling_it_has() {
        // VALORANT's value is there in another letter case: Kestrel updates it and adds no second one.
        let rig = Rig::on(arranged());
        rig.pc.remove_value(SID, DIRECTX_KEY, EXE);
        rig.pc
            .set_string(SID, DIRECTX_KEY, EXE_LOWER, "GpuPreference=0;");
        let before = rig.pc.dump();

        assert!(rig.apply(GpuPreference).ok);

        assert_eq!(
            rig.open()[0].value_name.as_deref(),
            Some(EXE_LOWER),
            "the intent names the value as it is spelled there"
        );
        assert_eq!(
            rig.pc.value_names(SID, DIRECTX_KEY),
            Read::Value(vec![EXE_LOWER.to_string(), DIRECTX_SETTINGS.to_string()])
        );
        assert_eq!(
            rig.pc.string(SID, DIRECTX_KEY, EXE_LOWER).as_deref(),
            Some("GpuPreference=2;")
        );

        assert!(rig.undo(GpuPreference).ok);
        assert_eq!(rig.pc.dump(), before);
    }

    #[test]
    fn a_gpu_record_goes_by_the_name_it_was_written_with() {
        // The scan moves on (VALORANT reinstalled elsewhere, or not found), and the change is still in the value it was
        // made to.
        let rig = Rig::on(arranged());
        assert!(rig.apply(GpuPreference).ok);
        let moved = PlanInput {
            valorant_exe: Some(MOVED_EXE.to_string()),
            ..input()
        };
        let lost = PlanInput {
            valorant_exe: None,
            ..input()
        };

        for scan in [&moved, &lost] {
            let view = rig.engine.view(scan, Gate::Open).unwrap();
            assert_eq!(row_in(&view, GpuPreference).state, Applied);
            assert_eq!(
                rig.engine.apply(scan, &[GpuPreference]).unwrap()[0].message,
                ALREADY_SET_TEXT
            );
        }

        change_elsewhere(&rig.pc, GpuPreference);
        assert!(rig.engine.apply_again(&moved, GpuPreference).unwrap().ok);
        assert_eq!(
            rig.pc.string(SID, DIRECTX_KEY, EXE).as_deref(),
            Some("GpuPreference=2;")
        );
        assert_eq!(rig.pc.string(SID, DIRECTX_KEY, MOVED_EXE), None);
        assert!(rig.undo(GpuPreference).ok);
        assert_eq!(
            rig.pc.string(SID, DIRECTX_KEY, EXE).as_deref(),
            Some("GpuPreference=0;")
        );
    }

    #[test]
    fn a_gpu_key_that_cant_be_listed_stops_apply() {
        let rig = Rig::on(arranged());
        rig.pc.fail_read(DIRECTX_KEY);

        let item = rig.apply(GpuPreference);

        assert!(!item.ok);
        assert!(
            item.message.contains("told that reading"),
            "{}",
            item.message
        );
        assert!(rig.pc.writes().is_empty());
        assert!(rig.story().is_empty());
    }

    // A journal file can say anything.

    #[test]
    fn a_record_that_doesnt_fit_its_tweak_fails_safely() {
        // Game Mode saved as a plan: the value is neither what the record planned nor its prior.
        let rig = Rig::on(arranged());
        let held_plan = saved_plan(EXM, "EXM");
        rig.left_by_an_earlier_run(&[
            intent(GameMode, Some(SID), held_plan.clone(), held_plan.clone()),
            outcome(GameMode, Some(SID)),
            // A value that fits what it planned and a prior that doesn't fit at all.
            intent(BackgroundRecording, Some(SID), held_plan, dword(1)),
            outcome(BackgroundRecording, Some(SID)),
            // The plan saved as a DWORD.
            intent(PowerPlan, None, dword(1), saved_plan(KESTREL, "Kestrel")),
            outcome(PowerPlan, None),
        ]);
        rig.pc.add_plan(KESTREL, "Kestrel");
        rig.pc.set_active(KESTREL);
        let held = rig.pc.dump();
        let entries = rig.story();

        let view = rig.view();
        assert_eq!(row_in(&view, GameMode).state, ChangedElsewhere);
        assert_eq!(row_in(&view, BackgroundRecording).state, Applied);
        assert_eq!(row_in(&view, PowerPlan).state, Applied);

        let items = rig.engine.revert_all().unwrap();
        assert_eq!(
            items
                .iter()
                .map(|item| (item.id, item.ok))
                .collect::<Vec<_>>(),
            [
                (PowerPlan, false),
                (BackgroundRecording, false),
                (GameMode, false)
            ]
        );
        assert_eq!(items[2].message, LEFT_ALONE_TEXT);
        // Restore anyway is the one that doesn't care what holds now, and it refuses too.
        let restored = rig.engine.restore_anyway(GameMode).unwrap();
        assert!(!restored.ok);
        assert!(
            restored.message.contains("nothing was changed"),
            "{}",
            restored.message
        );

        assert_eq!(rig.pc.dump(), held);
        assert!(rig.pc.writes().is_empty());
        assert_eq!(rig.story(), entries);
        assert_eq!(rig.open().len(), 3, "kept, and shown for what they are");
    }

    #[test]
    fn a_setting_recorded_with_no_account_is_left_alone() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[
            intent(GameMode, None, dword(0), dword(1)),
            outcome(GameMode, None),
            intent(BackgroundRecording, None, dword(1), dword(0)),
        ]);
        let entries = rig.story();

        let view = rig.view();

        // The row is this account's setting, and that record says nothing about it.
        assert_eq!(row_in(&view, GameMode).state, Available);
        assert!(view.interrupted.is_empty());
        assert!(rig.engine.applied_ids().is_empty());
        let items = rig.engine.revert_all().unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| !item.ok), "{items:?}");
        assert!(rig.pc.writes().is_empty());
        assert_eq!(rig.story(), entries);
    }

    // What the view says.

    #[test]
    fn the_view_passes_the_gate_on_and_lists_the_essential_rows_in_order() {
        let rig = Rig::on(arranged());
        for gate in [Gate::Open, Gate::GameRunning, Gate::BenchmarkActive] {
            let view = rig.engine.view(&input(), gate).unwrap();
            assert_eq!(view.gate, gate);
            assert_eq!(
                view.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
                TweakId::ESSENTIAL
            );
        }
        // Windowed optimizations is hidden below Windows 11 22H2.
        let old = PlanInput {
            build: 19045,
            ..input()
        };
        let view = rig.engine.view(&old, Gate::Open).unwrap();
        assert!(!view.rows.iter().any(|row| row.id == WindowedOptimizations));
    }

    #[test]
    fn the_view_counts_the_lines_it_couldnt_read() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[]);
        let path = rig.folder().join("journal.jsonl");
        std::fs::write(&path, "not json\n{\"v\":1,\"kind\":\"format-c\"}\n").unwrap();

        assert_eq!(rig.view().unreadable_lines, 2);
    }

    #[test]
    fn any_active_is_about_this_account_and_machine_wide() {
        let rig = Rig::on(arranged());
        assert!(!rig.view().any_active);
        assert!(rig.apply(PowerPlan).ok);
        assert!(rig.view().any_active, "machine-wide");
        assert!(rig.undo(PowerPlan).ok);
        assert!(!rig.view().any_active);
        assert!(rig.apply(GameMode).ok);
        assert!(rig.view().any_active, "this account");
        change_elsewhere(&rig.pc, GameMode);
        assert!(rig.view().any_active, "a flagged row is still open");
    }

    #[test]
    fn applied_ids_lists_only_applied_rows() {
        let rig = Rig::on(arranged());
        assert!(rig.engine.applied_ids().is_empty());
        for id in TweakId::ESSENTIAL {
            assert!(rig.apply(id).ok, "{id:?}");
        }
        assert_eq!(
            rig.engine.applied_ids(),
            [
                "power-plan",
                "game-mode",
                "background-recording",
                "windowed-optimizations",
                "gpu-preference",
                "pointer-precision"
            ]
        );

        // Flagged, inactive, back at the prior, and unreadable.
        change_elsewhere(&rig.pc, GameMode);
        rig.pc.set_active(BALANCED);
        put_back_by_hand(&rig.pc, BackgroundRecording);
        rig.pc.fail_read(MOUSE_SPEED);

        assert_eq!(
            rig.engine.applied_ids(),
            ["windowed-optimizations", "gpu-preference"]
        );
        // It settles the journal first, as the view does.
        assert_eq!(
            rig.story().last().unwrap(),
            "closed background-recording WentBack"
        );
    }

    #[test]
    fn has_open_records_settles_first() {
        let rig = Rig::on(arranged());
        assert!(!rig.engine.has_open_records());
        assert!(rig.apply(GameMode).ok);
        assert!(rig.engine.has_open_records());
        put_back_by_hand(&rig.pc, GameMode);
        assert!(!rig.engine.has_open_records(), "that was the last of them");
        assert_eq!(rig.story().last().unwrap(), "closed game-mode WentBack");
    }

    // The Preview.

    #[test]
    fn the_preview_says_where_each_setting_lives_and_what_it_will_hold() {
        let rig = Rig::on(arranged());
        let held = rig.pc.dump();

        let preview = rig.engine.preview(&input(), &TweakId::ESSENTIAL);

        assert_eq!(preview.restore, Readiness::Ready);
        assert_eq!(
            preview.items.iter().map(|item| item.id).collect::<Vec<_>>(),
            TweakId::ESSENTIAL
        );
        let expected = [
            (
                GameMode,
                r"HKCU\Software\Microsoft\GameBar, AutoGameModeEnabled".to_string(),
                "off",
                "on",
                Timing::Now,
            ),
            (
                BackgroundRecording,
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR, HistoricalCaptureEnabled"
                    .to_string(),
                "on",
                "off",
                Timing::Now,
            ),
            (
                WindowedOptimizations,
                r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences, DirectXUserGlobalSettings"
                    .to_string(),
                "not set",
                "on",
                Timing::NextLaunch,
            ),
            (
                GpuPreference,
                format!(r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences, {EXE}"),
                "Windows decides",
                "High performance",
                Timing::NextLaunch,
            ),
            (
                PointerPrecision,
                r"HKCU\Control Panel\Mouse, MouseSpeed, MouseThreshold1, MouseThreshold2"
                    .to_string(),
                "on",
                "off",
                Timing::Now,
            ),
        ];
        for (id, location, now, after, timing) in expected {
            let item = preview.items.iter().find(|item| item.id == id).unwrap();
            let title = info(id).title;
            assert_eq!(item.title, title, "{id:?}");
            assert_eq!(item.location, location, "{id:?}");
            assert_eq!(
                item.changes,
                [ValueChange {
                    label: title.to_string(),
                    now: now.to_string(),
                    after: after.to_string(),
                }],
                "{id:?}"
            );
            assert_eq!(item.timing, timing, "{id:?}");
            assert_eq!(item.note, None, "{id:?}");
        }
        // The plan's own Preview.
        let plan = &preview.items[0];
        assert_eq!(plan.location, "Power plan");
        assert_eq!(plan.changes.len(), 3);
        assert!(plan.note.as_deref().unwrap().contains("Undo returns"));

        assert_eq!(rig.pc.dump(), held);
        assert!(rig.pc.writes().is_empty());
        assert!(!rig.folder().exists(), "a Preview touches nothing");
    }

    #[test]
    fn the_preview_uses_the_gpu_value_as_it_is_spelled() {
        let rig = Rig::on(arranged());
        rig.pc.remove_value(SID, DIRECTX_KEY, EXE);
        rig.pc
            .set_string(SID, DIRECTX_KEY, EXE_LOWER, "GpuPreference=1;");

        let preview = rig.engine.preview(&input(), &[GpuPreference]);

        assert_eq!(
            preview.items[0].location,
            format!(r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences, {EXE_LOWER}")
        );
        assert_eq!(preview.items[0].changes[0].now, "Power saving");
    }

    #[test]
    fn the_preview_leaves_out_what_isnt_available() {
        let rig = Rig::on(FakeSystem::new());
        // Already set.
        assert!(rig.engine.preview(&input(), &[GameMode]).items.is_empty());
        // Blocked by policy, Not needed, or hidden.
        let rig = Rig::on(arranged());
        rig.pc
            .set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_AUTO_GAME_MODE, 0);
        rig.pc
            .set_policy_string(POWER_POLICY_KEY, POWER_POLICY_VALUE, BALANCED);
        let one_gpu = PlanInput {
            gpu_choice: false,
            build: 19045,
            ..input()
        };
        let preview = rig.engine.preview(
            &one_gpu,
            &[
                PowerPlan,
                GameMode,
                GpuPreference,
                WindowedOptimizations,
                SystemProtection,
            ],
        );
        assert!(preview.items.is_empty());
        // Couldn't read.
        let rig = Rig::on(arranged());
        rig.pc.fail_read(HISTORICAL_CAPTURE);
        assert!(rig
            .engine
            .preview(&input(), &[BackgroundRecording])
            .items
            .is_empty());
        // Applied, flagged and inactive.
        let rig = Rig::on(arranged());
        for id in [GameMode, BackgroundRecording, PowerPlan] {
            assert!(rig.apply(id).ok);
        }
        change_elsewhere(&rig.pc, BackgroundRecording);
        rig.pc.set_active(BALANCED);
        let preview = rig
            .engine
            .preview(&input(), &[GameMode, BackgroundRecording, PowerPlan]);
        assert!(preview.items.is_empty());
    }

    #[test]
    fn the_preview_treats_a_value_back_at_the_prior_as_apply_would() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        put_back_by_hand(&rig.pc, GameMode);
        let entries = rig.story();

        let preview = rig.engine.preview(&input(), &[GameMode]);

        assert_eq!(preview.items.len(), 1);
        assert_eq!(rig.story(), entries, "the next view closes it");
    }

    #[test]
    fn the_preview_passes_on_what_windows_says_about_restore_points() {
        for readiness in [
            Readiness::Ready,
            Readiness::ProtectionOff,
            Readiness::BlockedByPolicy,
            Readiness::Unknown,
        ] {
            let rig = Rig::on(arranged());
            rig.pc.set_protection(readiness);
            assert_eq!(rig.engine.preview(&input(), &[]).restore, readiness);
        }
    }

    #[test]
    fn accounts_differ_preview_says_next_sign_in_for_the_pointer() {
        let rig = Rig::on(with_differing_accounts(arranged()));
        let preview = rig
            .engine
            .preview(&input(), &[PointerPrecision, GameMode, GpuPreference]);
        let timing = |id: TweakId| {
            preview
                .items
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .timing
        };
        assert_eq!(timing(PointerPrecision), Timing::NextSignIn);
        assert_eq!(timing(GameMode), Timing::Now, "only the pointer");
        assert_eq!(timing(GpuPreference), Timing::NextLaunch);

        let rig = Rig::on(arranged());
        let preview = rig.engine.preview(&input(), &[PointerPrecision]);
        assert_eq!(preview.items[0].timing, Timing::Now);
    }

    // The report.

    const REPORT_AT: &str = "2026-11-02T18:20:00+00:00";

    fn report_of(engine: &Engine) -> String {
        engine.report("0.5.0", 26200, REPORT_AT)
    }

    /// The lines under a heading of a report, without their indent.
    fn under(report: &str, heading: &str) -> Vec<String> {
        report
            .lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| line.starts_with("  "))
            .map(|line| line[2..].to_string())
            .collect()
    }

    /// What the History says, newest first, without the times: the engine stamps its entries with the clock.
    fn history_told(report: &str) -> Vec<String> {
        under(report, "History")
            .iter()
            .map(|line| line.chars().skip(17).collect())
            .collect()
    }

    /// What the Active list says, without the time.
    fn active_told(report: &str) -> Vec<String> {
        under(report, "Active")
            .iter()
            .map(|line| {
                line.rsplit_once(" (")
                    .map_or(line.as_str(), |(told, _)| told)
            })
            .map(str::to_string)
            .collect()
    }

    const NO_CHANGES_TEXT: &str =
        "Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20\nNo changes yet.";

    #[test]
    fn the_report_tells_what_apply_and_undo_did() {
        let rig = Rig::on(arranged());
        for id in [PowerPlan, GameMode, BackgroundRecording] {
            assert!(rig.apply(id).ok, "{id:?}");
        }
        assert!(rig.undo(BackgroundRecording).ok);

        let report = report_of(&rig.engine);

        assert!(
            report.starts_with(
                "Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20\nActive\n"
            ),
            "{report}"
        );
        assert_eq!(
            active_told(&report),
            [
                "Game Mode: off → on",
                "Kestrel power plan: Balanced → Kestrel"
            ]
        );
        assert_eq!(
            history_told(&report),
            [
                "Undid Background recording off: back to on",
                "Applied Background recording off: on → off",
                "Applied Game Mode: off → on",
                "Applied Kestrel power plan: Balanced → Kestrel",
            ]
        );
        // Each line has the real time of its entry, to the minute.
        for line in under(&report, "History") {
            let stamp: Vec<char> = line.chars().take(16).collect();
            assert!(
                stamp.iter().enumerate().all(|(at, c)| match at {
                    4 | 7 => *c == '-',
                    10 => *c == ' ',
                    13 => *c == ':',
                    _ => c.is_ascii_digit(),
                }),
                "{line}"
            );
        }
        // The page would read the same after the next look.
        rig.view();
        assert_eq!(report_of(&rig.engine), report);
    }

    #[test]
    fn the_report_tells_apply_again_and_a_change_that_didnt_take() {
        let rig = Rig::on(arranged());
        let refused = "Couldn't apply Game Mode: Windows refused the change";

        // A first apply that Windows refused: it opened no record.
        rig.pc.fail_write(AUTO_GAME_MODE);
        assert!(!rig.apply(GameMode).ok);
        rig.pc.clear_failures();
        assert_eq!(
            under(&report_of(&rig.engine), "Active"),
            ["Nothing is active."]
        );

        assert!(rig.apply(GameMode).ok);
        change_elsewhere(&rig.pc, GameMode);
        // An apply again that Windows refused, which leaves the record as it was, and one that went through.
        rig.pc.fail_write(AUTO_GAME_MODE);
        assert!(!rig.engine.apply_again(&input(), GameMode).unwrap().ok);
        rig.pc.clear_failures();
        assert!(rig.engine.apply_again(&input(), GameMode).unwrap().ok);

        let report = report_of(&rig.engine);

        let told = history_told(&report);
        assert_eq!(told.len(), 4, "{report}");
        assert_eq!(told[0], "Applied Game Mode again");
        assert!(told[1].starts_with(refused), "{}", told[1]);
        assert_eq!(told[2], "Applied Game Mode: off → on");
        assert!(told[3].starts_with(refused), "{}", told[3]);
        // The record is the first apply's, however many times it was tried again.
        assert_eq!(active_told(&report), ["Game Mode: off → on"]);
    }

    #[test]
    fn the_report_doesnt_call_a_change_it_couldnt_put_back_a_success() {
        // The read-back lies, and the key Kestrel made can't be deleted again. The value itself did go back, so what is
        // left is an empty key and a record that stays open until the next look.
        let pc = FakeSystem::new();
        pc.fail_write(DIRECTX_KEY);
        let rig = Rig::on(pc);
        rig.pc.lie_on_readback(DIRECTX_SETTINGS);
        let item = rig.apply(WindowedOptimizations);
        assert!(!item.ok);

        let report = report_of(&rig.engine);

        assert_eq!(
            history_told(&report),
            [format!(
                "Couldn't apply Optimizations for windowed games: {}",
                item.message
            )]
        );
        // The setting is back at its prior, so it is not active, and the next look closes the record.
        assert_eq!(under(&report, "Active"), ["Nothing is active."]);
        assert_eq!(rig.state(WindowedOptimizations), Available);

        // A put-back that failed with the value still Kestrel's is a change on the PC. It is listed as active, and the
        // History still doesn't call it a success.
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        let why = "Windows didn't keep the change. Kestrel couldn't put the old value back: no";
        rig.left_by_an_earlier_run(&[
            game_mode_intent(),
            Entry::Outcome {
                at: WHEN.to_string(),
                tweak: GameMode.as_str().to_string(),
                sid: Some(SID.to_string()),
                applied: true,
                detail: Some(why.to_string()),
                interrupted: false,
            },
        ]);

        let report = report_of(&rig.engine);

        assert_eq!(
            under(&report, "History"),
            [format!("2026-11-02 18:12 Couldn't apply Game Mode: {why}")]
        );
        assert_eq!(active_told(&report), ["Game Mode: off → on"]);
        assert_eq!(rig.state(GameMode), Applied);
    }

    #[test]
    fn the_report_tells_a_crash_without_calling_it_a_success() {
        // Cut short after the write: the next look settles it, and the report tells the interruption.
        let rig = Rig::on(FakeSystem::new());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        rig.view();

        let report = report_of(&rig.engine);

        assert_eq!(
            under(&report, "History"),
            ["2026-11-02 18:12 Kestrel was interrupted while applying Game Mode: off → on"]
        );
        assert_eq!(active_told(&report), ["Game Mode: off → on"]);

        // Cut short during Apply again.
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&[
            game_mode_intent(),
            outcome(GameMode, Some(SID)),
            intent(GameMode, Some(SID), dword(2), dword(1)),
        ]);
        rig.view();

        let report = report_of(&rig.engine);

        assert_eq!(
            under(&report, "History"),
            [
                "2026-11-02 18:12 Kestrel was interrupted while applying Game Mode again",
                "2026-11-02 18:12 Applied Game Mode: off → on",
            ]
        );

        // Cut short before the write: it never happened.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        rig.view();
        assert_eq!(report_of(&rig.engine), NO_CHANGES_TEXT);
    }

    #[test]
    fn the_report_tells_what_went_back_outside_kestrel() {
        let rig = Rig::on(arranged());
        assert!(rig.apply(GameMode).ok);
        put_back_by_hand(&rig.pc, GameMode);
        rig.view();

        let report = report_of(&rig.engine);

        assert_eq!(
            history_told(&report),
            [
                "Game Mode went back outside Kestrel",
                "Applied Game Mode: off → on"
            ]
        );
        assert_eq!(under(&report, "Active"), ["Nothing is active."]);

        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(BALANCED);
        rig.pc.delete_plan(KESTREL).unwrap();
        rig.view();

        assert_eq!(
            history_told(&report_of(&rig.engine)),
            [
                "Kestrel power plan was removed outside Kestrel",
                "Applied Kestrel power plan: Balanced → Kestrel"
            ]
        );
    }

    #[test]
    fn the_report_says_which_plan_an_undo_went_back_to() {
        // The plan that was in use before.
        let rig = Rig::on(arranged());
        rig.pc.add_plan(EXM, EXM_NAME);
        rig.pc.set_active(EXM);
        assert!(rig.apply(PowerPlan).ok);
        assert!(rig.undo(PowerPlan).ok);
        assert_eq!(
            history_told(&report_of(&rig.engine)),
            [
                "Undid Kestrel power plan: Back to EXM Free Power Plan V8.",
                "Applied Kestrel power plan: EXM Free Power Plan V8 → Kestrel"
            ]
        );

        // That plan is gone, and Windows' Balanced took its place.
        let rig = Rig::on(arranged());
        rig.pc.add_plan(EXM, EXM_NAME);
        rig.pc.set_active(EXM);
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.delete_plan(EXM).unwrap();
        assert!(rig.undo(PowerPlan).ok);
        assert_eq!(
            history_told(&report_of(&rig.engine))[0],
            "Undid Kestrel power plan: Your previous plan is gone, so Kestrel switched to Balanced."
        );

        // Another plan was picked since: Undo removes Kestrel's and leaves that one in use.
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(HIGH_PERFORMANCE);
        assert!(rig.undo(PowerPlan).ok);
        assert_eq!(
            history_told(&report_of(&rig.engine))[0],
            "Undid Kestrel power plan: removed Kestrel's plan and left the plan in use alone"
        );

        // Use it again is the plan applied again.
        let rig = Rig::on(arranged());
        assert!(rig.apply(PowerPlan).ok);
        rig.pc.set_active(BALANCED);
        assert!(rig.engine.use_plan_again().unwrap().ok);
        assert_eq!(
            history_told(&report_of(&rig.engine)),
            [
                "Applied Kestrel power plan again",
                "Applied Kestrel power plan: Balanced → Kestrel"
            ]
        );
    }

    #[test]
    fn the_report_has_the_restore_points_and_system_protection() {
        let rig = Rig::on(arranged());
        let mut entries = vec![Entry::RestorePoint {
            at: WHEN.to_string(),
            outcome: RestoreOutcome::Created { sequence: 57 },
        }];
        entries.extend(protection_record());
        rig.left_by_an_earlier_run(&entries);

        let report = report_of(&rig.engine);

        assert_eq!(
            under(&report, "History"),
            [
                "2026-11-02 18:12 Applied System Protection: off → on",
                "2026-11-02 18:12 Restore point created (#57)"
            ]
        );
        assert_eq!(
            under(&report, "Active"),
            ["System Protection: off → on (2026-11-02 18:12)"]
        );
    }

    #[test]
    fn the_report_reads_the_journal_and_nothing_else() {
        // A PC that never applied has no folder to make, and nothing to write to.
        let rig = Rig::on(arranged());
        assert_eq!(report_of(&rig.engine), NO_CHANGES_TEXT);
        assert!(!rig.folder().exists());
        assert!(rig.pc.writes().is_empty());

        // A crash's leftovers are what a look at the tab would settle in the journal. The report leaves them as they
        // are: it doesn't judge them, and it doesn't write.
        let rig = Rig::on(arranged());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        let file = rig.folder().join("journal.jsonl");
        let before = std::fs::read(&file).unwrap();
        let dump = rig.pc.dump();

        let report = report_of(&rig.engine);

        // It holds what Kestrel wrote, so it is listed as the next look would call it; the interruption isn't told yet.
        assert_eq!(
            under(&report, "Active"),
            ["Game Mode: off → on (2026-11-02 18:12)"]
        );
        assert_eq!(under(&report, "History"), ["No finished changes yet."]);
        assert_eq!(
            std::fs::read(&file).unwrap(),
            before,
            "the journal is as it was"
        );
        assert_eq!(rig.pc.dump(), dump);
        assert!(rig.pc.writes().is_empty());
        assert_eq!(
            rig.engine
                .view(&input(), Gate::Open)
                .unwrap()
                .interrupted
                .len(),
            1,
            "the look at the tab is the one that settles it"
        );

        // It doesn't ask for the lock, either: with only the journal's own file written (which takes none), there is no
        // lock file to find afterwards, and one that someone else holds doesn't stop it.
        let rig = Rig::on(arranged());
        std::fs::create_dir_all(rig.folder()).unwrap();
        rig.journal().append(&game_mode_intent()).unwrap();
        rig.journal().append(&outcome(GameMode, Some(SID))).unwrap();
        let lock_file = rig.folder().join("journal.lock");
        assert!(!lock_file.exists());

        let report = report_of(&rig.engine);

        assert!(!lock_file.exists(), "the report took the lock");
        assert_eq!(
            under(&report, "History"),
            ["2026-11-02 18:12 Applied Game Mode: off → on"]
        );
        let held_elsewhere = rig.journal().lock().unwrap();
        assert_eq!(report_of(&rig.engine), report);
        drop(held_elsewhere);
    }

    /// What the report says of a record's change, from the record itself: `Title: before → after`.
    fn change_told(record: &OpenRecord) -> String {
        format!(
            "{}: {} → {}",
            info(record.tweak).title,
            targets::describe(record.tweak, &record.prior),
            targets::describe(record.tweak, &record.planned)
        )
    }

    /// What happens to the PC after Apply, in a test.
    type Afterwards = Box<dyn Fn(&Rig)>;

    #[test]
    fn active_is_what_the_row_would_call_applied() {
        // After Apply, something happens to the PC; the report is taken before anything looks at the journal. Whether
        // the record is listed is whether its row would say Applied, and the History keeps its line either way.
        let cases: Vec<(&str, TweakId, Afterwards, RowState)> = vec![
            ("it holds", GameMode, Box::new(|_| {}), Applied),
            (
                "a third value",
                GameMode,
                Box::new(|rig| change_elsewhere(&rig.pc, GameMode)),
                ChangedElsewhere,
            ),
            (
                "put back by hand",
                GameMode,
                Box::new(|rig| put_back_by_hand(&rig.pc, GameMode)),
                Available,
            ),
            (
                "an inactive plan",
                PowerPlan,
                Box::new(|rig| rig.pc.set_active(BALANCED)),
                PlanInactive,
            ),
            (
                "a value that can't be read",
                GameMode,
                Box::new(|rig| rig.pc.fail_read(AUTO_GAME_MODE)),
                CouldntRead,
            ),
            (
                "a plan that can't be read",
                PowerPlan,
                Box::new(|rig| rig.pc.fail_read(ACTIVE_PLAN)),
                CouldntRead,
            ),
        ];
        for (what, id, arrange, row) in cases {
            let rig = Rig::on(arranged());
            assert!(rig.apply(id).ok, "{what}");
            arrange(&rig);
            let change = change_told(&rig.open()[0]);

            let report = report_of(&rig.engine);

            let expected = if row == Applied {
                vec![change.clone()]
            } else {
                vec!["Nothing is active.".to_string()]
            };
            assert_eq!(active_told(&report), expected, "{what}");
            assert_eq!(
                history_told(&report),
                [format!("Applied {change}")],
                "{what}: the History keeps its line"
            );
            // The tab says the same, and it is the look at the tab that settles what the report left alone.
            assert_eq!(rig.state(id), row, "{what}");
        }
    }

    #[test]
    fn another_accounts_record_is_in_the_history_and_not_under_active() {
        // Nothing is set for that account, which for Game Mode means on: if anyone read it, it would hold.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[
            intent(GameMode, Some(OTHER_SID), dword(0), dword(1)),
            outcome(GameMode, Some(OTHER_SID)),
        ]);
        let dump = rig.pc.dump();

        let report = report_of(&rig.engine);

        assert_eq!(under(&report, "Active"), ["Nothing is active."]);
        assert_eq!(
            under(&report, "History"),
            ["2026-11-02 18:12 Applied Game Mode: off → on"]
        );
        assert_eq!(rig.pc.dump(), dump);
        let view = rig.view();
        assert_eq!(view.pending_other_accounts, 1, "it waits for its account");
        assert_ne!(row_in(&view, GameMode).state, Applied);

        // This account's own change is listed, and the other's still isn't.
        assert!(rig.apply(GameMode).ok);
        let report = report_of(&rig.engine);
        assert_eq!(active_told(&report), ["Game Mode: off → on"]);
        assert_eq!(history_told(&report).len(), 2, "{report}");
    }

    #[test]
    fn a_change_cut_short_that_holds_is_active_before_the_look_that_settles_it() {
        let rig = Rig::on(FakeSystem::new());
        rig.pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        rig.left_by_an_earlier_run(&[game_mode_intent()]);

        let report = report_of(&rig.engine);

        assert_eq!(
            report,
            "\
Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20
Active
  Game Mode: off → on (2026-11-02 18:12)
History
  No finished changes yet."
        );
        assert_eq!(
            rig.story(),
            ["intent game-mode"],
            "the report wrote nothing"
        );

        // The look at the tab calls it Applied, and from then on the History tells how it ended.
        assert_eq!(row_in(&rig.view(), GameMode).state, Applied);
        let report = report_of(&rig.engine);
        assert_eq!(
            under(&report, "Active"),
            ["Game Mode: off → on (2026-11-02 18:12)"]
        );
        assert_eq!(
            under(&report, "History"),
            ["2026-11-02 18:12 Kestrel was interrupted while applying Game Mode: off → on"]
        );

        // One that never reached the write holds nothing, so it is neither listed nor told.
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent()]);
        assert_eq!(report_of(&rig.engine), NO_CHANGES_TEXT);
    }

    #[test]
    fn system_protection_is_active_while_windows_says_it_is_on() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&protection_record());
        let line = "2026-11-02 18:12 Applied System Protection: off → on";

        let report = report_of(&rig.engine);

        assert_eq!(
            under(&report, "Active"),
            ["System Protection: off → on (2026-11-02 18:12)"]
        );
        assert_eq!(under(&report, "History"), [line]);

        // Off, the next look would close the record; blocked or unknown, nothing can be said of it. Neither is listed,
        // and the History keeps its line.
        for readiness in [
            Readiness::ProtectionOff,
            Readiness::BlockedByPolicy,
            Readiness::Unknown,
        ] {
            rig.pc.set_protection(readiness);
            let report = report_of(&rig.engine);
            assert_eq!(
                under(&report, "Active"),
                ["Nothing is active."],
                "{readiness:?}"
            );
            assert_eq!(under(&report, "History"), [line], "{readiness:?}");
        }
    }

    #[test]
    fn a_pc_with_every_record_in_another_state_is_reported_as_its_rows_are_and_changes_nothing() {
        let rig = Rig::on(arranged());
        for id in TweakId::ESSENTIAL {
            assert!(rig.apply(id).ok, "{id:?}");
        }
        let records = rig.open();
        rig.pc.set_active(BALANCED); // the plan is not in use
        change_elsewhere(&rig.pc, GameMode); // a third value
        put_back_by_hand(&rig.pc, BackgroundRecording); // back where it was
        rig.pc.fail_read(DIRECTX_SETTINGS); // can't be read
                                            // The graphics preference and the pointer still hold what Kestrel wrote.
        let file = rig.folder().join("journal.jsonl");
        let journal = std::fs::read(&file).unwrap();
        let dump = rig.pc.dump();
        let writes = rig.pc.writes();

        let report = report_of(&rig.engine);

        let told = |id: TweakId| {
            change_told(
                records
                    .iter()
                    .find(|record| record.tweak == id)
                    .expect("a record"),
            )
        };
        assert_eq!(
            active_told(&report),
            [told(PointerPrecision), told(GpuPreference)]
        );
        // The report closes nothing, so every change is still in the History, newest first.
        let applied: Vec<String> = TweakId::ESSENTIAL
            .into_iter()
            .rev()
            .map(|id| format!("Applied {}", told(id)))
            .collect();
        assert_eq!(history_told(&report), applied);
        // And it only read: the journal, the PC and the log of what was written to it are as they were.
        assert_eq!(std::fs::read(&file).unwrap(), journal);
        assert_eq!(rig.pc.dump(), dump);
        assert_eq!(rig.pc.writes(), writes);
        // Judging what holds takes no lock: with another process holding it, the report is the same.
        let held_elsewhere = rig.journal().lock().unwrap();
        assert_eq!(report_of(&rig.engine), report);
        drop(held_elsewhere);

        // The rows and `applied_ids` pick the same two.
        let view = rig.view();
        let applied_rows: Vec<TweakId> = TweakId::ESSENTIAL
            .into_iter()
            .filter(|id| row_in(&view, *id).state == Applied)
            .collect();
        assert_eq!(applied_rows, [GpuPreference, PointerPrecision]);
        assert_eq!(
            rig.engine.applied_ids(),
            ["gpu-preference", "pointer-precision"]
        );
    }

    #[test]
    fn the_report_warns_of_lines_it_couldnt_read() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[game_mode_intent(), outcome(GameMode, Some(SID))]);
        let file = rig.folder().join("journal.jsonl");

        // A line cut short at the end of the file is no cause for alarm.
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.push_str(r#"{"v":1,"kind":"outc"#);
        std::fs::write(&file, &text).unwrap();
        let report = report_of(&rig.engine);
        assert!(!report.contains("couldn't be read"), "{report}");

        // One in the middle is.
        let mut lines: Vec<&str> = text.lines().collect();
        lines.insert(1, "this is not a line of the journal");
        lines.pop();
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        let report = report_of(&rig.engine);
        assert_eq!(
            report.lines().last(),
            Some("Part of Kestrel's history couldn't be read.")
        );
        assert_eq!(rig.view().unreadable_lines, 1);
        assert_eq!(
            history_told(&report),
            ["Applied Game Mode: off → on"],
            "what could be read is still told"
        );
    }

    #[test]
    fn a_journal_that_cant_be_read_is_told_and_not_taken_for_an_empty_one() {
        let rig = Rig::on(arranged());
        rig.left_by_an_earlier_run(&[]);
        std::fs::create_dir(rig.folder().join("journal.jsonl")).unwrap();

        let report = report_of(&rig.engine);

        assert_eq!(report.lines().count(), 2, "{report}");
        assert!(
            report.starts_with(
                "Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20\n"
            ),
            "{report}"
        );
        assert!(
            report
                .lines()
                .nth(1)
                .unwrap()
                .starts_with("Kestrel's history couldn't be read: "),
            "{report}"
        );
        assert!(!report.contains("No changes yet."));

        // A folder the journal's check refuses is the same.
        let rig = Rig::on(arranged());
        let refusing = hooked_journal(&rig.folder(), |_| {
            Err(io::Error::other("not a trusted folder"))
        });
        let report = Engine::new(rig.pc.clone(), refusing).report("0.5.0", 26200, REPORT_AT);
        assert_eq!(
            report,
            "Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20\nKestrel's history couldn't be read: not a trusted folder"
        );
    }

    // How the page reads them.

    #[test]
    fn the_engines_types_are_spelled_the_way_the_page_reads_them() {
        for (gate, text) in [
            (Gate::Open, "open"),
            (Gate::GameRunning, "game-running"),
            (Gate::BenchmarkActive, "benchmark-active"),
        ] {
            let json = serde_json::to_string(&gate).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<Gate>(&json).unwrap(), gate);
        }
        let view = TweaksView {
            rows: Vec::new(),
            gate: Gate::Open,
            other_account: true,
            pending_other_accounts: 2,
            interrupted: vec!["a line".to_string()],
            unreadable_lines: 3,
            any_active: true,
        };
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            serde_json::json!({
                "rows": [],
                "gate": "open",
                "otherAccount": true,
                "pendingOtherAccounts": 2,
                "interrupted": ["a line"],
                "unreadableLines": 3,
                "anyActive": true,
            })
        );
        let preview = PreviewView {
            items: Vec::new(),
            restore: Readiness::ProtectionOff,
        };
        assert_eq!(
            serde_json::to_value(&preview).unwrap(),
            serde_json::json!({ "items": [], "restore": "protection-off" })
        );
        let item = ItemOutcome {
            id: GameMode,
            title: "Game Mode".to_string(),
            ok: true,
            message: "Applied.".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&item).unwrap(),
            serde_json::json!({ "id": "game-mode", "title": "Game Mode", "ok": true, "message": "Applied." })
        );
    }

    #[test]
    fn an_engine_can_be_shared_between_threads() {
        fn shareable<T: Send + Sync>() {}
        shareable::<Engine>();
    }

    #[test]
    fn building_an_engine_touches_nothing() {
        let rig = Rig::on(arranged());
        assert!(!rig.folder().exists());
        assert!(rig.pc.writes().is_empty());
    }
}
