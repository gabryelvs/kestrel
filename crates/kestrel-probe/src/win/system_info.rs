//! Free space on VALORANT's drive and busy background apps. Busy apps come from Windows performance counters
//! (see `pdh`), so Kestrel itself opens no handle to any other process. The cross-platform `sysinfo` crate is
//! used only for disk sizes and the memory total, and neither touches a process.
use super::{pdh, problem};
use crate::parse;
use kestrel_core::{BusyProcess, ProbeError, SystemSnapshot};
use std::time::Duration;
use sysinfo::{DiskRefreshKind, Disks, System};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    if let Some(install) = s.storage.valorant.as_mut() {
        let prefix = format!("{}:", install.drive_letter);
        // Sizes only: the other refresh kinds open each volume's device to ask about its hardware and I/O.
        let disks =
            Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_storage());
        if let Some(d) = disks.list().iter().find(|d| {
            d.mount_point()
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with(&prefix)
        }) {
            install.free_gb = d.available_space() as f64 / GIB;
            install.total_gb = d.total_space() as f64 / GIB;
        }
    }
    s.background.busy_processes = busy_processes();
    if s.background.busy_processes.is_none() {
        errors.push(problem("background", "couldn't measure CPU use per app"));
    }
    // The WMI probe runs first and sets the total from Win32_PhysicalMemory, which can come back empty (a virtual
    // machine, say); then ask Windows for the total instead. `System::new()` loads nothing, so refresh memory first.
    if s.memory.total_mb == 0 {
        let mut sys = System::new();
        sys.refresh_memory();
        s.memory.total_mb = sys.total_memory() / 1024 / 1024;
    }
}

/// Which apps keep the CPU busy, measured over about a second and a half. None if Windows can't give the counters.
fn busy_processes() -> Option<Vec<BusyProcess>> {
    let query = pdh::Query::open(r"\Process(*)\% Processor Time")?;
    // The counter is a rate: its value is the average between two samples.
    if !query.collect() {
        return None;
    }
    std::thread::sleep(Duration::from_millis(1500));
    if !query.collect() {
        return None;
    }
    let instances = query.formatted_array(pdh::FMT_NOCAP100)?;
    // Windows always lists at least "Idle" and "_Total", so an empty answer means the counters didn't work,
    // not that nothing is running.
    if instances.is_empty() {
        return None;
    }
    // One instance per process ("chrome", "chrome#1", ...), where 100 means one whole core.
    let samples = instances
        .into_iter()
        .filter_map(|(instance, percent)| {
            Some((parse::pdh_instance_to_exe(&instance)?, percent as f32))
        })
        .collect();
    let threads = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    Some(parse::busy_processes(samples, threads))
}
