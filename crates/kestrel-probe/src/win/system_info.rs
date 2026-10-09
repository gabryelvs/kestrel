//! Free space on VALORANT's drive and busy background apps, via the cross-platform `sysinfo` crate.
use crate::parse;
use kestrel_core::SystemSnapshot;
use std::time::Duration;
use sysinfo::{Disks, ProcessesToUpdate, System};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn fill(s: &mut SystemSnapshot) {
    if let Some(install) = s.storage.valorant.as_mut() {
        let prefix = format!("{}:", install.drive_letter);
        let disks = Disks::new_with_refreshed_list();
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
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    std::thread::sleep(Duration::from_millis(1500));
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let samples = sys
        .processes()
        .values()
        .map(|p| (p.name().to_string_lossy().into_owned(), p.cpu_usage()))
        .collect();
    let threads = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    s.background.busy_processes = Some(parse::busy_processes(samples, threads));
    // Win32_PhysicalMemory can come back empty (a virtual machine, say); ask Windows for the total instead.
    // `System::new()` loads nothing, so the memory figures have to be refreshed first.
    if s.memory.total_mb == 0 {
        sys.refresh_memory();
        s.memory.total_mb = sys.total_memory() / 1024 / 1024;
    }
}
