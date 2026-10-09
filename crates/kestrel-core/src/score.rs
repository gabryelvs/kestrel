//! The system score: 100 minus what the problems cost.
use crate::health::{Finding, Severity, Status};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub value: u8,
    pub problems: u32,
    pub potential_gain: u8,
}

pub fn score(findings: &[Finding]) -> Score {
    let problems: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.status == Status::Problem)
        .collect();
    let cost: u32 = problems
        .iter()
        .map(|f| match f.severity {
            Severity::High => 15,
            Severity::Medium => 8,
            Severity::Low => 3,
            Severity::Info => 0,
        })
        .sum();
    let value = 100u32.saturating_sub(cost) as u8;
    Score {
        value,
        problems: problems.len() as u32,
        potential_gain: 100 - value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::run_health_check;
    use crate::snapshot::owner_pc_fixture;
    use chrono::NaiveDate;

    #[test]
    fn no_problems_scores_100() {
        assert_eq!(
            score(&[]),
            Score {
                value: 100,
                problems: 0,
                potential_gain: 0
            }
        );
    }

    #[test]
    fn owner_pc_scores_86() {
        let f = run_health_check(
            &owner_pc_fixture(),
            NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
        );
        assert_eq!(
            score(&f),
            Score {
                value: 86,
                problems: 3,
                potential_gain: 14
            }
        );
    }

    #[test]
    fn score_never_goes_below_zero() {
        let mut f = run_health_check(
            &owner_pc_fixture(),
            NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
        );
        for finding in &mut f {
            finding.status = Status::Problem;
            finding.severity = Severity::High;
        }
        assert_eq!(score(&f).value, 0);
        assert_eq!(score(&f).potential_gain, 100);
    }
}
