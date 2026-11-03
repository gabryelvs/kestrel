//! The registry. Per-user values are always under `HKEY_USERS\<sid>`, even for the account Kestrel runs as, so there is
//! one code path and `HKEY_CURRENT_USER` never comes into it. The machine-wide reads have no matching write. Reads open
//! their keys with `KEY_READ`, so a look at a setting can't change it.
use std::io;

use kestrel_probe::parse::user_key_path;
use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use winreg::enums::{HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ, KEY_SET_VALUE};
use winreg::{RegKey, HKEY};

use crate::parse;
use crate::system::Read;

/// A value as the registry holds it: its type's number and its bytes.
type RawValue = (u32, Vec<u8>);

/// Only this means "not there". Every other error is a failed read, which is never taken for absent, and a missing path
/// or a refused read in particular isn't (`io::ErrorKind::NotFound` would let ERROR_PATH_NOT_FOUND through).
fn is_missing(error: &io::Error) -> bool {
    error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32)
}

/// Reads a value. A key or value that isn't there is `Absent`. `shown` is the key without its hive, for a sentence.
fn raw_value(root: HKEY, path: &str, shown: &str, name: &str) -> Read<RawValue> {
    let key = match RegKey::predef(root).open_subkey_with_flags(path, KEY_READ) {
        Ok(key) => key,
        Err(error) if is_missing(&error) => return Read::Absent,
        Err(error) => return Read::Failed(parse::failed_read(shown, &error)),
    };
    match key.get_raw_value(name) {
        Ok(value) => Read::Value((value.vtype as u32, value.bytes.into_owned())),
        Err(error) if is_missing(&error) => Read::Absent,
        Err(error) => Read::Failed(parse::failed_read(name, &error)),
    }
}

fn user_value(sid: &str, key: &str, name: &str) -> Read<RawValue> {
    raw_value(HKEY_USERS, &user_key_path(sid, key), key, name)
}

/// A value under `HKEY_LOCAL_MACHINE` as it is held, for the one that isn't a DWORD or a plain string.
pub(super) fn machine_value(key: &str, name: &str) -> Read<RawValue> {
    raw_value(HKEY_LOCAL_MACHINE, key, key, name)
}

/// A value that is there, read as the type asked for.
fn decoded<T>(read: Read<RawValue>, as_type: impl FnOnce(u32, &[u8]) -> Read<T>) -> Read<T> {
    match read {
        Read::Absent => Read::Absent,
        Read::Failed(why) => Read::Failed(why),
        Read::Value((kind, bytes)) => as_type(kind, &bytes),
    }
}

pub(super) fn read_dword(sid: &str, key: &str, name: &str) -> Read<u32> {
    decoded(user_value(sid, key, name), |kind, bytes| {
        parse::dword_value(name, kind, bytes)
    })
}

pub(super) fn read_string(sid: &str, key: &str, name: &str) -> Read<String> {
    decoded(user_value(sid, key, name), |kind, bytes| {
        parse::string_value(name, kind, bytes)
    })
}

pub(super) fn read_policy_dword(key: &str, name: &str) -> Read<u32> {
    decoded(machine_value(key, name), |kind, bytes| {
        parse::dword_value(name, kind, bytes)
    })
}

pub(super) fn read_policy_string(key: &str, name: &str) -> Read<String> {
    decoded(machine_value(key, name), |kind, bytes| {
        parse::string_value(name, kind, bytes)
    })
}

pub(super) fn value_names(sid: &str, key: &str) -> Read<Vec<String>> {
    let path = user_key_path(sid, key);
    let opened = match RegKey::predef(HKEY_USERS).open_subkey_with_flags(path, KEY_READ) {
        Ok(opened) => opened,
        Err(error) if is_missing(&error) => return Read::Absent,
        Err(error) => return Read::Failed(parse::failed_read(key, &error)),
    };
    let mut names = Vec::new();
    for value in opened.enum_values() {
        match value {
            Ok((name, _)) => names.push(name),
            Err(error) => return Read::Failed(parse::failed_read(key, &error)),
        }
    }
    Read::Value(names)
}

/// Stands below the rules: what Kestrel must never write can't be written through here.
fn refuse_forbidden(key: &str, name: &str) -> io::Result<()> {
    if parse::may_write_value(key, name) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("Kestrel never writes {name} in {key}."),
    ))
}

/// Opens a key to set values in it, creating it when it isn't there.
fn for_writing(sid: &str, key: &str, name: &str) -> io::Result<RegKey> {
    refuse_forbidden(key, name)?;
    let (opened, _) = RegKey::predef(HKEY_USERS)
        .create_subkey_with_flags(user_key_path(sid, key), KEY_SET_VALUE)?;
    Ok(opened)
}

pub(super) fn write_dword(sid: &str, key: &str, name: &str, value: u32) -> io::Result<()> {
    for_writing(sid, key, name)?.set_value(name, &value)
}

pub(super) fn write_string(sid: &str, key: &str, name: &str, value: &str) -> io::Result<()> {
    for_writing(sid, key, name)?.set_value(name, &value)
}

/// A value or key that isn't there comes back as `NotFound`, which is what ERROR_FILE_NOT_FOUND is as an `io::Error`.
pub(super) fn delete_value(sid: &str, key: &str, name: &str) -> io::Result<()> {
    refuse_forbidden(key, name)?;
    RegKey::predef(HKEY_USERS)
        .open_subkey_with_flags(user_key_path(sid, key), KEY_SET_VALUE)?
        .delete_value(name)
}

pub(super) fn delete_key_if_empty(sid: &str, key: &str) -> io::Result<()> {
    refuse_forbidden(key, "")?;
    let users = RegKey::predef(HKEY_USERS);
    let path = user_key_path(sid, key);
    let contents = match users.open_subkey_with_flags(&path, KEY_READ) {
        Ok(opened) => opened.query_info()?,
        Err(error) if is_missing(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    if contents.sub_keys > 0 || contents.values > 0 {
        return Ok(());
    }
    match users.delete_subkey(&path) {
        Err(error) if is_missing(&error) => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_probe::account::account;
    use winreg::enums::{REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_SZ};
    use winreg::RegValue;

    /// A key of its own under this test process's hive, gone when the test ends, however it ends.
    struct Scratch {
        sid: String,
        key: String,
    }

    impl Scratch {
        fn new(test: &str) -> Self {
            Scratch {
                sid: account()
                    .this_process
                    .expect("a process can read its own token"),
                key: format!(r"Software\KestrelTest-{test}-{}", std::process::id()),
            }
        }

        /// The key itself, made to hold raw values the functions under test can't write.
        fn opened(&self) -> RegKey {
            let path = user_key_path(&self.sid, &self.key);
            RegKey::predef(HKEY_USERS)
                .create_subkey_with_flags(path, KEY_SET_VALUE)
                .expect("the scratch key should open")
                .0
        }

        fn exists(&self) -> bool {
            let path = user_key_path(&self.sid, &self.key);
            RegKey::predef(HKEY_USERS)
                .open_subkey_with_flags(path, KEY_READ)
                .is_ok()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let path = user_key_path(&self.sid, &self.key);
            let _ = RegKey::predef(HKEY_USERS).delete_subkey_all(path);
        }
    }

    #[test]
    fn a_key_that_is_not_there_is_absent_and_reading_does_not_make_it() {
        let s = Scratch::new("no-key");
        assert_eq!(read_dword(&s.sid, &s.key, "V"), Read::Absent);
        assert_eq!(read_string(&s.sid, &s.key, "V"), Read::Absent);
        assert_eq!(value_names(&s.sid, &s.key), Read::Absent);
        assert!(!s.exists());
    }

    #[test]
    fn a_key_with_no_such_value_is_absent_for_that_value() {
        let s = Scratch::new("no-value");
        s.opened();
        assert_eq!(read_dword(&s.sid, &s.key, "V"), Read::Absent);
        assert_eq!(read_string(&s.sid, &s.key, "V"), Read::Absent);
        assert_eq!(value_names(&s.sid, &s.key), Read::Value(Vec::new()));
    }

    #[test]
    fn a_dword_and_a_string_read_back_as_written() {
        let s = Scratch::new("round");
        write_dword(&s.sid, &s.key, "D", 7).unwrap();
        write_string(&s.sid, &s.key, "S", "VRROptimizeEnable=1;é").unwrap();
        assert_eq!(read_dword(&s.sid, &s.key, "D"), Read::Value(7));
        assert_eq!(
            read_string(&s.sid, &s.key, "S"),
            Read::Value("VRROptimizeEnable=1;é".to_string())
        );
        let Read::Value(mut names) = value_names(&s.sid, &s.key) else {
            panic!("the key should list its values");
        };
        names.sort();
        assert_eq!(names, ["D", "S"]);
    }

    #[test]
    fn a_string_is_stored_as_a_terminated_string_of_the_string_type() {
        let s = Scratch::new("raw-string");
        write_string(&s.sid, &s.key, "S", "Ke").unwrap();
        let path = user_key_path(&s.sid, &s.key);
        let raw = RegKey::predef(HKEY_USERS)
            .open_subkey_with_flags(path, KEY_READ)
            .unwrap()
            .get_raw_value("S")
            .unwrap();
        assert_eq!(raw.vtype, REG_SZ);
        assert_eq!(&raw.bytes[..], &[0x4B, 0, 0x65, 0, 0, 0]);
    }

    #[test]
    fn a_value_of_the_wrong_type_is_a_failed_read() {
        let s = Scratch::new("wrong-type");
        write_string(&s.sid, &s.key, "S", "1").unwrap();
        write_dword(&s.sid, &s.key, "D", 1).unwrap();
        let reads_dword = read_dword(&s.sid, &s.key, "S");
        assert_eq!(
            reads_dword,
            Read::Failed("S is not a DWORD value".to_string())
        );
        let reads_string = read_string(&s.sid, &s.key, "D");
        assert_eq!(
            reads_string,
            Read::Failed("D is not a string value".to_string())
        );
    }

    #[test]
    fn raw_values_a_dword_or_a_string_can_not_be_are_failed_reads() {
        let s = Scratch::new("odd-values");
        let key = s.opened();
        let raw = |name: &str, vtype, bytes: &[u8]| {
            let value = RegValue {
                vtype,
                bytes: bytes.to_vec().into(),
            };
            key.set_raw_value(name, &value).unwrap();
        };
        raw("short", REG_DWORD, &[1, 0]);
        raw("binary", REG_BINARY, &[1, 0, 0, 0]);
        raw("expand", REG_EXPAND_SZ, &[0x4B, 0, 0, 0]);
        let failed = |read: Read<String>| matches!(read, Read::Failed(_));
        assert!(matches!(
            read_dword(&s.sid, &s.key, "short"),
            Read::Failed(_)
        ));
        assert!(matches!(
            read_dword(&s.sid, &s.key, "binary"),
            Read::Failed(_)
        ));
        assert!(failed(read_string(&s.sid, &s.key, "binary")));
        assert!(failed(read_string(&s.sid, &s.key, "expand")));
    }

    #[test]
    fn deleting_what_is_not_there_is_not_found() {
        let s = Scratch::new("delete");
        let no_key = delete_value(&s.sid, &s.key, "V").unwrap_err();
        assert_eq!(no_key.kind(), io::ErrorKind::NotFound);
        s.opened();
        let no_value = delete_value(&s.sid, &s.key, "V").unwrap_err();
        assert_eq!(no_value.kind(), io::ErrorKind::NotFound);
        write_dword(&s.sid, &s.key, "V", 1).unwrap();
        delete_value(&s.sid, &s.key, "V").unwrap();
        assert_eq!(read_dword(&s.sid, &s.key, "V"), Read::Absent);
    }

    #[test]
    fn only_an_empty_key_is_deleted() {
        let s = Scratch::new("empty");
        // Missing: nothing to do, and no error.
        delete_key_if_empty(&s.sid, &s.key).unwrap();
        // A value keeps it.
        write_dword(&s.sid, &s.key, "V", 1).unwrap();
        delete_key_if_empty(&s.sid, &s.key).unwrap();
        assert!(s.exists());
        // So does a subkey, with no value left.
        delete_value(&s.sid, &s.key, "V").unwrap();
        let child = format!(r"{}\child", s.key);
        write_dword(&s.sid, &child, "V", 1).unwrap();
        delete_key_if_empty(&s.sid, &s.key).unwrap();
        assert!(s.exists());
        // With nothing in it, it goes.
        delete_value(&s.sid, &child, "V").unwrap();
        delete_key_if_empty(&s.sid, &child).unwrap();
        delete_key_if_empty(&s.sid, &s.key).unwrap();
        assert!(!s.exists());
    }

    #[test]
    fn what_kestrel_never_writes_is_refused_before_windows_is_asked() {
        let s = Scratch::new("refused");
        let policies = format!(r"{}\Policies", s.key);
        for refused in [
            write_dword(&s.sid, &policies, "V", 1),
            write_string(&s.sid, &policies, "V", "1"),
            delete_value(&s.sid, &policies, "V"),
            delete_key_if_empty(&s.sid, &policies),
            write_dword(&s.sid, &s.key, "AppCaptureEnabled", 0),
        ] {
            assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        }
        assert!(!s.exists());
    }

    #[test]
    fn a_policy_value_that_is_not_there_is_absent() {
        let key = r"SOFTWARE\Policies\Kestrel\NoSuchKey";
        assert_eq!(read_policy_dword(key, "V"), Read::Absent);
        assert_eq!(read_policy_string(key, "V"), Read::Absent);
    }

    #[test]
    fn a_machine_value_of_the_wrong_type_is_a_failed_read() {
        // Every Windows has this string; its type is what's being tested, not the policy.
        let key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        assert!(matches!(
            read_policy_string(key, "ProductName"),
            Read::Value(_)
        ));
        assert!(matches!(
            read_policy_dword(key, "ProductName"),
            Read::Failed(_)
        ));
    }
}
