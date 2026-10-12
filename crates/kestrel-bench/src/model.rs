//! What a benchmark measures and keeps. Sent to the UI and saved as JSON: camelCase fields, kebab-case enums.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The name the first saved benchmark gets (decision 41: it becomes the baseline).
pub const STARTING_POINT: &str = "Starting point";

/// What limits the frame rate in a run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Bottleneck {
    Gpu,
    Cpu,
    Mixed,
    #[default]
    Unknown,
}

/// The numbers from one capture (spec "Measurements").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RunMetrics {
    pub frames: u32,
    pub duration_s: f64,
    pub avg_fps: f64,
    pub low1_fps: f64,
    pub low01_fps: f64,
    pub stutters_per_min: f64,
    /// NVIDIA Reflex PC latency; None when the game didn't report it.
    pub pc_latency_ms: Option<f64>,
    /// Median share of each frame the GPU was busy; None when PresentMon had no GPU numbers.
    pub gpu_busy_ratio: Option<f64>,
    pub bottleneck: Bottleneck,
}

/// Why a run can't be trusted (decision 44).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Flag {
    TooShort,
    NotInFront,
    FpsCapped,
    DroppedData,
    BusyBackground { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RunRecord {
    pub metrics: RunMetrics,
    pub flags: Vec<Flag>,
    /// The owner chose "Keep anyway" for this flagged run.
    pub kept: bool,
}

/// How far apart the runs were: (max − min) / middle value, so 0.03 means 3%.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Spread {
    pub avg_fps: f64,
    pub low1_fps: f64,
    pub low01_fps: f64,
}

/// A benchmark's result: the middle value of each metric over its runs (decision 38).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BenchResult {
    pub metrics: RunMetrics,
    pub spread: Spread,
}

/// VALORANT's video settings as read from its files (decision 43). None: the file doesn't hold the setting.
/// GameUserSettings.ini lists every setting. RiotUserSettings.ini lists only those changed from the default, so
/// there None means the default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ValorantSettings {
    pub resolution: Option<[u32; 2]>,
    pub fullscreen_mode: Option<u8>,
    pub vsync: Option<bool>,
    pub frame_rate_limit: Option<f32>,
    pub reflex: Option<u8>,
    pub multithreaded_rendering: Option<bool>,
    /// Quality settings by key as written, e.g. "sg.ShadowQuality" → "0".
    pub quality: BTreeMap<String, String>,
    /// Any frame-limit setting found in RiotUserSettings.ini, by key as written.
    pub limits: BTreeMap<String, String>,
    /// Whether RiotUserSettings.ini was read. It holds only settings changed from their default, so when both
    /// sides read it, a Riot setting one side lacks is at its default there.
    pub riot_settings_read: bool,
}

/// The PC and game when the benchmark ran (spec "Saved data").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Context {
    pub power_plan: Option<String>,
    pub ram_mts: Option<u32>,
    pub gpu: Option<String>,
    pub gpu_driver: Option<String>,
    pub windows_build: u32,
    pub display_hz: Option<u32>,
    pub game_settings: Option<ValorantSettings>,
}

/// One saved benchmark.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Benchmark {
    pub id: String,
    pub name: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    pub profile_id: String,
    pub route_version: u32,
    pub kestrel_version: String,
    pub presentmon_version: String,
    pub runs: Vec<RunRecord>,
    pub result: BenchResult,
    pub context: Context,
}

/// One row of the results list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BenchmarkSummary {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub profile_id: String,
    pub route_version: u32,
    pub result: BenchResult,
    pub is_baseline: bool,
    pub flagged_runs_kept: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MetricId {
    AvgFps,
    Low1Fps,
    Low01Fps,
    StuttersPerMin,
    PcLatencyMs,
}

/// Decision 41: "real" only when every run on one side beats every run on the other.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    RealGain,
    RealLoss,
    Noise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricDelta {
    pub metric: MetricId,
    pub baseline: f64,
    pub current: f64,
    /// The change in %, or None when the baseline is 0 (no percentage exists).
    /// (current − baseline) / baseline × 100.
    pub delta_pct: Option<f64>,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Comparison {
    pub baseline_id: String,
    pub current_id: String,
    pub metrics: Vec<MetricDelta>,
    /// VALORANT settings that differ between the two, counted as `settings::differences` counts them.
    pub settings_differ: Vec<String>,
    /// Different game or route version: not compared, `metrics` is empty.
    pub route_mismatch: bool,
}
