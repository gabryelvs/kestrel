//! Kestrel core: the data model and every decision Kestrel makes. Pure Rust, no OS calls.

pub mod bios_guide;
pub mod cpu;
pub mod gpu;
pub mod health;
pub mod ram;
pub mod report;
pub mod score;
pub mod snapshot;
pub mod topology;
pub mod valorant;

pub use report::{build_report, Report};
pub use snapshot::*;
