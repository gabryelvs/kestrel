//! The pure half of the Windows code: how GUIDs, text and registry values are spelled on their way to and from Windows,
//! and the refusals that stand below the rules. No Windows call is in here, so all of it is tested on any OS, as
//! `kestrel_probe::parse` is for the probe.
use std::io;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};

use crate::journal::is_guid;
use crate::power::KESTREL_PLAN_GUID;
use crate::system::{Read, Readiness};
use crate::targets::{
    ALLOW_AUTO_GAME_MODE, ALLOW_GAME_DVR, APP_CAPTURE, MOUSE_SPEED, MOUSE_THRESHOLD_1,
    MOUSE_THRESHOLD_2,
};

/// The registry's number for a string value.
pub const REG_SZ: u32 = 1;
/// The registry's number for a DWORD value.
pub const REG_DWORD: u32 = 4;
/// The registry's number for a list of strings.
pub const REG_MULTI_SZ: u32 = 7;

/// Values Kestrel only ever reads. `AppCaptureEnabled` would also stop manual recording and screenshots, and
/// `GameDVR_Enabled`, which other tools write beside it, does something the research couldn't pin down. `AllowGameDVR`
/// and `AllowAutoGameMode` are an organisation's policy, which Kestrel doesn't fight.
/// `SystemRestorePointCreationFrequency` would change how often Windows makes restore points for everyone.
const NEVER_WRITTEN: [&str; 5] = [
    APP_CAPTURE,
    "GameDVR_Enabled",
    ALLOW_GAME_DVR,
    ALLOW_AUTO_GAME_MODE,
    "SystemRestorePointCreationFrequency",
];

/// A GUID in its text form as the number `GUID::from_u128` builds it from; None for anything else, braces included.
pub fn guid_value(text: &str) -> Option<u128> {
    if !is_guid(text) {
        return None;
    }
    u128::from_str_radix(&text.replace('-', ""), 16).ok()
}

/// A GUID's parts as text, in lower case, as `System` names plans.
pub fn guid_text(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> String {
    let [a, b, c, d, e, f, g, h] = data4;
    format!("{data1:08x}-{data2:04x}-{data3:04x}-{a:02x}{b:02x}-{c:02x}{d:02x}{e:02x}{f:02x}{g:02x}{h:02x}")
}

/// Text as Windows' wide strings have it, with the terminator.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Text as the bytes of a wide string, terminator included: what the power functions that take a byte buffer want.
pub fn wide_bytes(text: &str) -> Vec<u8> {
    wide(text).into_iter().flat_map(u16::to_le_bytes).collect()
}

/// The text in the bytes of a wide string, without the terminator if there is one. None when the bytes aren't whole,
/// valid UTF-16: mending them would change what is there once it was written back.
pub fn text_from_wide_bytes(bytes: &[u8]) -> Option<String> {
    let (units, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    let units: Vec<u16> = units.iter().map(|unit| u16::from_le_bytes(*unit)).collect();
    let text = String::from_utf16(&units).ok()?;
    Some(text.trim_end_matches('\0').to_string())
}

/// A registry value of type `kind` read as a DWORD: only four bytes of the DWORD type are one.
pub fn dword_value(name: &str, kind: u32, bytes: &[u8]) -> Read<u32> {
    if kind != REG_DWORD {
        return Read::Failed(format!("{name} is not a DWORD value"));
    }
    match <[u8; 4]>::try_from(bytes) {
        Ok(bytes) => Read::Value(u32::from_le_bytes(bytes)),
        Err(_) => Read::Failed(format!(
            "{name} is a DWORD value of {} bytes instead of 4",
            bytes.len()
        )),
    }
}

/// A registry value of type `kind` read as a string: only valid text of the string type is one. Expandable and
/// multi-line strings are other types, and Kestrel writing a plain string back would change them.
pub fn string_value(name: &str, kind: u32, bytes: &[u8]) -> Read<String> {
    if kind != REG_SZ {
        return Read::Failed(format!("{name} is not a string value"));
    }
    match text_from_wide_bytes(bytes) {
        Some(text) => Read::Value(text),
        None => Read::Failed(format!("{name} does not hold valid text")),
    }
}

/// Why something couldn't be read, in a sentence for the report: what was being read and what Windows said, with its
/// error's number.
pub fn failed_read(what: &str, why: &io::Error) -> String {
    format!("Windows wouldn't read {what}: {why}")
}

/// Kestrel only ever deletes its own plan. `power` already keeps to that; this stands below it.
pub fn may_delete_plan(guid: &str) -> bool {
    guid.eq_ignore_ascii_case(KESTREL_PLAN_GUID)
}

/// Whether Kestrel may ever write this value: never in a `Policies` key, and never one it only reads. `targets` already
/// keeps to that; this stands below it.
pub fn may_write_value(key: &str, name: &str) -> bool {
    !key.to_ascii_lowercase().contains("policies")
        && !NEVER_WRITTEN
            .iter()
            .any(|never| never.eq_ignore_ascii_case(name))
}

/// Whether a plan is on this PC, from the list of plans or the failure to get it. A list that couldn't be read says the
/// plan is there: `System::plan_exists` has no way to say a read failed, and "not there" would close a record as
/// removed outside Kestrel, which loses the undo. The worst "there" can do is a later step failing with Windows' own
/// reason.
pub fn plan_is_listed<E>(listing: &Result<Vec<String>, E>, guid: &str) -> bool {
    match listing {
        Ok(plans) => plans.iter().any(|plan| plan.eq_ignore_ascii_case(guid)),
        Err(_) => true,
    }
}

/// Whether the mouse in force reads exactly what Windows was just given. Windows can say yes to `SPI_SETMOUSE` and
/// change nothing, and the error says so in words, because it reaches the person as the reason their change didn't
/// take.
pub fn mouse_reads_as_given(given: [i32; 3], reads: [i32; 3]) -> io::Result<()> {
    if reads == given {
        return Ok(());
    }
    let spelled = |values: [i32; 3]| values.map(|value| value.to_string()).join(", ");
    Err(io::Error::other(format!(
        "Windows said it set the mouse to {}, and it reads {}.",
        spelled(given),
        spelled(reads)
    )))
}

/// The three strings `SPIF_UPDATEINIFILE` saves for the mouse setting [threshold 1, threshold 2, acceleration].
pub fn mouse_strings(values: [i32; 3]) -> [(&'static str, String); 3] {
    let [threshold1, threshold2, acceleration] = values;
    [
        (MOUSE_SPEED, acceleration.to_string()),
        (MOUSE_THRESHOLD_1, threshold1.to_string()),
        (MOUSE_THRESHOLD_2, threshold2.to_string()),
    ]
}

/// Whether System Protection is on for the Windows drive, from what can be read about it. `disable_sr` and
/// `disable_config` are the policy values, and `volume` is the Windows drive's volume name, if Windows gave it.
/// `clients` is the value under `SPP\Clients` that lists the protected volumes, as its type and bytes: `Absent` when the
/// key isn't there, and `Value(None)` when the key is there and the value isn't.
///
/// Only a policy that is set blocks, whatever became of the other reads. A policy that couldn't be read isn't no
/// policy, and a list or a volume that couldn't be read says nothing, so those are `Unknown`.
///
/// A list that doesn't name the drive is `ProtectionOff`, and so is no list under a key that is there, which is how
/// Sophia Script and the published REG file for turning protection off read it. If that's wrong, the dialog offers to
/// turn on what is already on, and `Enable` on an enabled drive does no harm. A key that isn't there says nothing: a
/// Windows with no System Restore may have none.
pub fn readiness(
    disable_sr: &Read<u32>,
    disable_config: &Read<u32>,
    clients: &Read<Option<(u32, Vec<u8>)>>,
    volume: Option<&str>,
) -> Readiness {
    let policies = [disable_sr, disable_config];
    if policies
        .iter()
        .any(|read| matches!(read, Read::Value(value) if *value != 0))
    {
        return Readiness::BlockedByPolicy;
    }
    if policies.iter().any(|read| matches!(read, Read::Failed(_))) {
        return Readiness::Unknown;
    }
    let Some(guid) = volume.and_then(volume_guid) else {
        return Readiness::Unknown;
    };
    let guid = guid.to_ascii_lowercase();
    match clients {
        Read::Value(Some((kind, bytes))) => match protected_volumes(*kind, bytes) {
            Some(list) if list.to_ascii_lowercase().contains(&guid) => Readiness::Ready,
            Some(_) => Readiness::ProtectionOff,
            None => Readiness::Unknown,
        },
        Read::Value(None) => Readiness::ProtectionOff,
        Read::Absent | Read::Failed(_) => Readiness::Unknown,
    }
}

/// The text of the value that lists the protected volumes, or None if it isn't text. On a real PC it is a list of
/// strings, each like `\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416}\:(C%3A)`, which comes out here with terminators
/// between them. Only whether a volume's GUID is in it matters, so a plain string reads the same, however it's
/// separated.
fn protected_volumes(kind: u32, bytes: &[u8]) -> Option<String> {
    matches!(kind, REG_SZ | REG_MULTI_SZ)
        .then(|| text_from_wide_bytes(bytes))
        .flatten()
}

/// The GUID in a volume name like `\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416}\`, without its braces.
pub fn volume_guid(volume_name: &str) -> Option<&str> {
    let (_, rest) = volume_name.split_once('{')?;
    let (guid, _) = rest.split_once('}')?;
    is_guid(guid).then_some(guid)
}

/// The drive the Windows folder is on, as `C:\`, which is how `SystemRestore` and `GetVolumeNameForVolumeMountPoint`
/// want a drive. Microsoft says the folder has no trailing backslash and may be just `C:`.
pub fn system_drive(windows_folder: &str) -> Option<String> {
    let mut chars = windows_folder.chars();
    let (letter, colon) = (chars.next()?, chars.next()?);
    (letter.is_ascii_alphabetic() && colon == ':').then(|| format!("{letter}:\\"))
}

/// A restore point's description as `RESTOREPOINTINFOW` holds it: wide text and its terminator in 256 units. A longer
/// one is cut, and never through the middle of a character.
pub fn restore_description(text: &str) -> [u16; 256] {
    let mut units = [0u16; 256];
    for (slot, unit) in units[..255].iter_mut().zip(text.encode_utf16()) {
        *slot = unit;
    }
    if (0xD800..0xDC00).contains(&units[254]) {
        units[254] = 0;
    }
    units
}

/// A time as WMI spells it, like `20261102090000.000000-000` (date and time, microseconds, minutes from UTC), as the
/// journal spells times: RFC 3339 in this PC's own time, which is what the report shows the first 16 characters of.
/// Text that isn't such a time comes back as it is.
pub fn local_time(wmi_time: &str) -> String {
    match instant_of_wmi_time(wmi_time) {
        Some(instant) => instant.with_timezone(&chrono::Local).to_rfc3339(),
        None => wmi_time.to_string(),
    }
}

fn instant_of_wmi_time(text: &str) -> Option<DateTime<Utc>> {
    let bytes = text.as_bytes();
    if bytes.len() != 25 || bytes[14] != b'.' {
        return None;
    }
    let sign = match bytes[21] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let number = |digits: &[u8]| {
        digits.iter().all(u8::is_ascii_digit).then(|| {
            digits
                .iter()
                .fold(0u32, |number, digit| number * 10 + u32::from(digit - b'0'))
        })
    };
    let date = NaiveDate::from_ymd_opt(
        i32::try_from(number(&bytes[0..4])?).ok()?,
        number(&bytes[4..6])?,
        number(&bytes[6..8])?,
    )?;
    let as_written = date.and_hms_micro_opt(
        number(&bytes[8..10])?,
        number(&bytes[10..12])?,
        number(&bytes[12..14])?,
        number(&bytes[15..21])?,
    )?;
    // The offset is how far the written time is ahead of UTC.
    let ahead = TimeDelta::try_minutes(sign * i64::from(number(&bytes[22..25])?))?;
    Some(as_written.checked_sub_signed(ahead)?.and_utc())
}

/// What `SystemRestore.Enable` or `Disable` answered: whether to go on to the read of System Protection that settles
/// it, or fail now. Microsoft says zero is success and anything else is an error code, whose number the person gets.
/// 1717, the interface is unknown, goes on too: PowerShell's own cmdlets let it past for both methods, because the drive
/// can change all the same, and the read is the truth either way.
pub fn method_answer(method: &str, answer: u32) -> Result<(), String> {
    const INTERFACE_UNKNOWN: u32 = 1717;
    match answer {
        0 | INTERFACE_UNKNOWN => Ok(()),
        other => Err(format!(
            "Windows refused: SystemRestore.{method} answered {}.",
            hex(other)
        )),
    }
}

/// An error code in the form Microsoft's lists use.
pub fn hex(code: u32) -> String {
    format!("0x{code:08X}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::KESTREL_PLAN_GUID;
    use crate::system::Read;
    use std::io;

    const REG_EXPAND_SZ: u32 = 2;
    const REG_BINARY: u32 = 3;
    const REG_DWORD_BIG_ENDIAN: u32 = 5;
    const REG_QWORD: u32 = 11;
    const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";

    #[test]
    fn a_guid_is_read_as_the_number_windows_builds_it_from() {
        assert_eq!(
            guid_value(KESTREL_PLAN_GUID),
            Some(0x4b455354_524c_4e50_a1b2_c3d4e5f60705)
        );
        assert_eq!(
            guid_value("4B455354-524C-4E50-A1B2-C3D4E5F60705"),
            Some(0x4b455354_524c_4e50_a1b2_c3d4e5f60705)
        );
    }

    #[test]
    fn only_the_text_form_of_a_guid_is_read() {
        for bad in [
            "",
            "4b455354",
            "{4b455354-524c-4e50-a1b2-c3d4e5f60705}",
            "4b455354-524c-4e50-a1b2-c3d4e5f6070",
            "4b455354-524c-4e50-a1b2-c3d4e5f607055",
            "4b455354_524c_4e50_a1b2_c3d4e5f60705",
            "4b45535g-524c-4e50-a1b2-c3d4e5f60705",
            "+b455354-524c-4e50-a1b2-c3d4e5f60705",
        ] {
            assert_eq!(guid_value(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_guid_is_spelled_in_lower_case_with_every_digit() {
        assert_eq!(
            guid_text(
                0x4b455354,
                0x524c,
                0x4e50,
                [0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x05]
            ),
            KESTREL_PLAN_GUID
        );
        assert_eq!(
            guid_text(1, 2, 3, [0, 4, 0, 0, 0, 0, 0, 5]),
            "00000001-0002-0003-0004-000000000005"
        );
    }

    #[test]
    fn wide_text_ends_with_its_terminator() {
        assert_eq!(wide("Ke"), [0x4B, 0x65, 0]);
        assert_eq!(wide(""), [0]);
        assert_eq!(wide("é"), [0xE9, 0]);
        // Outside the basic plane a character takes two units.
        assert_eq!(wide("😀"), [0xD83D, 0xDE00, 0]);
    }

    #[test]
    fn wide_bytes_are_little_endian_and_count_the_terminator() {
        assert_eq!(wide_bytes("Ke"), [0x4B, 0, 0x65, 0, 0, 0]);
        assert_eq!(wide_bytes(""), [0, 0]);
    }

    #[test]
    fn wide_bytes_come_back_as_the_text_they_were() {
        for text in ["", "Kestrel", "Alto desempenho é", "😀 plan"] {
            assert_eq!(
                text_from_wide_bytes(&wide_bytes(text)).as_deref(),
                Some(text)
            );
        }
    }

    #[test]
    fn terminators_are_not_part_of_the_text() {
        // No terminator at all, as some writers leave it, and more than one.
        assert_eq!(text_from_wide_bytes(&[0x4B, 0]).as_deref(), Some("K"));
        assert_eq!(
            text_from_wide_bytes(&[0x4B, 0, 0, 0, 0, 0]).as_deref(),
            Some("K")
        );
        assert_eq!(text_from_wide_bytes(&[]).as_deref(), Some(""));
    }

    #[test]
    fn bytes_that_are_not_wide_text_are_no_text() {
        // An odd number of bytes, and half of a surrogate pair.
        assert_eq!(text_from_wide_bytes(&[0x4B, 0, 0x65]), None);
        assert_eq!(text_from_wide_bytes(&[0x3D, 0xD8, 0, 0]), None);
    }

    #[test]
    fn a_dword_is_four_bytes_of_the_dword_type() {
        assert_eq!(
            dword_value("AutoGameModeEnabled", REG_DWORD, &1u32.to_le_bytes()),
            Read::Value(1)
        );
        assert_eq!(
            dword_value(
                "AutoGameModeEnabled",
                REG_DWORD,
                &0x0102_0304u32.to_le_bytes()
            ),
            Read::Value(0x0102_0304)
        );
    }

    #[test]
    fn any_other_type_is_a_failed_read_of_a_dword() {
        for kind in [
            REG_SZ,
            REG_EXPAND_SZ,
            REG_BINARY,
            REG_DWORD_BIG_ENDIAN,
            REG_MULTI_SZ,
            REG_QWORD,
        ] {
            assert_eq!(
                dword_value("AutoGameModeEnabled", kind, &1u32.to_le_bytes()),
                Read::Failed("AutoGameModeEnabled is not a DWORD value".to_string()),
                "type {kind}"
            );
        }
    }

    #[test]
    fn a_dword_of_the_wrong_length_is_a_failed_read() {
        for bytes in [&[][..], &[1], &[1, 0], &[1, 0, 0], &[1, 0, 0, 0, 0]] {
            assert_eq!(
                dword_value("AutoGameModeEnabled", REG_DWORD, bytes),
                Read::Failed(format!(
                    "AutoGameModeEnabled is a DWORD value of {} bytes instead of 4",
                    bytes.len()
                ))
            );
        }
    }

    #[test]
    fn a_string_is_wide_text_of_the_string_type() {
        assert_eq!(
            string_value(
                "DirectXUserGlobalSettings",
                REG_SZ,
                &wide_bytes("VRROptimizeEnable=1;")
            ),
            Read::Value("VRROptimizeEnable=1;".to_string())
        );
        // An empty string is a value, not an absent one.
        assert_eq!(
            string_value("DirectXUserGlobalSettings", REG_SZ, &wide_bytes("")),
            Read::Value(String::new())
        );
    }

    #[test]
    fn any_other_type_is_a_failed_read_of_a_string() {
        // Writing a string back over REG_EXPAND_SZ would change its type, so it counts as wrong too.
        for kind in [
            REG_EXPAND_SZ,
            REG_BINARY,
            REG_DWORD,
            REG_DWORD_BIG_ENDIAN,
            REG_MULTI_SZ,
            REG_QWORD,
        ] {
            assert_eq!(
                string_value("DirectXUserGlobalSettings", kind, &wide_bytes("a=1;")),
                Read::Failed("DirectXUserGlobalSettings is not a string value".to_string()),
                "type {kind}"
            );
        }
    }

    #[test]
    fn a_string_that_is_not_text_is_a_failed_read() {
        // Decoding it with replacement characters and writing it back would change what the person had.
        for bytes in [&[0x4B, 0, 0x65][..], &[0x3D, 0xD8, 0, 0]] {
            assert_eq!(
                string_value("DirectXUserGlobalSettings", REG_SZ, bytes),
                Read::Failed("DirectXUserGlobalSettings does not hold valid text".to_string())
            );
        }
    }

    #[test]
    fn a_failed_read_says_what_windows_said_in_a_sentence() {
        let error = io::Error::from_raw_os_error(5);
        let said = failed_read("AutoGameModeEnabled", &error);
        assert!(
            said.starts_with("Windows wouldn't read AutoGameModeEnabled: "),
            "{said}"
        );
        assert!(said.ends_with("(os error 5)"), "{said}");
        assert!(!said.contains("Os {"), "{said}");
    }

    #[test]
    fn only_kestrels_plan_may_be_deleted() {
        assert!(may_delete_plan(KESTREL_PLAN_GUID));
        assert!(may_delete_plan("4B455354-524C-4E50-A1B2-C3D4E5F60705"));
        assert!(!may_delete_plan(BALANCED));
        assert!(!may_delete_plan(""));
        assert!(!may_delete_plan("4b455354-524c-4e50-a1b2-c3d4e5f60706"));
    }

    #[test]
    fn the_settings_kestrel_changes_may_be_written() {
        for (key, name) in [
            (r"Software\Microsoft\GameBar", "AutoGameModeEnabled"),
            (
                r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
                "HistoricalCaptureEnabled",
            ),
            (
                r"Software\Microsoft\DirectX\UserGpuPreferences",
                "DirectXUserGlobalSettings",
            ),
            (
                r"Software\Microsoft\DirectX\UserGpuPreferences",
                r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe",
            ),
            (r"Control Panel\Mouse", "MouseSpeed"),
        ] {
            assert!(may_write_value(key, name), "{key}, {name}");
        }
    }

    #[test]
    fn a_policies_key_is_never_written() {
        for key in [
            r"Software\Policies\Microsoft\Windows\GameDVR",
            r"SOFTWARE\POLICIES\Microsoft\Power\PowerSettings",
            r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer",
        ] {
            assert!(!may_write_value(key, "AnyValue"), "{key}");
        }
    }

    #[test]
    fn the_values_kestrel_leaves_alone_are_never_written() {
        for name in [
            "AppCaptureEnabled",
            "GameDVR_Enabled",
            "AllowGameDVR",
            "AllowAutoGameMode",
            "SystemRestorePointCreationFrequency",
            "appcaptureenabled",
        ] {
            assert!(
                !may_write_value(r"Software\Microsoft\GameBar", name),
                "{name}"
            );
        }
    }

    #[test]
    fn a_plan_is_there_when_it_is_listed_in_any_case() {
        let listed: Result<Vec<String>, io::Error> = Ok(vec![BALANCED.to_string()]);
        assert!(plan_is_listed(&listed, BALANCED));
        assert!(plan_is_listed(&listed, &BALANCED.to_uppercase()));
        assert!(!plan_is_listed(&listed, KESTREL_PLAN_GUID));
    }

    #[test]
    fn a_list_that_could_not_be_read_never_says_a_plan_is_gone() {
        // "Gone" closes a record for good, and a failed read must not be able to do that.
        let failed: Result<Vec<String>, io::Error> = Err(io::Error::from_raw_os_error(5));
        assert!(plan_is_listed(&failed, KESTREL_PLAN_GUID));
    }

    #[test]
    fn the_mouse_in_force_has_to_read_exactly_what_windows_was_given() {
        assert!(mouse_reads_as_given([0, 0, 0], [0, 0, 0]).is_ok());
        assert!(mouse_reads_as_given([6, 10, 1], [6, 10, 1]).is_ok());
    }

    #[test]
    fn a_mouse_that_said_yes_and_did_not_change_is_an_error_that_says_so() {
        let said = mouse_reads_as_given([0, 0, 0], [6, 10, 1]).unwrap_err();
        assert_eq!(
            said.to_string(),
            "Windows said it set the mouse to 0, 0, 0, and it reads 6, 10, 1."
        );
        // Any one of the three is enough.
        for reads in [[1, 0, 0], [0, 1, 0], [0, 0, 1]] {
            assert!(mouse_reads_as_given([0, 0, 0], reads).is_err(), "{reads:?}");
        }
    }

    #[test]
    fn the_mouse_strings_say_the_same_as_the_three_numbers() {
        let strings = mouse_strings([6, 10, 1]);
        assert_eq!(
            strings,
            [
                ("MouseSpeed", "1".to_string()),
                ("MouseThreshold1", "6".to_string()),
                ("MouseThreshold2", "10".to_string()),
            ]
        );
    }

    // System Protection.

    const VOLUME: &str = r"\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416}\";
    const OTHER_VOLUME: &str = r"\\?\Volume{7e5b5ffd-0000-0000-0000-100000000000}\:(D%3A)";

    /// The value as a real PC holds it, from the REG file published for turning System Protection on: a list of strings
    /// with one entry, `\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416}\:(C%3A)`.
    const CLIENTS_FROM_A_PC: &[u8] = &[
        0x5c, 0x00, 0x5c, 0x00, 0x3f, 0x00, 0x5c, 0x00, 0x56, 0x00, 0x6f, 0x00, 0x6c, 0x00, 0x75,
        0x00, 0x6d, 0x00, 0x65, 0x00, 0x7b, 0x00, 0x61, 0x00, 0x30, 0x00, 0x31, 0x00, 0x36, 0x00,
        0x30, 0x00, 0x31, 0x00, 0x32, 0x00, 0x65, 0x00, 0x2d, 0x00, 0x39, 0x00, 0x62, 0x00, 0x63,
        0x00, 0x33, 0x00, 0x2d, 0x00, 0x34, 0x00, 0x64, 0x00, 0x37, 0x00, 0x31, 0x00, 0x2d, 0x00,
        0x38, 0x00, 0x39, 0x00, 0x66, 0x00, 0x31, 0x00, 0x2d, 0x00, 0x31, 0x00, 0x37, 0x00, 0x36,
        0x00, 0x35, 0x00, 0x63, 0x00, 0x66, 0x00, 0x62, 0x00, 0x31, 0x00, 0x61, 0x00, 0x34, 0x00,
        0x31, 0x00, 0x36, 0x00, 0x7d, 0x00, 0x5c, 0x00, 0x3a, 0x00, 0x28, 0x00, 0x43, 0x00, 0x25,
        0x00, 0x33, 0x00, 0x41, 0x00, 0x29, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    /// A list of strings as the registry holds it: each one terminated, and one terminator more.
    fn multi_string(entries: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for entry in entries {
            bytes.extend(wide_bytes(entry));
        }
        bytes.extend([0, 0]);
        bytes
    }

    /// What the key and value that list the protected volumes read as: `Absent` when the key isn't there, and
    /// `Value(None)` when it is and the value isn't.
    type Clients = Read<Option<(u32, Vec<u8>)>>;

    /// A value of this type holding these bytes.
    fn held(kind: u32, bytes: Vec<u8>) -> Clients {
        Read::Value(Some((kind, bytes)))
    }

    fn clients_with(entries: &[&str]) -> Clients {
        held(REG_MULTI_SZ, multi_string(entries))
    }

    /// What the System Protection reads say with no policy set and the Windows drive's volume known.
    fn with_no_policy(clients: &Clients) -> Readiness {
        readiness(&Read::Absent, &Read::Absent, clients, Some(VOLUME))
    }

    #[test]
    fn the_value_from_a_real_pc_says_its_drive_is_protected() {
        let clients = held(REG_MULTI_SZ, CLIENTS_FROM_A_PC.to_vec());
        assert_eq!(with_no_policy(&clients), Readiness::Ready);
        // That is a list of strings, which the policy reads refuse as "not a string value".
        assert!(matches!(
            string_value("value", REG_MULTI_SZ, CLIENTS_FROM_A_PC),
            Read::Failed(_)
        ));
    }

    #[test]
    fn a_drive_on_the_list_is_ready_whatever_else_is_on_it() {
        let entry = r"\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416}\:(C%3A)";
        for clients in [
            clients_with(&[entry]),
            clients_with(&[OTHER_VOLUME, entry]),
            clients_with(&[entry, OTHER_VOLUME]),
            // In one string, however it is separated.
            held(
                REG_SZ,
                wide_bytes(&format!("{OTHER_VOLUME};some text;{entry};")),
            ),
        ] {
            assert_eq!(with_no_policy(&clients), Readiness::Ready);
        }
    }

    #[test]
    fn letter_case_doesnt_hide_a_drive() {
        let upper = r"\\?\VOLUME{A016012E-9BC3-4D71-89F1-1765CFB1A416}\:(C%3A)";
        assert_eq!(
            with_no_policy(&clients_with(&[upper])),
            Readiness::Ready,
            "the list in upper case"
        );
        let entry = r"\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416}\:(C%3A)";
        let volume = r"\\?\Volume{A016012E-9BC3-4D71-89F1-1765CFB1A416}\";
        assert_eq!(
            readiness(
                &Read::Absent,
                &Read::Absent,
                &clients_with(&[entry]),
                Some(volume)
            ),
            Readiness::Ready,
            "the volume in upper case"
        );
    }

    #[test]
    fn a_list_without_the_drive_is_protection_off() {
        for clients in [
            clients_with(&[OTHER_VOLUME]),
            // What's left when nothing is listed, and what some guides have people write there.
            clients_with(&[]),
            held(REG_MULTI_SZ, Vec::new()),
            clients_with(&["1"]),
            held(REG_SZ, wide_bytes("")),
        ] {
            assert_eq!(with_no_policy(&clients), Readiness::ProtectionOff);
        }
    }

    #[test]
    fn a_nearly_matching_volume_is_another_volume() {
        let nearly = r"\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a417}\:(C%3A)";
        assert_eq!(
            with_no_policy(&clients_with(&[nearly])),
            Readiness::ProtectionOff
        );
    }

    #[test]
    fn a_value_that_isnt_there_under_a_key_that_is_means_nothing_is_protected() {
        // It's how Sophia Script reads it, and the published REG file for turning protection off deletes the value.
        assert_eq!(with_no_policy(&Read::Value(None)), Readiness::ProtectionOff);
    }

    #[test]
    fn a_key_that_isnt_there_says_nothing() {
        // A Windows with no System Restore may not have one.
        assert_eq!(with_no_policy(&Read::Absent), Readiness::Unknown);
    }

    #[test]
    fn a_read_that_failed_says_nothing() {
        let failed = Read::Failed("Windows wouldn't read it".to_string());
        assert_eq!(with_no_policy(&failed), Readiness::Unknown);
    }

    #[test]
    fn with_no_readable_list_nobody_can_say() {
        for clients in [
            // A value of another type, and bytes that aren't wide text.
            held(REG_DWORD, 1u32.to_le_bytes().to_vec()),
            held(REG_BINARY, multi_string(&[VOLUME])),
            held(REG_MULTI_SZ, vec![0x41, 0, 0x42]),
            held(REG_MULTI_SZ, vec![0x3D, 0xD8, 0, 0]),
        ] {
            assert_eq!(with_no_policy(&clients), Readiness::Unknown, "{clients:?}");
        }
    }

    #[test]
    fn with_no_volume_nobody_can_say() {
        // Whatever the list says, the empty one included: a failed read of the volume decides nothing.
        for clients in [clients_with(&[VOLUME]), Read::Value(None)] {
            for volume in [
                None,
                Some(""),
                Some("C:\\"),
                Some(r"\\?\Volume{not-a-guid}\"),
                Some(r"\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416\"),
            ] {
                assert_eq!(
                    readiness(&Read::Absent, &Read::Absent, &clients, volume),
                    Readiness::Unknown,
                    "{clients:?}, {volume:?}"
                );
            }
        }
    }

    #[test]
    fn only_a_policy_that_is_set_blocks() {
        let listed = clients_with(&[VOLUME]);
        let set = Read::Value(1);
        let off = Read::Value(0);
        for (disable_sr, disable_config) in [
            (&set, &Read::Absent),
            (&Read::Absent, &set),
            (&set, &set),
            (&set, &off),
            (&off, &Read::Value(2)),
        ] {
            assert_eq!(
                readiness(disable_sr, disable_config, &listed, Some(VOLUME)),
                Readiness::BlockedByPolicy
            );
        }
        for (disable_sr, disable_config) in [
            (&Read::Absent, &Read::Absent),
            (&off, &Read::Absent),
            (&Read::Absent, &off),
            (&off, &off),
        ] {
            assert_eq!(
                readiness(disable_sr, disable_config, &listed, Some(VOLUME)),
                Readiness::Ready
            );
        }
    }

    #[test]
    fn a_policy_that_cant_be_read_is_not_no_policy() {
        let listed = clients_with(&[VOLUME]);
        let failed = Read::Failed("Windows wouldn't read it".to_string());
        assert_eq!(
            readiness(&failed, &Read::Absent, &listed, Some(VOLUME)),
            Readiness::Unknown
        );
        assert_eq!(
            readiness(&Read::Value(0), &failed, &listed, Some(VOLUME)),
            Readiness::Unknown
        );
        // Either one set is enough, whatever became of the other, and it doesn't need the list or the volume.
        assert_eq!(
            readiness(&failed, &Read::Value(1), &listed, Some(VOLUME)),
            Readiness::BlockedByPolicy
        );
        assert_eq!(
            readiness(&Read::Value(1), &failed, &Read::Absent, None),
            Readiness::BlockedByPolicy
        );
        assert_eq!(
            readiness(&Read::Value(1), &Read::Absent, &Read::Value(None), None),
            Readiness::BlockedByPolicy
        );
    }

    #[test]
    fn a_volumes_guid_is_what_is_between_its_braces() {
        assert_eq!(
            volume_guid(VOLUME),
            Some("a016012e-9bc3-4d71-89f1-1765cfb1a416")
        );
        for bad in [
            "",
            "C:\\",
            r"\\?\Volume{}\",
            r"\\?\Volume{a016012e}\",
            r"\\?\Volume{a016012e-9bc3-4d71-89f1-1765cfb1a416",
            r"\\?\Volume\a016012e-9bc3-4d71-89f1-1765cfb1a416\",
        ] {
            assert_eq!(volume_guid(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_windows_folders_drive_is_its_letter_and_a_backslash() {
        for (folder, drive) in [
            (r"C:\Windows", r"C:\"),
            (r"D:\WINDOWS", r"D:\"),
            (r"c:\windows", r"c:\"),
            // Windows installed in the root, as Microsoft says it's then spelled.
            ("C:", r"C:\"),
            (r"C:\", r"C:\"),
        ] {
            assert_eq!(system_drive(folder).as_deref(), Some(drive), "{folder}");
        }
        for bad in [
            "",
            "C",
            r"\\server\share",
            r"\Windows",
            r"1:\Windows",
            "é:\\",
        ] {
            assert_eq!(system_drive(bad), None, "{bad}");
        }
    }

    // Restore points.

    #[test]
    fn a_description_is_terminated_wide_text() {
        let units = restore_description("Kestrel");
        assert_eq!(&units[..8], &wide("Kestrel")[..]);
        assert!(units[7..].iter().all(|&unit| unit == 0));
    }

    #[test]
    fn kestrels_description_fits_whole() {
        let text = crate::restore::DESCRIPTION;
        let units = restore_description(text);
        let end = units.iter().position(|&unit| unit == 0).unwrap();
        assert_eq!(String::from_utf16(&units[..end]).unwrap(), text);
    }

    #[test]
    fn a_description_too_long_is_cut_and_still_terminated() {
        let units = restore_description(&"x".repeat(300));
        assert_eq!(units.len(), 256);
        assert!(units[..255].iter().all(|&unit| unit == u16::from(b'x')));
        assert_eq!(units[255], 0);
    }

    #[test]
    fn a_description_is_never_cut_through_a_character() {
        // 254 units and a character of two: the second half wouldn't fit, and half of one isn't text.
        let text = format!("{}😀", "x".repeat(254));
        let units = restore_description(&text);
        let end = units.iter().position(|&unit| unit == 0).unwrap();
        assert_eq!(end, 254);
        assert!(String::from_utf16(&units[..end]).is_ok());
        // With room for it, it stays.
        let text = format!("{}😀", "x".repeat(253));
        let units = restore_description(&text);
        assert_eq!(String::from_utf16(&units[..255]).unwrap(), text);
    }

    /// The instant a time of the journal's kind stands for, and that it is spelled in this PC's own time.
    fn instant_of(said: &str) -> chrono::DateTime<chrono::Utc> {
        let parsed = chrono::DateTime::parse_from_rfc3339(said)
            .unwrap_or_else(|why| panic!("{said} isn't RFC 3339: {why}"));
        let at = parsed.with_timezone(&chrono::Utc);
        assert_eq!(said, at.with_timezone(&chrono::Local).to_rfc3339());
        at
    }

    fn utc(hour: u32, minute: u32) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::Utc
            .with_ymd_and_hms(2026, 11, 2, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn a_wmi_time_in_utc_becomes_this_pcs_time() {
        assert_eq!(
            instant_of(&local_time("20261102090000.000000-000")),
            utc(9, 0)
        );
        assert_eq!(
            instant_of(&local_time("20261102090000.000000+000")),
            utc(9, 0)
        );
    }

    #[test]
    fn a_wmi_times_offset_is_minutes_from_utc_and_it_counts() {
        // Nine in the morning an hour east of UTC is eight in UTC, and five hours west of it is two in the afternoon.
        assert_eq!(
            instant_of(&local_time("20261102090000.000000+060")),
            utc(8, 0)
        );
        assert_eq!(
            instant_of(&local_time("20261102090000.000000-300")),
            utc(14, 0)
        );
        assert_eq!(
            instant_of(&local_time("20261102090000.000000+330")),
            utc(3, 30)
        );
    }

    #[test]
    fn a_wmi_time_keeps_its_fraction_of_a_second() {
        let said = local_time("20261102090000.250000-000");
        let at = instant_of(&said);
        assert_eq!(at.timestamp_subsec_micros(), 250_000);
    }

    #[test]
    fn the_report_shows_a_wmi_times_first_sixteen_characters_as_this_pcs_time() {
        let said = local_time("20261102090000.000000-000");
        let wanted = utc(9, 0)
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%dT%H:%M")
            .to_string();
        assert_eq!(&said[..16], wanted);
    }

    #[test]
    fn what_isnt_a_wmi_time_comes_back_as_it_is() {
        for odd in [
            "",
            "not a time",
            "20261102090000",
            "20261102090000.000000",
            // One character short and one too long.
            "20261102090000.000000-00",
            "20261102090000.000000-0000",
            // Fields that can't be.
            "20261302090000.000000-000",
            "20260230090000.000000-000",
            "20261102250000.000000-000",
            "20261102096000.000000-000",
            "20261102090060.000000-000",
            // The wildcards the format allows, and things that only look like it.
            "2026****090000.000000-000",
            "20261102090000.******+***",
            "+0261102090000.000000-000",
            "20261102090000,000000-000",
            "20261102090000.000000*000",
            "20261102090000.000000-+00",
            // The right length in bytes, with a character across where the fields are cut.
            "202611020900é.000000-000",
            "2026110209000é.000000-00",
            "😀😀😀😀😀😀.",
        ] {
            assert_eq!(local_time(odd), odd);
        }
    }

    #[test]
    fn a_wmi_time_at_the_edge_of_the_calendar_doesnt_panic() {
        // Whether these parse doesn't matter. There is no restore point from then, and no one may crash on seeing one.
        for edge in [
            "00000101000000.000000+999",
            "00000101000000.000000-999",
            "99991231235959.999999-999",
            "99991231235959.999999+999",
        ] {
            assert!(!local_time(edge).is_empty(), "{edge}");
        }
    }

    #[test]
    fn a_wmi_methods_answer_of_zero_is_success() {
        assert_eq!(method_answer("Enable", 0), Ok(()));
    }

    #[test]
    fn a_wmi_methods_answer_of_1717_goes_on_to_the_read_that_settles_it() {
        // The interface is unknown. PowerShell's own cmdlets let this one past, for both methods.
        assert_eq!(method_answer("Enable", 1717), Ok(()));
        assert_eq!(method_answer("Disable", 1717), Ok(()));
    }

    #[test]
    fn any_other_answer_is_a_failure_with_its_number() {
        assert_eq!(
            method_answer("Enable", 0x8007_0422),
            Err("Windows refused: SystemRestore.Enable answered 0x80070422.".to_string())
        );
        assert_eq!(
            method_answer("Disable", 1056),
            Err("Windows refused: SystemRestore.Disable answered 0x00000420.".to_string())
        );
        // Beside it, and the same number as an HRESULT, are other answers.
        for other in [1716, 1718, 0x8007_06B5] {
            assert!(method_answer("Enable", other).is_err(), "{other}");
        }
    }
}
