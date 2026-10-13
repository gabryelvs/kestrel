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

/// Who makes a chipset, from its letter and number. AMD's are A, B and X with a 300–800 number ending in 0 (its
/// B-series ends in 40 or 50: B350 … B850). Intel's are B, H, Q, W and Z with a 100–800 number, so B360, B460,
/// B560, B660, B760 and B860 are Intel's.
fn maker(letter: char, number: u32) -> Option<&'static str> {
    let series = number / 100;
    match letter {
        'A' | 'X' if (3..=8).contains(&series) && number.is_multiple_of(10) => Some("AMD"),
        'B' if (3..=8).contains(&series) && matches!(number % 100, 40 | 50) => Some("AMD"),
        'B' | 'H' | 'Q' | 'W' | 'Z' if (1..=8).contains(&series) => Some("Intel"),
        _ => None,
    }
}

/// Chipset tokens in `text`, in order, as (maker, token): "PRO B760M-A WIFI" → ("Intel", "B760"). AMD tokens keep
/// an "E" written right after the number (X670E); other letters after it (B760M, X570S) aren't part of the chipset.
fn tokens(text: &str) -> impl Iterator<Item = (&'static str, String)> + '_ {
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let re = TOKEN.get_or_init(|| Regex::new(r"(?i)\b([a-z])(\d+)(e?)").expect("valid pattern"));
    re.captures_iter(text).filter_map(|caps| {
        let digits = caps.get(2)?.as_str();
        if digits.len() != 3 {
            return None;
        }
        let letter = caps.get(1)?.as_str().chars().next()?.to_ascii_uppercase();
        let maker = maker(letter, digits.parse().ok()?)?;
        let e = if maker == "AMD" && !caps.get(3)?.as_str().is_empty() {
            "E"
        } else {
            ""
        };
        Some((maker, format!("{letter}{digits}{e}")))
    })
}

/// A chipset token (e.g. "Z590", "B550", "X670E", "A620", "H610") from the board name, else from the Intel
/// LPC/eSPI controller's description ("Intel(R) Z590 LPC/eSPI Controller - 4385" → "Intel Z590"). Board-name
/// tokens get the maker prefix from the token (Intel for Z/H/B/Q/W-series 1xx–8xx, AMD for A/B/X 3xx–8xx).
pub fn chipset(board_product: &str, lpc_description: Option<&str>) -> Option<String> {
    if let Some((maker, token)) = tokens(board_product).next() {
        return Some(format!("{maker} {token}"));
    }
    let description = lpc_description?;
    if !description.to_ascii_lowercase().contains("intel") {
        return None;
    }
    tokens(description)
        .find(|(maker, _)| *maker == "Intel")
        .map(|(_, token)| format!("Intel {token}"))
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

    #[test]
    fn tokens_that_are_not_chipsets_are_skipped() {
        for board in [
            "TRX40 AORUS XTREME",
            "X299 AORUS Gaming 7",
            "X99-A II",
            "B5500 GAMING",
            "Z10PE-D16 WS",
            "ROG CROSSHAIR VIII HERO",
            "A305",
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
