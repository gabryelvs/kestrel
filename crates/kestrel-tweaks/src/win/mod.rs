//! `System` on real Windows, and the journal's folder. Everything here is only type-checked on a Mac: the first thing
//! to run it is the Windows job on GitHub (its tests, then the `kestrel-tweak-roundtrip` binary).
use std::io;

use kestrel_probe::account::{account, Account};

use crate::parse;
use crate::system::{Read, Readiness, System};
use crate::targets::MOUSE_KEY;

mod folder;
mod mouse;
mod power;
mod registry;

pub(crate) use folder::program_data;
pub use folder::AdminFolder;

const NO_PROTECTION_YET: &str = "This version of Kestrel can't change System Protection yet.";

/// This PC. Its reads change nothing, so the engine can call them at any moment.
#[derive(Debug, Default)]
pub struct WinSystem {
    /// Stands in for the account Windows names as signed in. See `with_signed_in`.
    signed_in: Option<String>,
}

impl WinSystem {
    pub fn new() -> Self {
        Self::default()
    }

    /// A system that takes `sid` for the signed-in account, whatever Windows says. Only the round trip uses this, on a
    /// build runner where no shell names an account. The app never does: the engine would then write per-user settings
    /// to the hive of someone who isn't at the screen.
    pub fn with_signed_in(sid: String) -> Self {
        Self {
            signed_in: Some(sid),
        }
    }

    /// The GUID of every plan on this PC. The round trip compares this list before and after; `plan_exists` is what the
    /// rules use.
    pub fn plans(&self) -> io::Result<Vec<String>> {
        power::plans()
    }

    /// The mouse in force with Windows' own error, where `mouse_live` has it in a sentence. The round trip tells by the
    /// error's number whether it is the session that refuses; the rules use `mouse_live`.
    pub fn mouse_in_force(&self) -> io::Result<[i32; 3]> {
        mouse::read()
    }
}

impl System for WinSystem {
    fn account(&self) -> Account {
        let mut who = account();
        if let Some(sid) = &self.signed_in {
            who.signed_in = Some(sid.clone());
        }
        who
    }

    fn read_dword(&self, sid: &str, key: &str, name: &str) -> Read<u32> {
        registry::read_dword(sid, key, name)
    }

    fn write_dword(&self, sid: &str, key: &str, name: &str, value: u32) -> io::Result<()> {
        registry::write_dword(sid, key, name, value)
    }

    fn read_string(&self, sid: &str, key: &str, name: &str) -> Read<String> {
        registry::read_string(sid, key, name)
    }

    fn write_string(&self, sid: &str, key: &str, name: &str, value: &str) -> io::Result<()> {
        registry::write_string(sid, key, name, value)
    }

    fn delete_value(&self, sid: &str, key: &str, name: &str) -> io::Result<()> {
        registry::delete_value(sid, key, name)
    }

    fn value_names(&self, sid: &str, key: &str) -> Read<Vec<String>> {
        registry::value_names(sid, key)
    }

    fn delete_key_if_empty(&self, sid: &str, key: &str) -> io::Result<()> {
        registry::delete_key_if_empty(sid, key)
    }

    fn read_policy_dword(&self, key: &str, name: &str) -> Read<u32> {
        registry::read_policy_dword(key, name)
    }

    fn read_policy_string(&self, key: &str, name: &str) -> Read<String> {
        registry::read_policy_string(key, name)
    }

    fn mouse_live(&self) -> Read<[i32; 3]> {
        mouse::live()
    }

    /// Microsoft says `SPIF_UPDATEINIFILE` writes the caller's profile and doesn't say when, and every pointer Apply is
    /// rolled back if the strings don't read "0" straight after this. So once Windows has taken the setting, the
    /// strings are also written here, under the account the call ran as, which makes that read-back true by
    /// construction. It costs nothing if Windows already wrote them, and the same text is what a late write from
    /// Windows would leave.
    ///
    /// That read-back then only reads Kestrel's own write, so it can't see an `SPI_SETMOUSE` that said yes and changed
    /// nothing. The mouse in force has to read what it was given first, or this is an error and the engine takes the
    /// change back, as research section 5's two reads ask.
    fn set_mouse_live(&self, values: [i32; 3], persist: bool) -> io::Result<()> {
        mouse::set_live(values, persist)?;
        if persist {
            parse::mouse_reads_as_given(values, mouse::read()?)?;
            if let Some(sid) = self.account().this_process {
                for (name, text) in parse::mouse_strings(values) {
                    registry::write_string(&sid, MOUSE_KEY, name, &text)?;
                }
            }
        }
        Ok(())
    }

    fn active_plan(&self) -> Read<String> {
        power::active()
    }

    /// A list that can't be read answers yes, because no closes a record for good. See `parse::plan_is_listed`.
    fn plan_exists(&self, guid: &str) -> bool {
        parse::plan_is_listed(&power::plans(), guid)
    }

    fn plan_name(&self, guid: &str) -> Option<String> {
        power::name(guid)
    }

    fn duplicate_plan(&self, base: &str, new: &str) -> io::Result<()> {
        power::duplicate(base, new)
    }

    fn describe_plan(&self, guid: &str, name: &str, description: &str) -> io::Result<()> {
        power::describe(guid, name, description)
    }

    fn read_plan_value(&self, guid: &str, subgroup: &str, setting: &str) -> Read<u32> {
        power::read_value(guid, subgroup, setting)
    }

    fn plan_value_range(&self, subgroup: &str, setting: &str) -> Option<(u32, u32)> {
        power::value_range(subgroup, setting)
    }

    fn write_plan_value(
        &self,
        guid: &str,
        subgroup: &str,
        setting: &str,
        value: u32,
    ) -> io::Result<()> {
        power::write_value(guid, subgroup, setting, value)
    }

    fn activate_plan(&self, guid: &str) -> io::Result<()> {
        power::activate(guid)
    }

    fn delete_plan(&self, guid: &str) -> io::Result<()> {
        power::delete(guid)
    }

    fn modern_standby(&self) -> Option<bool> {
        power::modern_standby()
    }

    fn protection(&self) -> Readiness {
        Readiness::Unknown
    }

    fn turn_protection_on(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            NO_PROTECTION_YET,
        ))
    }

    fn turn_protection_off(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            NO_PROTECTION_YET,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stand_in_account_is_the_signed_in_one_and_this_process_stays_itself() {
        let system = WinSystem::with_signed_in("S-1-5-21-1-1001".to_string());
        let who = system.account();
        assert_eq!(who.signed_in.as_deref(), Some("S-1-5-21-1-1001"));
        assert_eq!(who.this_process, account().this_process);
    }

    #[test]
    fn with_no_stand_in_windows_says_who_is_signed_in() {
        assert_eq!(WinSystem::new().account(), account());
    }

    #[test]
    fn system_protection_waits_for_its_own_task() {
        let system = WinSystem::new();
        assert_eq!(system.protection(), Readiness::Unknown);
        for result in [system.turn_protection_on(), system.turn_protection_off()] {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Unsupported);
        }
    }
}
