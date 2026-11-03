//! Power plans, through `powrprof.dll`. Every call's first argument is reserved and must be null. Plans are named by
//! GUID in lower-case text here, as in `System`. Only plugged-in values are ever written.
use std::io;
use std::ptr::{null, null_mut};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    LocalFree, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
    WIN32_ERROR,
};
use windows_sys::Win32::System::Power::{
    GetPwrCapabilities, PowerDeleteScheme, PowerDuplicateScheme, PowerEnumerate,
    PowerGetActiveScheme, PowerReadACValueIndex, PowerReadFriendlyName, PowerReadValueMax,
    PowerReadValueMin, PowerSetActiveScheme, PowerWriteACValueIndex, PowerWriteDescription,
    PowerWriteFriendlyName, ACCESS_SCHEME, SYSTEM_POWER_CAPABILITIES,
};

use crate::parse;
use crate::system::Read;

/// A GUID from its text. A journal's GUIDs have been checked by the time they get here, and the rest are constants, so
/// this failing means a bug.
fn guid(text: &str) -> io::Result<GUID> {
    parse::guid_value(text)
        .map(GUID::from_u128)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{text} is not a GUID")))
}

fn text_of(guid: &GUID) -> String {
    parse::guid_text(guid.data1, guid.data2, guid.data3, guid.data4)
}

fn windows_error(code: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(code as i32)
}

fn checked(code: WIN32_ERROR) -> io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(windows_error(code))
    }
}

pub(super) fn active() -> Read<String> {
    let mut found: *mut GUID = null_mut();
    // SAFETY: `found` is a valid place for the pointer to the GUID Windows allocates.
    let code = unsafe { PowerGetActiveScheme(null_mut(), &mut found) };
    if code != ERROR_SUCCESS {
        return Read::Failed(parse::failed_read(
            "the power plan in use",
            &windows_error(code),
        ));
    }
    if found.is_null() {
        return Read::Failed("Windows didn't say which power plan is in use".to_string());
    }
    // SAFETY: on success `found` points to a GUID Windows allocated, which is freed only below.
    let guid = unsafe { *found };
    // SAFETY: PowerGetActiveScheme says to free it with LocalFree, and it isn't used again.
    unsafe { LocalFree(found.cast()) };
    Read::Value(text_of(&guid))
}

/// Every plan on this PC, in Windows' order.
pub(super) fn plans() -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    let mut index = 0;
    loop {
        let mut plan = GUID::default();
        let mut size = size_of::<GUID>() as u32;
        // SAFETY: with ACCESS_SCHEME Windows ignores the two GUID arguments and writes one GUID to the buffer, which is
        // `plan`, of the size `size` says.
        let code = unsafe {
            PowerEnumerate(
                null_mut(),
                null(),
                null(),
                ACCESS_SCHEME,
                index,
                (&raw mut plan).cast(),
                &mut size,
            )
        };
        match code {
            ERROR_SUCCESS => found.push(text_of(&plan)),
            ERROR_NO_MORE_ITEMS => return Ok(found),
            code => return Err(windows_error(code)),
        }
        index += 1;
    }
}

/// A plan's name as Windows shows it. None when the plan isn't there, Windows won't say, or the name is blank.
pub(super) fn name(plan: &str) -> Option<String> {
    let plan = guid(plan).ok()?;
    let mut size = 0u32;
    // SAFETY: with both the subgroup and the setting null this asks about the plan itself, and with a null buffer it
    // only reports in `size` how much room the name needs. Microsoft documents that as a success; the same function's
    // "too small" is taken as well, since the size is there either way.
    let code =
        unsafe { PowerReadFriendlyName(null_mut(), &plan, null(), null(), null_mut(), &mut size) };
    if !matches!(code, ERROR_SUCCESS | ERROR_MORE_DATA) || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    // SAFETY: as above, and `buffer` is writable for the `size` bytes passed with it. `size` comes back as what was
    // written.
    let code = unsafe {
        PowerReadFriendlyName(
            null_mut(),
            &plan,
            null(),
            null(),
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    buffer.truncate(size as usize);
    parse::text_from_wide_bytes(&buffer).filter(|name| !name.trim().is_empty())
}

/// Copies `base` to a new plan under the GUID `new`. A non-null destination tells Windows which GUID to use, and it
/// answers ERROR_ALREADY_EXISTS if that GUID is taken; a null one would have it allocate a GUID of its own. That second
/// behaviour must not happen here, because a crash would then leave a plan nobody can name, so it is checked for.
pub(super) fn duplicate(base: &str, new: &str) -> io::Result<()> {
    let source = guid(base)?;
    let mut wanted = guid(new)?;
    let slot: *mut GUID = &mut wanted;
    let mut destination = slot;
    // SAFETY: `source` is a valid GUID and `destination` a valid place holding a pointer to a valid GUID. Both outlive
    // the call.
    checked(unsafe { PowerDuplicateScheme(null_mut(), &source, &mut destination) })?;
    if destination == slot {
        return Ok(());
    }
    // SAFETY: Windows put a pointer to a GUID of its own here, which is freed only below.
    let chosen = unsafe { *destination };
    // SAFETY: an allocated destination is freed with LocalFree, and it isn't used again.
    unsafe { LocalFree(destination.cast()) };
    Err(io::Error::other(format!(
        "Windows copied the plan under {} instead of {new}",
        text_of(&chosen)
    )))
}

pub(super) fn describe(plan: &str, name: &str, description: &str) -> io::Result<()> {
    let plan = guid(plan)?;
    let name = parse::wide_bytes(name);
    let description = parse::wide_bytes(description);
    // SAFETY: with both the subgroup and the setting null this names the plan itself. The buffer is valid for the
    // length passed, in bytes and with the terminator, as Microsoft asks.
    checked(unsafe {
        PowerWriteFriendlyName(
            null_mut(),
            &plan,
            null(),
            null(),
            name.as_ptr(),
            name.len() as u32,
        )
    })?;
    // SAFETY: as for the name.
    checked(unsafe {
        PowerWriteDescription(
            null_mut(),
            &plan,
            null(),
            null(),
            description.as_ptr(),
            description.len() as u32,
        )
    })
}

/// A setting's plugged-in value. Only ERROR_FILE_NOT_FOUND is `Absent`.
pub(super) fn read_value(plan: &str, subgroup: &str, setting: &str) -> Read<u32> {
    let (Ok(scheme), Ok(group), Ok(item)) = (guid(plan), guid(subgroup), guid(setting)) else {
        return Read::Failed(format!("{plan}, {subgroup} or {setting} is not a GUID"));
    };
    let mut value = 0u32;
    // SAFETY: the three GUIDs are valid and `value` is a valid place for the answer.
    let code = unsafe { PowerReadACValueIndex(null_mut(), &scheme, &group, &item, &mut value) };
    match code {
        ERROR_SUCCESS => Read::Value(value),
        ERROR_FILE_NOT_FOUND => Read::Absent,
        code => Read::Failed(parse::failed_read(
            &format!("power setting {setting}"),
            &windows_error(code),
        )),
    }
}

/// Settings that offer a list of choices rather than a range may not have one, and that is None too.
pub(super) fn value_range(subgroup: &str, setting: &str) -> Option<(u32, u32)> {
    let group = guid(subgroup).ok()?;
    let setting = guid(setting).ok()?;
    let (mut min, mut max) = (0u32, 0u32);
    // SAFETY: both GUIDs are valid and `min` is a valid place for the answer.
    let read_min = unsafe { PowerReadValueMin(null_mut(), &group, &setting, &mut min) };
    // SAFETY: as above, for `max`.
    let read_max = unsafe { PowerReadValueMax(null_mut(), &group, &setting, &mut max) };
    (read_min == ERROR_SUCCESS && read_max == ERROR_SUCCESS).then_some((min, max))
}

/// Windows only applies a change to the plan in use when it's made active again, so this does that for the plan in use
/// and nothing else: activating any other plan would be a change of its own.
pub(super) fn write_value(plan: &str, subgroup: &str, setting: &str, value: u32) -> io::Result<()> {
    let (scheme, group, item) = (guid(plan)?, guid(subgroup)?, guid(setting)?);
    // SAFETY: the three GUIDs are valid and outlive the call.
    checked(unsafe { PowerWriteACValueIndex(null_mut(), &scheme, &group, &item, value) })?;
    if matches!(active(), Read::Value(in_use) if in_use.eq_ignore_ascii_case(plan)) {
        activate(plan)?;
    }
    Ok(())
}

pub(super) fn activate(plan: &str) -> io::Result<()> {
    let plan = guid(plan)?;
    // SAFETY: the GUID is valid and outlives the call.
    checked(unsafe { PowerSetActiveScheme(null_mut(), &plan) })
}

/// Refuses every plan but Kestrel's, as a last line of defence below `crate::power`.
pub(super) fn delete(plan: &str) -> io::Result<()> {
    if !parse::may_delete_plan(plan) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Kestrel only ever deletes its own power plan, not {plan}."),
        ));
    }
    let plan = guid(plan)?;
    // SAFETY: the GUID is valid and outlives the call.
    checked(unsafe { PowerDeleteScheme(null_mut(), &plan) })
}

pub(super) fn modern_standby() -> Option<bool> {
    let mut capabilities = SYSTEM_POWER_CAPABILITIES::default();
    // SAFETY: `capabilities` is a valid place for the answer.
    unsafe { GetPwrCapabilities(&mut capabilities) }.then_some(capabilities.AoAc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::KESTREL_PLAN_GUID;

    const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
    const NO_SUCH_PLAN: &str = "00000000-0000-0000-0000-000000000001";
    const PROCESSOR: &str = "54533251-82be-4824-96c1-47b60b740d00";
    const PARKING: &str = "0cc5b647-c1df-4637-891a-dec35c318583";

    fn in_use() -> String {
        match active() {
            Read::Value(plan) => plan,
            other => panic!("Windows should name the plan in use: {other:?}"),
        }
    }

    #[test]
    fn a_guid_comes_back_as_the_text_it_was() {
        assert_eq!(
            text_of(&guid(KESTREL_PLAN_GUID).unwrap()),
            KESTREL_PLAN_GUID
        );
        assert!(guid("{not a guid}").is_err());
    }

    #[test]
    fn the_plan_in_use_is_on_the_list_and_has_a_name() {
        let active = in_use();
        assert_eq!(active, active.to_lowercase());
        assert!(plans().unwrap().contains(&active), "{active}");
        assert!(name(&active).is_some(), "{active}");
    }

    #[test]
    fn a_plan_that_is_not_there_has_no_name_and_is_not_on_the_list() {
        assert!(!plans().unwrap().iter().any(|plan| plan == NO_SUCH_PLAN));
        assert_eq!(name(NO_SUCH_PLAN), None);
    }

    #[test]
    fn a_setting_of_the_plan_in_use_can_be_read() {
        let read = read_value(&in_use(), PROCESSOR, PARKING);
        assert!(matches!(read, Read::Value(_) | Read::Absent), "{read:?}");
    }

    #[test]
    fn a_range_for_core_parking_is_one_of_percentages() {
        // Windows may not say, and then Kestrel writes its value as it is. If it does say, it has to leave some room: a
        // range of 0 to 0 would hold parking at 0, the opposite of what Kestrel's plan is for.
        if let Some((min, max)) = value_range(PROCESSOR, PARKING) {
            assert!(min < max && max <= 100, "{min} to {max}");
        }
    }

    #[test]
    fn no_plan_but_kestrels_can_be_deleted() {
        let refused = delete(BALANCED).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn windows_says_whether_this_is_modern_standby() {
        assert!(modern_standby().is_some());
    }
}
