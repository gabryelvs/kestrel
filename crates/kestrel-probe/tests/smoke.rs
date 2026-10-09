#[test]
fn report_has_every_check() {
    let report = kestrel_probe::collect_report("test");
    assert_eq!(report.findings.len(), 27);
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
