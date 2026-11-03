//! The pure half of the Windows code: how GUIDs, text and registry values are spelled on their way to and from Windows,
//! and the refusals that stand below the rules. No Windows call is in here, so all of it is tested on any OS, as
//! `kestrel_probe::parse` is for the probe.
use std::io;

use crate::journal::is_guid;
use crate::power::KESTREL_PLAN_GUID;
use crate::system::Read;
use crate::targets::{
    ALLOW_AUTO_GAME_MODE, ALLOW_GAME_DVR, APP_CAPTURE, MOUSE_SPEED, MOUSE_THRESHOLD_1,
    MOUSE_THRESHOLD_2,
};

/// The registry's number for a string value.
pub const REG_SZ: u32 = 1;
/// The registry's number for a DWORD value.
pub const REG_DWORD: u32 = 4;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::KESTREL_PLAN_GUID;
    use crate::system::Read;
    use std::io;

    const REG_EXPAND_SZ: u32 = 2;
    const REG_BINARY: u32 = 3;
    const REG_DWORD_BIG_ENDIAN: u32 = 5;
    const REG_MULTI_SZ: u32 = 7;
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
}
