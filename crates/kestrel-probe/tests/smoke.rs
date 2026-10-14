#[test]
fn report_has_every_check() {
    let report = kestrel_probe::collect_report("test");
    assert_eq!(report.findings.len(), 31);
    assert_eq!(report.app_version, "test");
}

#[cfg(not(windows))]
#[test]
fn mock_says_it_is_a_sample() {
    let s = kestrel_probe::collect_snapshot();
    assert!(s.probe_errors.iter().any(|e| e.probe == "mock"));
    assert_eq!(s.cpu.threads, 16);
}

#[cfg(windows)]
#[test]
fn windows_probe_reads_the_basics() {
    let s = kestrel_probe::collect_snapshot();
    assert!(!s.cpu.name.is_empty(), "probe errors: {:?}", s.probe_errors);
    assert!(s.os.build >= 19041, "probe errors: {:?}", s.probe_errors);
    assert!(s.memory.total_mb > 0, "probe errors: {:?}", s.probe_errors);
}

#[cfg(windows)]
#[test]
fn windows_probe_reads_the_hardware() {
    // A hosted CI runner is a Hyper-V virtual machine without a graphics card: the PCIe link, Resizable BAR, the
    // chipset and monitor names can all be missing there, so only what every Windows PC reports is checked.
    let s = kestrel_probe::collect_snapshot();
    let errors = &s.probe_errors;
    assert!(s.cpu.family.is_some(), "probe errors: {errors:?}");
    assert!(
        s.cpu.instructions.iter().any(|i| i == "SSE4.2"),
        "probe errors: {errors:?}"
    );
    assert!(!s.cpu.caches.is_empty(), "probe errors: {errors:?}");
    assert!(s.cpu.base_mhz.is_some(), "probe errors: {errors:?}");
    assert_ne!(
        s.board.boot_mode,
        kestrel_core::BootMode::Unknown,
        "probe errors: {errors:?}"
    );
    assert!(!s.storage.drives.is_empty(), "probe errors: {errors:?}");
}
