//! The graphics card's PCIe link under load. The probe never fills a card's `link_under_load`: it comes from the
//! benchmarks, each of which recorded its card and the best link read during its captures. Every report the app
//! hands out carries the newest one measured on the same card, with the findings, score and guide rebuilt to match.
use chrono::NaiveDate;
use kestrel_bench::{GpuLink, Store};
use kestrel_core::pcie::{apply_links, LinkMeasurement};
use kestrel_core::{PcieLink, Report};
use std::collections::HashSet;

/// A link as a benchmark records it. A function, not a `From` impl: both types belong to other crates.
pub(crate) fn gpu_link_from(link: PcieLink) -> GpuLink {
    GpuLink {
        gen: link.gen,
        width: link.width,
    }
}

/// A link as a report carries it.
fn pcie_link_from(link: GpuLink) -> PcieLink {
    PcieLink {
        gen: link.gen,
        width: link.width,
    }
}

/// The PCIe link each card showed in the newest benchmark that recorded it: at most one measurement per card (ids
/// compare whatever their case), listed newest benchmark first. A card whose newest benchmark read no link has none:
/// an older benchmark's link doesn't stand in for it. Each benchmark is read once. `measured_at` is the benchmark's
/// `created_at` exactly as it was saved (UTC, whole seconds), which `apply_links` compares as text.
pub fn link_measurements(store: &Store) -> Vec<LinkMeasurement> {
    let mut decided = HashSet::new();
    let mut measurements = Vec::new();
    for id in store.ids() {
        let Some(bench) = store.get(&id) else {
            continue;
        };
        let Some(pnp_device_id) = bench.context.gpu_pnp_device_id else {
            continue;
        };
        // The first benchmark met on a card is its newest; an older one has no say.
        if !decided.insert(pnp_device_id.to_ascii_lowercase()) {
            continue;
        }
        if let Some(link) = bench.context.gpu_link {
            measurements.push(LinkMeasurement {
                pnp_device_id,
                link: pcie_link_from(link),
                measured_at: bench.created_at,
                benchmark_id: id,
            });
        }
    }
    measurements
}

/// `report` with each card's newest link under load applied, and its findings, score and guide rebuilt for `today`.
pub fn with_links(report: Report, store: &Store, today: NaiveDate) -> Report {
    let Report {
        app_version,
        mut snapshot,
        ..
    } = report;
    apply_links(&mut snapshot, &link_measurements(store));
    kestrel_core::build_report(snapshot, today, &app_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_bench::{Benchmark, Context};
    use kestrel_core::health::{Finding, Severity, Status};
    use kestrel_core::{GpuVendor, LinkUnderLoad, PcieInfo};

    const CARD: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";
    const OTHER_CARD: &str = "PCI\\VEN_1002&DEV_73BF&SUBSYS_23181458&REV_C1\\6&9F8E7D6C&0&00000008";

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 14).unwrap()
    }

    fn pcie(gen: u8, width: u8) -> PcieLink {
        PcieLink { gen, width }
    }

    /// The owner's PC as the probe reports it, its card known by `CARD`: card and slot 4.0 x16, idling at 1.0 now,
    /// and no link under load (the probe never fills that).
    fn scanned() -> Report {
        let mut s = kestrel_core::owner_pc_fixture();
        for gpu in &mut s.gpus {
            gpu.link_under_load = None;
            if gpu.vendor == GpuVendor::Nvidia {
                gpu.pnp_device_id = CARD.into();
                gpu.pcie = Some(PcieInfo {
                    card_max: Some(pcie(4, 16)),
                    slot_max: Some(pcie(4, 16)),
                    now: Some(pcie(1, 16)),
                });
            }
        }
        kestrel_core::build_report(s, today(), "test")
    }

    /// Saves a benchmark that recorded `card` and `link`; returns its id.
    fn save(store: &Store, created_at: &str, card: Option<&str>, link: Option<GpuLink>) -> String {
        let mut bench = Benchmark {
            name: "Benchmark".into(),
            created_at: created_at.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            context: Context {
                gpu_pnp_device_id: card.map(str::to_string),
                gpu_link: link,
                ..Default::default()
            },
            ..Default::default()
        };
        store.save(&mut bench, &[]).unwrap()
    }

    fn finding<'a>(report: &'a Report, id: &str) -> &'a Finding {
        report
            .findings
            .iter()
            .find(|f| f.id == id)
            .unwrap_or_else(|| panic!("no {id} finding"))
    }

    fn card(report: &Report) -> &kestrel_core::GpuInfo {
        report
            .snapshot
            .gpus
            .iter()
            .find(|g| g.pnp_device_id == CARD)
            .unwrap()
    }

    #[test]
    fn a_link_keeps_its_generation_and_lanes_either_way() {
        assert_eq!(gpu_link_from(pcie(3, 8)), GpuLink { gen: 3, width: 8 });
        assert_eq!(pcie_link_from(GpuLink { gen: 4, width: 16 }), pcie(4, 16));
    }

    #[test]
    fn only_benchmarks_with_a_card_and_a_link_are_measurements() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let measured = save(
            &store,
            "2026-10-13T20:00:00Z",
            Some(CARD),
            Some(GpuLink { gen: 4, width: 16 }),
        );
        // Newer, but it recorded no card (v0.2.0 didn't): it says nothing about this one.
        save(
            &store,
            "2026-10-13T22:00:00Z",
            None,
            Some(GpuLink { gen: 4, width: 16 }),
        );
        assert_eq!(
            link_measurements(&store),
            vec![LinkMeasurement {
                pnp_device_id: CARD.into(),
                link: pcie(4, 16),
                measured_at: "2026-10-13T20:00:00Z".into(),
                benchmark_id: measured,
            }]
        );
    }

    #[test]
    fn the_newest_benchmark_on_the_same_card_sets_the_link_and_the_findings() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        save(
            &store,
            "2026-10-12T20:00:00Z",
            Some(CARD),
            Some(GpuLink { gen: 4, width: 16 }),
        );
        let newer = save(
            &store,
            "2026-10-13T20:00:00Z",
            Some(&CARD.to_lowercase()),
            Some(GpuLink { gen: 3, width: 16 }),
        );
        // Newer still, but measured on another card: never applied to this one.
        save(
            &store,
            "2026-10-14T09:00:00Z",
            Some(OTHER_CARD),
            Some(GpuLink { gen: 1, width: 8 }),
        );
        let plain = scanned();
        let linked = with_links(plain.clone(), &store, today());
        assert_eq!(
            card(&linked).link_under_load,
            Some(LinkUnderLoad {
                link: pcie(3, 16),
                measured_at: "2026-10-13T20:00:00Z".into(),
                benchmark_id: newer,
            })
        );
        assert!(linked
            .snapshot
            .gpus
            .iter()
            .filter(|g| g.pnp_device_id != CARD)
            .all(|g| g.link_under_load.is_none()));
        let speed = finding(&linked, "pcie-speed");
        assert_eq!(
            (speed.status, speed.severity, speed.title.as_str()),
            (
                Status::Problem,
                Severity::Low,
                "PCIe link runs at 3.0 under load instead of 4.0"
            )
        );
        assert!(
            speed.detail.contains("latest benchmark"),
            "{}",
            speed.detail
        );
        assert_eq!(
            linked.score.value + 3,
            plain.score.value,
            "a Low problem costs 3 points"
        );
        assert_eq!(linked.app_version, "test");
    }

    #[test]
    fn a_newer_benchmark_that_read_no_link_leaves_its_card_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        save(
            &store,
            "2026-10-12T20:00:00Z",
            Some(CARD),
            Some(GpuLink { gen: 4, width: 16 }),
        );
        let other = save(
            &store,
            "2026-10-13T08:00:00Z",
            Some(OTHER_CARD),
            Some(GpuLink { gen: 3, width: 8 }),
        );
        // The newest benchmark on `CARD` (its id in lower case: ids compare whatever the case) read no link. The
        // older one's doesn't stand in for it.
        save(
            &store,
            "2026-10-13T20:00:00Z",
            Some(&CARD.to_lowercase()),
            None,
        );
        assert_eq!(
            link_measurements(&store),
            vec![LinkMeasurement {
                pnp_device_id: OTHER_CARD.into(),
                link: pcie(3, 8),
                measured_at: "2026-10-13T08:00:00Z".into(),
                benchmark_id: other,
            }],
            "the other card's own newest benchmark did read a link"
        );
        let plain = scanned();
        let linked = with_links(plain.clone(), &store, today());
        assert_eq!(card(&linked).link_under_load, None);
        assert_eq!(
            finding(&linked, "pcie-speed").title,
            "Run a benchmark to check the PCIe link under load"
        );
        assert_eq!(linked, plain, "nothing to apply: the report is unchanged");
    }

    #[test]
    fn without_measurements_the_report_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("benchmarks"));
        let plain = scanned();
        let same = with_links(plain.clone(), &store, today());
        assert_eq!(same, plain);
        assert_eq!(
            finding(&same, "pcie-speed").title,
            "Run a benchmark to check the PCIe link under load"
        );
        // A benchmark from v0.2.0 recorded neither the card nor the link: still nothing to apply.
        save(&store, "2026-10-13T19:15:00Z", None, None);
        assert_eq!(with_links(plain.clone(), &store, today()), plain);
    }
}
