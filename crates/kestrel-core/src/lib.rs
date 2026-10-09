//! Kestrel core: the data model and every decision Kestrel makes. Pure Rust, no OS calls.

pub mod gpu;
pub mod ram;
pub mod snapshot;
pub use snapshot::*;
