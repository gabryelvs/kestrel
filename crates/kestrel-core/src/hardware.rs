//! Hardware tab helpers: BIOS filler names, the chipset, how a monitor is plugged in, and drives with their volumes.

use crate::snapshot::{DriveBus, DriveInfo, DriveMedia, VolumeInfo};
use regex::Regex;
use std::sync::OnceLock;

/// Text board makers leave in SMBIOS when they don't fill a field in.
const PLACEHOLDERS: &[&str] = &[
    "To be filled by O.E.M.",
    "Default string",
    "System Product Name",
    "System manufacturer",
    "System Version",
    "Not Applicable",
    "Not Specified",
    "Base Board Product Name",
    "Type2 - Board Product Name1",
    "O.E.M.",
    "None",
    "x.x",
];

/// True for empty text and BIOS filler: "To be filled by O.E.M.", "Default string", "System Product Name",
/// "System manufacturer", "System Version", "Not Applicable", "Not Specified", "Base Board Product Name",
/// "Type2 - Board Product Name1", "O.E.M.", "None", "x.x" (case-insensitive, trimmed).
pub fn is_placeholder(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || PLACEHOLDERS.iter().any(|p| p.eq_ignore_ascii_case(text))
}

/// Intel's desktop and workstation chipsets, by generation: the 100 series to the 800 series, then X299.
const INTEL_CHIPSETS: &[&str] = &[
    "H110", "B150", "Q150", "H170", "Q170", "Z170", "B250", "Q250", "H270", "Q270", "Z270", "H310",
    "B360", "B365", "H370", "Q370", "Z370", "Z390", "H410", "B460", "H470", "Q470", "Z490", "W480",
    "H510", "B560", "H570", "Q570", "Z590", "W580", "H610", "B660", "H670", "Q670", "Z690", "W680",
    "B760", "H770", "Z790", "W790", "H810", "B860", "Q870", "Z890", "W880", "X299",
];

/// AMD's chipsets: AM4 (300 to 500 series), AM5 (600 and 800 series), Threadripper and Threadripper PRO.
const AMD_CHIPSETS: &[&str] = &[
    "A320", "B350", "X370", "B450", "X470", "A520", "B550", "X570", "A620", "B650", "B650E",
    "X670", "X670E", "B840", "B850", "X870", "X870E", "X399", "TRX40", "TRX50", "WRX80", "WRX90",
];

/// AMD's 300 and 400 series, which Gigabyte's boards may name with an extra leading "A": AB350M, AX370.
const AMD_300_400: &[&str] = &["A320", "B350", "X370", "X399", "B450", "X470"];

/// The one letter a board name may put right after a chipset: "M" for micro-ATX ("B550M"), "I" for mini-ITX ("B550I"),
/// "S" for the fanless "X570S", "X" as in Gigabyte's "Z170X" and "E" as in ASUS's "WRX80E". Any other letter means the
/// word is something else: an ASUS laptop called "X570ZD" has no X570 in it.
const AFTER_NAME: [char; 5] = ['M', 'S', 'I', 'X', 'E'];

/// `name` as it is written on `list`, if it is on it.
fn listed(list: &'static [&'static str], name: &str) -> Option<&'static str> {
    list.iter().copied().find(|candidate| *candidate == name)
}

/// The chipset `name` is, as (maker, name): a name on one of the lists, or, for AMD's 300 and 400 series, one with
/// Gigabyte's extra "A" before it.
fn named(name: &str) -> Option<(&'static str, &'static str)> {
    if let Some(found) = listed(INTEL_CHIPSETS, name) {
        return Some(("Intel", found));
    }
    if let Some(found) = listed(AMD_CHIPSETS, name) {
        return Some(("AMD", found));
    }
    let found = listed(AMD_300_400, name.strip_prefix('A')?)?;
    Some(("AMD", found))
}

/// The chipset one word of a board name spells, as (maker, name): a listed name in any case, with at most one letter
/// of `AFTER_NAME` after it. The word is tried whole first, so "B650E" is B650E and not B650 with an "E".
fn chipset_word(word: &str) -> Option<(&'static str, &'static str)> {
    let word = word.to_ascii_uppercase();
    named(&word).or_else(|| named(word.strip_suffix(AFTER_NAME)?))
}

/// The chipsets a board name spells, in order, as (maker, name): "PRO B760M-A WIFI" → ("Intel", "B760"). Each run of
/// letters and digits is a word of its own, so a name inside a longer word is not found.
fn chipsets(text: &str) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(chipset_word)
}

/// The first Intel chipset an Intel controller's description names: B, H, Q, W or Z and three digits at the start of a
/// word (series 1xx to 8xx), whatever follows them ("(H310D)" → "H310"). AMD's B350 to B850 share the letter B and are
/// skipped. This is looser than the board-name lists on purpose: an OEM desktop's board name is a code like "0KWVT8",
/// and the description is all there is to read.
fn intel_in(description: &str) -> Option<String> {
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let re = TOKEN.get_or_init(|| Regex::new(r"(?i)\b([bhqwz])([0-9]+)").expect("valid pattern"));
    re.captures_iter(description).find_map(|caps| {
        let digits = caps.get(2)?.as_str();
        if digits.len() != 3 {
            return None;
        }
        let letter = caps.get(1)?.as_str().to_ascii_uppercase();
        let number: u32 = digits.parse().ok()?;
        let series = number / 100;
        let amd_b = letter == "B" && series >= 3 && matches!(number % 100, 40 | 50);
        ((1..=8).contains(&series) && !amd_b).then(|| format!("{letter}{digits}"))
    })
}

/// The chipset the board's name spells ("ROG STRIX X670E-E GAMING WIFI" → "AMD X670E"), else the one the Intel
/// LPC/eSPI controller's description names ("Intel(R) Z590 LPC/eSPI Controller - 4385" → "Intel Z590").
///
/// A word of the board name is a chipset when it is on the lists above, in any case, with at most one letter of M, S,
/// I, X or E after it ("B550M", "B550I", "GA-Z170X-Gaming 7", "WRX80E-SAGE"); AMD's 300 and 400 series also with
/// Gigabyte's extra leading "A" ("GA-AB350M-Gaming 3" → "AMD B350"). Anything else is not one, which keeps laptop
/// model codes ("X540LA", "X570ZD", "Q524UQ"), numbers no chipset has ("Z999") and two-digit chipsets ("H61", "Z97")
/// out.
///
/// The description is read only when it mentions Intel and the board name has no chipset, and only for an Intel
/// chipset, by a looser rule than the lists (see `intel_in`): "(H310D)" is "Intel H310" and "(Q870)" is "Intel Q870".
pub fn chipset(board_product: &str, lpc_description: Option<&str>) -> Option<String> {
    if let Some((maker, name)) = chipsets(board_product).next() {
        return Some(format!("{maker} {name}"));
    }
    let description = lpc_description?;
    if !description.to_ascii_lowercase().contains("intel") {
        return None;
    }
    intel_in(description).map(|name| format!("Intel {name}"))
}

/// DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY → "VGA", "DVI", "HDMI", "DisplayPort", "Internal", "USB-C", "Wireless".
/// A laptop's own panel reports LVDS, embedded DisplayPort (11), embedded UDI (13) or INTERNAL, so those are
/// "Internal"; only the external DisplayPort code (10) is "DisplayPort". windows-sys declares the type as i32;
/// INTERNAL is 0x8000_0000 once read as u32.
pub fn connection_name(output_technology: u32) -> Option<&'static str> {
    match output_technology {
        0 => Some("VGA"),                              // HD15
        4 => Some("DVI"),                              // DVI
        5 => Some("HDMI"),                             // HDMI
        10 => Some("DisplayPort"),                     // DISPLAYPORT_EXTERNAL
        6 | 11 | 13 | 0x8000_0000 => Some("Internal"), // LVDS, DISPLAYPORT_EMBEDDED, UDI_EMBEDDED, INTERNAL
        18 => Some("USB-C"),                           // DISPLAYPORT_USB_TUNNEL
        15 => Some("Wireless"),                        // MIRACAST
        _ => None,
    }
}

/// One `MSFT_PhysicalDisk`, with its size already in GiB.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskRecord {
    pub number: u32,
    pub model: String,
    pub media: DriveMedia,
    pub bus: DriveBus,
    pub size_gb: f64,
}

/// Drives by disk number; each with its lettered volumes (letter order) and their space; disks without a
/// lettered volume are still listed.
///
/// `partitions` are (disk number, drive letter); a partition without a letter (Windows gives "\0") or on a disk
/// that isn't listed is left out. `space` is (letter, free GiB, total GiB); a volume without one shows 0.
pub fn drives(
    disks: &[DiskRecord],
    partitions: &[(u32, char)],
    space: &[(char, f64, f64)],
) -> Vec<DriveInfo> {
    let mut sorted: Vec<&DiskRecord> = disks.iter().collect();
    sorted.sort_by_key(|d| d.number);
    sorted
        .into_iter()
        .map(|disk| {
            let mut letters: Vec<char> = partitions
                .iter()
                .filter(|(number, letter)| *number == disk.number && letter.is_ascii_alphabetic())
                .map(|(_, letter)| letter.to_ascii_uppercase())
                .collect();
            letters.sort_unstable();
            letters.dedup();
            let volumes = letters
                .into_iter()
                .map(|letter| {
                    let (free_gb, total_gb) = space
                        .iter()
                        .find(|(l, _, _)| l.eq_ignore_ascii_case(&letter))
                        .map_or((0.0, 0.0), |&(_, free, total)| (free, total));
                    VolumeInfo {
                        letter: letter.to_string(),
                        free_gb,
                        total_gb,
                    }
                })
                .collect();
            DriveInfo {
                model: disk.model.clone(),
                media: disk.media,
                bus: disk.bus,
                size_gb: disk.size_gb,
                volumes,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    #[test]
    fn bios_filler_is_a_placeholder() {
        for filler in [
            "To be filled by O.E.M.",
            "To Be Filled By O.E.M.",
            "Default string",
            "System Product Name",
            "System manufacturer",
            "System Version",
            "Not Applicable",
            "Not Specified",
            "Base Board Product Name",
            "Type2 - Board Product Name1",
            "O.E.M.",
            "None",
            "x.x",
            "  default STRING  ",
            "",
            "   ",
        ] {
            assert!(is_placeholder(filler), "{filler:?}");
        }
    }

    #[test]
    fn real_names_are_not_placeholders() {
        for name in [
            "ROG MAXIMUS XIII HERO",
            "ASUSTeK COMPUTER INC.",
            "American Megatrends Inc.",
            "2103",
            "Nonexistent",
            "System Product Name 2",
            "1.x",
        ] {
            assert!(!is_placeholder(name), "{name:?}");
        }
    }

    #[test]
    fn the_owners_chipset_comes_from_the_lpc_controller() {
        assert_eq!(
            chipset(
                "ROG MAXIMUS XIII HERO",
                Some("Intel(R) Z590 LPC/eSPI Controller - 4385")
            )
            .as_deref(),
            Some("Intel Z590")
        );
    }

    #[test]
    fn board_names_carry_the_chipset() {
        for (board, expected) in [
            ("PRIME B550-PLUS", "AMD B550"),
            ("MAG B760 TOMAHAWK WIFI", "Intel B760"),
            ("ROG STRIX X670E-E GAMING WIFI", "AMD X670E"),
            ("B650M PG Riptide", "AMD B650"),
            ("Z790 AORUS ELITE AX", "Intel Z790"),
            ("PRO B760M-A WIFI", "Intel B760"),
            ("PRO H610M-E DDR4", "Intel H610"),
            ("A620M-HDV/M.2+", "AMD A620"),
            ("MPG X570S EDGE MAX WIFI", "AMD X570"),
            ("X870E AORUS MASTER", "AMD X870E"),
            ("MAG B850 TOMAHAWK MAX WIFI", "AMD B850"),
            ("B840M-HDV/M.2", "AMD B840"),
            ("prime b550-plus", "AMD B550"),
            ("W480 VISION D", "Intel W480"),
            ("Q370M-C", "Intel Q370"),
            // A micro-ATX "M" after the name, and the workstation chipsets.
            ("B550M DS3H", "AMD B550"),
            ("PRIME X670E-PRO WIFI", "AMD X670E"),
            ("TRX40 AORUS MASTER", "AMD TRX40"),
            ("TRX40 AORUS XTREME", "AMD TRX40"),
            ("ROG STRIX X299-E GAMING II", "Intel X299"),
            ("X299 AORUS Gaming 7", "Intel X299"),
            // Gigabyte's AM4 boards of the 300 and 400 series start with an extra "A".
            ("GA-AB350M-Gaming 3", "AMD B350"),
            ("GA-AX370-Gaming K7", "AMD X370"),
            ("GA-AB450M-DS3H", "AMD B450"),
            // One more letter after the name: mini-ITX "I", Gigabyte's "X", ASUS's "E".
            ("B550I AORUS PRO AX", "AMD B550"),
            ("GA-Z170X-Gaming 7", "Intel Z170"),
            ("Pro WS WRX80E-SAGE SE", "AMD WRX80"),
            // Behind a hyphen the letter is a word of its own, and the name is read as before.
            ("ROG STRIX Z790-I GAMING WIFI", "Intel Z790"),
            // An 800-series business chipset with its micro-ATX "M".
            ("Pro Q870M-C-CSM", "Intel Q870"),
        ] {
            assert_eq!(chipset(board, None).as_deref(), Some(expected), "{board}");
        }
    }

    #[test]
    fn b_series_numbers_tell_intel_from_amd() {
        for (token, maker) in [
            ("B150", "Intel"),
            ("B250", "Intel"),
            ("B350", "AMD"),
            ("B360", "Intel"),
            ("B365", "Intel"),
            ("B450", "AMD"),
            ("B460", "Intel"),
            ("B550", "AMD"),
            ("B560", "Intel"),
            ("B650", "AMD"),
            ("B650E", "AMD"),
            ("B660", "Intel"),
            ("B760", "Intel"),
            ("B840", "AMD"),
            ("B850", "AMD"),
            ("B860", "Intel"),
        ] {
            let board = format!("{token} GAMING");
            assert_eq!(
                chipset(&board, None),
                Some(format!("{maker} {token}")),
                "{board}"
            );
        }
    }

    #[test]
    fn every_chipset_series_is_named() {
        for (token, maker) in [
            ("A320", "AMD"),
            ("X370", "AMD"),
            ("X470", "AMD"),
            ("A520", "AMD"),
            ("X570", "AMD"),
            ("X670", "AMD"),
            ("X870", "AMD"),
            ("H110", "Intel"),
            ("Z170", "Intel"),
            ("Z270", "Intel"),
            ("H310", "Intel"),
            ("Z390", "Intel"),
            ("Z490", "Intel"),
            ("H510", "Intel"),
            ("Z590", "Intel"),
            ("Z690", "Intel"),
            ("H770", "Intel"),
            ("Z890", "Intel"),
        ] {
            assert_eq!(
                chipset(&format!("PRIME {token}-P"), None),
                Some(format!("{maker} {token}")),
                "{token}"
            );
        }
    }

    /// Every chipset a board name can carry, written out a second time so a slip in the module's lists shows:
    /// Intel's by generation, then AMD's.
    const EXPECTED_INTEL: [&str; 46] = [
        "H110", "B150", "Q150", "H170", "Q170", "Z170", "B250", "Q250", "H270", "Q270", "Z270",
        "H310", "B360", "B365", "H370", "Q370", "Z370", "Z390", "H410", "B460", "H470", "Q470",
        "Z490", "W480", "H510", "B560", "H570", "Q570", "Z590", "W580", "H610", "B660", "H670",
        "Q670", "Z690", "W680", "B760", "H770", "Z790", "W790", "H810", "B860", "Q870", "Z890",
        "W880", "X299",
    ];
    const EXPECTED_AMD: [&str; 22] = [
        "A320", "B350", "X370", "B450", "X470", "A520", "B550", "X570", "A620", "B650", "B650E",
        "X670", "X670E", "B840", "B850", "X870", "X870E", "X399", "TRX40", "TRX50", "WRX80",
        "WRX90",
    ];

    #[test]
    fn every_listed_chipset_is_named_with_its_maker() {
        for (maker, names) in [
            ("Intel", EXPECTED_INTEL.as_slice()),
            ("AMD", EXPECTED_AMD.as_slice()),
        ] {
            for name in names {
                assert_eq!(
                    chipset(&format!("PRIME {name}-P"), None),
                    Some(format!("{maker} {name}")),
                    "{name}"
                );
            }
        }
    }

    #[test]
    fn only_one_of_m_s_i_x_e_may_follow_a_chipset_name() {
        for (board, expected) in [
            ("B550M GAMING", "AMD B550"),
            ("b550m gaming", "AMD B550"),
            ("X570S EDGE", "AMD X570"),
            ("B550I AORUS PRO AX", "AMD B550"),
            ("Z170X GAMING", "Intel Z170"),
            ("WRX80E SAGE", "AMD WRX80"),
            ("Z590M GAMING X", "Intel Z590"),
        ] {
            assert_eq!(chipset(board, None).as_deref(), Some(expected), "{board}");
        }
        // Any other letter, or a second one, makes it another product: an ASUS laptop called "X570ZD" has no X570
        // in it.
        for letter in ('A'..='Z').filter(|letter| !matches!(letter, 'M' | 'S' | 'I' | 'X' | 'E')) {
            let board = format!("B550{letter} GAMING");
            assert_eq!(chipset(&board, None), None, "{board}");
        }
        for board in [
            "B550MM GAMING",
            "X570SM EDGE",
            "Z590MS GAMING X",
            "B550IX AORUS",
            "Z170XE GAMING",
        ] {
            assert_eq!(chipset(board, None), None, "{board}");
        }
    }

    #[test]
    fn tokens_that_are_not_chipsets_are_skipped() {
        for board in [
            // Laptop model codes that look like a chipset.
            "X540LA",
            "X550JK",
            "X570ZD",
            "Q524UQ",
            // Numbers no chipset has, and two-digit chipsets.
            "Z999 GAMING",
            "X99-A II",
            "H61M-K",
            "Z97-A",
            "B5500 GAMING",
            "Z10PE-D16 WS",
            "A305",
            // A name inside a longer word, and the extra "A" where Gigabyte doesn't write it.
            "XB760",
            "AB550M",
            "AX570",
            "AB760M",
            // No chipset in the name at all.
            "ROG CROSSHAIR VIII HERO",
        ] {
            assert_eq!(chipset(board, None), None, "{board}");
        }
    }

    #[test]
    fn the_lpc_description_is_used_only_when_the_board_name_has_no_token() {
        let z590 = Some("Intel(R) Z590 LPC/eSPI Controller - 4385");
        assert_eq!(chipset("PRIME Z790-P", z590).as_deref(), Some("Intel Z790"));
        assert_eq!(
            chipset("To be filled by O.E.M.", z590).as_deref(),
            Some("Intel Z590")
        );
        assert_eq!(
            chipset(
                "",
                Some("Intel(R) 300 Series Chipset Family LPC Controller (Z390) - A305")
            )
            .as_deref(),
            Some("Intel Z390")
        );
        assert_eq!(
            chipset(
                "",
                Some("Intel(R) 100 Series/C230 Series Chipset Family LPC Controller (Z170) - A145")
            )
            .as_deref(),
            Some("Intel Z170")
        );
    }

    #[test]
    fn an_oem_desktop_gets_its_chipset_from_the_controller_description() {
        // An OEM desktop's board name is a code like "0KWVT8", so the controller's description is all there is.
        // It names the chipset more loosely than a board name does: anything may follow the digits, as in "(H310D)".
        for (description, expected) in [
            (
                "Intel(R) 300 Series Chipset Family LPC Controller (H310D) - A303",
                "Intel H310",
            ),
            (
                "Intel(R) 300 Series Chipset Family LPC Controller (Q360) - A30B",
                "Intel Q360",
            ),
            (
                "Intel(R) Q870 Chipset LPC/eSPI Controller - 7F02",
                "Intel Q870",
            ),
            (
                "Intel(R) 300 Series Chipset Family LPC Controller (Z390) - A305",
                "Intel Z390",
            ),
            (
                "intel(r) 300 series chipset family lpc controller (h310d) - a303",
                "Intel H310",
            ),
        ] {
            assert_eq!(
                chipset("0KWVT8", Some(description)).as_deref(),
                Some(expected),
                "{description}"
            );
        }
    }

    #[test]
    fn the_description_names_an_intel_chipset_of_three_digits_and_the_first_one_wins() {
        let named = |token: &str| {
            chipset(
                "0KWVT8",
                Some(&format!("Intel(R) Chipset LPC Controller ({token}) - 0000")),
            )
        };
        // Every letter an Intel chipset starts with.
        for token in ["B365", "H310", "Q360", "W480", "Z170"] {
            assert_eq!(named(token), Some(format!("Intel {token}")), "{token}");
        }
        // Not Intel's: AMD's B-series, other letters, series 0 and 9, two or four digits (also a leading zero), and a
        // name inside a longer word.
        for token in [
            "B350", "B450", "B550", "B650", "B840", "B850", "X299", "A320", "C246", "H099", "Z999",
            "H31", "H3100", "H0310", "XH310",
        ] {
            assert_eq!(named(token), None, "{token}");
        }
        assert_eq!(
            chipset("0KWVT8", Some("Intel(R) Z390 and H310 LPC Controller")).as_deref(),
            Some("Intel Z390")
        );
    }

    #[test]
    fn no_token_anywhere_is_none() {
        assert_eq!(chipset("ROG MAXIMUS XIII HERO", None), None);
        assert_eq!(chipset("", None), None);
        assert_eq!(chipset("To be filled by O.E.M.", None), None);
        assert_eq!(
            chipset("ROG CROSSHAIR VIII HERO", Some("PCI standard ISA bridge")),
            None
        );
        // Only an Intel controller's description names the chipset.
        assert_eq!(chipset("", Some("Z590 LPC Controller")), None);
    }

    #[test]
    fn an_intel_description_names_only_intel_chipsets() {
        // An AMD chipset's name in an Intel controller's description is not read as Intel's.
        assert_eq!(chipset("", Some("Intel(R) X570 LPC Controller")), None);
        assert_eq!(
            chipset("", Some("Intel(R) B350 Z390 LPC Controller")).as_deref(),
            Some("Intel Z390")
        );
    }

    #[test]
    fn connection_names_follow_windows_output_technology() {
        for (code, name) in [
            (0, "VGA"),
            (4, "DVI"),
            (5, "HDMI"),
            (10, "DisplayPort"),
            // A laptop's own panel reports LVDS, embedded DisplayPort, embedded UDI or INTERNAL.
            (6, "Internal"),
            (11, "Internal"),
            (13, "Internal"),
            (0x8000_0000, "Internal"),
            (18, "USB-C"),
            (15, "Wireless"),
        ] {
            assert_eq!(connection_name(code), Some(name), "{code}");
        }
        // S-Video, composite, component, D-JPN, SDI, external UDI, SDTV dongle, indirect, OTHER (-1) and unknown codes.
        for code in [1, 2, 3, 7, 8, 9, 12, 14, 16, 17, 19, u32::MAX] {
            assert_eq!(connection_name(code), None, "{code}");
        }
    }

    fn disk(
        number: u32,
        model: &str,
        media: DriveMedia,
        bus: DriveBus,
        size_gb: f64,
    ) -> DiskRecord {
        DiskRecord {
            number,
            model: model.into(),
            media,
            bus,
            size_gb,
        }
    }

    /// The owner's two disks, in the order Windows happened to list them.
    fn owner_disks() -> Vec<DiskRecord> {
        vec![
            disk(
                1,
                "WDC WD20EZBX-00AYRA0",
                DriveMedia::Hdd,
                DriveBus::Sata,
                1863.0,
            ),
            disk(
                0,
                "Samsung SSD 980 PRO 1TB",
                DriveMedia::Nvme,
                DriveBus::Nvme,
                931.5,
            ),
        ]
    }

    #[test]
    fn the_owners_drives_group_with_their_volumes() {
        // The EFI and recovery partitions have no letter.
        let partitions = [(0, '\0'), (0, 'C'), (0, '\0'), (1, 'D')];
        let space = [('D', 1228.8, 1863.0), ('C', 210.5, 931.5)];
        assert_eq!(
            drives(&owner_disks(), &partitions, &space),
            owner_pc_fixture().storage.drives
        );
    }

    #[test]
    fn volumes_list_in_letter_order_with_their_space() {
        let partitions = [(0, 'C'), (1, 'G'), (1, 'e'), (1, 'g')];
        let space = [
            ('C', 210.5, 931.5),
            ('E', 400.0, 931.5),
            ('G', 828.8, 931.5),
        ];
        let found = drives(&owner_disks(), &partitions, &space);
        let letters: Vec<&str> = found[1].volumes.iter().map(|v| v.letter.as_str()).collect();
        assert_eq!(letters, ["E", "G"]);
        assert_eq!(
            found[1].volumes[1],
            VolumeInfo {
                letter: "G".into(),
                free_gb: 828.8,
                total_gb: 931.5
            }
        );
    }

    #[test]
    fn a_volume_without_space_shows_zero() {
        let found = drives(&owner_disks(), &[(0, 'C')], &[]);
        assert_eq!(
            found[0].volumes,
            [VolumeInfo {
                letter: "C".into(),
                free_gb: 0.0,
                total_gb: 0.0
            }]
        );
    }

    #[test]
    fn a_volume_finds_its_space_whatever_the_letter_case() {
        let found = drives(&owner_disks(), &[(0, 'c')], &[('c', 210.5, 931.5)]);
        assert_eq!(
            found[0].volumes,
            [VolumeInfo {
                letter: "C".into(),
                free_gb: 210.5,
                total_gb: 931.5
            }]
        );
    }

    #[test]
    fn a_disk_without_a_lettered_volume_is_still_listed() {
        let mut disks = owner_disks();
        disks.push(disk(
            2,
            "Msft Virtual Disk",
            DriveMedia::Unknown,
            DriveBus::Virtual,
            127.0,
        ));
        let found = drives(&disks, &[(0, 'C'), (1, '\0')], &[('C', 210.5, 931.5)]);
        let models: Vec<&str> = found.iter().map(|d| d.model.as_str()).collect();
        assert_eq!(
            models,
            [
                "Samsung SSD 980 PRO 1TB",
                "WDC WD20EZBX-00AYRA0",
                "Msft Virtual Disk"
            ]
        );
        assert!(found[1].volumes.is_empty());
        assert!(found[2].volumes.is_empty());
        assert_eq!(found[2].bus, DriveBus::Virtual);
    }

    #[test]
    fn partitions_on_unknown_disks_are_ignored() {
        let partitions = [(0, 'C'), (5, 'E')];
        let space = [('C', 210.5, 931.5), ('E', 10.0, 20.0)];
        let found = drives(&owner_disks(), &partitions, &space);
        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .flat_map(|d| &d.volumes)
            .all(|v| v.letter != "E"));
        assert!(drives(&[], &partitions, &space).is_empty());
    }
}
