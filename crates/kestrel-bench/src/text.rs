//! The text "Copy results" puts on the clipboard (decision 45). It's pasted into chat, so it's plain, short and
//! in English like the rest of the app.
use crate::model::{Benchmark, Bottleneck, Comparison, Flag, MetricId, Verdict};
use crate::profile::profile;

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
                        "{} {pct:+.1}% ({})",
                        metric_text(d.metric),
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
    lines.join("\n")
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
    use crate::model::{BenchResult, Context, MetricDelta, RunMetrics, RunRecord, Spread};

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
                windows_build: 26200,
                display_hz: Some(360),
                game_settings: None,
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
             PC: power plan EXM Free Power Plan V8, RAM 3600 MT/s, NVIDIA GeForce RTX 3060 Ti (driver 610.74), Windows build 26200, 360 Hz"
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
    }
}
