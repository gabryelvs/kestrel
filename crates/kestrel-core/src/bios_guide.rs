//! The BIOS guide: step-by-step help for settings Windows can't change, in this board's words.
use crate::hardware::is_placeholder;
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

/// The board's maker. BIOS filler such as "To be filled by O.E.M." never names one.
pub fn board_vendor(manufacturer: &str) -> BoardVendor {
    if is_placeholder(manufacturer) {
        return BoardVendor::Other;
    }
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

const XMP: &str = "XMP";
const EXPO: &str = "EXPO";
const DOCP: &str = "D.O.C.P.";
const A_XMP: &str = "A-XMP";

/// What this PC's BIOS calls the one-click memory speed profile. Intel boards say XMP. EXPO is AMD's name on AM5,
/// which takes DDR5. On AM4 (DDR4) each board maker names it: ASUS D.O.C.P., MSI A-XMP, Gigabyte and ASRock XMP.
/// An AMD PC whose memory generation isn't known is taken to be AM4 and goes by its board maker too.
pub fn memory_profile_name(s: &SystemSnapshot) -> &'static str {
    if s.cpu.vendor != CpuVendor::Amd {
        return XMP;
    }
    if s.memory.ddr_generation == Some(5) {
        return EXPO;
    }
    match board_vendor(&s.board.manufacturer) {
        BoardVendor::Asus => DOCP,
        BoardVendor::Msi => A_XMP,
        _ => XMP,
    }
}

struct Ctx<'a> {
    vendor: BoardVendor,
    board: &'a str,
    amd: bool,
    tweaker: &'static str,
    profile: &'static str,
    /// The key the page header names for opening the BIOS.
    enter_key: &'static str,
    /// 11th-gen Intel: the guide has the Gear 1 topic, and a BIOS update resets the gear.
    gear1: bool,
    /// A laptop has no graphics slot or M.2 drive to move: its design sets the card's lanes.
    laptop: bool,
}

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn build_guide(s: &SystemSnapshot) -> BiosGuide {
    let vendor = board_vendor(&s.board.manufacturer);
    let product = s.board.product.trim();
    let board = if is_placeholder(product) {
        "your motherboard"
    } else {
        product
    };
    let upper = product.to_ascii_uppercase();
    let rog = ["MAXIMUS", "CROSSHAIR", "RAMPAGE", "ZENITH"]
        .iter()
        .any(|n| upper.contains(n));
    let amd = s.cpu.vendor == CpuVendor::Amd;
    let key = enter_key(vendor, s.machine.form_factor);
    let ctx = Ctx {
        vendor,
        board,
        amd,
        tweaker: if rog { "Extreme Tweaker" } else { "Ai Tweaker" },
        profile: memory_profile_name(s),
        enter_key: key,
        gear1: s.cpu.vendor == CpuVendor::Intel && s.cpu.name.contains("11th Gen"),
        laptop: s.machine.form_factor == FormFactor::Laptop,
    };

    let mut sections = vec![xmp(&ctx)];
    if ctx.gear1 {
        sections.push(memory_gear(&ctx));
    }
    sections.extend([
        update_bios(&ctx),
        pcie_slot(&ctx),
        resizable_bar(&ctx),
        virtualization(&ctx),
        iommu(&ctx),
    ]);

    BiosGuide {
        vendor,
        board: board.into(),
        enter_key: key.into(),
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
        BoardVendor::Msi => "Press F7 to switch between EZ Mode and Advanced Mode.",
        BoardVendor::Gigabyte => "Press F2 to switch between Easy Mode and Advanced Mode.",
        BoardVendor::Asrock => "Press F6 to switch between Easy Mode and Advanced Mode.",
        BoardVendor::Other => "Menu names differ between makers. Look for each setting by name in the BIOS menus.",
    }
}

/// The step that opens the BIOS names the page header's key, so an ASUS laptop says F2, not Del.
fn open_bios(c: &Ctx) -> String {
    match c.vendor {
        BoardVendor::Other => "Restart and open the BIOS (usually Del or F2 at start-up).".into(),
        _ => format!("Restart and press {} to open the BIOS.", c.enter_key),
    }
}

fn xmp(c: &Ctx) -> GuideSection {
    let profile = c.profile;
    let steps = match c.vendor {
        BoardVendor::Asus => vec![
            open_bios(c),
            "Press F7 for Advanced Mode.".into(),
            format!("Open the {} tab.", c.tweaker),
            match profile {
                EXPO => "Set Ai Overclock Tuner to EXPO I.".into(),
                DOCP => "Set Ai Overclock Tuner to D.O.C.P. (if it then lists profiles, pick the one that matches your kit's speed).".into(),
                _ => "Set Ai Overclock Tuner to XMP I.".into(),
            },
            "Check that DRAM Frequency shows your kit's speed, then press F10 and choose Yes."
                .into(),
        ],
        BoardVendor::Msi => vec![
            open_bios(c),
            format!("On the EZ Mode screen, switch on {profile} and pick Profile 1."),
            "Press F10 and choose Yes.".into(),
        ],
        BoardVendor::Gigabyte => vec![
            open_bios(c),
            "Press F2 for Advanced Mode if needed, then open the Tweaker tab.".into(),
            if profile == EXPO {
                "Set Extreme Memory Profile (X.M.P.) or EXPO to Profile1.".into()
            } else {
                "Set Extreme Memory Profile (X.M.P.) to Profile1.".into()
            },
            "Press F10 and choose Yes.".into(),
        ],
        BoardVendor::Asrock => vec![
            open_bios(c),
            "Open OC Tweaker → DRAM Configuration.".into(),
            format!("Set Load {profile} Setting to Profile 1."),
            "Press F10 and choose Yes.".into(),
        ],
        BoardVendor::Other => vec![
            open_bios(c),
            format!("Find the memory overclocking or {profile} setting."),
            "Choose Profile 1, then save and exit.".into(),
        ],
    };
    GuideSection {
        topic: BiosTopic::Xmp,
        title: format!("Turn on {profile}"),
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
    let mut caution = format!("Don't turn off the PC while it updates. Updating resets BIOS settings, so turn {} back on afterwards.", c.profile);
    if c.gear1 {
        caution.push_str(
            " On 11th-gen Intel the memory gear goes back to its default too, so set Gear 1 again.",
        );
    }
    GuideSection {
        topic: BiosTopic::UpdateBios,
        title: "Update the BIOS".into(),
        why: "Riot has asked some players to install BIOS updates released after December 2025 to fix a security flaw. Updates also improve memory compatibility.".into(),
        steps: vec![
            format!("Download the latest BIOS for {} from {maker} support and copy it to a USB stick.", c.board),
            format!("In the BIOS, open {tool}."),
            "Pick the file on the USB stick and confirm.".into(),
        ],
        caution: Some(caution),
        link: url.map(|u| GuideLink { label: format!("Open {maker} support"), url: u.into() }),
    }
}

/// The card's slot is checked first, while the PC is off; then the slot's speed setting goes back to Auto. A laptop has
/// no slot to check and no M.2 drive to move, so only the speed setting is left.
fn pcie_slot(c: &Ctx) -> GuideSection {
    GuideSection {
        topic: BiosTopic::PcieSlot,
        title: "Run the graphics card at full PCIe speed".into(),
        why: "The card should use all its lanes (usually x16) at the PCIe generation it and the slot support. It drops when it sits in a lower slot, shares lanes with an M.2 drive, or the slot is set to an older generation.".into(),
        steps: if c.laptop {
            laptop_pcie_steps(c)
        } else {
            desktop_pcie_steps(c)
        },
        caution: Some("Menu names vary between boards and BIOS versions. Your board's manual lists which slots and M.2 drives share lanes.".into()),
        link: None,
    }
}

// Menu paths checked in October 2026 against: ASUS BIOS manuals E18567 (ROG MAXIMUS XIII HERO), E19474 (Intel 600),
// E21631 (ROG STRIX B760), E25827 (Intel 800), E17678 (ROG STRIX B550-XE), E21968 (AMD 600), E25269 (AMD 800) and
// FAQ 1055579; MSI BIOS user guides Intel 500/600/700/800, AMD X570-B550, AM5 and AM5 800; GIGABYTE BIOS Setup
// Intel 600/700/800 and AMD X670-B650; ASRock Z590 Pro4 manual, Software/BIOS Setup Guides Intel Z790, Intel 800,
// AMD X670 and AMD 800. Older boards name the slot setting differently, hence "or similar".
fn desktop_pcie_steps(c: &Ctx) -> Vec<String> {
    let mut steps = v(&[
        "Shut down the PC and check that the graphics card sits in the top long slot (the one closest to the CPU), pushed fully in with the slot's latch closed.",
        "If your board's manual says an M.2 slot shares lanes with the graphics card's slot, move that drive to another M.2 slot.",
        "Start the PC and open the BIOS.",
    ]);
    steps.extend(match (c.vendor, c.amd) {
        (BoardVendor::Asus, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → System Agent (SA) Configuration → PCI Express Configuration.",
            "Set the top x16 slot's link speed (PCIEX16_1 Link Speed or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asus, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → Onboard Devices Configuration, then PCIE Link Speed if your board lists it.",
            "Set the top x16 slot's link mode (PCIEX16_1 Link Mode or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Settings → Advanced → PCIe/PCI Sub-system Settings (on newer boards, Advanced → PCIe Sub-system Settings).",
            "Set the top x16 slot's speed (PCIE_E1 - Max Link Speed, PEG0 - Max Link Speed or PCI_E1 Gen Mode, depending on the board) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Settings → Advanced → PCIe/PCI Sub-system Settings (on newer boards, Advanced → PCIe Sub-system Settings).",
            "Set the top x16 slot's speed (PCI_E1 Gen Switch or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, false) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Settings → Miscellaneous (on newer boards, Settings → IO Ports → PCIe Link Speed Configuration).",
            "Set CPU PCIe Link Speed to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, true) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Settings → Miscellaneous (on newer boards, Settings → IO Ports → PCIe Link Speed Configuration).",
            "Set the top x16 slot's link speed (PCIEX16 Slot Link Speed or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, false) => v(&[
            "Open Advanced → Chipset Configuration.",
            "Set the top x16 slot's link speed (PCIE1 Link Speed or PCIE2 Link Speed, by the slot's number in your manual) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, true) => v(&[
            "Open Advanced → AMD PBS.",
            "Set PCIe x16 Link Speed to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, _) => v(&[
            "Find the link speed setting for the top x16 slot, usually with the PCIe, chipset or onboard device settings.",
            "Set it to Auto, then save and exit.",
        ]),
    });
    steps
}

/// The BIOS opened with the maker's keys, as in the other topics, then any PCIe speed setting back to Auto.
fn laptop_pcie_steps(c: &Ctx) -> Vec<String> {
    const AUTO: &str = "If the BIOS has a PCIe speed setting, set it to Auto.";
    let mut steps = vec![
        "Laptops have no graphics slot to reseat: the laptop's design sets the lanes.".into(),
        open_bios(c),
    ];
    steps.extend(match c.vendor {
        BoardVendor::Asus | BoardVendor::Msi => v(&[
            "Press F7 for Advanced Mode.",
            AUTO,
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Gigabyte => v(&[
            "Press F2 for Advanced Mode.",
            AUTO,
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Asrock => v(&[AUTO, "Press F10 and choose Yes."]),
        BoardVendor::Other => v(&[AUTO, "Save and exit (usually F10)."]),
    });
    steps
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
            "Press F7 for Advanced Mode.",
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
            "Press F7 for Advanced Mode.",
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
            "Press F7 for Advanced Mode.",
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

// Menu paths checked in October 2026 against: ASUS FAQ 1045141 and the ASUS BIOS manuals above (Advanced → CPU
// Configuration); MSI BIOS user guides Intel 500/600/700 (OC → CPU Features), Intel 800 (Overclocking → CPU
// Features), AMD X570-B550 and AM5 (OC → Advanced CPU Configuration → SVM Mode), AM5 800 (… → AMD CBS → SVM Enable);
// GIGABYTE BIOS Setup Intel 700 for X boards and AMD X670-B650 (Tweaker → Advanced CPU Settings); ASRock FAQ 405,
// Z590 Pro4 and B550 Pro4 manuals, Software/BIOS Setup Guides Intel Z790, Intel 800 and AMD X670 (CPU Configuration).
/// VT-x on Intel, SVM on AMD. Windows needs it for Memory Integrity; it doesn't change FPS.
fn virtualization(c: &Ctx) -> GuideSection {
    let steps = match (c.vendor, c.amd) {
        (BoardVendor::Asus, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → CPU Configuration.",
            "Set Intel (VMX) Virtualization Technology to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open OC → CPU Features (on newer boards, Overclocking → CPU Features).",
            "Set Intel Virtualization Tech to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, false) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Tweaker → Advanced CPU Settings.",
            "Set Intel (VMX) Virtualization Technology to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, false) => v(&[
            "Open Advanced → CPU Configuration.",
            "Set Intel Virtualization Technology to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, false) => v(&[
            "Find Intel Virtualization Technology (VT-x) in the CPU settings.",
            "Set it to Enabled, then save and exit.",
        ]),
        (BoardVendor::Asus, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → CPU Configuration.",
            "Set SVM Mode to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open OC → Advanced CPU Configuration (on newer boards, Overclocking → Advanced CPU Configuration → AMD CBS).",
            "Set SVM Mode (SVM Enable on newer boards) to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, true) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Tweaker → Advanced CPU Settings.",
            "Set SVM Mode to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, true) => v(&[
            "Open Advanced → CPU Configuration.",
            "Set SVM Mode to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, true) => v(&[
            "Find SVM Mode (AMD-V) in the CPU settings.",
            "Set it to Enabled, then save and exit.",
        ]),
    };
    GuideSection {
        topic: BiosTopic::Virtualization,
        title: "Turn on virtualization (only if you need it)".into(),
        why: "Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It doesn't change FPS.".into(),
        steps,
        caution: Some("Menu names vary between boards and BIOS versions. If you can't find it, search the BIOS for SVM or Virtualization.".into()),
        link: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{owner_pc_fixture, CpuVendor, FormFactor};

    fn section(g: &BiosGuide, topic: BiosTopic) -> &GuideSection {
        g.sections.iter().find(|s| s.topic == topic).unwrap()
    }

    const MAKERS: [&str; 5] = [
        "ASUSTeK COMPUTER INC.",
        "Micro-Star International Co., Ltd.",
        "Gigabyte Technology Co., Ltd.",
        "ASRock",
        "Dell Inc.",
    ];

    /// The owner's PC on a board made by `maker`, with an Intel CPU (the owner's 11th-gen) or an AM5 Ryzen.
    fn pc(maker: &str, cpu: CpuVendor) -> SystemSnapshot {
        let mut s = owner_pc_fixture();
        s.board.manufacturer = maker.into();
        s.cpu.vendor = cpu;
        if cpu == CpuVendor::Amd {
            s.cpu.name = "AMD Ryzen 7 7800X3D 8-Core Processor".into();
            s.memory.ddr_generation = Some(5);
        }
        s
    }

    fn topics(g: &BiosGuide) -> Vec<BiosTopic> {
        g.sections.iter().map(|s| s.topic).collect()
    }

    #[test]
    fn owner_pc_gets_asus_extreme_tweaker_steps() {
        let g = build_guide(&owner_pc_fixture());
        assert_eq!(g.vendor, BoardVendor::Asus);
        assert_eq!(g.board, "ROG MAXIMUS XIII HERO");
        assert_eq!(g.enter_key, "Del");
        assert_eq!(
            topics(&g),
            vec![
                BiosTopic::Xmp,
                BiosTopic::MemoryGear,
                BiosTopic::UpdateBios,
                BiosTopic::PcieSlot,
                BiosTopic::ResizableBar,
                BiosTopic::Virtualization,
                BiosTopic::Iommu
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
        s.memory.ddr_generation = Some(5);
        let g = build_guide(&s);
        let xmp = section(&g, BiosTopic::Xmp);
        assert_eq!(xmp.title, "Turn on EXPO");
        assert!(xmp.steps.join(" ").contains("EXPO I"));
        assert!(g.sections.iter().all(|s| s.topic != BiosTopic::MemoryGear));
        assert!(section(&g, BiosTopic::Iommu)
            .steps
            .join(" ")
            .contains("AMD CBS"));
    }

    /// A Ryzen 5000 on DDR4, made by `maker`, with the owner's PC for everything else.
    fn am4_pc(maker: &str) -> SystemSnapshot {
        let mut s = owner_pc_fixture();
        s.cpu.vendor = CpuVendor::Amd;
        s.cpu.name = "AMD Ryzen 5 5600X 6-Core Processor".into();
        s.board.manufacturer = maker.into();
        s.memory.ddr_generation = Some(4);
        s
    }

    #[test]
    fn am4_asus_board_calls_it_docp_not_expo() {
        let g = build_guide(&am4_pc("ASUSTeK COMPUTER INC."));
        let xmp = section(&g, BiosTopic::Xmp);
        assert_eq!(xmp.title, "Turn on D.O.C.P.");
        let steps = xmp.steps.join(" ");
        assert!(steps.contains("Ai Overclock Tuner to D.O.C.P."), "{steps}");
        assert!(!steps.contains("EXPO") && !xmp.why.contains("EXPO"));
        let caution = section(&g, BiosTopic::UpdateBios).caution.clone().unwrap();
        assert!(caution.contains("turn D.O.C.P. back on"), "{caution}");
    }

    #[test]
    fn am4_msi_board_calls_it_a_xmp_not_expo() {
        let g = build_guide(&am4_pc("Micro-Star International Co., Ltd."));
        let xmp = section(&g, BiosTopic::Xmp);
        assert_eq!(xmp.title, "Turn on A-XMP");
        let steps = xmp.steps.join(" ");
        assert!(steps.contains("switch on A-XMP"), "{steps}");
        assert!(!steps.contains("EXPO"));
    }

    #[test]
    fn am4_gigabyte_and_asrock_boards_say_xmp() {
        for maker in ["Gigabyte Technology Co., Ltd.", "ASRock"] {
            let g = build_guide(&am4_pc(maker));
            let xmp = section(&g, BiosTopic::Xmp);
            assert_eq!(xmp.title, "Turn on XMP", "{maker}");
            assert!(!xmp.steps.join(" ").contains("EXPO"), "{maker}");
        }
    }

    #[test]
    fn am5_steps_name_expo_on_every_board() {
        for maker in [
            "ASUSTeK COMPUTER INC.",
            "Micro-Star International Co., Ltd.",
            "Gigabyte Technology Co., Ltd.",
            "ASRock",
            "Dell Inc.",
        ] {
            let mut s = am4_pc(maker);
            s.memory.ddr_generation = Some(5);
            let xmp = section(&build_guide(&s), BiosTopic::Xmp).clone();
            assert_eq!(xmp.title, "Turn on EXPO", "{maker}");
            assert!(xmp.steps.join(" ").contains("EXPO"), "{maker}");
        }
    }

    #[test]
    fn memory_profile_names() {
        let name = |vendor, ddr, maker: &str| {
            let mut s = owner_pc_fixture();
            s.cpu.vendor = vendor;
            s.memory.ddr_generation = ddr;
            s.board.manufacturer = maker.into();
            memory_profile_name(&s)
        };
        let makers = [
            "ASUSTeK COMPUTER INC.",
            "Micro-Star International Co., Ltd.",
            "Gigabyte Technology Co., Ltd.",
            "ASRock",
            "Dell Inc.",
        ];
        for maker in makers {
            // Intel is XMP, and AMD on DDR5 is EXPO, whoever made the board.
            for ddr in [Some(4), Some(5), None] {
                assert_eq!(name(CpuVendor::Intel, ddr, maker), "XMP", "{maker}");
            }
            assert_eq!(name(CpuVendor::Amd, Some(5), maker), "EXPO", "{maker}");
        }
        // AMD on DDR4, or when the generation isn't known, goes by the board maker.
        for ddr in [Some(4), None] {
            assert_eq!(name(CpuVendor::Amd, ddr, makers[0]), "D.O.C.P.");
            assert_eq!(name(CpuVendor::Amd, ddr, makers[1]), "A-XMP");
            for maker in &makers[2..] {
                assert_eq!(name(CpuVendor::Amd, ddr, maker), "XMP", "{maker}");
            }
        }
        // No known CPU maker: the common name.
        assert_eq!(name(CpuVendor::Other, Some(4), makers[0]), "XMP");
    }

    #[test]
    fn the_owners_intel_pc_still_reads_xmp() {
        let g = build_guide(&owner_pc_fixture());
        let xmp = section(&g, BiosTopic::Xmp);
        assert_eq!(xmp.title, "Turn on XMP");
        assert!(xmp.why.contains("XMP runs it"), "{}", xmp.why);
        let caution = section(&g, BiosTopic::UpdateBios).caution.clone().unwrap();
        assert!(caution.contains("turn XMP back on"), "{caution}");
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

    #[test]
    fn topics_run_memory_update_pcie_bar_virtualization_iommu() {
        use BiosTopic::*;
        // 11th-gen Intel (the owner's i9-11900K) gets the Gear 1 topic right after XMP.
        assert_eq!(
            topics(&build_guide(&owner_pc_fixture())),
            vec![
                Xmp,
                MemoryGear,
                UpdateBios,
                PcieSlot,
                ResizableBar,
                Virtualization,
                Iommu
            ]
        );
        assert_eq!(
            topics(&build_guide(&pc(MAKERS[0], CpuVendor::Amd))),
            vec![
                Xmp,
                UpdateBios,
                PcieSlot,
                ResizableBar,
                Virtualization,
                Iommu
            ]
        );
    }

    #[test]
    fn pcie_slot_section_says_why_and_where_lanes_are_shared() {
        for maker in MAKERS {
            for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
                let g = build_guide(&pc(maker, cpu));
                let pcie = section(&g, BiosTopic::PcieSlot);
                assert_eq!(pcie.title, "Run the graphics card at full PCIe speed");
                assert_eq!(pcie.why, "The card should use all its lanes (usually x16) at the PCIe generation it and the slot support. It drops when it sits in a lower slot, shares lanes with an M.2 drive, or the slot is set to an older generation.");
                assert_eq!(
                    pcie.caution.as_deref(),
                    Some("Menu names vary between boards and BIOS versions. Your board's manual lists which slots and M.2 drives share lanes.")
                );
                assert_eq!(pcie.link, None);
            }
        }
    }

    #[test]
    fn pcie_slot_steps_check_the_slot_then_set_its_speed_to_auto() {
        let cases: [(&str, CpuVendor, &[&str]); 10] = [
            (
                MAKERS[0],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "Advanced → System Agent (SA) Configuration → PCI Express Configuration",
                    "PCIEX16_1 Link Speed",
                ],
            ),
            (
                MAKERS[0],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "Advanced → Onboard Devices Configuration",
                    "PCIEX16_1 Link Mode",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "Settings → Advanced → PCIe/PCI Sub-system Settings",
                    "PCIE_E1 - Max Link Speed",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "Settings → Advanced → PCIe/PCI Sub-system Settings",
                    "PCI_E1 Gen Switch",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Intel,
                &[
                    "Press F2 for Advanced Mode.",
                    "Settings → Miscellaneous",
                    "CPU PCIe Link Speed",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Amd,
                &[
                    "Press F2 for Advanced Mode.",
                    "Settings → Miscellaneous",
                    "PCIEX16 Slot Link Speed",
                ],
            ),
            (
                MAKERS[3],
                CpuVendor::Intel,
                &["Advanced → Chipset Configuration", "PCIE1 Link Speed"],
            ),
            (
                MAKERS[3],
                CpuVendor::Amd,
                &["Advanced → AMD PBS", "PCIe x16 Link Speed"],
            ),
            (
                MAKERS[4],
                CpuVendor::Intel,
                &["link speed setting for the top x16 slot"],
            ),
            (
                MAKERS[4],
                CpuVendor::Amd,
                &["link speed setting for the top x16 slot"],
            ),
        ];
        for (maker, cpu, expected) in cases {
            let steps = section(&build_guide(&pc(maker, cpu)), BiosTopic::PcieSlot)
                .steps
                .clone();
            let all = steps.join(" ");
            // The card goes in the top long slot first, and an M.2 drive that shares its lanes moves elsewhere.
            assert!(steps[0].contains("top long slot"), "{maker}: {all}");
            assert!(
                steps[1].contains("M.2") && steps[1].contains("shares lanes"),
                "{maker}: {all}"
            );
            for text in expected {
                assert!(all.contains(text), "{maker} {cpu:?} lacks {text:?}: {all}");
            }
            assert!(all.contains("to Auto"), "{maker} {cpu:?}: {all}");
            let last = steps.last().unwrap();
            assert!(
                last == "Press F10 and choose Yes." || last.ends_with("then save and exit."),
                "{maker}: {last}"
            );
        }
    }

    #[test]
    fn a_laptops_pcie_steps_have_no_slot_or_m2_to_change() {
        const NO_SLOT: &str =
            "Laptops have no graphics slot to reseat: the laptop's design sets the lanes.";
        const AUTO: &str = "If the BIOS has a PCIe speed setting, set it to Auto.";
        const SAVE: &str = "Press F10 and choose Yes.";
        let cases: [(&str, &[&str]); 5] = [
            (
                MAKERS[0],
                &[
                    NO_SLOT,
                    "Restart and press F2 to open the BIOS.",
                    "Press F7 for Advanced Mode.",
                    AUTO,
                    SAVE,
                ],
            ),
            (
                MAKERS[1],
                &[
                    NO_SLOT,
                    "Restart and press Del to open the BIOS.",
                    "Press F7 for Advanced Mode.",
                    AUTO,
                    SAVE,
                ],
            ),
            (
                MAKERS[2],
                &[
                    NO_SLOT,
                    "Restart and press Del to open the BIOS.",
                    "Press F2 for Advanced Mode.",
                    AUTO,
                    SAVE,
                ],
            ),
            (
                MAKERS[3],
                &[
                    NO_SLOT,
                    "Restart and press F2 or Del to open the BIOS.",
                    AUTO,
                    SAVE,
                ],
            ),
            (
                MAKERS[4],
                &[
                    NO_SLOT,
                    "Restart and open the BIOS (usually Del or F2 at start-up).",
                    AUTO,
                    "Save and exit (usually F10).",
                ],
            ),
        ];
        for (maker, expected) in cases {
            for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
                let mut s = pc(maker, cpu);
                s.machine.form_factor = FormFactor::Laptop;
                let steps = section(&build_guide(&s), BiosTopic::PcieSlot).steps.clone();
                assert_eq!(steps, expected, "{maker} {cpu:?}");
                // Only the first step speaks of a slot, to say there is none; none speaks of an M.2 drive.
                for step in &steps[1..] {
                    assert!(
                        !step.contains("slot") && !step.contains("M.2"),
                        "{maker} {cpu:?}: {step}"
                    );
                }
            }
        }
        // A PC whose form factor Windows didn't report keeps the desktop's slot and M.2 steps.
        let mut unknown = owner_pc_fixture();
        unknown.machine.form_factor = FormFactor::Unknown;
        let steps = section(&build_guide(&unknown), BiosTopic::PcieSlot)
            .steps
            .clone();
        assert!(steps[0].contains("top long slot"), "{steps:?}");
        assert!(steps[1].contains("M.2"), "{steps:?}");
    }

    #[test]
    fn virtualization_section_says_why_and_how_to_find_it() {
        for maker in MAKERS {
            for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
                let g = build_guide(&pc(maker, cpu));
                let virt = section(&g, BiosTopic::Virtualization);
                assert_eq!(virt.title, "Turn on virtualization (only if you need it)");
                assert_eq!(virt.why, "Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It doesn't change FPS.");
                assert_eq!(
                    virt.caution.as_deref(),
                    Some("Menu names vary between boards and BIOS versions. If you can't find it, search the BIOS for SVM or Virtualization.")
                );
                assert_eq!(virt.link, None);
            }
        }
    }

    #[test]
    fn virtualization_steps_name_each_boards_setting() {
        let cases: [(&str, CpuVendor, &[&str]); 10] = [
            (
                MAKERS[0],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "Open Advanced → CPU Configuration.",
                    "Set Intel (VMX) Virtualization Technology to Enabled.",
                ],
            ),
            (
                MAKERS[0],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "Open Advanced → CPU Configuration.",
                    "Set SVM Mode to Enabled.",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "OC → CPU Features",
                    "Set Intel Virtualization Tech to Enabled.",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "OC → Advanced CPU Configuration",
                    "SVM Mode",
                    "SVM Enable",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Intel,
                &[
                    "Press F2 for Advanced Mode.",
                    "Open Tweaker → Advanced CPU Settings.",
                    "Set Intel (VMX) Virtualization Technology to Enabled.",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Amd,
                &[
                    "Press F2 for Advanced Mode.",
                    "Open Tweaker → Advanced CPU Settings.",
                    "Set SVM Mode to Enabled.",
                ],
            ),
            (
                MAKERS[3],
                CpuVendor::Intel,
                &[
                    "Open Advanced → CPU Configuration.",
                    "Set Intel Virtualization Technology to Enabled.",
                ],
            ),
            (
                MAKERS[3],
                CpuVendor::Amd,
                &[
                    "Open Advanced → CPU Configuration.",
                    "Set SVM Mode to Enabled.",
                ],
            ),
            (
                MAKERS[4],
                CpuVendor::Intel,
                &["Intel Virtualization Technology (VT-x)"],
            ),
            (MAKERS[4], CpuVendor::Amd, &["SVM Mode (AMD-V)"]),
        ];
        for (maker, cpu, expected) in cases {
            let steps = section(&build_guide(&pc(maker, cpu)), BiosTopic::Virtualization)
                .steps
                .join(" ");
            for text in expected {
                assert!(
                    steps.contains(text),
                    "{maker} {cpu:?} lacks {text:?}: {steps}"
                );
            }
            // An Intel PC is never sent to the AMD setting, nor the other way round.
            let wrong = if cpu == CpuVendor::Amd {
                "Intel"
            } else {
                "SVM"
            };
            assert!(!steps.contains(wrong), "{maker} {cpu:?}: {steps}");
        }
    }

    #[test]
    fn an_asus_laptop_is_told_f2_and_never_del() {
        for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
            let mut s = pc(MAKERS[0], cpu);
            s.machine.form_factor = FormFactor::Laptop;
            s.board.product = "G614JV".into();
            let g = build_guide(&s);
            assert_eq!(g.enter_key, "F2");
            assert_eq!(
                section(&g, BiosTopic::Xmp).steps[0],
                "Restart and press F2 to open the BIOS."
            );
            for sec in &g.sections {
                for step in &sec.steps {
                    assert!(!step.contains("Del"), "{:?}: {step}", sec.topic);
                }
            }
        }
        // ASUS desktops keep Del.
        let desktop = build_guide(&owner_pc_fixture());
        assert_eq!(
            section(&desktop, BiosTopic::Xmp).steps[0],
            "Restart and press Del to open the BIOS."
        );
    }

    #[test]
    fn advanced_mode_is_spelled_one_way_everywhere() {
        for maker in MAKERS {
            for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
                let json = serde_json::to_string(&build_guide(&pc(maker, cpu))).unwrap();
                assert!(!json.contains("Advanced mode"), "{maker} {cpu:?}: {json}");
                // MSI's own manuals head the page "Advanced Mode" too.
                if maker == MAKERS[1] {
                    assert!(json.contains("EZ Mode and Advanced Mode"), "{json}");
                }
            }
        }
    }

    #[test]
    fn the_update_caution_asks_for_gear_1_again_only_on_11th_gen_intel() {
        let caution = |s: &SystemSnapshot| {
            section(&build_guide(s), BiosTopic::UpdateBios)
                .caution
                .clone()
                .unwrap()
        };
        let owner = caution(&owner_pc_fixture());
        assert_eq!(owner, "Don't turn off the PC while it updates. Updating resets BIOS settings, so turn XMP back on afterwards. On 11th-gen Intel the memory gear goes back to its default too, so set Gear 1 again.");
        let mut alder_lake = owner_pc_fixture();
        alder_lake.cpu.name = "12th Gen Intel(R) Core(TM) i7-12700K".into();
        for s in [alder_lake, pc(MAKERS[0], CpuVendor::Amd)] {
            let text = caution(&s);
            assert!(!text.contains("Gear"), "{}: {text}", s.cpu.name);
            assert!(text.ends_with("back on afterwards."), "{text}");
        }
    }

    #[test]
    fn placeholder_board_names_are_never_shown_or_matched() {
        for product in [
            "To Be Filled By O.E.M.",
            "Default string",
            " System Product Name ",
        ] {
            let mut s = owner_pc_fixture();
            s.board.product = product.into();
            let g = build_guide(&s);
            assert_eq!(g.board, "your motherboard", "{product}");
            let update = section(&g, BiosTopic::UpdateBios).steps[0].clone();
            assert!(
                update.contains("the latest BIOS for your motherboard from ASUS support"),
                "{update}"
            );
        }
        for maker in [
            "To be filled by O.E.M.",
            "Default string",
            "System manufacturer",
        ] {
            assert_eq!(board_vendor(maker), BoardVendor::Other, "{maker}");
            let mut s = owner_pc_fixture();
            s.board.manufacturer = maker.into();
            assert_eq!(build_guide(&s).vendor, BoardVendor::Other, "{maker}");
        }
    }
}
