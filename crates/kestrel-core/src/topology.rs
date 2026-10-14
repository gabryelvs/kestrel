//! Processor topology: physical cores and their P/E kind, caches grouped by size, and each core's clock from the
//! per-processor PDH counters.
use crate::snapshot::{CacheInfo, CacheKind};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashMap;

/// One physical core as `GetLogicalProcessorInformationEx` lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhysicalCore {
    pub efficiency_class: u8,
    /// (processor group, number within the group) of each logical processor on the core.
    pub logical: Vec<(u16, u8)>,
}

/// The efficiency class of the P-cores: the highest one, but only when the cores don't all share it.
fn performance_class(cores: &[PhysicalCore]) -> Option<u8> {
    let classes = cores.iter().map(|c| c.efficiency_class);
    let (low, high) = (classes.clone().min()?, classes.max()?);
    (low != high).then_some(high)
}

/// Some((P-cores, E-cores)) only when the cores have more than one efficiency class; P = the highest class.
pub fn p_e_split(cores: &[PhysicalCore]) -> Option<(u32, u32)> {
    let top = performance_class(cores)?;
    let p = cores.iter().filter(|c| c.efficiency_class == top).count() as u32;
    Some((p, cores.len() as u32 - p))
}

fn kind_order(kind: CacheKind) -> u8 {
    match kind {
        CacheKind::Data => 0,
        CacheKind::Instruction => 1,
        CacheKind::Unified => 2,
        CacheKind::Unknown => 3,
    }
}

/// One entry per distinct (level, kind, size) with its count; sorted by level, then Data, Instruction, Unified,
/// Unknown, then larger size first.
pub fn group_caches(caches: &[(u8, CacheKind, u32)]) -> Vec<CacheInfo> {
    let mut groups: Vec<CacheInfo> = Vec::new();
    for &(level, kind, size_kb) in caches {
        match groups
            .iter_mut()
            .find(|g| (g.level, g.kind, g.size_kb) == (level, kind, size_kb))
        {
            Some(group) => group.count += 1,
            None => groups.push(CacheInfo {
                level,
                kind,
                size_kb,
                count: 1,
            }),
        }
    }
    groups.sort_by_key(|g| (g.level, kind_order(g.kind), Reverse(g.size_kb)));
    groups
}

/// Plain decimal digits only: `str::parse` would also take a leading "+".
fn digits<T: std::str::FromStr>(text: &str) -> Option<T> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// PDH `Processor Information` instance "g,n" → (g, n); "_Total", "0,_Total" and anything else → None.
pub fn parse_instance(name: &str) -> Option<(u16, u8)> {
    let (group, number) = name.split_once(',')?;
    Some((digits(group)?, digits(number)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoreKind {
    Performance,
    Efficiency,
}

/// A physical core's clock right now, for the Hardware tab's live clocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreClock {
    pub core: u32,
    pub kind: Option<CoreKind>,
    pub mhz: Option<u32>,
}

/// Counter values by logical processor. A value that isn't a number above zero counts as not reported.
fn by_processor(values: &[(String, f64)]) -> HashMap<(u16, u8), f64> {
    values
        .iter()
        .filter(|(_, value)| value.is_finite() && *value > 0.0)
        .filter_map(|(name, value)| Some((parse_instance(name)?, *value)))
        .collect()
}

/// One clock per physical core, in `cores` order (`core` = its index). MHz = `Processor Frequency` ×
/// `% Processor Performance` / 100 of the core's first logical processor that has both values, rounded; None when
/// none has. `kind` is Some only on hybrid CPUs.
pub fn core_clocks(
    cores: &[PhysicalCore],
    frequency: &[(String, f64)],
    performance: &[(String, f64)],
) -> Vec<CoreClock> {
    let frequency = by_processor(frequency);
    let performance = by_processor(performance);
    let top = performance_class(cores);
    cores
        .iter()
        .enumerate()
        .map(|(index, core)| CoreClock {
            core: index as u32,
            kind: top.map(|top| {
                if core.efficiency_class == top {
                    CoreKind::Performance
                } else {
                    CoreKind::Efficiency
                }
            }),
            mhz: core.logical.iter().find_map(|processor| {
                let mhz = frequency.get(processor)? * performance.get(processor)? / 100.0;
                Some(mhz.round() as u32)
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The owner's i9-11900K: 8 cores of one class, two threads each.
    fn rocket_lake() -> Vec<PhysicalCore> {
        (0..8u8)
            .map(|i| PhysicalCore {
                efficiency_class: 0,
                logical: vec![(0, 2 * i), (0, 2 * i + 1)],
            })
            .collect()
    }

    /// An i9-13900K as Windows lists it: 8 P-cores (class 1, two threads each), then 16 E-cores (class 0, one each).
    fn raptor_lake() -> Vec<PhysicalCore> {
        let p = (0..8u8).map(|i| PhysicalCore {
            efficiency_class: 1,
            logical: vec![(0, 2 * i), (0, 2 * i + 1)],
        });
        let e = (16..32u8).map(|n| PhysicalCore {
            efficiency_class: 0,
            logical: vec![(0, n)],
        });
        p.chain(e).collect()
    }

    fn values(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs
            .iter()
            .map(|&(name, value)| (name.to_string(), value))
            .collect()
    }

    fn cache(level: u8, kind: CacheKind, size_kb: u32, count: u32) -> CacheInfo {
        CacheInfo {
            level,
            kind,
            size_kb,
            count,
        }
    }

    #[test]
    fn one_efficiency_class_is_not_hybrid() {
        assert_eq!(p_e_split(&rocket_lake()), None);
        assert_eq!(p_e_split(&[]), None);
    }

    #[test]
    fn hybrid_cores_split_into_p_and_e() {
        assert_eq!(p_e_split(&raptor_lake()), Some((8, 16)));
    }

    #[test]
    fn the_highest_class_is_p_and_every_other_class_is_e() {
        // Three classes, such as P-cores, E-cores and low-power E-cores: only the top one is P.
        let class = |efficiency_class, n| {
            (0..n).map(move |i| PhysicalCore {
                efficiency_class,
                logical: vec![(0, i)],
            })
        };
        let cores: Vec<PhysicalCore> = class(2, 6).chain(class(1, 8)).chain(class(0, 2)).collect();
        assert_eq!(p_e_split(&cores), Some((6, 10)));
    }

    #[test]
    fn the_owners_caches_group_into_four_lines() {
        // Windows lists every cache: each core's L1 data, L1 instruction and L2, then the shared L3.
        let mut list = Vec::new();
        for _ in 0..8 {
            list.extend([
                (1, CacheKind::Data, 48),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 512),
            ]);
        }
        list.push((3, CacheKind::Unified, 16384));
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 48, 8),
                cache(1, CacheKind::Instruction, 32, 8),
                cache(2, CacheKind::Unified, 512, 8),
                cache(3, CacheKind::Unified, 16384, 1),
            ]
        );
    }

    #[test]
    fn hybrid_caches_list_each_size_with_the_larger_first() {
        // i9-13900K: each P-core has 48 KB + 32 KB of L1 and 2 MB of L2; each E-core 32 KB + 64 KB of L1, and each
        // cluster of four E-cores shares 4 MB of L2.
        let mut list = Vec::new();
        for _ in 0..8 {
            list.extend([
                (1, CacheKind::Data, 48),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 2048),
            ]);
        }
        for _ in 0..16 {
            list.extend([(1, CacheKind::Data, 32), (1, CacheKind::Instruction, 64)]);
        }
        for _ in 0..4 {
            list.push((2, CacheKind::Unified, 4096));
        }
        list.push((3, CacheKind::Unified, 36864));
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 48, 8),
                cache(1, CacheKind::Data, 32, 16),
                cache(1, CacheKind::Instruction, 64, 16),
                cache(1, CacheKind::Instruction, 32, 8),
                cache(2, CacheKind::Unified, 4096, 4),
                cache(2, CacheKind::Unified, 2048, 8),
                cache(3, CacheKind::Unified, 36864, 1),
            ]
        );
    }

    #[test]
    fn two_l3_caches_are_counted_not_merged() {
        // Ryzen 9 7950X: two dies, each with its own 32 MB L3.
        let mut list = Vec::new();
        for _ in 0..16 {
            list.extend([
                (1, CacheKind::Data, 32),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 1024),
            ]);
        }
        list.extend([
            (3, CacheKind::Unified, 32768),
            (3, CacheKind::Unified, 32768),
        ]);
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 32, 16),
                cache(1, CacheKind::Instruction, 32, 16),
                cache(2, CacheKind::Unified, 1024, 16),
                cache(3, CacheKind::Unified, 32768, 2),
            ]
        );
    }

    #[test]
    fn caches_sort_by_level_then_kind_whatever_order_windows_gives() {
        let list = [
            (3, CacheKind::Unified, 8192),
            (2, CacheKind::Unknown, 256),
            (2, CacheKind::Unified, 1024),
            (1, CacheKind::Instruction, 32),
            (1, CacheKind::Data, 32),
        ];
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 32, 1),
                cache(1, CacheKind::Instruction, 32, 1),
                cache(2, CacheKind::Unified, 1024, 1),
                cache(2, CacheKind::Unknown, 256, 1),
                cache(3, CacheKind::Unified, 8192, 1),
            ]
        );
        assert!(group_caches(&[]).is_empty());
    }

    #[test]
    fn pdh_instance_names() {
        assert_eq!(parse_instance("0,15"), Some((0, 15)));
        assert_eq!(parse_instance("1,3"), Some((1, 3)));
        assert_eq!(parse_instance("2,63"), Some((2, 63)));
        for other in [
            "_Total", "0,_Total", "", "0", "0,", ",3", "0,1,2", "a,b", "-1,3", "+0,+1", " 0,1",
            "0,256", "70000,1",
        ] {
            assert_eq!(parse_instance(other), None, "{other:?}");
        }
    }

    #[test]
    fn clocks_are_frequency_times_performance_one_per_core() {
        let mut frequency = Vec::new();
        let mut performance = Vec::new();
        for n in 0..16u8 {
            frequency.push((format!("0,{n}"), 3500.0));
            // Above 100%: the core runs over its base clock.
            performance.push((format!("0,{n}"), 137.14 + f64::from(n / 2) * 2.0));
        }
        frequency.push(("0,_Total".into(), 3500.0));
        frequency.push(("_Total".into(), 3500.0));
        performance.push(("_Total".into(), 140.0));
        let clocks = core_clocks(&rocket_lake(), &frequency, &performance);
        assert_eq!(clocks.len(), 8);
        assert_eq!(
            clocks[0],
            CoreClock {
                core: 0,
                kind: None,
                mhz: Some(4800)
            }
        );
        // 3500 × 151.14% = 5289.9 MHz, rounded.
        assert_eq!(
            clocks[7],
            CoreClock {
                core: 7,
                kind: None,
                mhz: Some(5290)
            }
        );
        assert!(clocks.iter().all(|c| c.kind.is_none()));
    }

    #[test]
    fn hybrid_clocks_say_which_cores_are_p_and_e() {
        let cores = raptor_lake();
        let (mut frequency, mut performance) = (Vec::new(), Vec::new());
        for n in 0..32u8 {
            let (base, percent) = if n < 16 {
                (3000.0, 183.4)
            } else {
                (2200.0, 195.5)
            };
            frequency.push((format!("0,{n}"), base));
            performance.push((format!("0,{n}"), percent));
        }
        let clocks = core_clocks(&cores, &frequency, &performance);
        assert_eq!(clocks.len(), 24);
        assert!(clocks[..8]
            .iter()
            .all(|c| c.kind == Some(CoreKind::Performance) && c.mhz == Some(5502)));
        assert!(clocks[8..]
            .iter()
            .all(|c| c.kind == Some(CoreKind::Efficiency) && c.mhz == Some(4301)));
        assert_eq!(
            clocks.iter().map(|c| c.core).collect::<Vec<_>>(),
            (0..24).collect::<Vec<u32>>()
        );
    }

    #[test]
    fn clocks_map_cores_across_processor_groups() {
        // More than 64 logical processors: Windows splits them into groups, and PDH names each one "group,number".
        let core = |logical: Vec<(u16, u8)>| PhysicalCore {
            efficiency_class: 0,
            logical,
        };
        let cores = [
            core(vec![(0, 62), (0, 63)]),
            core(vec![(1, 0), (1, 1)]),
            core(vec![(1, 2), (1, 3)]),
        ];
        let frequency = values(&[
            ("0,1", 9999.0),
            ("0,62", 2500.0),
            ("0,63", 2500.0),
            ("1,0", 2600.0),
            ("1,1", 2600.0),
            ("1,2", 2700.0),
            ("1,3", 2700.0),
            ("0,_Total", 2500.0),
            ("1,_Total", 2650.0),
            ("_Total", 2600.0),
        ]);
        let performance = values(&[
            ("0,1", 100.0),
            ("0,62", 100.0),
            ("0,63", 100.0),
            ("1,0", 150.0),
            ("1,1", 150.0),
            ("1,2", 120.0),
            ("1,3", 120.0),
        ]);
        let mhz: Vec<Option<u32>> = core_clocks(&cores, &frequency, &performance)
            .iter()
            .map(|c| c.mhz)
            .collect();
        assert_eq!(mhz, [Some(2500), Some(3900), Some(3240)]);
    }

    #[test]
    fn a_core_uses_its_first_logical_processor_with_both_values() {
        let core = |logical: Vec<(u16, u8)>| PhysicalCore {
            efficiency_class: 0,
            logical,
        };
        let cores = [
            core(vec![(0, 0), (0, 1)]),
            core(vec![(0, 2), (0, 3)]),
            core(vec![(0, 4), (0, 5)]),
            core(vec![(0, 6), (0, 7)]),
        ];
        let frequency = values(&[
            ("0,0", 3500.0),
            ("0,1", 3500.0),
            ("0,2", 3500.0),
            ("0,3", 3500.0),
            ("0,6", 0.0),
            ("0,7", f64::NAN),
        ]);
        let performance = values(&[
            ("0,0", 100.0),
            ("0,1", 140.0),
            ("0,3", 120.0),
            ("0,6", 100.0),
            ("0,7", 100.0),
        ]);
        let mhz: Vec<Option<u32>> = core_clocks(&cores, &frequency, &performance)
            .iter()
            .map(|c| c.mhz)
            .collect();
        // Core 0 takes its first thread; core 1's first has no performance value, so its second counts; core 2 has
        // no data; core 3's values aren't real clocks (0 and not a number).
        assert_eq!(mhz, [Some(3500), Some(4200), None, None]);
    }

    #[test]
    fn no_cores_or_no_counters() {
        assert!(core_clocks(&[], &[], &[]).is_empty());
        let clocks = core_clocks(&rocket_lake(), &[], &[]);
        assert_eq!(clocks.len(), 8);
        assert!(clocks.iter().all(|c| c.mhz.is_none()));
    }

    #[test]
    fn clocks_reach_the_ui_as_camel_case_json() {
        let e = CoreClock {
            core: 9,
            kind: Some(CoreKind::Efficiency),
            mhz: Some(4300),
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"core": 9, "kind": "efficiency", "mhz": 4300})
        );
        let unknown = CoreClock {
            core: 0,
            kind: None,
            mhz: None,
        };
        assert_eq!(
            serde_json::to_value(&unknown).unwrap(),
            json!({"core": 0, "kind": null, "mhz": null})
        );
        assert_eq!(
            serde_json::to_value(CoreKind::Performance).unwrap(),
            json!("performance")
        );
    }
}
