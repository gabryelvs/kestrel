//! A benchmark's result from its runs (decision 38), comparisons with another benchmark (decision 41), and the
//! frame-time graph's points.
use crate::model::{
    BenchResult, Benchmark, Bottleneck, Comparison, Context, MetricDelta, MetricId, RunMetrics,
    RunRecord, Spread, Verdict,
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

/// Each metric both benchmarks have, with its change and whether that change is real. A different game or route
/// version gives no metrics and sets `route_mismatch`.
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
            delta_pct: (base != 0.0).then(|| (now - base) / base * 100.0),
            verdict: verdict(&base_runs, &now_runs, higher_is_better),
        });
    }
    if let (Some(a), Some(b)) = (
        &baseline.context.game_settings,
        &current.context.game_settings,
    ) {
        comparison.settings_differ = differences(a, b);
    }
    // A side with no list (a benchmark from before stage 5) is unknown, not empty, so it compares nothing.
    if let (Some(before), Some(now)) = (
        &baseline.context.applied_tweaks,
        &current.context.applied_tweaks,
    ) {
        comparison.tweaks_added = only_in(now, before);
        comparison.tweaks_removed = only_in(before, now);
    }
    comparison.power_plan_change = plan_change(&baseline.context, &current.context);
    comparison
}

/// The ids in `list` that `other` lacks, in `list`'s own order and each once: a hand-edited file may repeat an id.
fn only_in(list: &[String], other: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for id in list {
        if !other.contains(id) && !found.contains(id) {
            found.push(id.clone());
        }
    }
    found
}

/// The power plan's name on each side, [baseline, current], when both recorded one and the names differ. A blank name
/// is no name, so a side that has none has nothing to compare.
fn plan_change(baseline: &Context, current: &Context) -> Option<[String; 2]> {
    let name = |context: &Context| -> Option<String> {
        let name = context.power_plan.as_deref()?.trim();
        (!name.is_empty()).then(|| name.to_string())
    };
    let (before, now) = (name(baseline)?, name(current)?);
    (before != now).then_some([before, now])
}

/// The widest spread (see `Spread`) that still counts as runs close together: 10%.
const MAX_SPREAD: f64 = 0.10;

/// Whether a result's runs are too far apart for a small change to count as real: a spread strictly above 10%. The
/// page's `variesTooMuch` uses the same line.
pub fn varies_too_much(spread: f64) -> bool {
    spread > MAX_SPREAD
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
        assert!(
            (get(MetricId::AvgFps).delta_pct.unwrap() - (425.0 - 405.0) / 405.0 * 100.0).abs()
                < 1e-9
        );
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
        // Zero stutters before, four after: a real change with no percentage to show.
        let zero = bench("c", vec![run(400.0, 280.0, 0.0, None, Bottleneck::Gpu); 3]);
        let worse = bench("d", vec![run(400.0, 280.0, 4.0, None, Bottleneck::Gpu); 3]);
        let from_zero = compare(&zero, &worse);
        let row = from_zero
            .metrics
            .iter()
            .find(|m| m.metric == MetricId::StuttersPerMin)
            .unwrap();
        assert_eq!(row.delta_pct, None, "a zero baseline has no percentage");
        assert_eq!(
            row.verdict,
            Verdict::RealLoss,
            "the change is real from a zero baseline"
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

    /// A benchmark with the same runs as any other, whose context names the active tweaks (None: no list).
    fn with_tweaks(id: &str, tweaks: Option<&[&str]>) -> Benchmark {
        let mut b = bench(id, vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        b.context.applied_tweaks =
            tweaks.map(|list| list.iter().map(|id| id.to_string()).collect());
        b
    }

    /// A benchmark whose context names the power plan (None: the scan didn't read one).
    fn on_plan(id: &str, plan: Option<&str>) -> Benchmark {
        let mut b = bench(id, vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        b.context.power_plan = plan.map(str::to_string);
        b
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn the_comparison_lists_tweak_changes() {
        let base = with_tweaks("a", Some(&["game-mode"]));
        let now = with_tweaks("b", Some(&["game-mode", "power-plan"]));
        let c = compare(&base, &now);
        assert_eq!(c.tweaks_added, ids(&["power-plan"]));
        assert!(c.tweaks_removed.is_empty());
        // Read the other way round, the same tweak was taken off.
        let back = compare(&now, &base);
        assert!(back.tweaks_added.is_empty());
        assert_eq!(back.tweaks_removed, ids(&["power-plan"]));
        // Both at once.
        let swapped = compare(
            &with_tweaks("a", Some(&["game-mode"])),
            &with_tweaks("b", Some(&["power-plan"])),
        );
        assert_eq!(swapped.tweaks_added, ids(&["power-plan"]));
        assert_eq!(swapped.tweaks_removed, ids(&["game-mode"]));
        // A list with nothing in it is a list: no tweaks then, so everything now is new.
        let from_none = compare(
            &with_tweaks("a", Some(&[])),
            &with_tweaks("b", Some(&["game-mode"])),
        );
        assert_eq!(from_none.tweaks_added, ids(&["game-mode"]));
        assert!(from_none.tweaks_removed.is_empty());
        // The same tweaks, in any order, are no change.
        let same = compare(
            &with_tweaks("a", Some(&["game-mode", "power-plan"])),
            &with_tweaks("b", Some(&["power-plan", "game-mode"])),
        );
        assert!(same.tweaks_added.is_empty() && same.tweaks_removed.is_empty());
    }

    #[test]
    fn tweak_changes_keep_each_lists_own_order_and_name_an_id_once() {
        // Not sorted: added follows the current list's order, removed follows the baseline's.
        let c = compare(
            &with_tweaks("a", Some(&["power-plan", "game-mode", "gpu-preference"])),
            &with_tweaks(
                "b",
                Some(&[
                    "gpu-preference",
                    "windowed-optimizations",
                    "background-recording",
                ]),
            ),
        );
        assert_eq!(
            c.tweaks_added,
            ids(&["windowed-optimizations", "background-recording"])
        );
        assert_eq!(c.tweaks_removed, ids(&["power-plan", "game-mode"]));
        // A hand-edited file may name an id twice; it is still one change.
        let doubled = compare(
            &with_tweaks("a", Some(&["game-mode", "game-mode"])),
            &with_tweaks("b", Some(&["power-plan", "power-plan", "power-plan"])),
        );
        assert_eq!(doubled.tweaks_added, ids(&["power-plan"]));
        assert_eq!(doubled.tweaks_removed, ids(&["game-mode"]));
        // An id this build doesn't know (an old or a future file) is listed as written, System Protection like any other.
        let unknown = compare(
            &with_tweaks("a", Some(&["from-the-future"])),
            &with_tweaks("b", Some(&["system-protection", "Game-Mode"])),
        );
        assert_eq!(
            unknown.tweaks_added,
            ids(&["system-protection", "Game-Mode"])
        );
        assert_eq!(unknown.tweaks_removed, ids(&["from-the-future"]));
    }

    #[test]
    fn a_side_with_no_list_compares_nothing() {
        let list = with_tweaks("b", Some(&["game-mode", "power-plan"]));
        let empty = with_tweaks("c", Some(&[]));
        for (base, now) in [
            (with_tweaks("a", None), list.clone()),
            (list.clone(), with_tweaks("a", None)),
            (with_tweaks("a", None), with_tweaks("d", None)),
            (with_tweaks("a", None), empty.clone()),
            (empty.clone(), with_tweaks("a", None)),
        ] {
            let c = compare(&base, &now);
            assert!(
                c.tweaks_added.is_empty() && c.tweaks_removed.is_empty(),
                "{:?} against {:?}: {c:?}",
                base.context.applied_tweaks,
                now.context.applied_tweaks
            );
        }
    }

    #[test]
    fn a_changed_plan_is_named() {
        let c = compare(
            &on_plan("a", Some("EXM Free Power Plan V8")),
            &on_plan("b", Some("Kestrel")),
        );
        assert_eq!(
            c.power_plan_change,
            Some(["EXM Free Power Plan V8".to_string(), "Kestrel".to_string()])
        );
        // The same name is no change, and a side with no name has nothing to compare.
        for (base, now) in [
            (Some("Kestrel"), Some("Kestrel")),
            (None, Some("Kestrel")),
            (Some("Kestrel"), None),
            (None, None),
        ] {
            assert_eq!(
                compare(&on_plan("a", base), &on_plan("b", now)).power_plan_change,
                None,
                "{base:?} then {now:?}"
            );
        }
    }

    #[test]
    fn a_blank_plan_name_is_a_missing_side() {
        for blank in ["", " ", "\t \n"] {
            for (base, now) in [(blank, "Kestrel"), ("Kestrel", blank), (blank, blank)] {
                assert_eq!(
                    compare(&on_plan("a", Some(base)), &on_plan("b", Some(now))).power_plan_change,
                    None,
                    "{base:?} then {now:?}"
                );
            }
        }
        // Spaces around a name are not part of it: the same plan is no change, and a change is shown without them.
        let padded = compare(
            &on_plan("a", Some(" Kestrel ")),
            &on_plan("b", Some("Kestrel")),
        );
        assert_eq!(padded.power_plan_change, None);
        let renamed = compare(
            &on_plan("a", Some(" Balanced")),
            &on_plan("b", Some("Kestrel ")),
        );
        assert_eq!(
            renamed.power_plan_change,
            Some(["Balanced".to_string(), "Kestrel".to_string()])
        );
    }

    #[test]
    fn the_plan_and_the_tweaks_change_independently() {
        let mut base = with_tweaks("a", Some(&["game-mode"]));
        base.context.power_plan = Some("Balanced".into());
        let mut now = with_tweaks("b", Some(&["game-mode"]));
        now.context.power_plan = Some("High performance".into());
        let c = compare(&base, &now);
        assert!(c.tweaks_added.is_empty() && c.tweaks_removed.is_empty());
        assert_eq!(
            c.power_plan_change,
            Some(["Balanced".to_string(), "High performance".to_string()])
        );
        // And the other way: a tweak list with no plan names on either side.
        let c = compare(
            &with_tweaks("a", Some(&[])),
            &with_tweaks("b", Some(&["power-plan"])),
        );
        assert_eq!(c.tweaks_added, ids(&["power-plan"]));
        assert_eq!(c.power_plan_change, None);
    }

    #[test]
    fn another_route_compares_no_tweaks_or_plan_either() {
        let mut base = with_tweaks("a", Some(&[]));
        base.context.power_plan = Some("Balanced".into());
        let mut now = with_tweaks("b", Some(&["power-plan"]));
        now.context.power_plan = Some("Kestrel".into());
        now.route_version = 2;
        let c = compare(&base, &now);
        assert!(c.route_mismatch);
        assert!(c.tweaks_added.is_empty() && c.tweaks_removed.is_empty());
        assert_eq!(c.power_plan_change, None);
    }

    #[test]
    fn the_page_reads_the_tweak_fields_by_their_camel_case_names() {
        let mut base = with_tweaks("a", Some(&["game-mode"]));
        base.context.power_plan = Some("EXM Free Power Plan V8".into());
        let mut now = with_tweaks("b", Some(&["power-plan"]));
        now.context.power_plan = Some("Kestrel".into());
        let json = serde_json::to_value(compare(&base, &now)).unwrap();
        assert_eq!(json["tweaksAdded"], serde_json::json!(["power-plan"]));
        assert_eq!(json["tweaksRemoved"], serde_json::json!(["game-mode"]));
        assert_eq!(
            json["powerPlanChange"],
            serde_json::json!(["EXM Free Power Plan V8", "Kestrel"])
        );
        // With nothing to say they are still there, as empty lists and null, so the page never has to check.
        let quiet =
            serde_json::to_value(compare(&on_plan("a", None), &on_plan("b", None))).unwrap();
        assert_eq!(quiet["tweaksAdded"], serde_json::json!([]));
        assert_eq!(quiet["tweaksRemoved"], serde_json::json!([]));
        assert_eq!(quiet["powerPlanChange"], serde_json::Value::Null);
        // The benchmark's own list is `appliedTweaks`, as the store test pins.
        let ctx = serde_json::to_value(&now.context).unwrap();
        assert_eq!(ctx["appliedTweaks"], serde_json::json!(["power-plan"]));
    }

    #[test]
    fn runs_varying_over_ten_percent_are_flagged() {
        assert!(varies_too_much(0.249));
        assert!(!varies_too_much(0.10));
        // Strictly above 10%: the line itself is fine, anything over it is not.
        assert!(varies_too_much(0.1001));
        assert!(!varies_too_much(0.0999));
        assert!(!varies_too_much(0.05));
        assert!(!varies_too_much(0.0), "runs that agree exactly");
        assert!(
            !varies_too_much(f64::NAN),
            "no reading is not a wide spread"
        );
        // The spread a result carries is the one `combine` computes: 305 to 410 around a middle of 407 is about 26%.
        let wide = combine(&[
            run(305.0, 1.0, 0.0, None, Bottleneck::Gpu),
            run(410.0, 1.0, 0.0, None, Bottleneck::Gpu),
            run(407.0, 1.0, 0.0, None, Bottleneck::Gpu),
        ]);
        assert!(varies_too_much(wide.spread.avg_fps), "{:?}", wide.spread);
        let steady = combine(&[
            run(400.0, 1.0, 0.0, None, Bottleneck::Gpu),
            run(410.0, 1.0, 0.0, None, Bottleneck::Gpu),
            run(405.0, 1.0, 0.0, None, Bottleneck::Gpu),
        ]);
        assert!(!varies_too_much(steady.spread.avg_fps));
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
