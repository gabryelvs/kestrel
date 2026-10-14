//! The graphics card's PCIe link: decoding Windows' link codes, finding the slot's link above the card, the
//! Resizable BAR rule, and applying links measured during benchmarks.

use crate::gpu;
use crate::snapshot::{GpuVendor, LinkUnderLoad, PcieInfo, PcieLink, SystemSnapshot};
use regex::Regex;
use std::sync::OnceLock;

/// `DEVPKEY_PciDevice_DeviceType` values (pciprop.h) of the bridges on the way from a card to the CPU.
pub const ROOT_PORT: u32 = 8;
pub const UPSTREAM_SWITCH_PORT: u32 = 9;
pub const DOWNSTREAM_SWITCH_PORT: u32 = 10;
/// The BAR every card gets without Resizable BAR; a larger memory range means it is on.
pub const REBAR_THRESHOLD_MB: u64 = 256;

const MIB: u64 = 1024 * 1024;

/// Speed codes 1–6 are PCIe 1.0–6.0; widths 1–32 lanes. Anything else (0 = asleep) → None.
pub fn link_from_codes(speed: Option<u32>, width: Option<u32>) -> Option<PcieLink> {
    let gen = u8::try_from(speed?).ok().filter(|g| (1..=6).contains(g))?;
    let width = u8::try_from(width?).ok().filter(|w| (1..=32).contains(w))?;
    Some(PcieLink { gen, width })
}

/// A PCI device and its link, as Configuration Manager reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PciNode {
    pub instance_id: String,
    pub parent: Option<String>,
    pub device_type: Option<u32>,
    pub current: Option<PcieLink>,
    pub max: Option<PcieLink>,
}

fn find<'a>(nodes: &'a [PciNode], instance_id: &str) -> Option<&'a PciNode> {
    if instance_id.is_empty() {
        return None;
    }
    nodes
        .iter()
        .find(|n| n.instance_id.eq_ignore_ascii_case(instance_id))
}

fn parent<'a>(nodes: &'a [PciNode], child: &PciNode) -> Option<&'a PciNode> {
    find(nodes, child.parent.as_deref()?)
}

/// The PCI vendor in an instance id, upper case: "PCI\\VEN_1002&DEV_1478&…" → "1002".
fn pci_vendor(instance_id: &str) -> Option<String> {
    let upper = instance_id.to_ascii_uppercase();
    let start = upper.find("VEN_")? + 4;
    let id = upper.get(start..start + 4)?;
    id.bytes()
        .all(|b| b.is_ascii_hexdigit())
        .then(|| id.to_string())
}

/// The device whose link is the slot's: the card itself, or — through a card's internal switch — the upstream
/// switch port above it (walk up while the parent is a downstream switch port whose parent is an upstream switch
/// port). Instance ids compare case-insensitively.
///
/// A switch counts as the card's own only when both its ports carry the card's PCI vendor id (AMD Radeon and
/// Intel Arc cards have one). A chipset that is a switch (AMD's X570, B650…) or a Thunderbolt enclosure has
/// another vendor's id, so the walk stops at the card and its own link is read.
pub fn slot_device<'a>(nodes: &'a [PciNode], gpu_instance_id: &str) -> Option<&'a PciNode> {
    let mut device = find(nodes, gpu_instance_id)?;
    let Some(vendor) = pci_vendor(&device.instance_id) else {
        return Some(device);
    };
    let card_port = |node: &PciNode, kind: u32| {
        node.device_type == Some(kind) && pci_vendor(&node.instance_id).as_ref() == Some(&vendor)
    };
    // Bounded, so a malformed list whose ports point at each other can't hang the scan.
    for _ in 0..nodes.len() {
        let Some(down) = parent(nodes, device).filter(|p| card_port(p, DOWNSTREAM_SWITCH_PORT))
        else {
            break;
        };
        let Some(up) = parent(nodes, down).filter(|p| card_port(p, UPSTREAM_SWITCH_PORT)) else {
            break;
        };
        device = up;
    }
    Some(device)
}

/// card_max and now from the slot device; slot_max from its parent when that is a root port. None when the card
/// isn't in `nodes`.
pub fn resolve(nodes: &[PciNode], gpu_instance_id: &str) -> Option<PcieInfo> {
    let device = slot_device(nodes, gpu_instance_id)?;
    let slot_max = parent(nodes, device)
        .filter(|p| p.device_type == Some(ROOT_PORT))
        .and_then(|p| p.max.clone());
    Some(PcieInfo {
        card_max: device.max.clone(),
        slot_max,
        now: device.current.clone(),
    })
}

/// (largest range in MB, Resizable BAR on). Empty → (None, None); a range > 256 MB → Some(true); else Some(false).
/// MB are rounded to the nearest whole one, and the rule looks at that rounded number.
pub fn bar_summary(range_bytes: &[u64]) -> (Option<u64>, Option<bool>) {
    let Some(&largest) = range_bytes.iter().max() else {
        return (None, None);
    };
    let mb = largest / MIB + u64::from(largest % MIB >= MIB / 2);
    (Some(mb), Some(mb > REBAR_THRESHOLD_MB))
}

/// The series digit `pattern` captures from a card's name: "RTX 3060" → 3.
fn series(cell: &OnceLock<Regex>, pattern: &str, name: &str) -> Option<u32> {
    let re = cell.get_or_init(|| Regex::new(pattern).expect("valid pattern"));
    re.captures(name)?.get(1)?.as_str().parse().ok()
}

/// NVIDIA RTX 30 and newer, AMD RX 6000 and newer, discrete Intel Arc.
pub fn uses_resizable_bar(vendor: GpuVendor, name: &str) -> bool {
    static RTX: OnceLock<Regex> = OnceLock::new();
    static RX: OnceLock<Regex> = OnceLock::new();
    match vendor {
        // Turing's Quadro RTX 4000–8000 share the numbers of later GeForce cards.
        GpuVendor::Nvidia => {
            !name.to_ascii_lowercase().contains("quadro")
                && series(&RTX, r"(?i)\brtx\s*(\d)\d{3}", name).is_some_and(|s| s >= 3)
        }
        GpuVendor::Amd => series(&RX, r"(?i)\brx\s*(\d)\d{3}", name).is_some_and(|s| s >= 6),
        // Discrete Arc (A- and B-series); the integrated "Arc Graphics" doesn't count.
        GpuVendor::Intel => !gpu::looks_integrated(GpuVendor::Intel, name, None),
        GpuVendor::Virtual | GpuVendor::Other => false,
    }
}

/// 4 → "4.0".
pub fn gen_label(gen: u8) -> String {
    format!("{gen}.0")
}

/// A link measured during a saved benchmark, on the card with this PNP device id.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkMeasurement {
    pub pnp_device_id: String,
    pub link: PcieLink,
    pub measured_at: String,
    pub benchmark_id: String,
}

/// For each GPU, the newest measurement (largest `measured_at`, RFC 3339) whose `pnp_device_id` matches it
/// case-insensitively becomes its `link_under_load`; GPUs without one keep None.
pub fn apply_links(snapshot: &mut SystemSnapshot, measurements: &[LinkMeasurement]) {
    for gpu in &mut snapshot.gpus {
        if gpu.pnp_device_id.is_empty() {
            continue;
        }
        // Kestrel writes every timestamp in UTC with whole seconds, so text order is time order.
        let newest = measurements
            .iter()
            .filter(|m| m.pnp_device_id.eq_ignore_ascii_case(&gpu.pnp_device_id))
            .max_by(|a, b| a.measured_at.cmp(&b.measured_at));
        if let Some(m) = newest {
            gpu.link_under_load = Some(LinkUnderLoad {
                link: m.link.clone(),
                measured_at: m.measured_at.clone(),
                benchmark_id: m.benchmark_id.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    const RTX: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&2B0D3A0E&0&0008";
    const INTEL_ROOT: &str = "PCI\\VEN_8086&DEV_4C01&SUBSYS_86941043&REV_01\\3&11583659&0&08";
    const RADEON: &str = "PCI\\VEN_1002&DEV_73BF&SUBSYS_0E3A1002&REV_C1\\6&1B3F2C1A&0&00000019";
    const RADEON_DOWN: &str = "PCI\\VEN_1002&DEV_1479&SUBSYS_00000000&REV_00\\5&22A3F1B&0&000019";
    const RADEON_UP: &str = "PCI\\VEN_1002&DEV_1478&SUBSYS_00000000&REV_C1\\4&2C9E7C26&0&0019";
    const AMD_ROOT: &str = "PCI\\VEN_1022&DEV_1483&SUBSYS_14531022&REV_00\\3&2411E6FE&0&09";
    const HOST_BRIDGE: &str = "ACPI\\PNP0A08\\0";
    /// Device types in the lists below: the graphics card (a PCI Express endpoint) and the two kinds of switch port.
    const ENDPOINT: u32 = 2;
    const UP: u32 = UPSTREAM_SWITCH_PORT;
    const DOWN: u32 = DOWNSTREAM_SWITCH_PORT;

    fn link(gen: u8, width: u8) -> Option<PcieLink> {
        Some(PcieLink { gen, width })
    }

    fn node(
        id: &str,
        parent: &str,
        device_type: u32,
        current: Option<PcieLink>,
        max: Option<PcieLink>,
    ) -> PciNode {
        PciNode {
            instance_id: id.into(),
            parent: Some(parent.into()),
            device_type: Some(device_type),
            current,
            max,
        }
    }

    /// The owner's RTX 3060 Ti in the CPU's x16 slot, idling at PCIe 1.0.
    fn owner_nodes() -> Vec<PciNode> {
        vec![
            node(INTEL_ROOT, HOST_BRIDGE, ROOT_PORT, link(1, 16), link(4, 16)),
            node(RTX, INTEL_ROOT, ENDPOINT, link(1, 16), link(4, 16)),
        ]
    }

    /// An RX 6800 XT: the GPU sits behind the card's own switch, whose upstream port is in a PCIe 3.0 slot.
    fn radeon_nodes() -> Vec<PciNode> {
        vec![
            node(AMD_ROOT, HOST_BRIDGE, ROOT_PORT, link(3, 8), link(3, 16)),
            node(RADEON_UP, AMD_ROOT, UP, link(3, 8), link(4, 16)),
            node(RADEON_DOWN, RADEON_UP, DOWN, link(4, 16), link(4, 16)),
            node(RADEON, RADEON_DOWN, ENDPOINT, link(4, 16), link(4, 16)),
        ]
    }

    fn slot_id<'a>(nodes: &'a [PciNode], gpu: &str) -> Option<&'a str> {
        slot_device(nodes, gpu).map(|n| n.instance_id.as_str())
    }

    #[test]
    fn link_codes_decode_to_generations_and_lanes() {
        for gen in 1..=6u8 {
            assert_eq!(
                link_from_codes(Some(u32::from(gen)), Some(16)),
                link(gen, 16)
            );
        }
        for width in [1u8, 2, 4, 8, 12, 16, 32] {
            assert_eq!(
                link_from_codes(Some(4), Some(u32::from(width))),
                link(4, width)
            );
        }
    }

    #[test]
    fn a_sleeping_or_unreadable_link_is_none_never_x0() {
        assert_eq!(link_from_codes(Some(4), Some(0)), None);
        assert_eq!(link_from_codes(Some(0), Some(16)), None);
        assert_eq!(link_from_codes(Some(7), Some(16)), None);
        assert_eq!(link_from_codes(Some(4), Some(33)), None);
        // 272 would wrap to 16 if it were cast to a byte.
        assert_eq!(link_from_codes(Some(4), Some(272)), None);
        assert_eq!(link_from_codes(Some(u32::MAX), Some(16)), None);
        assert_eq!(link_from_codes(None, Some(16)), None);
        assert_eq!(link_from_codes(Some(4), None), None);
        assert_eq!(link_from_codes(None, None), None);
    }

    #[test]
    fn a_card_straight_under_the_root_port_is_its_own_slot_device() {
        let nodes = owner_nodes();
        assert_eq!(slot_id(&nodes, RTX), Some(RTX));
        assert_eq!(
            resolve(&nodes, RTX),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: link(4, 16),
                now: link(1, 16),
            })
        );
        assert_eq!(resolve(&nodes, RTX), owner_pc_fixture().gpus[1].pcie);
    }

    #[test]
    fn an_amd_card_is_read_above_its_own_switch() {
        // The GPU's own link to its switch is internal (here 4.0 x16); the slot's link is the upstream port's.
        let nodes = radeon_nodes();
        assert_eq!(slot_id(&nodes, RADEON), Some(RADEON_UP));
        assert_eq!(
            resolve(&nodes, RADEON),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: link(3, 16),
                now: link(3, 8),
            })
        );
    }

    #[test]
    fn a_chipset_switch_is_not_the_cards_own_and_its_slot_maximum_is_unknown() {
        // AMD's X570 chipset is a PCIe switch (vendor 1022). Cards in its slot run at x4 there.
        const X570_UP: &str = "PCI\\VEN_1022&DEV_57AD&SUBSYS_00000000&REV_00\\4&1A2B3C4D&0&0209";
        const X570_DOWN: &str =
            "PCI\\VEN_1022&DEV_57A4&SUBSYS_00000000&REV_00\\5&3E4F5A6B&0&400209";
        let chipset = || {
            vec![
                node(AMD_ROOT, HOST_BRIDGE, ROOT_PORT, link(4, 4), link(4, 4)),
                node(X570_UP, AMD_ROOT, UP, link(4, 4), link(4, 4)),
                node(X570_DOWN, X570_UP, DOWN, link(4, 4), link(4, 4)),
            ]
        };
        let mut nvidia = chipset();
        nvidia.push(node(RTX, X570_DOWN, ENDPOINT, link(4, 4), link(4, 16)));
        assert_eq!(slot_id(&nvidia, RTX), Some(RTX));
        assert_eq!(
            resolve(&nvidia, RTX),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: None,
                now: link(4, 4),
            })
        );
        // A Radeon there climbs its own switch, then stops at the chipset's.
        let mut amd = chipset();
        amd.extend([
            node(RADEON_UP, X570_DOWN, UP, link(4, 4), link(4, 16)),
            node(RADEON_DOWN, RADEON_UP, DOWN, link(4, 16), link(4, 16)),
            node(RADEON, RADEON_DOWN, ENDPOINT, link(4, 16), link(4, 16)),
        ]);
        assert_eq!(slot_id(&amd, RADEON), Some(RADEON_UP));
        assert_eq!(
            resolve(&amd, RADEON),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: None,
                now: link(4, 4),
            })
        );
    }

    #[test]
    fn a_card_in_a_thunderbolt_enclosure_reads_its_own_link() {
        // The enclosure's and the PC's Thunderbolt controllers are Intel switches; the card's x4 link is real.
        const BOX_DOWN: &str = "PCI\\VEN_8086&DEV_15EF&SUBSYS_00000000&REV_06\\7&1F2E3D4C&0&20";
        const BOX_UP: &str = "PCI\\VEN_8086&DEV_15EF&SUBSYS_00000000&REV_06\\6&2A3B4C5D&0&00";
        const PC_DOWN: &str = "PCI\\VEN_8086&DEV_15EA&SUBSYS_00000000&REV_06\\5&3B4C5D6E&0&08";
        const PC_UP: &str = "PCI\\VEN_8086&DEV_15EA&SUBSYS_00000000&REV_06\\4&4C5D6E7F&0&00";
        let nodes = vec![
            node(INTEL_ROOT, HOST_BRIDGE, ROOT_PORT, link(3, 4), link(3, 4)),
            node(PC_UP, INTEL_ROOT, UP, link(3, 4), link(3, 4)),
            node(PC_DOWN, PC_UP, DOWN, link(1, 4), link(3, 4)),
            node(BOX_UP, PC_DOWN, UP, link(1, 4), link(3, 4)),
            node(BOX_DOWN, BOX_UP, DOWN, link(3, 4), link(3, 4)),
            node(RTX, BOX_DOWN, ENDPOINT, link(3, 4), link(4, 16)),
        ];
        assert_eq!(slot_id(&nodes, RTX), Some(RTX));
        assert_eq!(
            resolve(&nodes, RTX),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: None,
                now: link(3, 4),
            })
        );
    }

    #[test]
    fn a_card_missing_from_the_list_has_no_link() {
        let nodes = owner_nodes();
        let other = "PCI\\VEN_10DE&DEV_2704&SUBSYS_00000000&REV_A1\\4&0&0&0008";
        assert_eq!(slot_device(&nodes, other), None);
        assert_eq!(resolve(&nodes, other), None);
        assert_eq!(resolve(&nodes, ""), None);
        assert_eq!(resolve(&[], RTX), None);
    }

    #[test]
    fn instance_ids_compare_case_insensitively() {
        let lower = RTX.to_ascii_lowercase();
        assert_eq!(
            resolve(&owner_nodes(), &lower),
            resolve(&owner_nodes(), RTX)
        );
        let mut nodes = radeon_nodes();
        for n in &mut nodes {
            n.parent = n.parent.as_ref().map(|p| p.to_ascii_lowercase());
        }
        assert_eq!(slot_id(&nodes, RADEON), Some(RADEON_UP));
    }

    #[test]
    fn a_card_without_link_data_resolves_to_not_reported() {
        // A laptop card asleep, a virtual machine or an old Windows: the properties are missing.
        let mut nodes = owner_nodes();
        nodes[1].current = None;
        nodes[1].max = None;
        assert_eq!(
            resolve(&nodes, RTX),
            Some(PcieInfo {
                card_max: None,
                slot_max: link(4, 16),
                now: None,
            })
        );
        nodes[0].max = None;
        assert_eq!(resolve(&nodes, RTX), Some(PcieInfo::default()));
    }

    #[test]
    fn ports_that_point_at_each_other_do_not_hang_the_walk() {
        let nodes = vec![
            node(RADEON_UP, RADEON_DOWN, UP, link(4, 16), link(4, 16)),
            node(RADEON_DOWN, RADEON_UP, DOWN, link(4, 16), link(4, 16)),
            node(RADEON, RADEON_DOWN, ENDPOINT, link(4, 16), link(4, 16)),
        ];
        assert_eq!(slot_id(&nodes, RADEON), Some(RADEON_UP));
    }

    #[test]
    fn resizable_bar_is_on_only_with_a_range_over_256_mb() {
        assert_eq!(bar_summary(&[]), (None, None));
        assert_eq!(bar_summary(&[256 * MIB]), (Some(256), Some(false)));
        assert_eq!(
            bar_summary(&[16 * MIB, 256 * MIB, 32 * MIB]),
            (Some(256), Some(false))
        );
        assert_eq!(
            bar_summary(&[16 * MIB, 8192 * MIB, 32 * MIB]),
            (Some(8192), Some(true))
        );
        assert_eq!(bar_summary(&[512 * MIB]), (Some(512), Some(true)));
    }

    #[test]
    fn bar_sizes_round_to_the_nearest_mb() {
        // A range one byte short of 256 MB is still the standard BAR.
        assert_eq!(bar_summary(&[256 * MIB - 1]), (Some(256), Some(false)));
        assert_eq!(
            bar_summary(&[256 * MIB + MIB / 2 - 1]),
            (Some(256), Some(false))
        );
        assert_eq!(bar_summary(&[256 * MIB + MIB / 2]), (Some(257), Some(true)));
        assert_eq!(bar_summary(&[MIB / 2 - 1]), (Some(0), Some(false)));
    }

    #[test]
    fn which_cards_use_resizable_bar() {
        use GpuVendor::{Amd, Intel, Nvidia, Other, Virtual};
        for (vendor, name, uses) in [
            (Nvidia, "NVIDIA GeForce RTX 3060 Ti", true),
            (Nvidia, "NVIDIA GeForce RTX 4090 Laptop GPU", true),
            (Nvidia, "NVIDIA GeForce RTX 5070", true),
            (Nvidia, "RTX 2080", false),
            (Nvidia, "GTX 1660", false),
            (Nvidia, "Quadro RTX 4000", false),
            (Amd, "AMD Radeon RX 6600 XT", true),
            (Amd, "AMD Radeon RX 7900 XTX", true),
            (Amd, "Radeon RX 9070 XT", true),
            (Amd, "RX 5700 XT", false),
            (Amd, "Radeon (TM) RX 480 Graphics", false),
            (Amd, "AMD Radeon(TM) RX Vega 10 Graphics", false),
            (Intel, "Intel(R) Arc(TM) A770 Graphics", true),
            (Intel, "Intel(R) Arc(TM) B580 Graphics", true),
            (Intel, "Intel(R) UHD Graphics 750", false),
            (Intel, "Intel(R) Arc(TM) Graphics", false),
            // The vendor decides which rule applies.
            (Amd, "NVIDIA GeForce RTX 3060 Ti", false),
            (Nvidia, "AMD Radeon RX 6600 XT", false),
            (Intel, "NVIDIA GeForce RTX 3060 Ti", false),
            (Virtual, "Parsec Virtual Display Adapter", false),
            (Other, "NVIDIA GeForce RTX 3060 Ti", false),
        ] {
            assert_eq!(uses_resizable_bar(vendor, name), uses, "{vendor:?} {name}");
        }
    }

    #[test]
    fn generation_labels() {
        assert_eq!(gen_label(1), "1.0");
        assert_eq!(gen_label(4), "4.0");
        assert_eq!(gen_label(6), "6.0");
    }

    fn measured(pnp_device_id: &str, gen: u8, at: &str) -> LinkMeasurement {
        LinkMeasurement {
            pnp_device_id: pnp_device_id.into(),
            link: PcieLink { gen, width: 16 },
            measured_at: at.into(),
            benchmark_id: format!("bench-{at}"),
        }
    }

    #[test]
    fn each_card_gets_its_newest_measurement() {
        let mut s = owner_pc_fixture();
        apply_links(
            &mut s,
            &[
                measured(RTX, 3, "2026-10-12T21:30:05Z"),
                measured(RTX, 4, "2026-10-13T08:15:00Z"),
                measured(RTX, 2, "2026-10-11T19:00:00Z"),
                // Newer, but measured on another card.
                measured(RADEON, 1, "2026-10-14T10:00:00Z"),
            ],
        );
        assert_eq!(
            s.gpus[1].link_under_load,
            Some(LinkUnderLoad {
                link: PcieLink { gen: 4, width: 16 },
                measured_at: "2026-10-13T08:15:00Z".into(),
                benchmark_id: "bench-2026-10-13T08:15:00Z".into(),
            })
        );
        // Parsec's virtual adapter has no measurement of its own.
        assert_eq!(s.gpus[0].link_under_load, None);
    }

    #[test]
    fn a_measurement_on_another_card_is_never_applied() {
        let mut s = owner_pc_fixture();
        apply_links(&mut s, &[measured(RADEON, 4, "2026-10-13T08:15:00Z")]);
        assert!(s.gpus.iter().all(|g| g.link_under_load.is_none()));
        apply_links(&mut s, &[]);
        assert_eq!(s, owner_pc_fixture());
    }

    #[test]
    fn card_ids_match_whatever_their_case() {
        let mut s = owner_pc_fixture();
        let lower = RTX.to_ascii_lowercase();
        apply_links(&mut s, &[measured(&lower, 4, "2026-10-13T08:15:00Z")]);
        assert_eq!(
            s.gpus[1].link_under_load.as_ref().map(|l| l.link.gen),
            Some(4)
        );
    }

    #[test]
    fn an_empty_card_id_never_matches() {
        let mut s = owner_pc_fixture();
        s.gpus[1].pnp_device_id.clear();
        apply_links(&mut s, &[measured("", 4, "2026-10-13T08:15:00Z")]);
        assert!(s.gpus.iter().all(|g| g.link_under_load.is_none()));
    }
}
