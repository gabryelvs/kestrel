//! Kestrel probe: looks at the PC and fills a `SystemSnapshot`. Read-only by design.
pub mod parse;

#[cfg(not(windows))]
mod mock;

use kestrel_core::{Report, SystemSnapshot};

/// Looks at this PC. Never panics: anything unreadable becomes `None` plus a `probe_errors` entry.
pub fn collect_snapshot() -> SystemSnapshot {
    #[cfg(not(windows))]
    {
        mock::collect()
    }
    #[cfg(windows)]
    {
        unimplemented!("Windows probes arrive in Task 9")
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
