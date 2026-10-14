//! Free space on each volume (VALORANT's drive, the drive list) and busy background apps. Busy apps come from Windows
//! performance counters (see `pdh`), so Kestrel itself opens no handle to any other process. The cross-platform
//! `sysinfo` crate is used only for disk sizes and the memory total, and neither touches a process.
use super::{pdh, problem};
use crate::parse;
use kestrel_core::{BusyProcess, ProbeError, SystemSnapshot};
use std::time::Duration;
use sysinfo::{DiskRefreshKind, Disks, System};

pub(crate) const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// `space` is each volume's (letter, free GiB, total GiB) from `volume_space`, which the scan reads once and shares
/// with the drive list.
pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>, space: &[(char, f64, f64)]) {
    if let Some(install) = s.storage.valorant.as_mut() {
        let letter = install
            .drive_letter
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase());
        if let Some(&(_, free, total)) = space.iter().find(|(l, ..)| Some(*l) == letter) {
            install.free_gb = free;
            install.total_gb = total;
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

/// Free and total space in GiB of each volume with a drive letter: (letter, free, total).
pub(crate) fn volume_space() -> Vec<(char, f64, f64)> {
    // Sizes only: the other refresh kinds open each volume's device to ask about its hardware and I/O.
    let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_storage());
    disks
        .list()
        .iter()
        .filter_map(|d| {
            let letter = parse::volume_letter(&d.mount_point().to_string_lossy())?;
            Some((
                letter,
                d.available_space() as f64 / GIB,
                d.total_space() as f64 / GIB,
            ))
        })
        .collect()
}

/// Which apps keep the CPU busy, measured over about a second and a half. None if Windows can't give the counters.
pub(crate) fn busy_processes() -> Option<Vec<BusyProcess>> {
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
