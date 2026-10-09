//! RAM analysis: what the kit is rated for, and whether it runs in dual channel.
use crate::snapshot::MemoryModule;
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelLayout {
    Single,
    Dual,
    Unknown,
}

struct Rule {
    re: Regex,
    scale: u32,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let rule = |pattern: &str, scale: u32| Rule {
            re: Regex::new(pattern).expect("valid pattern"),
            scale,
        };
        vec![
            rule(r"^CM[A-Z]{1,3}\d+GX[45]M\d[A-Z]?(\d{4})C\d{2}", 1), // Corsair
            rule(r"^F[45]-(\d{4})[A-Z]", 1),                          // G.Skill
            rule(r"^(?:KF|HX)[45](\d{2})C\d{2}", 100),                // Kingston FURY / HyperX
            rule(r"^(?:BL|CP)(?:\dK)?\d+G(\d{2})C\d{2}", 100),        // Crucial Ballistix / Pro
            rule(r"(\d{4})HC\d{2}", 1),                               // TeamGroup
            rule(r"^AX[45]U(\d{4})", 1),                              // ADATA XPG
            rule(r"^PV[A-Z]*[45]\d{1,3}G(\d{3})C", 10),               // Patriot Viper
        ]
    })
}

/// Rated speed in MT/s read from a module's part number, if the brand's format is known.
pub fn rated_mts_from_part(part: &str) -> Option<u32> {
    let part = part.trim().to_ascii_uppercase();
    if part.is_empty() {
        return None;
    }
    rules().iter().find_map(|rule| {
        let digits = rule.re.captures(&part)?.get(1)?.as_str();
        let mts = digits.parse::<u32>().ok()? * rule.scale;
        (1600..=12_000).contains(&mts).then_some(mts)
    })
}

/// The kit's rated speed: the slowest module's part-number rating. When a part number isn't
/// recognised, fall back to the SMBIOS maximum, but only if every module reports one.
pub fn rated_mts(modules: &[MemoryModule]) -> Option<u32> {
    if modules.is_empty() {
        return None;
    }
    let from_parts: Vec<u32> = modules
        .iter()
        .filter_map(|m| rated_mts_from_part(&m.part_number))
        .collect();
    if from_parts.len() == modules.len() {
        return from_parts.into_iter().min();
    }
    let maxima: Vec<u32> = modules
        .iter()
        .filter_map(|m| m.max_mts.filter(|&v| v > 0))
        .collect();
    (maxima.len() == modules.len())
        .then(|| maxima.into_iter().min())
        .flatten()
}

/// The speed the RAM actually runs at (slowest module). `None` if any module doesn't report it.
pub fn configured_mts(modules: &[MemoryModule]) -> Option<u32> {
    let speeds: Vec<u32> = modules
        .iter()
        .filter_map(|m| m.configured_mts.filter(|&v| v > 0))
        .collect();
    (!speeds.is_empty() && speeds.len() == modules.len())
        .then(|| speeds.into_iter().min())
        .flatten()
}

fn channel_letter(module: &MemoryModule) -> Option<char> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)channel\s*([a-h])\b|\bdimm[_\s]?([a-h])\d").expect("valid pattern")
    });
    [&module.device_locator, &module.bank_label]
        .into_iter()
        .find_map(|text| {
            let caps = re.captures(text)?;
            let letter = caps.get(1).or_else(|| caps.get(2))?;
            letter
                .as_str()
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
        })
}

/// Whether the modules sit on more than one memory channel.
pub fn channel_layout(modules: &[MemoryModule]) -> ChannelLayout {
    match modules.len() {
        0 => ChannelLayout::Unknown,
        1 => ChannelLayout::Single,
        _ => match modules
            .iter()
            .map(channel_letter)
            .collect::<Option<Vec<char>>>()
        {
            Some(mut letters) => {
                letters.sort_unstable();
                letters.dedup();
                if letters.len() >= 2 {
                    ChannelLayout::Dual
                } else {
                    ChannelLayout::Single
                }
            }
            None => ChannelLayout::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(
        part: &str,
        configured: Option<u32>,
        max: Option<u32>,
        locator: &str,
        bank: &str,
    ) -> MemoryModule {
        MemoryModule {
            capacity_mb: 16384,
            configured_mts: configured,
            max_mts: max,
            part_number: part.into(),
            manufacturer: String::new(),
            bank_label: bank.into(),
            device_locator: locator.into(),
        }
    }

    #[test]
    fn corsair_parts() {
        assert_eq!(rated_mts_from_part("CMG32GX4M2D3600C18"), Some(3600));
        assert_eq!(rated_mts_from_part("CMK16GX4M2B3200C16"), Some(3200));
        assert_eq!(rated_mts_from_part("CMH32GX5M2B6000C30"), Some(6000));
    }

    #[test]
    fn gskill_parts() {
        assert_eq!(rated_mts_from_part("F4-3600C16-8GVKC"), Some(3600));
        assert_eq!(rated_mts_from_part("F5-6000J3038F16GX2-TZ5RK"), Some(6000));
    }

    #[test]
    fn kingston_parts() {
        assert_eq!(rated_mts_from_part("KF436C17BBK2/16"), Some(3600));
        assert_eq!(rated_mts_from_part("HX432C16PB3K2/16"), Some(3200));
        assert_eq!(rated_mts_from_part("KF560C36BBEK2-32"), Some(6000));
    }

    #[test]
    fn crucial_parts() {
        assert_eq!(rated_mts_from_part("BL8G32C16U4B.M8FE1"), Some(3200));
        assert_eq!(rated_mts_from_part("BL2K16G36C16U4B"), Some(3600));
        assert_eq!(rated_mts_from_part("CP16G56C46U5.M8B1"), Some(5600));
    }

    #[test]
    fn teamgroup_adata_patriot_parts() {
        assert_eq!(rated_mts_from_part("TF13D432G3600HC18JDC01"), Some(3600));
        assert_eq!(rated_mts_from_part("AX4U360038G18A-DT60"), Some(3600));
        assert_eq!(rated_mts_from_part("PVS416G360C8K"), Some(3600));
    }

    #[test]
    fn part_numbers_are_trimmed_and_case_insensitive() {
        assert_eq!(rated_mts_from_part("  cmg32gx4m2d3600c18   "), Some(3600));
    }

    #[test]
    fn unknown_part_gives_none() {
        assert_eq!(rated_mts_from_part("CT16G4DFRA32A"), None);
        assert_eq!(rated_mts_from_part(""), None);
        assert_eq!(rated_mts_from_part("0000"), None);
    }

    #[test]
    fn mixed_kits_use_slowest_rating() {
        let m = [
            module("CMG32GX4M2D3600C18", Some(3200), None, "", ""),
            module("F4-3200C16-8GVKB", Some(3200), None, "", ""),
        ];
        assert_eq!(rated_mts(&m), Some(3200));
    }

    #[test]
    fn unknown_parts_fall_back_to_smbios_max_only_when_every_module_reports_it() {
        assert_eq!(
            rated_mts(&[module("CT16G4DFRA32A", Some(2400), Some(3200), "", "")]),
            Some(3200)
        );
        assert_eq!(
            rated_mts(&[module("CT16G4DFRA32A", Some(2400), None, "", "")]),
            None
        );
        assert_eq!(
            rated_mts(&[module("CT16G4DFRA32A", Some(2400), Some(0), "", "")]),
            None
        );
        assert_eq!(rated_mts(&[]), None);
    }

    #[test]
    fn configured_speed_ignores_zero_and_missing() {
        assert_eq!(
            configured_mts(&[
                module("", Some(3600), None, "", ""),
                module("", Some(0), None, "", "")
            ]),
            None
        );
        assert_eq!(
            configured_mts(&[
                module("", Some(3600), None, "", ""),
                module("", Some(3600), None, "", "")
            ]),
            Some(3600)
        );
        assert_eq!(configured_mts(&[]), None);
    }

    #[test]
    fn channel_layouts() {
        let dual = [
            module("", None, None, "ChannelA-DIMM2", "BANK 1"),
            module("", None, None, "ChannelB-DIMM2", "BANK 3"),
        ];
        assert_eq!(channel_layout(&dual), ChannelLayout::Dual);
        let same = [
            module("", None, None, "ChannelA-DIMM1", ""),
            module("", None, None, "ChannelA-DIMM2", ""),
        ];
        assert_eq!(channel_layout(&same), ChannelLayout::Single);
        let msi = [
            module("", None, None, "DIMM_A2", ""),
            module("", None, None, "DIMM_B2", ""),
        ];
        assert_eq!(channel_layout(&msi), ChannelLayout::Dual);
        let amd = [
            module("", None, None, "DIMM 0", "P0 CHANNEL A"),
            module("", None, None, "DIMM 1", "P0 CHANNEL B"),
        ];
        assert_eq!(channel_layout(&amd), ChannelLayout::Dual);
        let unknown = [
            module("", None, None, "DIMM 0", "BANK 0"),
            module("", None, None, "DIMM 1", "BANK 2"),
        ];
        assert_eq!(channel_layout(&unknown), ChannelLayout::Unknown);
        assert_eq!(channel_layout(&dual[..1]), ChannelLayout::Single);
        assert_eq!(channel_layout(&[]), ChannelLayout::Unknown);
    }
}
