//! Off Windows (development on a Mac), the probe returns the owner's PC as sample data.
use kestrel_core::{owner_pc_fixture, ProbeError, SystemSnapshot};

pub fn collect() -> SystemSnapshot {
    let mut s = owner_pc_fixture();
    s.collected_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    s.probe_errors.push(ProbeError {
        probe: "mock".into(),
        message: "Not running on Windows, so this is the sample PC from fixtures/owner-pc.json."
            .into(),
    });
    s
}
