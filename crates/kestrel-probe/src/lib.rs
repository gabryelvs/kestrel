//! Kestrel probe: looks at the PC and fills a `SystemSnapshot`. Read-only by design.
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
