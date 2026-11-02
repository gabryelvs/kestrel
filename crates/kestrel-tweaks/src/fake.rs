//! A pretend Windows, in memory and on any OS. The engine and the rules above it run against it on a Mac exactly as
//! they will run against `System` on Windows, and the app runs against it while it is being developed.
//!
//! It behaves the way Windows does where a mistake would otherwise pass unnoticed: a registry value has one type, so
//! reading a string as a DWORD is a failed read; names and key paths ignore case, and a name keeps the spelling it was
//! first written with; a key outlives its values; deleting a value that isn't there is an error; the plan in use can't
//! be deleted; `persist` saves the mouse strings in the profile of the account the call runs as.
//!
//! To arrange a PC, use the `set_*` and `add_*` methods. They are the test's own hands, not changes Kestrel made, so
//! they never appear in `writes()` and the failure switches don't stop them. The switches (`fail_*`,
//! `lie_on_readback`) make what goes through `System` go wrong. Failures are told apart from absence: a read that is
//! made to fail is `Read::Failed` even when the value isn't there. Once set, a switch stays until `clear_failures`.
//!
//! `writes()` records what changed, one line per change made through `System`, in the order they happened; a refused
//! change is not a change. `dump()` is the comparable state of the PC itself, without that log and the switches.
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::{Mutex, MutexGuard, PoisonError};

use kestrel_probe::account::Account;

use crate::system::{Read, Readiness, System};

/// The account a new `FakeSystem` is signed in as, and runs as.
pub const SID: &str = "S-1-5-21-1-2-3-1001";
/// Windows' own plans, which a new `FakeSystem` has.
pub const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const POWER_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";
/// Names for `fail_read` and `fail_write` where the thing read or written is not a registry value.
pub const MOUSE_LIVE: &str = "SPI_GETMOUSE/SPI_SETMOUSE";
pub const ACTIVE_PLAN: &str = "PowerGetActiveScheme";

/// Where Windows saves the mouse setting when `persist` is asked for.
const MOUSE_KEY: &str = r"Control Panel\Mouse";
/// What a lying readback of a string says: nothing a caller would take for a token.
const LIE: &str = "this is not what was written";

/// Registry names and paths are not case-sensitive.
fn fold(text: &str) -> String {
    text.to_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Dword(u32),
    Text(String),
}

impl Value {
    /// What a lying readback says instead: a number nothing would ever hold, or text with no token in it.
    fn lie(&self) -> Value {
        match self {
            Value::Dword(value) => Value::Dword(!value),
            Value::Text(_) => Value::Text(LIE.to_string()),
        }
    }
}

/// The values of one key, by folded name, each with the spelling it was first written with.
type Values = BTreeMap<String, (String, Value)>;

/// One registry hive: its keys by folded path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Hive {
    keys: BTreeMap<String, Values>,
}

impl Hive {
    fn get(&self, key: &str, name: &str) -> Option<&Value> {
        let (_, value) = self.keys.get(&fold(key))?.get(&fold(name))?;
        Some(value)
    }

    /// Sets a value, creating the key when it isn't there. A value that is there keeps its spelling.
    fn set(&mut self, key: &str, name: &str, value: Value) {
        match self.keys.entry(fold(key)).or_default().entry(fold(name)) {
            Entry::Occupied(mut found) => found.get_mut().1 = value,
            Entry::Vacant(slot) => {
                slot.insert((name.to_string(), value));
            }
        }
    }

    /// True when there was a value to remove.
    fn remove(&mut self, key: &str, name: &str) -> bool {
        self.keys
            .get_mut(&fold(key))
            .is_some_and(|values| values.remove(&fold(name)).is_some())
    }

    /// The names in a key, in name order; None when the key isn't there.
    fn names(&self, key: &str) -> Option<Vec<String>> {
        let values = self.keys.get(&fold(key))?;
        Some(values.values().map(|(name, _)| name.clone()).collect())
    }

    fn add_key(&mut self, key: &str) {
        self.keys.entry(fold(key)).or_default();
    }

    /// True when the key was there with nothing in it, and is gone now.
    fn remove_key_if_empty(&mut self, key: &str) -> bool {
        let empty = self.keys.get(&fold(key)).is_some_and(BTreeMap::is_empty);
        if empty {
            self.keys.remove(&fold(key));
        }
        empty
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    guid: String,
    name: String,
    description: String,
    /// The plugged-in value of each setting that has one, by (subgroup, setting).
    values: BTreeMap<(String, String), u32>,
}

/// The PC itself: everything `dump` compares.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    account: Account,
    /// Each account's hive, by SID as given.
    users: BTreeMap<String, Hive>,
    /// What `HKEY_LOCAL_MACHINE` holds, which is only ever policy here.
    policy: Hive,
    mouse: [i32; 3],
    /// In the order they were made.
    plans: Vec<Plan>,
    active: String,
    ranges: BTreeMap<(String, String), (u32, u32)>,
    modern_standby: Option<bool>,
    protection: Readiness,
}

/// What the fake has been told to get wrong. Names are folded; GUIDs are as given.
#[derive(Debug, Default)]
struct Failures {
    reads: BTreeSet<String>,
    writes: BTreeSet<String>,
    /// Base plans that can't be copied.
    copies: BTreeSet<String>,
    /// Plans that can't be activated.
    activations: BTreeSet<String>,
    /// Plans that can't be deleted.
    deletions: BTreeSet<String>,
    /// Values whose next write will read back wrongly once.
    lies: BTreeSet<String>,
    /// Values written under a lie that have not been read back yet.
    lying: BTreeSet<String>,
}

struct Inner {
    state: State,
    failures: Failures,
    log: Vec<String>,
}

fn no_such_plan(guid: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("there is no power plan {guid}"),
    )
}

fn failed_read<T>(name: &str) -> Read<T> {
    Read::Failed(format!("the fake was told that reading {name} fails"))
}

fn as_dword(read: Read<Value>, name: &str) -> Read<u32> {
    match read {
        Read::Absent => Read::Absent,
        Read::Failed(why) => Read::Failed(why),
        Read::Value(Value::Dword(value)) => Read::Value(value),
        Read::Value(Value::Text(_)) => Read::Failed(format!("{name} is not a DWORD value")),
    }
}

fn as_string(read: Read<Value>, name: &str) -> Read<String> {
    match read {
        Read::Absent => Read::Absent,
        Read::Failed(why) => Read::Failed(why),
        Read::Value(Value::Text(text)) => Read::Value(text),
        Read::Value(Value::Dword(_)) => Read::Failed(format!("{name} is not a string value")),
    }
}

impl Inner {
    fn plan(&self, guid: &str) -> Option<&Plan> {
        self.state.plans.iter().find(|plan| plan.guid == guid)
    }

    fn plan_mut(&mut self, guid: &str) -> Option<&mut Plan> {
        self.state.plans.iter_mut().find(|plan| plan.guid == guid)
    }

    /// An error when `name` has been told to refuse writes.
    fn refuse_write(&self, name: &str) -> io::Result<()> {
        if self.failures.writes.contains(&fold(name)) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("the fake was told that writing {name} fails"),
            ));
        }
        Ok(())
    }

    fn read_user(&mut self, sid: &str, key: &str, name: &str) -> Read<Value> {
        if self.failures.reads.contains(&fold(name)) {
            return failed_read(name);
        }
        let lying = self.failures.lying.remove(&fold(name));
        match self
            .state
            .users
            .get(sid)
            .and_then(|hive| hive.get(key, name))
        {
            None => Read::Absent,
            Some(value) if lying => Read::Value(value.lie()),
            Some(value) => Read::Value(value.clone()),
        }
    }

    fn read_policy(&self, key: &str, name: &str) -> Read<Value> {
        if self.failures.reads.contains(&fold(name)) {
            return failed_read(name);
        }
        match self.state.policy.get(key, name) {
            None => Read::Absent,
            Some(value) => Read::Value(value.clone()),
        }
    }

    fn write_user(
        &mut self,
        sid: &str,
        key: &str,
        name: &str,
        value: Value,
        line: String,
    ) -> io::Result<()> {
        self.refuse_write(name)?;
        self.state
            .users
            .entry(sid.to_string())
            .or_default()
            .set(key, name, value);
        // A lie lasts from the write it was set for until the value is first read back.
        let folded = fold(name);
        if self.failures.lies.remove(&folded) {
            self.failures.lying.insert(folded);
        } else {
            self.failures.lying.remove(&folded);
        }
        self.log.push(line);
        Ok(())
    }

    fn switch_protection(&mut self, to: Readiness, line: &str) -> io::Result<()> {
        if self.state.protection == Readiness::BlockedByPolicy {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "a policy decides System Protection",
            ));
        }
        self.state.protection = to;
        self.log.push(line.to_string());
        Ok(())
    }
}

/// Everything the pretend PC holds, for comparing before and after. It leaves out the log and the failure switches,
/// which say how the fake behaves and what was done to it, not what the PC is like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dump(State);

/// A pretend PC. Shareable: every method takes `&self`, so a test can keep an `Arc<FakeSystem>` to set things up and
/// look at the result while the engine holds the same one as an `Arc<dyn System>`.
pub struct FakeSystem {
    inner: Mutex<Inner>,
}

impl Default for FakeSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeSystem {
    /// A stock PC: signed in and running as `SID`; Windows' three plans with Balanced in use; no Modern Standby;
    /// System Protection ready; the mouse at its Windows default of `[6, 10, 1]`, with the strings under
    /// `Control Panel\Mouse` that go with it (`MouseSpeed` 1, `MouseThreshold1` 6, `MouseThreshold2` 10). Nothing else
    /// is set in the registry or in policy.
    pub fn new() -> Self {
        let stock = |guid: &str, name: &str| Plan {
            guid: guid.to_string(),
            name: name.to_string(),
            description: String::new(),
            values: BTreeMap::new(),
        };
        // Every Windows PC has these from the start. With them in place, setting the mouse and putting it back leaves
        // the registry as it was, which a missing key and missing strings would not.
        let mut user = Hive::default();
        for (name, value) in [
            ("MouseSpeed", "1"),
            ("MouseThreshold1", "6"),
            ("MouseThreshold2", "10"),
        ] {
            user.set(MOUSE_KEY, name, Value::Text(value.to_string()));
        }
        FakeSystem {
            inner: Mutex::new(Inner {
                state: State {
                    account: Account {
                        signed_in: Some(SID.to_string()),
                        this_process: Some(SID.to_string()),
                    },
                    users: BTreeMap::from([(SID.to_string(), user)]),
                    policy: Hive::default(),
                    mouse: [6, 10, 1],
                    plans: vec![
                        stock(BALANCED, "Balanced"),
                        stock(HIGH_PERFORMANCE, "High performance"),
                        stock(POWER_SAVER, "Power saver"),
                    ],
                    active: BALANCED.to_string(),
                    ranges: BTreeMap::new(),
                    modern_standby: Some(false),
                    protection: Readiness::Ready,
                },
                failures: Failures::default(),
                log: Vec::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A test that panicked while holding the lock must not bury its own failure under a poisoned one.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Setting up the PC.

    pub fn set_account(&self, account: Account) {
        self.lock().state.account = account;
    }

    pub fn set_dword(&self, sid: &str, key: &str, name: &str, value: u32) {
        let mut inner = self.lock();
        let hive = inner.state.users.entry(sid.to_string()).or_default();
        hive.set(key, name, Value::Dword(value));
    }

    /// The DWORD value; None when it is absent or of another type.
    pub fn dword(&self, sid: &str, key: &str, name: &str) -> Option<u32> {
        match self.lock().state.users.get(sid)?.get(key, name)? {
            Value::Dword(value) => Some(*value),
            Value::Text(_) => None,
        }
    }

    pub fn set_string(&self, sid: &str, key: &str, name: &str, value: &str) {
        let mut inner = self.lock();
        let hive = inner.state.users.entry(sid.to_string()).or_default();
        hive.set(key, name, Value::Text(value.to_string()));
    }

    /// The string value; None when it is absent or of another type.
    pub fn string(&self, sid: &str, key: &str, name: &str) -> Option<String> {
        match self.lock().state.users.get(sid)?.get(key, name)? {
            Value::Text(text) => Some(text.clone()),
            Value::Dword(_) => None,
        }
    }

    /// Removes a value if it is there, leaving its key.
    pub fn remove_value(&self, sid: &str, key: &str, name: &str) {
        if let Some(hive) = self.lock().state.users.get_mut(sid) {
            hive.remove(key, name);
        }
    }

    /// Makes a key exist with nothing in it, or leaves it as it is.
    pub fn add_key(&self, sid: &str, key: &str) {
        let mut inner = self.lock();
        inner
            .state
            .users
            .entry(sid.to_string())
            .or_default()
            .add_key(key);
    }

    pub fn set_policy_dword(&self, key: &str, name: &str, value: u32) {
        self.lock().state.policy.set(key, name, Value::Dword(value));
    }

    pub fn set_policy_string(&self, key: &str, name: &str, value: &str) {
        let value = Value::Text(value.to_string());
        self.lock().state.policy.set(key, name, value);
    }

    /// Sets the mouse in force now, without anything being saved.
    pub fn set_mouse(&self, values: [i32; 3]) {
        self.lock().state.mouse = values;
    }

    /// Adds a plan with this name and no settings, or renames the one that is there.
    pub fn add_plan(&self, guid: &str, name: &str) {
        let mut inner = self.lock();
        if let Some(plan) = inner.plan_mut(guid) {
            plan.name = name.to_string();
            return;
        }
        inner.state.plans.push(Plan {
            guid: guid.to_string(),
            name: name.to_string(),
            description: String::new(),
            values: BTreeMap::new(),
        });
    }

    /// Makes a plan that exists the one in use.
    pub fn set_active(&self, guid: &str) {
        let mut inner = self.lock();
        assert!(
            inner.plan(guid).is_some(),
            "the fake has no power plan {guid}"
        );
        inner.state.active = guid.to_string();
    }

    /// The GUID and name of every plan, in the order they were made.
    pub fn plans(&self) -> Vec<(String, String)> {
        let inner = self.lock();
        let plans = &inner.state.plans;
        plans
            .iter()
            .map(|plan| (plan.guid.clone(), plan.name.clone()))
            .collect()
    }

    /// Sets the plugged-in value of a setting in a plan that exists.
    pub fn set_plan_value(&self, guid: &str, subgroup: &str, setting: &str, value: u32) {
        let mut inner = self.lock();
        let plan = inner
            .plan_mut(guid)
            .unwrap_or_else(|| panic!("the fake has no power plan {guid}"));
        plan.values
            .insert((subgroup.to_string(), setting.to_string()), value);
    }

    pub fn plan_value(&self, guid: &str, subgroup: &str, setting: &str) -> Option<u32> {
        let inner = self.lock();
        let plan = inner.plan(guid)?;
        plan.values
            .get(&(subgroup.to_string(), setting.to_string()))
            .copied()
    }

    pub fn plan_description(&self, guid: &str) -> Option<String> {
        self.lock().plan(guid).map(|plan| plan.description.clone())
    }

    /// Gives a setting the range Windows says it accepts. Settings with no range say None, as when Windows won't say.
    pub fn set_plan_range(&self, subgroup: &str, setting: &str, min: u32, max: u32) {
        self.lock()
            .state
            .ranges
            .insert((subgroup.to_string(), setting.to_string()), (min, max));
    }

    pub fn set_modern_standby(&self, modern_standby: Option<bool>) {
        self.lock().state.modern_standby = modern_standby;
    }

    pub fn set_protection(&self, readiness: Readiness) {
        self.lock().state.protection = readiness;
    }

    // Making things go wrong. Reads and writes are named by the registry value they touch, in any key or account. A
    // key is named by its path: `fail_read` makes listing its values fail, and `fail_write` makes deleting it when it
    // is empty fail. `MOUSE_LIVE` names the live mouse setting and `ACTIVE_PLAN` the plan in use. A setting of a power
    // plan is named by its GUID, in any plan, and describing a plan (`fail_write` only) by the plan's own GUID. Case
    // doesn't matter.

    /// Reads of this fail, whether or not it is there.
    pub fn fail_read(&self, name: &str) {
        self.lock().failures.reads.insert(fold(name));
    }

    /// Writes and deletes of this are refused (`PermissionDenied`) and change nothing.
    pub fn fail_write(&self, name: &str) {
        self.lock().failures.writes.insert(fold(name));
    }

    /// Copying this plan fails.
    pub fn fail_duplicate(&self, guid: &str) {
        self.lock().failures.copies.insert(guid.to_string());
    }

    /// Activating this plan fails.
    pub fn fail_activate(&self, guid: &str) {
        self.lock().failures.activations.insert(guid.to_string());
    }

    /// Deleting this plan fails.
    pub fn fail_delete(&self, guid: &str) {
        self.lock().failures.deletions.insert(guid.to_string());
    }

    /// The next write to this value is stored as written, but the first read of it afterwards says something that
    /// isn't there (a DWORD that was never written, a string with no tokens in it). Later reads, and the accessors
    /// `dword` and `string`, are true; so is a read before the write, which is how a caller finds the prior value.
    pub fn lie_on_readback(&self, name: &str) {
        self.lock().failures.lies.insert(fold(name));
    }

    /// Takes back every switch above, and any lie not yet told.
    pub fn clear_failures(&self) {
        self.lock().failures = Failures::default();
    }

    // Looking at what happened.

    /// One line for each change made through `System`, in order: `dword <sid>\<key>\<name> = <value>`,
    /// `string <sid>\<key>\<name> = <value>`, `delete <sid>\<key>\<name>`, `delete-key <sid>\<key>`,
    /// `mouse [a, b, c]` (with ` persist` when asked), `duplicate-plan <base> -> <new>`, `describe-plan <guid> <name>`,
    /// `plan-value <guid> <subgroup>/<setting> = <value>`, `activate-plan <guid>`, `delete-plan <guid>`,
    /// `protection on` and `protection off`. A change that was refused, or that deleted nothing, is not in it.
    pub fn writes(&self) -> Vec<String> {
        self.lock().log.clone()
    }

    pub fn dump(&self) -> Dump {
        let mut state = self.lock().state.clone();
        // A hive with nothing in it is the same as no hive.
        state.users.retain(|_, hive| !hive.keys.is_empty());
        Dump(state)
    }
}

impl System for FakeSystem {
    fn account(&self) -> Account {
        self.lock().state.account.clone()
    }

    fn read_dword(&self, sid: &str, key: &str, name: &str) -> Read<u32> {
        let read = self.lock().read_user(sid, key, name);
        as_dword(read, name)
    }

    fn write_dword(&self, sid: &str, key: &str, name: &str, value: u32) -> io::Result<()> {
        let line = format!(r"dword {sid}\{key}\{name} = {value}");
        let mut inner = self.lock();
        inner.write_user(sid, key, name, Value::Dword(value), line)
    }

    fn read_string(&self, sid: &str, key: &str, name: &str) -> Read<String> {
        let read = self.lock().read_user(sid, key, name);
        as_string(read, name)
    }

    fn write_string(&self, sid: &str, key: &str, name: &str, value: &str) -> io::Result<()> {
        let line = format!(r"string {sid}\{key}\{name} = {value}");
        let mut inner = self.lock();
        inner.write_user(sid, key, name, Value::Text(value.to_string()), line)
    }

    fn delete_value(&self, sid: &str, key: &str, name: &str) -> io::Result<()> {
        let mut inner = self.lock();
        inner.refuse_write(name)?;
        let removed = inner
            .state
            .users
            .get_mut(sid)
            .is_some_and(|hive| hive.remove(key, name));
        if !removed {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{name} is not in {key}"),
            ));
        }
        // Whatever lie was waiting for this value went with it.
        inner.failures.lying.remove(&fold(name));
        inner.log.push(format!(r"delete {sid}\{key}\{name}"));
        Ok(())
    }

    fn value_names(&self, sid: &str, key: &str) -> Read<Vec<String>> {
        let inner = self.lock();
        if inner.failures.reads.contains(&fold(key)) {
            return failed_read(key);
        }
        match inner.state.users.get(sid).and_then(|hive| hive.names(key)) {
            Some(names) => Read::Value(names),
            None => Read::Absent,
        }
    }

    fn delete_key_if_empty(&self, sid: &str, key: &str) -> io::Result<()> {
        let mut inner = self.lock();
        inner.refuse_write(key)?;
        let deleted = inner
            .state
            .users
            .get_mut(sid)
            .is_some_and(|hive| hive.remove_key_if_empty(key));
        if deleted {
            inner.log.push(format!(r"delete-key {sid}\{key}"));
        }
        Ok(())
    }

    fn read_policy_dword(&self, key: &str, name: &str) -> Read<u32> {
        as_dword(self.lock().read_policy(key, name), name)
    }

    fn read_policy_string(&self, key: &str, name: &str) -> Read<String> {
        as_string(self.lock().read_policy(key, name), name)
    }

    fn mouse_live(&self) -> Read<[i32; 3]> {
        let inner = self.lock();
        if inner.failures.reads.contains(&fold(MOUSE_LIVE)) {
            return failed_read(MOUSE_LIVE);
        }
        Read::Value(inner.state.mouse)
    }

    fn set_mouse_live(&self, values: [i32; 3], persist: bool) -> io::Result<()> {
        let mut inner = self.lock();
        inner.refuse_write(MOUSE_LIVE)?;
        inner.state.mouse = values;
        let mut line = format!("mouse {values:?}");
        if persist {
            line.push_str(" persist");
            // Windows saves them in the profile of the account the call runs as, which is not the signed-in one when
            // the accounts differ. With no profile to name, nothing is saved.
            if let Some(sid) = inner.state.account.this_process.clone() {
                let hive = inner.state.users.entry(sid).or_default();
                let text = |number: i32| Value::Text(number.to_string());
                hive.set(MOUSE_KEY, "MouseSpeed", text(values[2]));
                hive.set(MOUSE_KEY, "MouseThreshold1", text(values[0]));
                hive.set(MOUSE_KEY, "MouseThreshold2", text(values[1]));
            }
        }
        inner.log.push(line);
        Ok(())
    }

    fn active_plan(&self) -> Read<String> {
        let inner = self.lock();
        if inner.failures.reads.contains(&fold(ACTIVE_PLAN)) {
            return failed_read(ACTIVE_PLAN);
        }
        Read::Value(inner.state.active.clone())
    }

    fn plan_exists(&self, guid: &str) -> bool {
        self.lock().plan(guid).is_some()
    }

    fn plan_name(&self, guid: &str) -> Option<String> {
        self.lock().plan(guid).map(|plan| plan.name.clone())
    }

    fn duplicate_plan(&self, base: &str, new: &str) -> io::Result<()> {
        let mut inner = self.lock();
        if inner.failures.copies.contains(base) {
            return Err(io::Error::other(format!(
                "the fake was told that copying power plan {base} fails"
            )));
        }
        let copy = inner.plan(base).ok_or_else(|| no_such_plan(base))?.clone();
        if inner.plan(new).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("power plan {new} already exists"),
            ));
        }
        inner.state.plans.push(Plan {
            guid: new.to_string(),
            ..copy
        });
        inner.log.push(format!("duplicate-plan {base} -> {new}"));
        Ok(())
    }

    fn describe_plan(&self, guid: &str, name: &str, description: &str) -> io::Result<()> {
        let mut inner = self.lock();
        inner.refuse_write(guid)?;
        let plan = inner.plan_mut(guid).ok_or_else(|| no_such_plan(guid))?;
        plan.name = name.to_string();
        plan.description = description.to_string();
        inner.log.push(format!("describe-plan {guid} {name}"));
        Ok(())
    }

    fn read_plan_value(&self, guid: &str, subgroup: &str, setting: &str) -> Read<u32> {
        let mut inner = self.lock();
        let folded = fold(setting);
        if inner.failures.reads.contains(&folded) {
            return failed_read(setting);
        }
        let lying = inner.failures.lying.remove(&folded);
        let key = (subgroup.to_string(), setting.to_string());
        match inner.plan(guid).and_then(|plan| plan.values.get(&key)) {
            Some(value) if lying => Read::Value(!value),
            Some(value) => Read::Value(*value),
            None => Read::Absent,
        }
    }

    fn plan_value_range(&self, subgroup: &str, setting: &str) -> Option<(u32, u32)> {
        let inner = self.lock();
        let key = (subgroup.to_string(), setting.to_string());
        inner.state.ranges.get(&key).copied()
    }

    fn write_plan_value(
        &self,
        guid: &str,
        subgroup: &str,
        setting: &str,
        value: u32,
    ) -> io::Result<()> {
        let mut inner = self.lock();
        inner.refuse_write(setting)?;
        let plan = inner.plan_mut(guid).ok_or_else(|| no_such_plan(guid))?;
        plan.values
            .insert((subgroup.to_string(), setting.to_string()), value);
        // A lie lasts from the write it was set for until the value is first read back.
        let folded = fold(setting);
        if inner.failures.lies.remove(&folded) {
            inner.failures.lying.insert(folded);
        } else {
            inner.failures.lying.remove(&folded);
        }
        inner
            .log
            .push(format!("plan-value {guid} {subgroup}/{setting} = {value}"));
        Ok(())
    }

    fn activate_plan(&self, guid: &str) -> io::Result<()> {
        let mut inner = self.lock();
        if inner.failures.activations.contains(guid) {
            return Err(io::Error::other(format!(
                "the fake was told that activating power plan {guid} fails"
            )));
        }
        if inner.plan(guid).is_none() {
            return Err(no_such_plan(guid));
        }
        inner.state.active = guid.to_string();
        inner.log.push(format!("activate-plan {guid}"));
        Ok(())
    }

    fn delete_plan(&self, guid: &str) -> io::Result<()> {
        let mut inner = self.lock();
        if inner.failures.deletions.contains(guid) {
            return Err(io::Error::other(format!(
                "the fake was told that deleting power plan {guid} fails"
            )));
        }
        if inner.plan(guid).is_none() {
            return Err(no_such_plan(guid));
        }
        if inner.state.active == guid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Windows won't delete the power plan in use",
            ));
        }
        inner.state.plans.retain(|plan| plan.guid != guid);
        inner.log.push(format!("delete-plan {guid}"));
        Ok(())
    }

    fn modern_standby(&self) -> Option<bool> {
        self.lock().state.modern_standby
    }

    fn protection(&self) -> Readiness {
        self.lock().state.protection
    }

    fn turn_protection_on(&self) -> io::Result<()> {
        self.lock()
            .switch_protection(Readiness::Ready, "protection on")
    }

    fn turn_protection_off(&self) -> io::Result<()> {
        self.lock()
            .switch_protection(Readiness::ProtectionOff, "protection off")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens;
    use std::sync::Arc;

    const GAME_BAR: &str = r"Software\Microsoft\GameBar";
    const GAME_MODE: &str = "AutoGameModeEnabled";
    const DIRECTX: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
    const SETTINGS: &str = "DirectXUserGlobalSettings";
    const MOUSE_KEY: &str = r"Control Panel\Mouse";
    const POLICY: &str = r"SOFTWARE\Policies\Microsoft\Windows\GameDVR";
    const POWER_POLICY: &str = r"SOFTWARE\Policies\Microsoft\Power\PowerSettings";
    const OTHER_SID: &str = "S-1-5-21-1-2-3-1002";
    const KESTREL: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";
    const EXM: &str = "0e1d2c3b-4a59-4687-b6a5-948372615049";
    const SUBGROUP: &str = "54533251-82be-4824-96c1-47b60b740d00";
    const SETTING: &str = "0cc5b647-c1df-4637-891a-dec35c318583";

    fn plan(guid: &str, name: &str) -> (String, String) {
        (guid.to_string(), name.to_string())
    }

    fn failed<T>(read: &Read<T>) -> bool {
        matches!(read, Read::Failed(_))
    }

    /// The names in a key that exists.
    fn names(pc: &FakeSystem, sid: &str, key: &str) -> Vec<String> {
        match pc.value_names(sid, key) {
            Read::Value(names) => names,
            other => panic!("expected the key to exist, got {other:?}"),
        }
    }

    #[test]
    fn a_new_pc_is_a_stock_one() {
        let pc = FakeSystem::new();
        let me = Account {
            signed_in: Some(SID.to_string()),
            this_process: Some(SID.to_string()),
        };
        assert_eq!(SID, "S-1-5-21-1-2-3-1001");
        assert_eq!(pc.account(), me);
        assert!(!pc.account().differs());
        assert_eq!(
            pc.plans(),
            [
                plan(BALANCED, "Balanced"),
                plan(HIGH_PERFORMANCE, "High performance"),
                plan(POWER_SAVER, "Power saver"),
            ]
        );
        assert_eq!(pc.active_plan(), Read::Value(BALANCED.to_string()));
        assert_eq!(pc.plan_name(BALANCED).as_deref(), Some("Balanced"));
        assert_eq!(pc.modern_standby(), Some(false));
        assert_eq!(pc.protection(), Readiness::Ready);
        assert_eq!(pc.mouse_live(), Read::Value([6, 10, 1]));
        // The strings that go with that mouse setting are there, as on any Windows PC...
        for (name, value) in [
            ("MouseSpeed", "1"),
            ("MouseThreshold1", "6"),
            ("MouseThreshold2", "10"),
        ] {
            assert_eq!(pc.string(SID, MOUSE_KEY, name).as_deref(), Some(value));
        }
        assert_eq!(names(&pc, SID, MOUSE_KEY).len(), 3);
        // ...and nothing else is set, so every other read says Absent.
        assert_eq!(pc.read_dword(SID, GAME_BAR, GAME_MODE), Read::Absent);
        assert_eq!(pc.read_string(SID, DIRECTX, SETTINGS), Read::Absent);
        assert_eq!(pc.value_names(SID, DIRECTX), Read::Absent);
        assert_eq!(pc.value_names(OTHER_SID, MOUSE_KEY), Read::Absent);
        assert_eq!(
            pc.read_policy_dword(POLICY, "AllowAutoGameMode"),
            Read::Absent
        );
        assert_eq!(
            pc.read_policy_string(POLICY, "AllowAutoGameMode"),
            Read::Absent
        );
        assert_eq!(
            pc.read_plan_value(BALANCED, SUBGROUP, SETTING),
            Read::Absent
        );
        assert!(pc.writes().is_empty());
    }

    #[test]
    fn a_value_has_one_type_and_belongs_to_one_account() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 0);
        assert_eq!(pc.read_dword(SID, GAME_BAR, GAME_MODE), Read::Value(0));
        assert_eq!(pc.dword(SID, GAME_BAR, GAME_MODE), Some(0));
        assert_eq!(pc.string(SID, GAME_BAR, GAME_MODE), None);
        // Another account's hive doesn't see it.
        assert_eq!(pc.read_dword(OTHER_SID, GAME_BAR, GAME_MODE), Read::Absent);
        // Writing the other type replaces the value, as the registry does.
        pc.write_string(SID, GAME_BAR, GAME_MODE, "1").unwrap();
        assert_eq!(
            pc.read_string(SID, GAME_BAR, GAME_MODE),
            Read::Value("1".to_string())
        );
        assert_eq!(pc.dword(SID, GAME_BAR, GAME_MODE), None);
        assert_eq!(names(&pc, SID, GAME_BAR), [GAME_MODE]);
    }

    #[test]
    fn policy_values_are_machine_wide_and_typed() {
        let pc = FakeSystem::new();
        pc.set_policy_dword(POLICY, "AllowAutoGameMode", 0);
        pc.set_policy_string(POWER_POLICY, "ActivePowerScheme", EXM);
        assert_eq!(
            pc.read_policy_dword(POLICY, "allowautogamemode"),
            Read::Value(0)
        );
        assert_eq!(
            pc.read_policy_string(POWER_POLICY, "ActivePowerScheme"),
            Read::Value(EXM.to_string())
        );
        // A value of the other type is a failure, not a missing value.
        assert!(failed(&pc.read_policy_string(POLICY, "AllowAutoGameMode")));
        assert!(failed(
            &pc.read_policy_dword(POWER_POLICY, "ActivePowerScheme")
        ));
        // Policy belongs to no account, so the per-user reads don't see it.
        assert_eq!(
            pc.read_dword(SID, POLICY, "AllowAutoGameMode"),
            Read::Absent
        );
    }

    #[test]
    fn failed_reads_are_failed_not_absent() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 1);
        pc.set_string(SID, DIRECTX, SETTINGS, "VRROptimizeEnable=0;");

        // A value of the wrong type is a failure.
        assert!(failed(&pc.read_string(SID, GAME_BAR, GAME_MODE)));
        assert!(failed(&pc.read_dword(SID, DIRECTX, SETTINGS)));

        // A read told to fail fails for every kind of read of that name, whether or not the value is there.
        for name in [GAME_MODE, "NeverWritten"] {
            pc.fail_read(name);
            assert!(failed(&pc.read_dword(SID, GAME_BAR, name)), "{name}");
            assert!(failed(&pc.read_string(SID, GAME_BAR, name)), "{name}");
            assert!(failed(&pc.read_policy_dword(POLICY, name)), "{name}");
            assert!(failed(&pc.read_policy_string(POLICY, name)), "{name}");
        }
        // A key's own listing fails by the key's path, and the two reads that aren't registry values by their own names.
        assert!(!failed(&pc.value_names(SID, DIRECTX)));
        pc.fail_read(DIRECTX);
        assert!(failed(&pc.value_names(SID, DIRECTX)));
        // Failing, even for a key that isn't there, is not the same as that key being absent.
        pc.fail_read(r"Software\Never\Made");
        assert!(failed(&pc.value_names(SID, r"Software\Never\Made")));
        assert_eq!(pc.value_names(SID, r"Software\Other"), Read::Absent);
        pc.fail_read(MOUSE_LIVE);
        assert!(failed(&pc.mouse_live()));
        pc.fail_read(ACTIVE_PLAN);
        assert!(failed(&pc.active_plan()));

        // Other reads are untouched, and a value that isn't there is still Absent.
        assert_eq!(pc.read_dword(SID, GAME_BAR, "Other"), Read::Absent);
        assert_eq!(
            pc.value_names(SID, GAME_BAR),
            Read::Value(vec![GAME_MODE.to_string()])
        );

        // Lifting the failures brings the values back as they were.
        pc.clear_failures();
        assert_eq!(pc.read_dword(SID, GAME_BAR, GAME_MODE), Read::Value(1));
        assert_eq!(pc.mouse_live(), Read::Value([6, 10, 1]));
        assert_eq!(pc.active_plan(), Read::Value(BALANCED.to_string()));
    }

    #[test]
    fn names_match_whatever_their_case_and_keep_the_spelling_they_were_first_written_with() {
        let pc = FakeSystem::new();
        let lower = r"c:\riot games\valorant\valorant-win64-shipping.exe";
        let proper = r"C:\Riot Games\VALORANT\VALORANT-Win64-Shipping.exe";
        let slashes = "C:/Riot Games/VALORANT/VALORANT-Win64-Shipping.exe";
        pc.set_string(SID, DIRECTX, lower, "GpuPreference=1;");

        // The same name in another case, in a key spelled another way, is the same value.
        pc.write_string(
            SID,
            r"SOFTWARE\MICROSOFT\DIRECTX\USERGPUPREFERENCES",
            proper,
            "GpuPreference=2;",
        )
        .unwrap();
        assert_eq!(names(&pc, SID, DIRECTX), [lower]);
        assert_eq!(
            pc.read_string(SID, DIRECTX, proper),
            Read::Value("GpuPreference=2;".to_string())
        );
        // A name with other slashes is another value.
        pc.write_string(SID, DIRECTX, slashes, "GpuPreference=2;")
            .unwrap();
        let mut both = names(&pc, SID, DIRECTX);
        both.sort();
        let mut expected = vec![lower.to_string(), slashes.to_string()];
        expected.sort();
        assert_eq!(both, expected);
        // Deleting by another spelling deletes the one value.
        pc.delete_value(SID, DIRECTX, proper).unwrap();
        assert_eq!(names(&pc, SID, DIRECTX), [slashes]);
        // The failure switches match the same way.
        pc.fail_read("DIRECTXUSERGLOBALSETTINGS");
        assert!(failed(&pc.read_string(SID, DIRECTX, SETTINGS)));
    }

    #[test]
    fn a_write_creates_its_key_and_only_an_empty_key_is_deleted() {
        let pc = FakeSystem::new();
        assert_eq!(pc.value_names(SID, DIRECTX), Read::Absent);

        pc.write_string(SID, DIRECTX, "A", "x").unwrap();
        assert_eq!(names(&pc, SID, DIRECTX), ["A"]);
        // With a value in it the key is left alone, and that is not an error.
        pc.delete_key_if_empty(SID, DIRECTX).unwrap();
        assert_eq!(names(&pc, SID, DIRECTX), ["A"]);

        pc.delete_value(SID, DIRECTX, "A").unwrap();
        // The key outlives its last value, as in the registry.
        assert_eq!(pc.value_names(SID, DIRECTX), Read::Value(vec![]));
        pc.delete_key_if_empty(SID, DIRECTX).unwrap();
        assert_eq!(pc.value_names(SID, DIRECTX), Read::Absent);
        // Already gone is fine too.
        pc.delete_key_if_empty(SID, DIRECTX).unwrap();

        // A key can exist with nothing in it from the start.
        pc.add_key(SID, DIRECTX);
        assert_eq!(pc.value_names(SID, DIRECTX), Read::Value(vec![]));
    }

    #[test]
    fn deleting_a_value_that_isnt_there_is_an_error_and_leaves_no_trace() {
        let pc = FakeSystem::new();
        let error = pc.delete_value(SID, GAME_BAR, GAME_MODE).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        // A key that exists without the value says the same.
        pc.add_key(SID, GAME_BAR);
        let error = pc.delete_value(SID, GAME_BAR, GAME_MODE).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(pc.writes().is_empty());
    }

    #[test]
    fn a_refused_write_changes_nothing_and_leaves_no_trace_in_the_log() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 0);
        pc.set_string(SID, DIRECTX, SETTINGS, "A=1;");
        let before = pc.dump();

        pc.fail_write(GAME_MODE);
        pc.fail_write(SETTINGS);
        pc.fail_write(MOUSE_LIVE);
        let refused = pc.write_dword(SID, GAME_BAR, GAME_MODE, 1).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert!(pc.delete_value(SID, GAME_BAR, GAME_MODE).is_err());
        assert!(pc.write_string(SID, DIRECTX, SETTINGS, "").is_err());
        assert!(pc.delete_value(SID, DIRECTX, SETTINGS).is_err());
        assert!(pc.set_mouse_live([0, 0, 0], true).is_err());
        assert_eq!(pc.dump(), before);
        assert!(pc.writes().is_empty());

        // A key's path can be refused too, so an empty key that would have gone is still there.
        let empty = r"Software\Kestrel\Empty";
        pc.add_key(SID, empty);
        let with_empty_key = pc.dump();
        pc.fail_write(empty);
        assert!(pc.delete_key_if_empty(SID, empty).is_err());
        assert_eq!(pc.dump(), with_empty_key);
        assert_eq!(pc.value_names(SID, empty), Read::Value(vec![]));
        assert!(pc.writes().is_empty());

        // Other values still write, and lifting the failures lets the refused ones through.
        pc.write_dword(SID, GAME_BAR, "Other", 1).unwrap();
        pc.clear_failures();
        pc.write_dword(SID, GAME_BAR, GAME_MODE, 1).unwrap();
        assert_eq!(pc.dword(SID, GAME_BAR, GAME_MODE), Some(1));
    }

    #[test]
    fn a_lying_readback_lies_once_after_the_next_write_and_never_to_the_accessors() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 0);
        pc.set_string(SID, DIRECTX, SETTINGS, "A=1;");
        pc.lie_on_readback(GAME_MODE);
        pc.lie_on_readback(SETTINGS);

        // Until something is written the reads are true, so the caller sees the real prior value.
        assert_eq!(pc.read_dword(SID, GAME_BAR, GAME_MODE), Read::Value(0));
        assert_eq!(
            pc.read_string(SID, DIRECTX, SETTINGS),
            Read::Value("A=1;".to_string())
        );

        pc.write_dword(SID, GAME_BAR, GAME_MODE, 1).unwrap();
        pc.write_string(SID, DIRECTX, SETTINGS, "A=1;B=1;").unwrap();
        // What is stored is what was written.
        assert_eq!(pc.dword(SID, GAME_BAR, GAME_MODE), Some(1));
        assert_eq!(
            pc.string(SID, DIRECTX, SETTINGS).as_deref(),
            Some("A=1;B=1;")
        );
        // The first read back says something else, which nothing would ever hold.
        match pc.read_dword(SID, GAME_BAR, GAME_MODE) {
            Read::Value(value) => assert!(value != 0 && value != 1, "{value}"),
            other => panic!("expected a value, got {other:?}"),
        }
        match pc.read_string(SID, DIRECTX, SETTINGS) {
            Read::Value(text) => {
                assert_eq!(tokens::get(&text, "A"), None, "{text:?}");
                assert_eq!(tokens::get(&text, "B"), None, "{text:?}");
            }
            other => panic!("expected a value, got {other:?}"),
        }
        // After that the reads are true again, so putting the prior back can be done from what is really there.
        assert_eq!(pc.read_dword(SID, GAME_BAR, GAME_MODE), Read::Value(1));
        assert_eq!(
            pc.read_string(SID, DIRECTX, SETTINGS),
            Read::Value("A=1;B=1;".to_string())
        );
        // The switch was used up by the first write: the next one reads back true.
        pc.write_dword(SID, GAME_BAR, GAME_MODE, 0).unwrap();
        assert_eq!(pc.read_dword(SID, GAME_BAR, GAME_MODE), Read::Value(0));
    }

    #[test]
    fn dump_changes_when_a_value_changes() {
        type Change = Box<dyn Fn(&FakeSystem)>;
        let changes: Vec<(&str, Change)> = vec![
            (
                "a DWORD",
                Box::new(|pc| pc.set_dword(SID, GAME_BAR, GAME_MODE, 0)),
            ),
            (
                "a string",
                Box::new(|pc| pc.set_string(SID, DIRECTX, SETTINGS, "A=1;")),
            ),
            ("an empty key", Box::new(|pc| pc.add_key(SID, DIRECTX))),
            (
                "another account's value",
                Box::new(|pc| pc.set_dword(OTHER_SID, GAME_BAR, GAME_MODE, 0)),
            ),
            (
                "a policy",
                Box::new(|pc| pc.set_policy_dword(POLICY, "AllowAutoGameMode", 0)),
            ),
            ("the mouse", Box::new(|pc| pc.set_mouse([0, 0, 0]))),
            (
                "a plan",
                Box::new(|pc| pc.add_plan(EXM, "EXM Free Power Plan V8")),
            ),
            (
                "the active plan",
                Box::new(|pc| pc.set_active(HIGH_PERFORMANCE)),
            ),
            (
                "a plan's value",
                Box::new(|pc| pc.set_plan_value(BALANCED, SUBGROUP, SETTING, 5)),
            ),
            (
                "a plan's range",
                Box::new(|pc| pc.set_plan_range(SUBGROUP, SETTING, 0, 50)),
            ),
            (
                "a plan's name",
                Box::new(|pc| pc.describe_plan(BALANCED, "Mine", "").unwrap()),
            ),
            (
                "Modern Standby",
                Box::new(|pc| pc.set_modern_standby(Some(true))),
            ),
            (
                "protection",
                Box::new(|pc| pc.set_protection(Readiness::ProtectionOff)),
            ),
            (
                "the account",
                Box::new(|pc| {
                    pc.set_account(Account {
                        signed_in: None,
                        this_process: Some(SID.to_string()),
                    })
                }),
            ),
        ];
        for (what, change) in changes {
            let pc = FakeSystem::new();
            let before = pc.dump();
            assert_eq!(pc.dump(), before, "{what}: looking changes nothing");
            change(&pc);
            let after = pc.dump();
            assert_ne!(after, before, "{what}");
            change(&pc);
            assert_eq!(
                pc.dump(),
                after,
                "{what}: the same change twice is one change"
            );
        }

        // A value of the same name changing only its value, in place, shows too.
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 0);
        let zero = pc.dump();
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 1);
        assert_ne!(pc.dump(), zero);
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 0);
        assert_eq!(pc.dump(), zero);
    }

    #[test]
    fn dump_leaves_out_the_log_and_the_failure_switches_and_forgets_what_was_put_back() {
        let pc = FakeSystem::new();
        let stock = pc.dump();
        pc.fail_read("a");
        pc.fail_write("b");
        pc.fail_duplicate(BALANCED);
        pc.fail_activate(BALANCED);
        pc.lie_on_readback("c");
        let _ = pc.read_dword(SID, GAME_BAR, GAME_MODE);
        assert_eq!(pc.dump(), stock);

        // A change undone to the last detail leaves the PC as it was, though the log remembers both.
        pc.write_dword(SID, GAME_BAR, GAME_MODE, 1).unwrap();
        assert_ne!(pc.dump(), stock);
        pc.delete_value(SID, GAME_BAR, GAME_MODE).unwrap();
        pc.delete_key_if_empty(SID, GAME_BAR).unwrap();
        assert_eq!(pc.dump(), stock);
        assert_eq!(pc.writes().len(), 3);
    }

    #[test]
    fn setting_the_mouse_for_good_writes_the_three_strings_the_way_windows_does() {
        let pc = FakeSystem::new();
        // Live only: the strings on disk keep what they had.
        pc.set_mouse_live([4, 8, 2], false).unwrap();
        assert_eq!(pc.mouse_live(), Read::Value([4, 8, 2]));
        assert_eq!(
            pc.string(SID, MOUSE_KEY, "MouseSpeed").as_deref(),
            Some("1")
        );
        assert_eq!(
            pc.string(SID, MOUSE_KEY, "MouseThreshold1").as_deref(),
            Some("6")
        );

        // For good: thresholds 1 and 2 are the first two numbers and MouseSpeed is the third, all as text.
        pc.set_mouse_live([4, 8, 2], true).unwrap();
        assert_eq!(
            pc.string(SID, MOUSE_KEY, "MouseThreshold1").as_deref(),
            Some("4")
        );
        assert_eq!(
            pc.string(SID, MOUSE_KEY, "MouseThreshold2").as_deref(),
            Some("8")
        );
        assert_eq!(
            pc.string(SID, MOUSE_KEY, "MouseSpeed").as_deref(),
            Some("2")
        );
        assert_eq!(pc.value_names(OTHER_SID, MOUSE_KEY), Read::Absent);
        assert_eq!(pc.writes(), ["mouse [4, 8, 2]", "mouse [4, 8, 2] persist"]);
    }

    #[test]
    fn setting_the_mouse_and_setting_it_back_leaves_the_pc_as_it_was() {
        // The strings Windows starts with are there, so putting the setting back leaves no empty key behind.
        let pc = FakeSystem::new();
        let before = pc.dump();
        pc.set_mouse_live([0, 0, 0], true).unwrap();
        assert_ne!(pc.dump(), before);
        pc.set_mouse_live([6, 10, 1], true).unwrap();
        assert_eq!(pc.dump(), before);
    }

    #[test]
    fn setting_the_mouse_for_good_writes_the_callers_profile_which_may_not_be_the_signed_in_ones() {
        // Kestrel runs as OTHER_SID (an administrator) for the person signed in as SID.
        let pc = FakeSystem::new();
        pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: Some(OTHER_SID.to_string()),
        });
        pc.set_mouse_live([0, 0, 0], true).unwrap();
        // The setting in force changes for the session, but the strings land in the administrator's profile and the
        // person at the screen keeps the old ones: why the engine writes those itself when the accounts differ.
        assert_eq!(pc.mouse_live(), Read::Value([0, 0, 0]));
        assert_eq!(
            pc.string(OTHER_SID, MOUSE_KEY, "MouseSpeed").as_deref(),
            Some("0")
        );
        assert_eq!(
            pc.string(SID, MOUSE_KEY, "MouseSpeed").as_deref(),
            Some("1")
        );

        // A process Windows couldn't name has no profile to write.
        let pc = FakeSystem::new();
        pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: None,
        });
        pc.set_mouse_live([0, 0, 0], true).unwrap();
        assert_eq!(pc.mouse_live(), Read::Value([0, 0, 0]));
        assert_eq!(
            pc.string(SID, MOUSE_KEY, "MouseSpeed").as_deref(),
            Some("1")
        );
    }

    #[test]
    fn a_copied_plan_starts_as_its_base_and_is_then_renamed() {
        let pc = FakeSystem::new();
        pc.set_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING, 100);
        pc.duplicate_plan(HIGH_PERFORMANCE, KESTREL).unwrap();
        assert_eq!(pc.plan_name(KESTREL).as_deref(), Some("High performance"));
        assert_eq!(
            pc.read_plan_value(KESTREL, SUBGROUP, SETTING),
            Read::Value(100)
        );

        pc.describe_plan(KESTREL, "Kestrel", "Created by Kestrel.")
            .unwrap();
        assert_eq!(pc.plan_name(KESTREL).as_deref(), Some("Kestrel"));
        assert_eq!(
            pc.plan_description(KESTREL).as_deref(),
            Some("Created by Kestrel.")
        );
        pc.write_plan_value(KESTREL, SUBGROUP, SETTING, 0).unwrap();
        assert_eq!(pc.plan_value(KESTREL, SUBGROUP, SETTING), Some(0));
        // The base plan is untouched, and so is the copy's place in the list.
        assert_eq!(
            pc.plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Some(100)
        );
        assert_eq!(
            pc.plans().last(),
            Some(&plan(KESTREL, "Kestrel")),
            "a new plan goes on the end"
        );

        // Windows won't copy onto a plan that is there, or from one that isn't.
        let again = pc.duplicate_plan(HIGH_PERFORMANCE, KESTREL).unwrap_err();
        assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
        let missing = pc.duplicate_plan(EXM, "other").unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        assert!(pc.describe_plan(EXM, "x", "y").is_err());
        assert!(pc.write_plan_value(EXM, SUBGROUP, SETTING, 1).is_err());

        // A plan told not to be copied makes nothing.
        let before = pc.plans();
        pc.fail_duplicate(BALANCED);
        assert!(pc.duplicate_plan(BALANCED, "other").is_err());
        assert_eq!(pc.plans(), before);
        assert!(!pc.plan_exists("other"));
        // Only that base is refused.
        pc.duplicate_plan(POWER_SAVER, "other").unwrap();
    }

    #[test]
    fn the_active_plan_cant_be_deleted_and_only_a_plan_that_exists_can_be_activated() {
        let pc = FakeSystem::new();
        pc.add_plan(EXM, "EXM Free Power Plan V8");
        assert!(pc.plan_exists(EXM));
        pc.activate_plan(EXM).unwrap();
        assert_eq!(pc.active_plan(), Read::Value(EXM.to_string()));

        // Windows won't delete the plan in use.
        assert!(pc.delete_plan(EXM).is_err());
        assert!(pc.plan_exists(EXM));
        assert_eq!(
            pc.activate_plan("no-such-plan").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            pc.delete_plan("no-such-plan").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );

        pc.activate_plan(BALANCED).unwrap();
        pc.delete_plan(EXM).unwrap();
        assert!(!pc.plan_exists(EXM));
        assert_eq!(pc.plan_name(EXM), None);

        // An activation told to fail leaves the active plan alone.
        pc.fail_activate(HIGH_PERFORMANCE);
        assert!(pc.activate_plan(HIGH_PERFORMANCE).is_err());
        assert_eq!(pc.active_plan(), Read::Value(BALANCED.to_string()));
        pc.activate_plan(POWER_SAVER).unwrap();
    }

    #[test]
    fn a_plan_setting_told_to_fail_fails_in_every_plan() {
        let pc = FakeSystem::new();
        pc.set_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING, 100);

        // A read told to fail is a failure and not "absent", whether or not the plan holds the value.
        pc.fail_read(SETTING);
        assert!(failed(&pc.read_plan_value(
            HIGH_PERFORMANCE,
            SUBGROUP,
            SETTING
        )));
        assert!(failed(&pc.read_plan_value(BALANCED, SUBGROUP, SETTING)));
        // The test's own look at the plan stays true, and other settings read as before.
        assert_eq!(
            pc.plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Some(100)
        );
        assert_eq!(
            pc.read_plan_value(HIGH_PERFORMANCE, SUBGROUP, "other"),
            Read::Absent
        );

        // A write told to fail is refused and changes nothing, in any plan.
        pc.fail_write(SETTING);
        for plan in [HIGH_PERFORMANCE, BALANCED] {
            let refused = pc.write_plan_value(plan, SUBGROUP, SETTING, 0).unwrap_err();
            assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{plan}");
        }
        assert_eq!(
            pc.plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Some(100)
        );
        assert_eq!(pc.plan_value(BALANCED, SUBGROUP, SETTING), None);
        assert!(pc.writes().is_empty());
        // Other settings are still written.
        pc.write_plan_value(BALANCED, SUBGROUP, "other", 1).unwrap();

        // Lifting the switches brings the plans back as they were.
        pc.clear_failures();
        assert_eq!(
            pc.read_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Read::Value(100)
        );
        pc.write_plan_value(BALANCED, SUBGROUP, SETTING, 0).unwrap();
        assert_eq!(pc.plan_value(BALANCED, SUBGROUP, SETTING), Some(0));
    }

    #[test]
    fn a_plan_setting_told_to_lie_reads_back_wrongly_once_after_its_next_write() {
        let pc = FakeSystem::new();
        pc.set_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING, 100);
        pc.lie_on_readback(SETTING);

        // A read before the write is true: that is how a caller finds the prior value.
        assert_eq!(
            pc.read_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Read::Value(100)
        );
        pc.write_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING, 0)
            .unwrap();
        // The test's own look is true, the first read through `System` is the lie, and the next is true again.
        assert_eq!(pc.plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING), Some(0));
        assert_eq!(
            pc.read_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Read::Value(!0)
        );
        assert_eq!(
            pc.read_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Read::Value(0)
        );

        // A lie is told once: the next write reads back as written.
        pc.write_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING, 5)
            .unwrap();
        assert_eq!(
            pc.read_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Read::Value(5)
        );

        // A lie lasts from the write it was set for until the value is first read back, and no longer: a second write
        // before any read has no lie set for it.
        pc.lie_on_readback(SETTING);
        pc.write_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING, 6)
            .unwrap();
        pc.write_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING, 7)
            .unwrap();
        assert_eq!(
            pc.read_plan_value(HIGH_PERFORMANCE, SUBGROUP, SETTING),
            Read::Value(7)
        );
    }

    #[test]
    fn describing_a_plan_told_to_refuse_changes_nothing() {
        let pc = FakeSystem::new();
        pc.add_plan(KESTREL, "High performance");
        pc.fail_write(KESTREL);

        let refused = pc
            .describe_plan(KESTREL, "Kestrel", "Created by Kestrel.")
            .unwrap_err();

        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(pc.plan_name(KESTREL).as_deref(), Some("High performance"));
        assert_eq!(pc.plan_description(KESTREL).as_deref(), Some(""));
        assert!(pc.writes().is_empty());
        // Other plans are described as usual.
        pc.describe_plan(BALANCED, "Mine", "Made by me.").unwrap();
        assert_eq!(pc.plan_name(BALANCED).as_deref(), Some("Mine"));
    }

    #[test]
    fn a_plan_told_not_to_be_deleted_stays() {
        let pc = FakeSystem::new();
        pc.add_plan(EXM, "EXM");
        pc.add_plan(KESTREL, "Kestrel");

        pc.fail_delete(KESTREL);
        let refused = pc.delete_plan(KESTREL).unwrap_err();
        assert!(refused.to_string().contains(KESTREL), "{refused}");
        assert!(pc.plan_exists(KESTREL));
        assert!(!pc
            .writes()
            .iter()
            .any(|line| line.starts_with("delete-plan")));

        // Only that plan is refused.
        pc.delete_plan(EXM).unwrap();
        assert!(!pc.plan_exists(EXM));

        pc.clear_failures();
        pc.delete_plan(KESTREL).unwrap();
        assert!(!pc.plan_exists(KESTREL));
    }

    #[test]
    fn a_range_is_known_only_for_the_settings_given_one() {
        let pc = FakeSystem::new();
        assert_eq!(pc.plan_value_range(SUBGROUP, SETTING), None);
        pc.set_plan_range(SUBGROUP, SETTING, 0, 50);
        assert_eq!(pc.plan_value_range(SUBGROUP, SETTING), Some((0, 50)));
        assert_eq!(pc.plan_value_range(SUBGROUP, "other"), None);
    }

    #[test]
    fn protection_follows_the_switches_unless_a_policy_blocks_them() {
        let pc = FakeSystem::new();
        pc.set_protection(Readiness::ProtectionOff);
        pc.turn_protection_on().unwrap();
        assert_eq!(pc.protection(), Readiness::Ready);
        pc.turn_protection_off().unwrap();
        assert_eq!(pc.protection(), Readiness::ProtectionOff);

        pc.set_protection(Readiness::BlockedByPolicy);
        assert!(pc.turn_protection_on().is_err());
        assert!(pc.turn_protection_off().is_err());
        assert_eq!(pc.protection(), Readiness::BlockedByPolicy);
    }

    #[test]
    fn the_log_lists_what_changed_in_order_and_nothing_else() {
        let pc = FakeSystem::new();
        // Setting the PC up, reading it and a refused write are not changes.
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 0);
        pc.add_plan(EXM, "EXM");
        let _ = pc.read_dword(SID, GAME_BAR, GAME_MODE);
        pc.fail_write("Blocked");
        assert!(pc.write_dword(SID, GAME_BAR, "Blocked", 1).is_err());
        assert!(pc.writes().is_empty());

        pc.write_dword(SID, GAME_BAR, GAME_MODE, 1).unwrap();
        pc.write_string(SID, DIRECTX, SETTINGS, "A=1;").unwrap();
        pc.delete_value(SID, DIRECTX, SETTINGS).unwrap();
        pc.delete_key_if_empty(SID, DIRECTX).unwrap();
        pc.delete_key_if_empty(SID, GAME_BAR).unwrap(); // not empty: nothing is deleted, so nothing is logged
        pc.set_mouse_live([0, 0, 0], false).unwrap();
        pc.duplicate_plan(BALANCED, KESTREL).unwrap();
        pc.describe_plan(KESTREL, "Kestrel", "Created by Kestrel.")
            .unwrap();
        pc.write_plan_value(KESTREL, SUBGROUP, SETTING, 100)
            .unwrap();
        pc.activate_plan(KESTREL).unwrap();
        pc.activate_plan(EXM).unwrap();
        pc.delete_plan(KESTREL).unwrap();
        pc.turn_protection_off().unwrap();
        pc.turn_protection_on().unwrap();

        assert_eq!(
            pc.writes(),
            [
                format!(r"dword {SID}\{GAME_BAR}\{GAME_MODE} = 1"),
                format!(r"string {SID}\{DIRECTX}\{SETTINGS} = A=1;"),
                format!(r"delete {SID}\{DIRECTX}\{SETTINGS}"),
                format!(r"delete-key {SID}\{DIRECTX}"),
                "mouse [0, 0, 0]".to_string(),
                format!("duplicate-plan {BALANCED} -> {KESTREL}"),
                format!("describe-plan {KESTREL} Kestrel"),
                format!("plan-value {KESTREL} {SUBGROUP}/{SETTING} = 100"),
                format!("activate-plan {KESTREL}"),
                format!("activate-plan {EXM}"),
                format!("delete-plan {KESTREL}"),
                "protection off".to_string(),
                "protection on".to_string(),
            ]
        );
    }

    #[test]
    fn it_is_shared_as_a_system_and_still_set_up_from_the_test() {
        let pc = Arc::new(FakeSystem::new());
        let shared: Arc<dyn System> = pc.clone();
        pc.set_dword(SID, GAME_BAR, GAME_MODE, 0);
        assert_eq!(shared.read_dword(SID, GAME_BAR, GAME_MODE), Read::Value(0));
        shared.write_dword(SID, GAME_BAR, GAME_MODE, 1).unwrap();
        assert_eq!(pc.dword(SID, GAME_BAR, GAME_MODE), Some(1));
        assert_eq!(pc.writes().len(), 1);
    }
}
