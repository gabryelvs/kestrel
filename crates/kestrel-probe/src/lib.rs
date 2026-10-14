//! Kestrel probe: looks at the PC and fills a `SystemSnapshot`. Read-only by design.
pub mod clocks;
pub mod load;
pub mod parse;
pub mod process;

#[cfg(not(windows))]
mod mock;
#[cfg(windows)]
mod win;

use kestrel_core::{Report, SystemSnapshot};

/// Looks at this PC. Never panics: anything unreadable becomes `None` plus a `probe_errors` entry.
pub fn collect_snapshot() -> SystemSnapshot {
    #[cfg(not(windows))]
    {
        mock::collect()
    }
    #[cfg(windows)]
    {
        win::collect()
    }
}

/// Snapshot, Health Check, score and BIOS guide, dated today.
pub fn collect_report(app_version: &str) -> Report {
    kestrel_core::build_report(
        collect_snapshot(),
        chrono::Local::now().date_naive(),
        app_version,
    )
}

/// The current PCIe link of the card's slot device (see kestrel_core::pcie::slot_device). Windows: Configuration
/// Manager, or for an NVIDIA card NVML's live link when NVML sees only that card (see parse::link_now); elsewhere
/// None. The benchmark reads it every 2 s while a pass runs: it reads only device properties of the card and the PCI
/// bridges above it, and NVML's current link of the card.
pub fn gpu_link(pnp_device_id: &str) -> Option<kestrel_core::PcieLink> {
    #[cfg(windows)]
    {
        win::cfgmgr::gpu_link(pnp_device_id)
    }
    #[cfg(not(windows))]
    {
        let _ = pnp_device_id;
        None
    }
}

/// Apps keeping the CPU busy right now (measured over about 1.5 s), for flagging a benchmark run. Off Windows:
/// none. None when Windows can't give the counters.
pub fn busy_now() -> Option<Vec<kestrel_core::BusyProcess>> {
    #[cfg(windows)]
    {
        win::system_info::busy_processes()
    }
    #[cfg(not(windows))]
    {
        Some(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn the_link_is_not_read_off_windows() {
        let id = "PCI\\VEN_10DE&DEV_2489&SUBSYS_88131043&REV_A1\\4&2B5E7C3&0&0008";
        assert_eq!(super::gpu_link(id), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_card_windows_does_not_list_has_no_link() {
        assert_eq!(
            super::gpu_link("PCI\\VEN_FFFF&DEV_FFFF\\0&KESTREL&0&0000"),
            None
        );
        assert_eq!(super::gpu_link("ROOT\\DISPLAY\\0000"), None);
        assert_eq!(super::gpu_link(""), None);
        // An NVIDIA card Windows doesn't list has no link either, whatever NVML says about the cards it sees.
        assert_eq!(
            super::gpu_link("PCI\\VEN_10DE&DEV_FFFF\\0&KESTREL&0&0000"),
            None
        );
    }
}
