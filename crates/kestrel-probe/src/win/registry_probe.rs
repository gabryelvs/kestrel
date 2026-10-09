//! Registry reads. Placeholder until the registry probes land: every function reports "couldn't tell".
use kestrel_core::{ProbeError, SystemSnapshot};

pub fn fill(_s: &mut SystemSnapshot, _errors: &mut Vec<ProbeError>) {}

pub fn gpu_memory_mb(_name: &str) -> Option<u64> {
    None
}

pub fn adapter_power_saving(_device_id: &str) -> Option<bool> {
    None
}
