//! Kestrel tweaks: the Essential tier's catalog, its plan rules, the journal and the undo decisions. Everything that
//! touches Windows goes through the `System` trait, or for a restore point `restore::RestorePoints`, so all of it is
//! tested on any OS against `fake::FakeSystem`. The real Windows behind those traits is in `win`, whose pure half,
//! `parse`, is tested on any OS too.
pub mod catalog;
pub mod engine;
pub mod fake;
pub mod folder;
pub mod journal;
pub mod parse;
pub mod plan;
pub mod power;
pub mod report;
pub mod restore;
pub mod system;
pub mod targets;
pub mod tokens;
#[cfg(windows)]
pub mod win;

use std::sync::Arc;

pub use catalog::{info, Impact, Info, Timing};
pub use engine::{Engine, Gate, ItemOutcome, PreviewView, TweaksView};
pub use folder::journal_file;
pub use journal::RestoreOutcome;
pub use plan::{PlanInput, PreviewItem, Row, RowState, ValueChange};
pub use restore::{helper_main, run_helper, RestoreChoice};
pub use system::{Read, Readiness, System};

/// This PC: the real Windows on Windows, and elsewhere a pretend PC on stock Balanced, so the app can be developed
/// there.
pub fn this_pc() -> Arc<dyn System> {
    #[cfg(windows)]
    {
        Arc::new(win::WinSystem::new())
    }
    #[cfg(not(windows))]
    {
        Arc::new(fake::FakeSystem::new())
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn off_windows_this_pc_is_a_pretend_one_on_stock_balanced() {
        let pc = this_pc();
        assert_eq!(pc.active_plan(), Read::Value(fake::BALANCED.to_string()));
        assert!(pc.plan_exists(fake::HIGH_PERFORMANCE));
    }
}
