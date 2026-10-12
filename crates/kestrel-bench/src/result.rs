//! A benchmark's result from its runs (decision 38), comparisons with another benchmark (decision 41), and the
//! frame-time graph's points.
use crate::model::{
    BenchResult, Benchmark, Bottleneck, Comparison, MetricDelta, MetricId, RunMetrics, RunRecord,
    Spread, Verdict,
};
use crate::settings::differences;

/// The middle value of each metric over the runs, and how far apart the runs were.
pub fn combine(runs: &[RunRecord]) -> BenchResult {
    if runs.is_empty() {
        return BenchResult::default();
    }
    let values = |get: fn(&RunMetrics) -> f64| -> Vec<f64> {
        runs.iter().map(|r| get(&r.metrics)).collect()
    };
    let optional = |get: fn(&RunMetrics) -> Option<f64>| -> Option<f64> {
        let present: Vec<f64> = runs.iter().filter_map(|r| get(&r.metrics)).collect();
        (!present.is_empty()).then(|| median(&present))
    };
    let metrics = RunMetrics {
        frames: median(&values(|m| f64::from(m.frames))).round() as u32,
        duration_s: median(&values(|m| m.duration_s)),
        avg_fps: median(&values(|m| m.avg_fps)),
        low1_fps: median(&values(|m| m.low1_fps)),
        low01_fps: median(&values(|m| m.low01_fps)),
        stutters_per_min: median(&values(|m| m.stutters_per_min)),
        pc_latency_ms: optional(|m| m.pc_latency_ms),
        gpu_busy_ratio: optional(|m| m.gpu_busy_ratio),
        bottleneck: shared_bottleneck(runs),
    };
    let spread = Spread {
        avg_fps: spread(&values(|m| m.avg_fps)),
        low1_fps: spread(&values(|m| m.low1_fps)),
        low01_fps: spread(&values(|m| m.low01_fps)),
    };
    BenchResult { metrics, spread }
}

type Getter = fn(&RunMetrics) -> Option<f64>;

/// Each compared metric and whether higher is better.
const METRICS: [(MetricId, bool, Getter); 5] = [
    (MetricId::AvgFps, true, avg_fps),
    (MetricId::Low1Fps, true, low1_fps),
    (MetricId::Low01Fps, true, low01_fps),
    (MetricId::StuttersPerMin, false, stutters),
    (MetricId::PcLatencyMs, false, pc_latency),
];

fn avg_fps(m: &RunMetrics) -> Option<f64> {
    Some(m.avg_fps)
}
fn low1_fps(m: &RunMetrics) -> Option<f64> {
    Some(m.low1_fps)
}
fn low01_fps(m: &RunMetrics) -> Option<f64> {
    Some(m.low01_fps)
}
fn stutters(m: &RunMetrics) -> Option<f64> {
    Some(m.stutters_per_min)
}
fn pc_latency(m: &RunMetrics) -> Option<f64> {
    m.pc_latency_ms
}

pub fn compare(baseline: &Benchmark, current: &Benchmark) -> Comparison {
    let mut comparison = Comparison {
        baseline_id: baseline.id.clone(),
        current_id: current.id.clone(),
        ..Default::default()
    };
    if baseline.profile_id != current.profile_id || baseline.route_version != current.route_version
    {
        comparison.route_mismatch = true;
        return comparison;
    }
    for (metric, higher_is_better, get) in METRICS {
        let (Some(base), Some(now)) = (get(&baseline.result.metrics), get(&current.result.metrics))
        else {
            continue;
        };
        let base_runs: Vec<f64> = baseline
            .runs
            .iter()
            .filter_map(|r| get(&r.metrics))
            .collect();
        let now_runs: Vec<f64> = current
            .runs
            .iter()
            .filter_map(|r| get(&r.metrics))
            .collect();
        comparison.metrics.push(MetricDelta {
            metric,
            baseline: base,
            current: now,
            delta_pct: if base != 0.0 {
                (now - base) / base * 100.0
            } else {
                0.0
            },
            verdict: verdict(&base_runs, &now_runs, higher_is_better),
        });
    }
    if let (Some(a), Some(b)) = (
        &baseline.context.game_settings,
        &current.context.game_settings,
    ) {
        comparison.settings_differ = differences(a, b);
    }
    comparison
}

/// The run with the middle average FPS.
pub fn middle_run(runs: &[RunRecord]) -> Option<usize> {
    let mut order: Vec<usize> = (0..runs.len()).collect();
    order.sort_by(|&a, &b| runs[a].metrics.avg_fps.total_cmp(&runs[b].metrics.avg_fps));
    order.get(order.len() / 2).copied()
}

/// At most `points` values: the slowest frame of each bucket, so hitches stay visible when squeezed.
pub fn graph_points(frame_times: &[f32], points: usize) -> Vec<f32> {
    if frame_times.is_empty() || points == 0 {
        return Vec::new();
    }
    let size = frame_times.len().div_ceil(points);
    frame_times
        .chunks(size)
        .map(|bucket| bucket.iter().copied().fold(0.0_f32, f32::max))
        .collect()
}

/// Real only when every current run is beyond every baseline run, in either direction.
fn verdict(baseline: &[f64], current: &[f64], higher_is_better: bool) -> Verdict {
    if baseline.is_empty() || current.is_empty() {
        return Verdict::Noise;
    }
    let (base_low, base_high) = min_max(baseline);
    let (now_low, now_high) = min_max(current);
    let went_up = now_low > base_high;
    let went_down = now_high < base_low;
    match (went_up, went_down, higher_is_better) {
        (true, _, true) | (_, true, false) => Verdict::RealGain,
        (true, _, false) | (_, true, true) => Verdict::RealLoss,
        _ => Verdict::Noise,
    }
}

fn shared_bottleneck(runs: &[RunRecord]) -> Bottleneck {
    let kinds = [
        Bottleneck::Gpu,
        Bottleneck::Cpu,
        Bottleneck::Mixed,
        Bottleneck::Unknown,
    ];
    kinds
        .into_iter()
        .find(|kind| {
            runs.iter()
                .filter(|r| r.metrics.bottleneck == *kind)
                .count()
                * 2
                > runs.len()
        })
        .unwrap_or(Bottleneck::Mixed)
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    match sorted.len() {
        0 => 0.0,
        n if n % 2 == 1 => sorted[n / 2],
        n => (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0,
    }
}

fn spread(values: &[f64]) -> f64 {
    let middle = median(values);
    if middle <= 0.0 {
        return 0.0;
    }
    let (low, high) = min_max(values);
    (high - low) / middle
}

fn min_max(values: &[f64]) -> (f64, f64) {
    values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), &v| {
            (low.min(v), high.max(v))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ValorantSettings;

    fn run(
        avg: f64,
        low1: f64,
        stutters: f64,
        latency: Option<f64>,
        bottleneck: Bottleneck,
    ) -> RunRecord {
        RunRecord {
            metrics: RunMetrics {
                frames: 30_000,
                duration_s: 90.0,
                avg_fps: avg,
                low1_fps: low1,
                low01_fps: low1 / 2.0,
                stutters_per_min: stutters,
                pc_latency_ms: latency,
                bottleneck,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn bench(id: &str, runs: Vec<RunRecord>) -> Benchmark {
        Benchmark {
            id: id.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            result: combine(&runs),
            runs,
            ..Default::default()
        }
    }

    #[test]
    fn the_result_is_the_middle_run_value_with_its_spread() {
        let runs = vec![
            run(400.0, 280.0, 1.0, Some(10.0), Bottleneck::Gpu),
            run(430.0, 300.0, 0.5, None, Bottleneck::Gpu),
            run(410.0, 290.0, 2.0, Some(12.0), Bottleneck::Cpu),
        ];
        let r = combine(&runs);
        assert_eq!(r.metrics.avg_fps, 410.0);
        assert_eq!(r.metrics.low1_fps, 290.0);
        assert_eq!(r.metrics.stutters_per_min, 1.0);
        assert_eq!(
            r.metrics.pc_latency_ms,
            Some(11.0),
            "median of the runs that have it"
        );
        assert_eq!(
            r.metrics.bottleneck,
            Bottleneck::Gpu,
            "two of three runs agree"
        );
        assert!((r.spread.avg_fps - 30.0 / 410.0).abs() < 1e-9);
        let split = vec![
            run(1.0, 1.0, 0.0, None, Bottleneck::Gpu),
            run(1.0, 1.0, 0.0, None, Bottleneck::Cpu),
            run(1.0, 1.0, 0.0, None, Bottleneck::Mixed),
        ];
        assert_eq!(combine(&split).metrics.bottleneck, Bottleneck::Mixed);
        assert_eq!(combine(&[]), BenchResult::default());
    }

    #[test]
    fn a_change_is_real_only_when_the_runs_do_not_overlap() {
        let base = bench(
            "a",
            vec![
                run(400.0, 280.0, 2.0, Some(10.0), Bottleneck::Gpu),
                run(405.0, 290.0, 2.2, Some(10.5), Bottleneck::Gpu),
                run(410.0, 300.0, 2.4, Some(11.0), Bottleneck::Gpu),
            ],
        );
        let better = bench(
            "b",
            vec![
                run(420.0, 285.0, 1.0, Some(12.0), Bottleneck::Gpu),
                run(425.0, 295.0, 1.1, Some(12.5), Bottleneck::Gpu),
                run(430.0, 305.0, 1.2, Some(13.0), Bottleneck::Gpu),
            ],
        );
        let c = compare(&base, &better);
        let get = |id: MetricId| c.metrics.iter().find(|m| m.metric == id).unwrap().clone();
        assert_eq!(get(MetricId::AvgFps).verdict, Verdict::RealGain);
        assert!((get(MetricId::AvgFps).delta_pct - (425.0 - 405.0) / 405.0 * 100.0).abs() < 1e-9);
        assert_eq!(
            get(MetricId::Low1Fps).verdict,
            Verdict::Noise,
            "ranges overlap"
        );
        assert_eq!(
            get(MetricId::StuttersPerMin).verdict,
            Verdict::RealGain,
            "fewer stutters is better"
        );
        assert_eq!(
            get(MetricId::PcLatencyMs).verdict,
            Verdict::RealLoss,
            "more latency is worse"
        );
        assert_eq!(
            (
                c.baseline_id.as_str(),
                c.current_id.as_str(),
                c.route_mismatch
            ),
            ("a", "b", false)
        );
    }

    #[test]
    fn metrics_one_side_lacks_and_other_routes_are_not_compared() {
        let base = bench("a", vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        let current = bench(
            "b",
            vec![run(420.0, 285.0, 1.0, Some(12.0), Bottleneck::Gpu); 3],
        );
        let compared: Vec<MetricId> = compare(&base, &current)
            .metrics
            .iter()
            .map(|m| m.metric)
            .collect();
        assert_eq!(
            compared,
            vec![
                MetricId::AvgFps,
                MetricId::Low1Fps,
                MetricId::Low01Fps,
                MetricId::StuttersPerMin
            ]
        );
        let mut other_route = current.clone();
        other_route.route_version = 2;
        let c = compare(&base, &other_route);
        assert!(c.route_mismatch && c.metrics.is_empty());
    }

    #[test]
    fn unknown_settings_never_differ() {
        let mut base = bench("a", vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        let mut current = bench("b", vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        current.context.game_settings = Some(ValorantSettings {
            resolution: Some([1920, 1080]),
            ..Default::default()
        });
        assert!(compare(&base, &current).settings_differ.is_empty());
        base.context.game_settings = Some(ValorantSettings {
            resolution: Some([1280, 720]),
            ..Default::default()
        });
        assert_eq!(
            compare(&base, &current).settings_differ,
            vec!["Resolution".to_string()]
        );
    }

    #[test]
    fn the_graph_keeps_each_buckets_slowest_frame() {
        let times = [1.0, 2.0, 9.0, 1.0, 1.0, 1.0, 1.0, 5.0, 1.0, 3.0];
        assert_eq!(graph_points(&times, 3), vec![9.0, 5.0, 3.0]);
        assert_eq!(graph_points(&times, 100), times.to_vec());
        assert!(graph_points(&[], 3).is_empty() && graph_points(&times, 0).is_empty());
    }

    #[test]
    fn the_middle_run_draws_the_graph() {
        let runs = vec![
            run(430.0, 1.0, 0.0, None, Bottleneck::Gpu),
            run(400.0, 1.0, 0.0, None, Bottleneck::Gpu),
            run(410.0, 1.0, 0.0, None, Bottleneck::Gpu),
        ];
        assert_eq!(middle_run(&runs), Some(2));
        assert_eq!(middle_run(&[]), None);
    }
}
