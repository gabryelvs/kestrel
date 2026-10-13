//! One run's numbers from its frames (spec "Measurements").
use crate::csv::Frame;
use crate::model::{Bottleneck, RunMetrics};

/// The run's metrics, plus its frame times in ms (as f32, for saving and the graph). No frames → all zero.
pub fn measure(frames: &[Frame]) -> (RunMetrics, Vec<f32>) {
    let times: Vec<f64> = frames.iter().map(|f| f.ms).collect();
    let n = times.len();
    if n == 0 {
        return (RunMetrics::default(), Vec::new());
    }
    let total_ms: f64 = times.iter().sum();
    let mut slowest_first = times.clone();
    slowest_first.sort_by(|a, b| b.total_cmp(a));
    let slowest_mean = |share: f64| {
        let k = ((n as f64 * share).ceil() as usize).clamp(1, n);
        slowest_first[..k].iter().sum::<f64>() / k as f64
    };
    let median = percentile(&times, 0.5);
    let stutters = times.iter().filter(|&&t| t > 2.0 * median).count() as f64;
    let duration_s = total_ms / 1000.0;

    let latencies: Vec<f64> = frames.iter().filter_map(|f| f.pc_latency_ms).collect();
    // A zero reading means PresentMon had no GPU data for that frame, so it doesn't count.
    let busy_shares: Vec<f64> = frames
        .iter()
        .filter_map(|f| {
            f.gpu_busy_ms
                .filter(|&busy| busy > 0.0)
                .map(|busy| (busy / f.ms).clamp(0.0, 1.0))
        })
        .collect();
    let gpu_busy_ratio = (!busy_shares.is_empty()).then(|| percentile(&busy_shares, 0.5));
    let bottleneck = match gpu_busy_ratio {
        Some(r) if r >= 0.90 => Bottleneck::Gpu,
        Some(r) if r <= 0.75 => Bottleneck::Cpu,
        Some(_) => Bottleneck::Mixed,
        None => Bottleneck::Unknown,
    };

    let metrics = RunMetrics {
        frames: n as u32,
        duration_s,
        avg_fps: 1000.0 / (total_ms / n as f64),
        low1_fps: 1000.0 / slowest_mean(0.01),
        low01_fps: 1000.0 / slowest_mean(0.001),
        stutters_per_min: stutters / (duration_s / 60.0),
        pc_latency_ms: (!latencies.is_empty())
            .then(|| latencies.iter().sum::<f64>() / latencies.len() as f64),
        gpu_busy_ratio,
        bottleneck,
    };
    (metrics, times.iter().map(|&t| t as f32).collect())
}

/// The nearest-rank percentile: the smallest value with at least `q` (0–1) of the values at or below it.
/// `values` needn't be sorted; empty gives 0.
pub fn percentile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::Frame;
    use crate::model::Bottleneck;

    fn frames(ms: &[f64]) -> Vec<Frame> {
        ms.iter()
            .map(|&ms| Frame {
                ms,
                ..Default::default()
            })
            .collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn steady_frames_give_equal_average_and_lows() {
        let (m, times) = measure(&frames(&[4.0; 22_500]));
        assert_eq!(m.frames, 22_500);
        assert!(close(m.duration_s, 90.0));
        assert!(close(m.avg_fps, 250.0) && close(m.low1_fps, 250.0) && close(m.low01_fps, 250.0));
        assert_eq!(m.stutters_per_min, 0.0);
        assert_eq!(
            (m.pc_latency_ms, m.gpu_busy_ratio, m.bottleneck),
            (None, None, Bottleneck::Unknown)
        );
        assert_eq!(times.len(), 22_500);
        assert_eq!(times[0], 4.0_f32);
    }

    #[test]
    fn the_lows_are_the_mean_of_the_slowest_frames() {
        // 9,990 frames of 4 ms and 10 of 40 ms. Slowest 1% = 100 frames: 10 × 40 + 90 × 4 → mean 7.6 ms.
        let mut ms = vec![4.0; 9_990];
        ms.extend([40.0; 10]);
        let (m, _) = measure(&frames(&ms));
        assert!(close(m.low1_fps, 1000.0 / 7.6));
        assert!(close(m.low01_fps, 25.0));
        assert!(close(m.avg_fps, 1000.0 / 4.036));
        // 10 frames slower than 2 × 4 ms over 40.36 s.
        assert!(close(m.stutters_per_min, 10.0 / (40.36 / 60.0)));
    }

    #[test]
    fn one_long_freeze_drags_the_lows_down() {
        let mut ms = vec![4.0; 22_499];
        ms.push(2_000.0);
        let (m, _) = measure(&frames(&ms));
        assert!(m.avg_fps.is_finite() && m.low1_fps.is_finite() && m.low01_fps.is_finite());
        assert!(close(
            m.low1_fps,
            1000.0 / ((2_000.0 + 224.0 * 4.0) / 225.0)
        ));
        assert!(close(m.low01_fps, 1000.0 / ((2_000.0 + 22.0 * 4.0) / 23.0)));
        assert!(close(m.stutters_per_min, 1.0 / (91.996 / 60.0)));
        // 1,500 FPS in the Range stays finite too.
        let (fast, _) = measure(&frames(&[1000.0 / 1500.0; 135_000]));
        assert!(close(fast.avg_fps, 1500.0) && close(fast.duration_s, 90.0));
    }

    #[test]
    fn gpu_busy_share_decides_the_bottleneck() {
        let with_gpu = |share: f64| -> Vec<Frame> {
            (0..1000)
                .map(|_| Frame {
                    ms: 4.0,
                    gpu_busy_ms: Some(4.0 * share),
                    ..Default::default()
                })
                .collect()
        };
        assert_eq!(measure(&with_gpu(0.95)).0.bottleneck, Bottleneck::Gpu);
        assert_eq!(measure(&with_gpu(0.5)).0.bottleneck, Bottleneck::Cpu);
        assert_eq!(measure(&with_gpu(0.8)).0.bottleneck, Bottleneck::Mixed);
        assert!(close(
            measure(&with_gpu(0.8)).0.gpu_busy_ratio.unwrap(),
            0.8
        ));
        // Busy longer than the frame (overlapping work) counts as fully busy.
        assert!(close(
            measure(&with_gpu(1.3)).0.gpu_busy_ratio.unwrap(),
            1.0
        ));
        // Zero GPU-busy time on every frame means PresentMon sent no GPU numbers: unknown.
        assert_eq!(measure(&with_gpu(0.0)).0.gpu_busy_ratio, None);
        assert_eq!(measure(&with_gpu(0.0)).0.bottleneck, Bottleneck::Unknown);
    }

    #[test]
    fn pc_latency_is_the_mean_of_frames_that_have_it() {
        let f = vec![
            Frame {
                ms: 4.0,
                pc_latency_ms: Some(10.0),
                ..Default::default()
            },
            Frame {
                ms: 4.0,
                pc_latency_ms: Some(20.0),
                ..Default::default()
            },
            Frame {
                ms: 4.0,
                ..Default::default()
            },
        ];
        assert_eq!(measure(&f).0.pc_latency_ms, Some(15.0));
    }

    #[test]
    fn no_frames_gives_zeroes_not_nan() {
        let (m, times) = measure(&[]);
        assert_eq!(m, crate::model::RunMetrics::default());
        assert!(times.is_empty());
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        assert_eq!(percentile(&[4.0, 1.0, 3.0, 2.0], 0.5), 2.0);
        assert_eq!(percentile(&[4.0, 1.0, 3.0, 2.0], 0.95), 4.0);
        assert_eq!(percentile(&[4.0, 1.0, 3.0, 2.0], 0.0), 1.0);
    }
}
