//! Kestrel tweaks: the Essential tier's catalog, its plan rules, the journal and the undo decisions. Everything that
//! touches Windows goes through the `System` trait, so all of it is tested on any OS against `fake::FakeSystem`.
pub mod catalog;
pub mod fake;
pub mod journal;
pub mod plan;
pub mod system;
pub mod targets;
pub mod tokens;

pub use catalog::{info, Impact, Info, Timing};
pub use journal::RestoreOutcome;
pub use plan::{PlanInput, PreviewItem, Row, RowState, ValueChange};
pub use system::{Read, Readiness, System};
