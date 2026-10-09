//! The BIOS guide: step-by-step help for settings Windows can't change, in this board's words.
use crate::health::BiosTopic;
use crate::snapshot::{CpuVendor, FormFactor, SystemSnapshot};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoardVendor {
    Asus,
    Msi,
    Gigabyte,
    Asrock,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideSection {
    pub topic: BiosTopic,
    pub title: String,
    pub why: String,
    pub steps: Vec<String>,
    pub caution: Option<String>,
    pub link: Option<GuideLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosGuide {
    pub vendor: BoardVendor,
    pub board: String,
    pub enter_key: String,
    pub tip: String,
    pub sections: Vec<GuideSection>,
}

pub fn board_vendor(manufacturer: &str) -> BoardVendor {
    let m = manufacturer.to_ascii_lowercase();
    if m.contains("asus") {
        BoardVendor::Asus
    } else if m.contains("micro-star") || m.contains("msi") {
        BoardVendor::Msi
    } else if m.contains("gigabyte") {
        BoardVendor::Gigabyte
    } else if m.contains("asrock") {
        BoardVendor::Asrock
    } else {
        BoardVendor::Other
    }
}

struct Ctx<'a> {
    vendor: BoardVendor,
    board: &'a str,
    amd: bool,
    tweaker: &'static str,
    profile: &'static str,
}

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn build_guide(s: &SystemSnapshot) -> BiosGuide {
    let vendor = board_vendor(&s.board.manufacturer);
    let product = s.board.product.trim();
    let board = if product.is_empty() {
        "your motherboard"
    } else {
        product
    };
    let upper = product.to_ascii_uppercase();
    let rog = ["MAXIMUS", "CROSSHAIR", "RAMPAGE", "ZENITH"]
        .iter()
        .any(|n| upper.contains(n));
    let amd = s.cpu.vendor == CpuVendor::Amd;
    let ctx = Ctx {
        vendor,
        board,
        amd,
        tweaker: if rog { "Extreme Tweaker" } else { "Ai Tweaker" },
        profile: if amd { "EXPO" } else { "XMP" },
    };

    let mut sections = vec![xmp(&ctx)];
    if s.cpu.vendor == CpuVendor::Intel && s.cpu.name.contains("11th Gen") {
        sections.push(memory_gear(&ctx));
    }
    sections.extend([update_bios(&ctx), iommu(&ctx), resizable_bar(&ctx)]);

    BiosGuide {
        vendor,
        board: board.into(),
        enter_key: enter_key(vendor, s.machine.form_factor).into(),
        tip: tip(vendor).into(),
        sections,
    }
}

fn enter_key(vendor: BoardVendor, form: FormFactor) -> &'static str {
    match (vendor, form) {
        (BoardVendor::Asus, FormFactor::Laptop) => "F2",
        (BoardVendor::Asus | BoardVendor::Msi | BoardVendor::Gigabyte, _) => "Del",
        (BoardVendor::Asrock, _) => "F2 or Del",
        (BoardVendor::Other, _) => "Del, F2 or F10 (the start-up screen shows which)",
    }
}

fn tip(vendor: BoardVendor) -> &'static str {
    match vendor {
        BoardVendor::Asus => "Press F7 to switch between EZ Mode and Advanced Mode. The Search button at the top finds any setting by name.",
        BoardVendor::Msi => "Press F7 to switch between EZ Mode and Advanced mode.",
        BoardVendor::Gigabyte => "Press F2 to switch between Easy Mode and Advanced Mode.",
        BoardVendor::Asrock => "Press F6 to switch between Easy Mode and Advanced Mode.",
        BoardVendor::Other => "Menu names differ between makers. Look for each setting by name in the BIOS menus.",
    }
}

fn xmp(c: &Ctx) -> GuideSection {
    let profile = c.profile;
    let steps = match c.vendor {
        BoardVendor::Asus => vec![
            "Restart and press Del to open the BIOS.".into(),
            "Press F7 for Advanced Mode.".into(),
            format!("Open the {} tab.", c.tweaker),
            if c.amd {
                "Set Ai Overclock Tuner to EXPO I (older boards call it D.O.C.P.).".into()
            } else {
                "Set Ai Overclock Tuner to XMP I.".into()
            },
            "Check that DRAM Frequency shows your kit's speed, then press F10 and choose Yes."
                .into(),
        ],
        BoardVendor::Msi => vec![
            "Restart and press Del to open the BIOS.".into(),
            format!("On the EZ Mode screen, switch on {profile} and pick Profile 1."),
            "Press F10 and choose Yes.".into(),
        ],
        BoardVendor::Gigabyte => vec![
            "Restart and press Del to open the BIOS.".into(),
            "Press F2 for Advanced Mode if needed, then open the Tweaker tab.".into(),
            if c.amd {
                "Set Extreme Memory Profile (X.M.P.) or EXPO to Profile1.".into()
            } else {
                "Set Extreme Memory Profile (X.M.P.) to Profile1.".into()
            },
            "Press F10 and choose Yes.".into(),
        ],
        BoardVendor::Asrock => v(&[
            "Restart and press F2 or Del to open the BIOS.",
            "Open OC Tweaker → DRAM Configuration.",
            "Set Load XMP Setting (EXPO on newer AMD boards) to Profile 1.",
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Other => vec![
            "Restart and open the BIOS (usually Del or F2 at start-up).".into(),
            format!("Find the memory overclocking or {profile} setting."),
            "Choose Profile 1, then save and exit.".into(),
        ],
    };
    GuideSection {
        topic: BiosTopic::Xmp,
        title: if c.amd { "Turn on EXPO (or DOCP)".into() } else { "Turn on XMP".into() },
        why: format!("Most RAM starts at a safe, slow default speed. {profile} runs it at the speed on the box, which can add a lot of FPS in CPU-heavy games like VALORANT."),
        steps,
        caution: Some("If the PC won't start, it usually retries with safe settings by itself. If it doesn't, clear the BIOS settings (Clear CMOS) and try a lower speed.".into()),
        link: None,
    }
}

fn memory_gear(c: &Ctx) -> GuideSection {
    let steps = match c.vendor {
        BoardVendor::Asus => vec![
            format!("On the {} tab, set Memory Controller : DRAM Frequency Ratio to 1:1.", c.tweaker),
            "If the PC is unstable at your kit's speed in 1:1, set DRAM Frequency to DDR4-3200 and keep 1:1.".into(),
            "Press F10 and choose Yes.".into(),
        ],
        _ => v(&[
            "Look for Gear Mode or Memory Controller : DRAM Frequency Ratio in the memory or overclocking settings.",
            "Choose Gear 1 (1:1).",
            "If the PC is unstable, lower DRAM Frequency to DDR4-3200 and keep Gear 1.",
            "Save and exit (usually F10).",
        ]),
    };
    GuideSection {
        topic: BiosTopic::MemoryGear,
        title: "Keep the memory controller in Gear 1".into(),
        why: "On 11th-gen Intel CPUs, Gear 1 runs the memory controller at full speed. Gear 2 adds delay that usually costs more FPS than the extra RAM speed gains.".into(),
        steps,
        caution: Some("Crashes or blue screens after this change mean the memory can't hold that speed in Gear 1. Lower the frequency.".into()),
        link: None,
    }
}

fn update_bios(c: &Ctx) -> GuideSection {
    let (maker, tool, url) = match c.vendor {
        BoardVendor::Asus => (
            "ASUS",
            "Tool → ASUS EZ Flash 3 Utility",
            Some("https://www.asus.com/support/download-center/"),
        ),
        BoardVendor::Msi => (
            "MSI",
            "M-FLASH",
            Some("https://www.msi.com/support/download"),
        ),
        BoardVendor::Gigabyte => (
            "GIGABYTE",
            "Q-Flash (F8)",
            Some("https://www.gigabyte.com/Support"),
        ),
        BoardVendor::Asrock => (
            "ASRock",
            "Tool → Instant Flash",
            Some("https://www.asrock.com/support/index.asp"),
        ),
        BoardVendor::Other => (
            "your motherboard maker",
            "the BIOS's built-in update tool",
            None,
        ),
    };
    GuideSection {
        topic: BiosTopic::UpdateBios,
        title: "Update the BIOS".into(),
        why: "Riot has asked some players to install BIOS updates released after December 2025 to fix a security flaw. Updates also improve memory compatibility.".into(),
        steps: vec![
            format!("Download the latest BIOS for {} from {maker} support and copy it to a USB stick.", c.board),
            format!("In the BIOS, open {tool}."),
            "Pick the file on the USB stick and confirm.".into(),
        ],
        caution: Some(format!("Don't turn off the PC while it updates. Updating resets BIOS settings, so turn {} back on afterwards.", c.profile)),
        link: url.map(|u| GuideLink { label: format!("Open {maker} support"), url: u.into() }),
    }
}

fn iommu(c: &Ctx) -> GuideSection {
    let steps = match (c.vendor, c.amd) {
        (BoardVendor::Asus, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → System Agent (SA) Configuration.",
            "Set VT-d to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, false) => v(&[
            "Press F7 for Advanced mode.",
            "Open OC → CPU Features.",
            "Set Intel VT-D Tech to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, false) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Settings → Miscellaneous.",
            "Set VT-d to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, false) => v(&[
            "Open Advanced → Chipset Configuration.",
            "Set VT-d to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, false) => v(&[
            "Find VT-d in the chipset or CPU settings.",
            "Set it to Enabled, then save and exit.",
        ]),
        (BoardVendor::Asus, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → AMD CBS → NBIO Common Options.",
            "Set IOMMU to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, true) => v(&[
            "Press F7 for Advanced mode.",
            "Open Settings → Advanced → AMD CBS → NBIO Common Options.",
            "Set IOMMU to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, true) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Settings → Miscellaneous.",
            "Set IOMMU to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, true) => v(&[
            "Open Advanced → AMD CBS → NBIO Common Options.",
            "Set IOMMU to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, true) => v(&[
            "Find IOMMU (sometimes called AMD-Vi) in the CPU or chipset settings.",
            "Set it to Enabled, then save and exit.",
        ]),
    };
    GuideSection {
        topic: BiosTopic::Iommu,
        title: "Turn on IOMMU (only if you need it)".into(),
        why: "Vanguard's optional On-Demand mode needs it, and Riot can ask for it on some accounts. It doesn't change FPS.".into(),
        steps,
        caution: Some("Menu names vary between boards and BIOS versions. If you can't find it, search the BIOS for VT-d or IOMMU.".into()),
        link: None,
    }
}

fn resizable_bar(c: &Ctx) -> GuideSection {
    let steps = match c.vendor {
        BoardVendor::Asus => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → PCI Subsystem Settings.",
            "Set Above 4G Decoding to Enabled, then Re-Size BAR Support to Auto.",
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Msi => v(&[
            "Press F7 for Advanced mode.",
            "Open Settings → Advanced → PCI Subsystem Settings.",
            "Enable Above 4G memory, then Re-Size BAR Support.",
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Gigabyte => v(&[
            "Press F2 for Advanced Mode.",
            "Open Settings → IO Ports.",
            "Set Above 4G Decoding and Re-Size BAR Support to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Asrock => v(&[
            "Open Advanced → Chipset Configuration.",
            "Enable Above 4G Decoding and Re-Size BAR Support (C.A.M. on some AMD boards).",
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Other => v(&[
            "Find Above 4G Decoding and Resizable BAR in the PCI or chipset settings.",
            "Enable both, then save and exit.",
        ]),
    };
    GuideSection {
        topic: BiosTopic::ResizableBar,
        title: "Resizable BAR (optional)".into(),
        why: "Lets the CPU reach all of the graphics card's memory at once. VALORANT gains little, but some other games do.".into(),
        steps,
        caution: Some("Windows must start in UEFI mode (CSM off) for this to work.".into()),
        link: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{owner_pc_fixture, CpuVendor};

    fn section(g: &BiosGuide, topic: BiosTopic) -> &GuideSection {
        g.sections.iter().find(|s| s.topic == topic).unwrap()
    }

    #[test]
    fn owner_pc_gets_asus_extreme_tweaker_steps() {
        let g = build_guide(&owner_pc_fixture());
        assert_eq!(g.vendor, BoardVendor::Asus);
        assert_eq!(g.board, "ROG MAXIMUS XIII HERO");
        assert_eq!(g.enter_key, "Del");
        let topics: Vec<BiosTopic> = g.sections.iter().map(|s| s.topic).collect();
        assert_eq!(
            topics,
            vec![
                BiosTopic::Xmp,
                BiosTopic::MemoryGear,
                BiosTopic::UpdateBios,
                BiosTopic::Iommu,
                BiosTopic::ResizableBar
            ]
        );
        let xmp = section(&g, BiosTopic::Xmp).steps.join(" ");
        assert!(
            xmp.contains("Extreme Tweaker") && xmp.contains("XMP I"),
            "{xmp}"
        );
        assert!(section(&g, BiosTopic::MemoryGear)
            .steps
            .join(" ")
            .contains("1:1"));
    }

    #[test]
    fn asus_non_rog_board_uses_ai_tweaker() {
        let mut s = owner_pc_fixture();
        s.board.product = "PRIME Z590-A".into();
        assert!(section(&build_guide(&s), BiosTopic::Xmp)
            .steps
            .join(" ")
            .contains("Ai Tweaker"));
    }

    #[test]
    fn amd_uses_expo_and_skips_gear() {
        let mut s = owner_pc_fixture();
        s.cpu.vendor = CpuVendor::Amd;
        s.cpu.name = "AMD Ryzen 7 7800X3D 8-Core Processor".into();
        let g = build_guide(&s);
        let xmp = section(&g, BiosTopic::Xmp);
        assert_eq!(xmp.title, "Turn on EXPO (or DOCP)");
        assert!(xmp.steps.join(" ").contains("EXPO I"));
        assert!(g.sections.iter().all(|s| s.topic != BiosTopic::MemoryGear));
        assert!(section(&g, BiosTopic::Iommu)
            .steps
            .join(" ")
            .contains("AMD CBS"));
    }

    #[test]
    fn vendor_detection() {
        assert_eq!(board_vendor("ASUSTeK COMPUTER INC."), BoardVendor::Asus);
        assert_eq!(
            board_vendor("Micro-Star International Co., Ltd."),
            BoardVendor::Msi
        );
        assert_eq!(
            board_vendor("Gigabyte Technology Co., Ltd."),
            BoardVendor::Gigabyte
        );
        assert_eq!(board_vendor("ASRock"), BoardVendor::Asrock);
        assert_eq!(board_vendor("Dell Inc."), BoardVendor::Other);
    }

    #[test]
    fn every_vendor_and_cpu_gets_complete_sections() {
        for maker in [
            "ASUSTeK COMPUTER INC.",
            "Micro-Star International Co., Ltd.",
            "Gigabyte Technology Co., Ltd.",
            "ASRock",
            "Dell Inc.",
        ] {
            for (vendor, name) in [
                (CpuVendor::Intel, "11th Gen Intel(R) Core(TM) i5-11400F"),
                (CpuVendor::Amd, "AMD Ryzen 5 5600X"),
            ] {
                let mut s = owner_pc_fixture();
                s.board.manufacturer = maker.into();
                s.cpu.vendor = vendor;
                s.cpu.name = name.into();
                for sec in build_guide(&s).sections {
                    assert!(sec.steps.len() >= 2, "{maker} {name} {:?}", sec.topic);
                    assert!(!sec.title.is_empty() && !sec.why.is_empty());
                }
            }
        }
    }

    #[test]
    fn update_section_names_the_board_and_links_support() {
        let g = build_guide(&owner_pc_fixture());
        let update = section(&g, BiosTopic::UpdateBios);
        assert!(update.steps[0].contains("ROG MAXIMUS XIII HERO"));
        assert!(update.link.as_ref().unwrap().url.contains("asus.com"));
    }

    #[test]
    fn unknown_board_name_reads_naturally() {
        let mut s = owner_pc_fixture();
        s.board.product = String::new();
        assert_eq!(build_guide(&s).board, "your motherboard");
    }
}
