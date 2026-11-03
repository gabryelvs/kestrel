//! The text "Copy results" puts on the clipboard (decision 45). It's pasted into chat, so it's plain, short and
//! in English like the rest of the app.
use crate::model::{Benchmark, Bottleneck, Comparison, Flag, GpuLink, MetricId, Verdict};
use crate::profile::profile;
use crate::result::varies_too_much;

pub fn results_text(
    bench: &Benchmark,
    comparison: Option<&Comparison>,
    baseline_name: Option<&str>,
) -> String {
    let m = &bench.result.metrics;
    let game = profile(&bench.profile_id).map_or(bench.profile_id.as_str(), |p| p.name);
    let runs: Vec<String> = bench
        .runs
        .iter()
        .map(|r| format!("{:.0}", r.metrics.avg_fps))
        .collect();
    let mut lines = vec![
        format!("Kestrel benchmark: {} ({})", bench.name, bench.created_at),
        format!(
            "{game}, route v{}, {} runs, Kestrel {}, PresentMon {}",
            bench.route_version,
            bench.runs.len(),
            bench.kestrel_version,
            bench.presentmon_version
        ),
        format!(
            "Average FPS {:.0} (spread {:.1}%) | 1% low {:.0} | 0.1% low {:.0}",
            m.avg_fps,
            bench.result.spread.avg_fps * 100.0,
            m.low1_fps,
            m.low01_fps
        ),
        format!(
            "Stutters {:.1}/min | PC latency {} | {}",
            m.stutters_per_min,
            m.pc_latency_ms
                .map_or("not reported".to_string(), |ms| format!("{ms:.1} ms")),
            bottleneck_text(m.bottleneck)
        ),
        format!("Runs: {} FPS", runs.join(" / ")),
    ];
    for (i, run) in bench.runs.iter().enumerate() {
        if run.kept && !run.flags.is_empty() {
            let reasons: Vec<String> = run.flags.iter().map(flag_text).collect();
            lines.push(format!(
                "Run {} kept despite: {}",
                i + 1,
                reasons.join(", ")
            ));
        }
    }
    if varies_too_much(bench.result.spread.avg_fps) {
        lines.push(format!(
            "These runs vary by {:.1}%, so only big changes will count as real. Close background apps and run again.",
            bench.result.spread.avg_fps * 100.0
        ));
    }
    if let Some(c) = comparison {
        let name = baseline_name.unwrap_or("the baseline");
        if c.route_mismatch {
            lines.push(format!("Not compared with {name}: different route"));
        } else if !c.metrics.is_empty() {
            let parts: Vec<String> = c
                .metrics
                .iter()
                .map(|d| match d.delta_pct {
                    Some(pct) => format!(
                        "{} {} ({})",
                        metric_text(d.metric),
                        pct_text(pct),
                        verdict_text(d.verdict)
                    ),
                    // No percentage exists from a baseline of 0.
                    None => format!("{} ({})", metric_text(d.metric), verdict_text(d.verdict)),
                })
                .collect();
            lines.push(format!("Versus {name}: {}", parts.join(", ")));
        }
        if !c.settings_differ.is_empty() {
            lines.push(format!(
                "VALORANT settings that differ from {name}: {}",
                c.settings_differ.join(", ")
            ));
        }
        // Ids are printed as they were saved, so one this build doesn't know is still there.
        let mut tweaks = Vec::new();
        if !c.tweaks_added.is_empty() {
            tweaks.push(format!("added {}", c.tweaks_added.join(", ")));
        }
        if !c.tweaks_removed.is_empty() {
            tweaks.push(format!("removed {}", c.tweaks_removed.join(", ")));
        }
        if !tweaks.is_empty() {
            lines.push(format!("Tweaks since {name}: {}", tweaks.join("; ")));
        }
        if let Some([was, now]) = &c.power_plan_change {
            lines.push(format!("Power plan since {name}: {was} → {now}"));
        }
    }
    let ctx = &bench.context;
    let mut pc = Vec::new();
    if let Some(plan) = &ctx.power_plan {
        pc.push(format!("power plan {plan}"));
    }
    if let Some(mts) = ctx.ram_mts {
        pc.push(format!("RAM {mts} MT/s"));
    }
    match (&ctx.gpu, &ctx.gpu_driver) {
        (Some(gpu), Some(driver)) => pc.push(format!("{gpu} (driver {driver})")),
        (Some(gpu), None) => pc.push(gpu.clone()),
        _ => {}
    }
    if ctx.windows_build > 0 {
        pc.push(format!("Windows build {}", ctx.windows_build));
    }
    if let Some(hz) = ctx.display_hz {
        pc.push(format!("{hz} Hz"));
    }
    if !pc.is_empty() {
        lines.push(format!("PC: {}", pc.join(", ")));
    }
    lines.push(format!("PCIe link under load: {}", link_text(ctx.gpu_link)));
    lines.join("\n")
}

/// "4.0 x16", or "not reported" when the link wasn't read. A generation or lane count of 0 is no reading, so the line
/// never says "x0".
fn link_text(link: Option<GpuLink>) -> String {
    match link {
        Some(GpuLink { gen, width }) if gen > 0 && width > 0 => format!("{gen}.0 x{width}"),
        _ => "not reported".to_string(),
    }
}

/// A change in %: "+4.9%", "-1.0%", or "0.0%" when it rounds to zero, so a tiny change never reads "-0.0%" (the UI's
/// `pct` does the same).
fn pct_text(pct: f64) -> String {
    if format!("{pct:.1}").trim_start_matches('-') == "0.0" {
        "0.0%".to_string()
    } else {
        format!("{pct:+.1}%")
    }
}

fn bottleneck_text(b: Bottleneck) -> &'static str {
    match b {
        Bottleneck::Gpu => "GPU-bound",
        Bottleneck::Cpu => "CPU-bound",
        Bottleneck::Mixed => "CPU and GPU both limit",
        Bottleneck::Unknown => "bottleneck unknown",
    }
}

fn flag_text(flag: &Flag) -> String {
    match flag {
        Flag::TooShort => "too short".into(),
        Flag::NotInFront => "game not in front".into(),
        Flag::FpsCapped => "FPS cap or V-Sync".into(),
        Flag::DroppedData => "dropped data".into(),
        Flag::BusyBackground { name } => format!("busy background app ({name})"),
    }
}

fn metric_text(metric: MetricId) -> &'static str {
    match metric {
        MetricId::AvgFps => "average FPS",
        MetricId::Low1Fps => "1% low",
        MetricId::Low01Fps => "0.1% low",
        MetricId::StuttersPerMin => "stutters",
        MetricId::PcLatencyMs => "PC latency",
    }
}

fn verdict_text(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::RealGain => "real gain",
        Verdict::RealLoss => "real loss",
        Verdict::Noise => "within noise",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BenchResult, Context, GpuLink, MetricDelta, RunMetrics, RunRecord, Spread};

    const CARD: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";

    fn starting_point() -> Benchmark {
        let run = |avg: f64| RunRecord {
            metrics: RunMetrics {
                avg_fps: avg,
                ..Default::default()
            },
            ..Default::default()
        };
        Benchmark {
            id: "20261012-213005".into(),
            name: "Starting point".into(),
            created_at: "2026-10-12T21:30:05Z".into(),
            profile_id: "valorant".into(),
            route_version: 1,
            kestrel_version: "0.2.0".into(),
            presentmon_version: "2.6.0".into(),
            runs: vec![
                run(410.0),
                RunRecord {
                    flags: vec![Flag::BusyBackground {
                        name: "chrome.exe".into(),
                    }],
                    kept: true,
                    ..run(405.0)
                },
                run(418.0),
            ],
            result: BenchResult {
                metrics: RunMetrics {
                    avg_fps: 410.0,
                    low1_fps: 288.0,
                    low01_fps: 201.0,
                    stutters_per_min: 0.4,
                    pc_latency_ms: Some(9.84),
                    bottleneck: Bottleneck::Gpu,
                    ..Default::default()
                },
                spread: Spread {
                    avg_fps: 0.0317,
                    ..Default::default()
                },
            },
            context: Context {
                power_plan: Some("EXM Free Power Plan V8".into()),
                ram_mts: Some(3600),
                gpu: Some("NVIDIA GeForce RTX 3060 Ti".into()),
                gpu_driver: Some("610.74".into()),
                gpu_pnp_device_id: Some(CARD.into()),
                gpu_link: Some(GpuLink { gen: 4, width: 16 }),
                windows_build: 26200,
                display_hz: Some(360),
                game_settings: None,
                applied_tweaks: None,
            },
        }
    }

    #[test]
    fn a_result_reads_as_plain_lines() {
        assert_eq!(
            results_text(&starting_point(), None, None),
            "Kestrel benchmark: Starting point (2026-10-12T21:30:05Z)\n\
             VALORANT, route v1, 3 runs, Kestrel 0.2.0, PresentMon 2.6.0\n\
             Average FPS 410 (spread 3.2%) | 1% low 288 | 0.1% low 201\n\
             Stutters 0.4/min | PC latency 9.8 ms | GPU-bound\n\
             Runs: 410 / 405 / 418 FPS\n\
             Run 2 kept despite: busy background app (chrome.exe)\n\
             PC: power plan EXM Free Power Plan V8, RAM 3600 MT/s, NVIDIA GeForce RTX 3060 Ti (driver 610.74), Windows build 26200, 360 Hz\n\
             PCIe link under load: 4.0 x16"
        );
    }

    #[test]
    fn the_pcie_line_follows_the_pc_line_and_says_when_the_link_wasnt_read() {
        let text = |bench: &Benchmark| results_text(bench, None, None);
        let mut bench = starting_point();
        bench.context.gpu_link = Some(GpuLink { gen: 3, width: 8 });
        assert!(
            text(&bench).ends_with("360 Hz\nPCIe link under load: 3.0 x8"),
            "{}",
            text(&bench)
        );
        bench.context.gpu_link = None;
        assert!(
            text(&bench).ends_with("360 Hz\nPCIe link under load: not reported"),
            "{}",
            text(&bench)
        );
        // A link read as zero is no reading: never "0.0 x0".
        bench.context.gpu_link = Some(GpuLink::default());
        assert!(
            text(&bench).ends_with("\nPCIe link under load: not reported"),
            "{}",
            text(&bench)
        );
        // Nor is a link with only one of its two numbers: never "4.0 x0" or "0.0 x16".
        for half in [GpuLink { gen: 4, width: 0 }, GpuLink { gen: 0, width: 16 }] {
            bench.context.gpu_link = Some(half);
            assert!(
                text(&bench).ends_with("\nPCIe link under load: not reported"),
                "{half:?}: {}",
                text(&bench)
            );
        }
        // Without any PC context (no PC line), the PCIe line is still there, last.
        bench.context = Context::default();
        let bare = text(&bench);
        assert!(!bare.contains("\nPC: "), "{bare}");
        assert!(
            bare.ends_with("Run 2 kept despite: busy background app (chrome.exe)\nPCIe link under load: not reported"),
            "{bare}"
        );
    }

    /// A result named "After tweaks" whose runs were `spread` apart (a fraction: 0.249 is 24.9%).
    fn after_tweaks(spread: f64) -> Benchmark {
        let mut later = starting_point();
        later.name = "After tweaks".into();
        later.result.spread.avg_fps = spread;
        later
    }

    #[test]
    fn copy_results_says_what_changed_and_warns_about_spread() {
        let comparison = Comparison {
            tweaks_added: vec!["power-plan".into()],
            tweaks_removed: vec!["game-mode".into()],
            power_plan_change: Some(["EXM Free Power Plan V8".into(), "Kestrel".into()]),
            ..Default::default()
        };
        let text = results_text(
            &after_tweaks(0.249),
            Some(&comparison),
            Some("Starting point"),
        );
        let lines: Vec<&str> = text.lines().collect();
        for line in [
            "These runs vary by 24.9%, so only big changes will count as real. Close background apps and run again.",
            "Tweaks since Starting point: added power-plan; removed game-mode",
            "Power plan since Starting point: EXM Free Power Plan V8 → Kestrel",
        ] {
            assert!(lines.contains(&line), "no line {line:?} in:\n{text}");
        }

        // A spread of 5% has no warning line, and no tweak changes means no tweaks line.
        let calm = results_text(
            &after_tweaks(0.05),
            Some(&Comparison::default()),
            Some("Starting point"),
        );
        for absent in ["These runs vary", "Tweaks since", "Power plan since"] {
            assert!(!calm.contains(absent), "{absent:?} in:\n{calm}");
        }
        // Exactly 10% is not over it.
        let edge = results_text(&after_tweaks(0.10), None, None);
        assert!(!edge.contains("These runs vary"), "{edge}");
    }

    #[test]
    fn the_new_lines_come_after_the_runs_and_the_settings_and_before_the_pc() {
        let mut later = after_tweaks(0.249);
        later.runs[0].kept = true;
        later.runs[0].flags = vec![Flag::TooShort];
        let comparison = Comparison {
            metrics: vec![MetricDelta {
                metric: MetricId::AvgFps,
                baseline: 410.0,
                current: 430.24,
                delta_pct: Some(4.94),
                verdict: Verdict::Noise,
            }],
            settings_differ: vec!["Resolution".into()],
            tweaks_added: vec!["power-plan".into(), "game-mode".into()],
            tweaks_removed: vec!["gpu-preference".into()],
            power_plan_change: Some(["EXM Free Power Plan V8".into(), "Kestrel".into()]),
            ..Default::default()
        };
        assert_eq!(
            results_text(&later, Some(&comparison), Some("Starting point")),
            "Kestrel benchmark: After tweaks (2026-10-12T21:30:05Z)\n\
             VALORANT, route v1, 3 runs, Kestrel 0.2.0, PresentMon 2.6.0\n\
             Average FPS 410 (spread 24.9%) | 1% low 288 | 0.1% low 201\n\
             Stutters 0.4/min | PC latency 9.8 ms | GPU-bound\n\
             Runs: 410 / 405 / 418 FPS\n\
             Run 1 kept despite: too short\n\
             Run 2 kept despite: busy background app (chrome.exe)\n\
             These runs vary by 24.9%, so only big changes will count as real. Close background apps and run again.\n\
             Versus Starting point: average FPS +4.9% (within noise)\n\
             VALORANT settings that differ from Starting point: Resolution\n\
             Tweaks since Starting point: added power-plan, game-mode; removed gpu-preference\n\
             Power plan since Starting point: EXM Free Power Plan V8 → Kestrel\n\
             PC: power plan EXM Free Power Plan V8, RAM 3600 MT/s, NVIDIA GeForce RTX 3060 Ti (driver 610.74), Windows build 26200, 360 Hz\n\
             PCIe link under load: 4.0 x16"
        );
    }

    #[test]
    fn the_tweaks_line_names_only_what_changed_and_prints_any_id_as_written() {
        let line = |added: &[&str], removed: &[&str]| {
            let comparison = Comparison {
                tweaks_added: added.iter().map(|id| id.to_string()).collect(),
                tweaks_removed: removed.iter().map(|id| id.to_string()).collect(),
                ..Default::default()
            };
            results_text(
                &after_tweaks(0.03),
                Some(&comparison),
                Some("Starting point"),
            )
            .lines()
            .find(|l| l.starts_with("Tweaks since"))
            .map(str::to_string)
        };
        assert_eq!(
            line(&["power-plan"], &[]).as_deref(),
            Some("Tweaks since Starting point: added power-plan")
        );
        assert_eq!(
            line(&[], &["game-mode"]).as_deref(),
            Some("Tweaks since Starting point: removed game-mode")
        );
        assert_eq!(line(&[], &[]), None);
        // An id from an older or a newer Kestrel is shown as it was saved, never dropped.
        assert_eq!(
            line(&["from-the-future", "system-protection"], &["Old Id"]).as_deref(),
            Some("Tweaks since Starting point: added from-the-future, system-protection; removed Old Id")
        );
    }

    #[test]
    fn the_warning_is_about_this_result_and_the_comparison_lines_need_a_comparison() {
        // No baseline to compare with: the warning stays, the lines about changes can't be there.
        let alone = results_text(&after_tweaks(0.249), None, None);
        assert!(
            alone.contains("\nThese runs vary by 24.9%, so only big changes will count as real."),
            "{alone}"
        );
        assert!(
            !alone.contains("Tweaks since") && !alone.contains("Power plan since"),
            "{alone}"
        );
        // A baseline with no name is still "the baseline".
        let comparison = Comparison {
            tweaks_added: vec!["power-plan".into()],
            power_plan_change: Some(["Balanced".into(), "Kestrel".into()]),
            ..Default::default()
        };
        let unnamed = results_text(&after_tweaks(0.03), Some(&comparison), None);
        assert!(
            unnamed.contains("\nTweaks since the baseline: added power-plan\nPower plan since the baseline: Balanced → Kestrel\n"),
            "{unnamed}"
        );
        // The warning's number is the one in the line above it, rounded the same way.
        let rounded = results_text(&after_tweaks(0.12349), None, None);
        assert!(rounded.contains("(spread 12.3%)"), "{rounded}");
        assert!(
            rounded.contains("These runs vary by 12.3%, so"),
            "{rounded}"
        );
    }

    #[test]
    fn a_comparison_adds_its_verdicts_and_setting_warnings() {
        let mut later = starting_point();
        later.name = "After tweaks".into();
        later.result.metrics.pc_latency_ms = None;
        later.result.metrics.bottleneck = Bottleneck::Cpu;
        let comparison = Comparison {
            metrics: vec![
                MetricDelta {
                    metric: MetricId::AvgFps,
                    baseline: 410.0,
                    current: 430.24,
                    delta_pct: Some(4.94),
                    verdict: Verdict::RealGain,
                },
                MetricDelta {
                    metric: MetricId::Low1Fps,
                    baseline: 288.0,
                    current: 290.9,
                    delta_pct: Some(1.0),
                    verdict: Verdict::Noise,
                },
            ],
            settings_differ: vec!["Resolution".into()],
            ..Default::default()
        };
        let text = results_text(&later, Some(&comparison), Some("Starting point"));
        assert!(
            text.contains("Stutters 0.4/min | PC latency not reported | CPU-bound\n"),
            "{text}"
        );
        assert!(text.contains("\nVersus Starting point: average FPS +4.9% (real gain), 1% low +1.0% (within noise)\n"), "{text}");
        assert!(
            text.contains("\nVALORANT settings that differ from Starting point: Resolution\n"),
            "{text}"
        );
        let other_route = Comparison {
            route_mismatch: true,
            ..Default::default()
        };
        assert!(
            results_text(&later, Some(&other_route), Some("Starting point"))
                .contains("\nNot compared with Starting point: different route\n")
        );
        // The sign follows the rounded digits: a change that rounds to zero reads "0.0%", never "-0.0%" or "+0.0%",
        // while one that rounds to a visible 0.1 keeps its sign.
        let delta = |metric, pct| MetricDelta {
            metric,
            baseline: 100.0,
            current: 100.0,
            delta_pct: Some(pct),
            verdict: Verdict::Noise,
        };
        let near_zero = Comparison {
            metrics: vec![
                delta(MetricId::AvgFps, -0.04),
                delta(MetricId::Low1Fps, 0.04),
                delta(MetricId::Low01Fps, -0.0),
                delta(MetricId::StuttersPerMin, -0.06),
                delta(MetricId::PcLatencyMs, -1.04),
            ],
            ..Default::default()
        };
        let text = results_text(&later, Some(&near_zero), Some("Starting point"));
        assert!(
            text.contains(
                "\nVersus Starting point: average FPS 0.0% (within noise), 1% low 0.0% (within noise), \
                 0.1% low 0.0% (within noise), stutters -0.1% (within noise), PC latency -1.0% (within noise)\n"
            ),
            "{text}"
        );
    }
}
