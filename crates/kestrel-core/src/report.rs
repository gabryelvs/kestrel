//! A full report: what Kestrel saw, what it found, the score, and the BIOS guide.
use crate::bios_guide::{build_guide, BiosGuide};
use crate::health::{run_health_check, Finding};
use crate::score::{score, Score};
use crate::snapshot::SystemSnapshot;
use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub app_version: String,
    pub snapshot: SystemSnapshot,
    pub findings: Vec<Finding>,
    pub score: Score,
    pub guide: BiosGuide,
}

pub fn build_report(snapshot: SystemSnapshot, today: NaiveDate, app_version: &str) -> Report {
    let findings = run_health_check(&snapshot, today);
    let score = score(&findings);
    let guide = build_guide(&snapshot);
    Report {
        app_version: app_version.into(),
        snapshot,
        findings,
        score,
        guide,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    #[test]
    fn owner_report_has_everything() {
        let r = build_report(
            owner_pc_fixture(),
            NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
            "0.1.0",
        );
        assert_eq!(r.findings.len(), 31);
        assert_eq!(r.score.value, 86);
        assert_eq!(r.app_version, "0.1.0");
        let json = serde_json::to_string(&r).unwrap();
        for key in [
            "\"appVersion\"",
            "\"snapshot\"",
            "\"findings\"",
            "\"score\"",
            "\"guide\"",
        ] {
            assert!(json.contains(key), "missing {key}");
        }
    }
}
