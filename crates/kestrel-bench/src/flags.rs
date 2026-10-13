//! When a run can't be trusted (decision 44; spec "When a run is flagged").
use crate::csv::Frame;
use crate::metrics::percentile;
use crate::model::{Flag, RunMetrics, ValorantSettings};

/// What the flags look at, gathered right after a capture.
pub struct RunInputs<'a> {
    pub frames: &'a [Frame],
    pub metrics: &'a RunMetrics,
    /// PresentMon's stderr: it reports lost events and buffer overflows there.
    pub stderr: &'a str,
    pub settings: Option<&'a ValorantSettings>,
    /// Background apps measured right after the run: name and share of the whole CPU, in %.
    pub busy: &'a [(String, f32)],
}

pub fn flags(run: &RunInputs) -> Vec<Flag> {
    let mut out = Vec::new();
    if run.metrics.duration_s < 85.0 || run.metrics.frames < 1000 {
        out.push(Flag::TooShort);
    }
    let composed = run
        .frames
        .iter()
        .filter(|f| f.present_mode.starts_with("Composed"))
        .count();
    if !run.frames.is_empty() && composed as f64 / run.frames.len() as f64 > 0.05 {
        out.push(Flag::NotInFront);
    }
    let times: Vec<f64> = run.frames.iter().map(|f| f.ms).collect();
    let flat = !times.is_empty() && {
        let (p5, p95) = (percentile(&times, 0.05), percentile(&times, 0.95));
        p5 > 0.0 && p95 <= p5 * 1.05
    };
    let capped_in_settings = run.settings.is_some_and(|s| {
        s.vsync == Some(true) || s.frame_rate_limit.is_some_and(|limit| limit > 0.0)
    });
    if flat || capped_in_settings {
        out.push(Flag::FpsCapped);
    }
    let stderr = run.stderr.to_ascii_lowercase();
    if stderr.contains("lost") || stderr.contains("overflow") {
        out.push(Flag::DroppedData);
    }
    if let Some((name, _)) = run.busy.iter().find(|(_, percent)| *percent >= 10.0) {
        out.push(Flag::BusyBackground { name: name.clone() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::Frame;
    use crate::metrics::measure;

    /// A normal uncapped run: frame times alternate between 3.5 and 4.5 ms for 90 s.
    fn varied(n: usize, mode: &str) -> Vec<Frame> {
        (0..n)
            .map(|i| Frame {
                ms: if i % 2 == 0 { 3.5 } else { 4.5 },
                present_mode: mode.into(),
                ..Default::default()
            })
            .collect()
    }

    fn check(
        frames: &[Frame],
        stderr: &str,
        settings: Option<&ValorantSettings>,
        busy: &[(String, f32)],
    ) -> Vec<Flag> {
        let (metrics, _) = measure(frames);
        flags(&RunInputs {
            frames,
            metrics: &metrics,
            stderr,
            settings,
            busy,
        })
    }

    #[test]
    fn a_clean_run_has_no_flags() {
        let settings = ValorantSettings {
            vsync: Some(false),
            frame_rate_limit: Some(0.0),
            ..Default::default()
        };
        assert!(check(
            &varied(22_500, "Hardware: Independent Flip"),
            "",
            Some(&settings),
            &[]
        )
        .is_empty());
    }

    #[test]
    fn empty_capture_is_only_too_short() {
        assert_eq!(check(&[], "", None, &[]), vec![Flag::TooShort]);
    }

    #[test]
    fn short_runs_are_flagged() {
        assert_eq!(
            check(&varied(10_000, "Hardware: Independent Flip"), "", None, &[]),
            vec![Flag::TooShort]
        );
        // 90 s but fewer than 1,000 frames (a slideshow) is too short too.
        let slow: Vec<Frame> = (0..900)
            .map(|i| Frame {
                ms: if i % 2 == 0 { 90.0 } else { 110.0 },
                ..Default::default()
            })
            .collect();
        assert_eq!(check(&slow, "", None, &[]), vec![Flag::TooShort]);
    }

    #[test]
    fn composed_frames_mean_the_game_was_not_in_front() {
        let mut frames = varied(20_000, "Hardware: Independent Flip");
        frames.extend(varied(2_500, "Composed: Flip"));
        assert_eq!(check(&frames, "", None, &[]), vec![Flag::NotInFront]);
        let mut five_percent = varied(21_375, "Hardware: Legacy Flip");
        five_percent.extend(varied(1_125, "Composed: Flip"));
        assert!(
            check(&five_percent, "", None, &[]).is_empty(),
            "exactly 5% is allowed"
        );
    }

    #[test]
    fn caps_are_found_in_the_settings_or_the_frame_times() {
        let flat: Vec<Frame> = (0..22_500)
            .map(|_| Frame {
                ms: 4.0,
                ..Default::default()
            })
            .collect();
        assert_eq!(check(&flat, "", None, &[]), vec![Flag::FpsCapped]);
        let vsync = ValorantSettings {
            vsync: Some(true),
            ..Default::default()
        };
        let limit = ValorantSettings {
            frame_rate_limit: Some(240.0),
            ..Default::default()
        };
        let run = varied(22_500, "Hardware: Independent Flip");
        assert_eq!(check(&run, "", Some(&vsync), &[]), vec![Flag::FpsCapped]);
        assert_eq!(check(&run, "", Some(&limit), &[]), vec![Flag::FpsCapped]);
    }

    #[test]
    fn presentmon_warnings_and_busy_apps_are_flagged() {
        let run = varied(22_500, "Hardware: Independent Flip");
        assert_eq!(
            check(&run, "Warning: 12 events lost", None, &[]),
            vec![Flag::DroppedData]
        );
        assert_eq!(
            check(&run, "ETW buffer OVERFLOW", None, &[]),
            vec![Flag::DroppedData]
        );
        let busy = vec![
            ("discord.exe".to_string(), 4.0),
            ("chrome.exe".to_string(), 12.5),
        ];
        assert_eq!(
            check(&run, "", None, &busy),
            vec![Flag::BusyBackground {
                name: "chrome.exe".into()
            }]
        );
        assert!(check(&run, "", None, &[("chrome.exe".to_string(), 9.9)]).is_empty());
    }
}
