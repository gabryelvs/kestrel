//! The one door to Windows. Everything the tweaks read or change goes through `System`, so the rules above it are
//! tested on any OS against `fake::FakeSystem`, and the Windows implementation is the only code that has to be checked
//! on a Windows PC.
use std::io;

use kestrel_probe::account::Account;
use serde::{Deserialize, Serialize};

/// What a read found. Only a key or value that isn't there is `Absent`. A value of the wrong type, a refused read and
/// every other error are `Failed`: treating those as absent would make an undo delete a real value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read<T> {
    Absent,
    Value(T),
    /// Why the read failed, in words fit for the report.
    Failed(String),
}

/// Whether Kestrel can ask Windows for a restore point, and if not, why. It decides which choices the dialog offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Readiness {
    /// System Protection is on for the Windows drive, so a restore point can be made.
    Ready,
    /// It is off: Kestrel can offer to turn it on.
    ProtectionOff,
    /// An organisation policy disables it, so Kestrel can only go on without a restore point.
    BlockedByPolicy,
    /// Windows wouldn't say.
    Unknown,
}

/// What the tweaks need from Windows. Reads change nothing. Per-user settings live under `HKEY_USERS\<sid>` and never
/// under the process's own `HKEY_CURRENT_USER`, because Kestrel may be running as another account than the one at the
/// screen (see `Account`). There is deliberately no machine-wide registry write, and no way to touch a `Policies` key.
pub trait System: Send + Sync {
    /// The account at the screen and the account Kestrel runs as.
    fn account(&self) -> Account;

    // Per-user registry values, under `HKEY_USERS\<sid>\<key>`.

    /// A DWORD value. A value of any other type is `Failed`.
    fn read_dword(&self, sid: &str, key: &str, name: &str) -> Read<u32>;
    /// Writes a DWORD value, creating the key when it isn't there.
    fn write_dword(&self, sid: &str, key: &str, name: &str, value: u32) -> io::Result<()>;
    /// A string value. A value of any other type is `Failed`.
    fn read_string(&self, sid: &str, key: &str, name: &str) -> Read<String>;
    /// Writes a string value, creating the key when it isn't there.
    fn write_string(&self, sid: &str, key: &str, name: &str, value: &str) -> io::Result<()>;
    /// Deletes one value. A value that isn't there is an error (`NotFound`), as in Windows.
    fn delete_value(&self, sid: &str, key: &str, name: &str) -> io::Result<()>;
    /// The names of the values in a key, as they are spelled there. `Absent` when the key doesn't exist.
    fn value_names(&self, sid: &str, key: &str) -> Read<Vec<String>>;
    /// Deletes the key only when it holds nothing. A key that is missing or not empty is left as it is, and that is
    /// not an error.
    fn delete_key_if_empty(&self, sid: &str, key: &str) -> io::Result<()>;

    // Machine-wide policy reads. There is deliberately no machine-wide registry write.

    /// A DWORD under `HKEY_LOCAL_MACHINE`. A value of any other type is `Failed`.
    fn read_policy_dword(&self, key: &str, name: &str) -> Read<u32>;
    /// A string under `HKEY_LOCAL_MACHINE`. A value of any other type is `Failed`.
    fn read_policy_string(&self, key: &str, name: &str) -> Read<String>;

    // Mouse: `SPI_GETMOUSE` / `SPI_SETMOUSE`, as [threshold 1, threshold 2, acceleration].

    /// The setting in force now.
    fn mouse_live(&self) -> Read<[i32; 3]>;
    /// Sets the setting in force now. `persist` adds `SPIF_UPDATEINIFILE`: Windows then also saves the three strings
    /// under `Control Panel\Mouse`, but in the profile of the account Kestrel runs as, which is not the signed-in one
    /// when `Account::differs` is true.
    fn set_mouse_live(&self, values: [i32; 3], persist: bool) -> io::Result<()>;

    // Power: plans are named by GUID, as lower-case strings.

    /// The plan in use.
    fn active_plan(&self) -> Read<String>;
    fn plan_exists(&self, guid: &str) -> bool;
    /// The name a plan shows in Windows, if the plan exists.
    fn plan_name(&self, guid: &str) -> Option<String>;
    /// Copies the plan `base` to a new plan with the GUID `new`, settings and name included.
    fn duplicate_plan(&self, base: &str, new: &str) -> io::Result<()>;
    /// Sets a plan's name and description.
    fn describe_plan(&self, guid: &str, name: &str, description: &str) -> io::Result<()>;
    /// One setting of a plan, while plugged in.
    fn read_plan_value(&self, guid: &str, subgroup: &str, setting: &str) -> Read<u32>;
    /// The smallest and largest value Windows accepts for a setting; None when it won't say.
    fn plan_value_range(&self, subgroup: &str, setting: &str) -> Option<(u32, u32)>;
    /// Sets one setting of a plan for when the PC is plugged in. Battery values are never written.
    fn write_plan_value(
        &self,
        guid: &str,
        subgroup: &str,
        setting: &str,
        value: u32,
    ) -> io::Result<()>;
    /// Makes a plan the one in use.
    fn activate_plan(&self, guid: &str) -> io::Result<()>;
    /// Deletes a plan. Windows won't delete the plan in use.
    fn delete_plan(&self, guid: &str) -> io::Result<()>;
    /// Whether this PC uses Modern Standby; None when Windows wouldn't say.
    fn modern_standby(&self) -> Option<bool>;

    // System Protection on the system drive.

    fn protection(&self) -> Readiness;
    fn turn_protection_on(&self) -> io::Result<()>;
    fn turn_protection_off(&self) -> io::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_is_spelled_in_kebab_case_the_way_the_page_reads_it() {
        for (readiness, text) in [
            (Readiness::Ready, "ready"),
            (Readiness::ProtectionOff, "protection-off"),
            (Readiness::BlockedByPolicy, "blocked-by-policy"),
            (Readiness::Unknown, "unknown"),
        ] {
            let json = serde_json::to_string(&readiness).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<Readiness>(&json).unwrap(), readiness);
        }
    }
}
