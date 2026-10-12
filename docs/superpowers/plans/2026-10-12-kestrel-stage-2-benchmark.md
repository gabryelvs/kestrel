# Kestrel Stage 2 (Benchmark) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Kestrel v0.2.0: a VALORANT benchmark that captures frames with PresentMon during a fixed Range routine (warm-up + 3 runs started with F10), saves the result in Kestrel's own folder as the owner's starting point, and compares later results against it, telling real changes from noise.

**Architecture:** A new pure crate, `kestrel-bench`, holds game profiles, PresentMon CSV parsing, metrics, run flags, the middle-of-three result and comparisons, VALORANT settings parsing, storage and the session state machine — all tested on macOS. `kestrel-probe` gains a handle-free "is the game running" check and a public background-CPU sampler. The Tauri app runs the bundled PresentMon as a hidden child process and drives sessions through a `Controller` whose platform (hotkey, beeps, probes, events) and capture engine are injected, so its flow is tested on the Mac with fakes. The React UI adds the Benchmark page, a Home card, a first-run step and an About section, in the current visual style.

**Tech Stack:** Rust workspace (serde, chrono, tempfile for tests), Tauri 2.12.2 + `tauri-plugin-global-shortcut` 2.4.0, `windows-sys` 0.61.2, Intel PresentMon 2.6.0 (bundled at build time), React 19 + TypeScript 7 + Vite 8 + Vitest 5, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-12-kestrel-stage-2-benchmark-design.md` (decisions 36–66 in `docs/superpowers/specs/2026-10-09-kestrel-design.md`; VALORANT settings keys in `docs/superpowers/notes/2026-10-12-valorant-settings-keys.md`).

## Global Constraints

- Kestrel may write files only inside its own app-data folder (`app.path().app_data_dir()`, i.e. `%APPDATA%\io.github.gabryelvs.kestrel`); nothing else on the PC changes.
- Never read or write any game's memory, never inject, never load a kernel driver, never start, stop or change Vanguard or any anti-cheat (querying state is allowed).
- Kestrel's own code never opens a handle to a game process: "is the game running" uses a Toolhelp32 process snapshot. Only the bundled PresentMon opens a query-only handle (owner approved, decision 46).
- No network calls at runtime except opening links. PresentMon is downloaded only at build time: `PresentMon-2.6.0-x64.exe`, 980,320 bytes, SHA-256 `b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af`, MIT licence shipped beside it.
- PresentMon runs only during a capture, hidden, with ETW session name `KestrelBench`.
- F10 is registered only while a benchmark session is armed.
- VALORANT's settings files are only read, never written.
- Version `0.2.0` everywhere (workspace, `tauri.conf.json`, `app/package.json`).
- Every Rust type crossing to the UI is `serde` with `rename_all = "camelCase"`; enums `rename_all = "kebab-case"`.
- All UI copy lives in `app/src/strings.ts`: English, sentence case, no exclamation marks. Text built in Rust (the Copy results text, the VALORANT setting names from `settings::differences`, error messages) is the exception, so each piece has one source. Keep the current look (tokens.css/base.css classes); no redesign in this stage.
- Commits end with a `Co-Authored-By:` line naming the model that wrote the commit (stage 1 ruling).
- `cargo` is in `~/.cargo/bin`; prefix commands with `PATH="$HOME/.cargo/bin:$PATH"` where it isn't on PATH. Windows-only Rust is checked on the Mac with `cargo clippy --target x86_64-pc-windows-msvc` (non-Tauri crates) and built on CI.

## Review Focus

1. A capture that recorded nothing or garbage (game never in front, PresentMon writes only a header, `NA` frame times, reordered columns) must give a flagged "Too short" run with finite numbers — never a panic, NaN or a crash. Pinned by `metrics::tests::no_frames_gives_zeroes_not_nan` (Task 3) and `flags::tests::empty_capture_is_only_too_short` (Task 5).
2. A single long freeze (a 2-second hitch) or 1,000+ FPS in the Range must stay finite and show in the 0.1% low and stutters. Pinned by `metrics::tests::one_long_freeze_drags_the_lows_down` (Task 3).
3. VALORANT never run, settings in another account's folder, UTF-16 files or junk lines: settings are "unknown", and comparisons never claim settings differ on missing data. Pinned by `settings::tests::missing_and_odd_files_are_unknown_not_errors` (Task 4) and `result::tests::unknown_settings_never_differ` (Task 6).
4. Saved data damaged by hand (index missing or corrupt, a benchmark folder deleted, renaming to blanks, deleting the baseline): the list skips what it can't read, blank names are refused, and deleting the baseline clears it. Pinned by `store::tests::damaged_folders_are_skipped_not_fatal` and `store::tests::rename_delete_and_baseline` (Task 7).
5. F10 pressed twice, during a capture, before the game runs, or Cancel during a capture: never two captures at once, nothing starts without the game, and Cancel stops the capture. Pinned by `session::tests::start_key_is_ignored_unless_waiting_with_the_game` (Task 8) and `controller::tests::cancel_during_capture_stops_it` (Task 12).

## File Structure

```
Cargo.toml                                   workspace: + crates/kestrel-bench, version 0.2.0
crates/kestrel-bench/                        NEW pure crate (tested on macOS)
├─ Cargo.toml
└─ src/
   ├─ lib.rs                                 module list + re-exports
   ├─ model.rs                               saved/sent data types (metrics, flags, runs, benchmarks, comparisons, settings)
   ├─ profile.rs                             GameProfile + Route; VALORANT (Route v1)
   ├─ csv.rs                                 PresentMon CSV → frames (by column name)
   ├─ metrics.rs                             frames → RunMetrics + frame times
   ├─ settings.rs                            VALORANT settings files → ValorantSettings; differences
   ├─ flags.rs                               run → flags (decision 44)
   ├─ result.rs                              middle-of-three result, comparison verdicts, graph points
   ├─ store.rs                               benchmarks folder: index, summaries, frame files
   ├─ session.rs                             session state machine (warm-up, runs 1–3, review, cancel)
   └─ text.rs                                "Copy results" text
crates/kestrel-probe/src/process.rs          NEW game_running() via Toolhelp32 (no handles)
crates/kestrel-probe/src/{lib.rs,parse.rs,win/system_info.rs}   busy_now(), exe-name helper
app/scripts/fetch-presentmon.mjs             NEW build-time download + SHA-256 check
app/src-tauri/tauri.windows.conf.json        NEW bundles PresentMon as resources (Windows builds only)
app/src-tauri/src/bench/                     NEW
├─ mod.rs                                    module list
├─ beep.rs                                   Windows Beep cues
├─ capture.rs                                Capture trait; PresentMon runner; simulated capture (non-Windows)
├─ controller.rs                             session runner: threads, hotkey, capture → analyse → session
└─ commands.rs                               Tauri commands, platform impl, diagnostics
app/src-tauri/src/lib.rs                     plugin + state + commands wiring
app/src/api/{types.ts,client.ts,bench.ts,bench.test.ts,sample-bench.ts}   bench types, client + browser fakes, helpers
app/src/pages/Benchmark.tsx                  NEW page
app/src/components/bench/*.tsx               NEW RouteCard, SessionPanel, ResultsList, ResultDetail, FrameGraph
app/src/components/LastBenchmark.tsx         NEW Home card
app/src/styles/bench.css                     NEW
app/src/{App.tsx,strings.ts,main.tsx,pages/Home.tsx,pages/Toolbox.tsx,first-run/FirstRun.tsx}   wiring
.github/workflows/{ci.yml,release.yml}       fetch PresentMon, smoke capture, checks
```

---

### Task 1: `kestrel-bench` crate, game profiles, data model, version 0.2.0

**Files:**
- Modify: `Cargo.toml` (members, version), `crates/kestrel-probe/Cargo.toml` (kestrel-core version), `app/src-tauri/tauri.conf.json` (version), `app/package.json` + `app/package-lock.json` (version), `app/src/api/mock-report.json` (appVersion)
- Create: `crates/kestrel-bench/Cargo.toml`, `crates/kestrel-bench/src/lib.rs`, `crates/kestrel-bench/src/profile.rs`, `crates/kestrel-bench/src/model.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `kestrel_bench::{GameProfile, Route, VALORANT, PROFILES, profile(id) -> Option<&'static GameProfile>}`; model types `Bottleneck {Gpu, Cpu, Mixed, Unknown}`, `RunMetrics {frames: u32, duration_s, avg_fps, low1_fps, low01_fps, stutters_per_min: f64, pc_latency_ms: Option<f64>, gpu_busy_ratio: Option<f64>, bottleneck}`, `Flag {TooShort, NotInFront, FpsCapped, DroppedData, BusyBackground {name}}` (JSON tag `kind`), `RunRecord {metrics, flags, kept}`, `Spread {avg_fps, low1_fps, low01_fps}`, `BenchResult {metrics, spread}`, `ValorantSettings {resolution: Option<[u32;2]>, fullscreen_mode: Option<u8>, vsync: Option<bool>, frame_rate_limit: Option<f32>, reflex: Option<u8>, multithreaded_rendering: Option<bool>, quality: BTreeMap<String,String>, limits: BTreeMap<String,String>}`, `Context {power_plan, ram_mts, gpu, gpu_driver, windows_build, display_hz, game_settings}`, `Benchmark {id, name, created_at, profile_id, route_version, kestrel_version, presentmon_version, runs, result, context}`, `BenchmarkSummary {id, name, created_at, profile_id, route_version, result, is_baseline, flagged_runs_kept}`, `MetricId`, `Verdict {RealGain, RealLoss, Noise}`, `MetricDelta {metric, baseline, current, delta_pct, verdict}`, `Comparison {baseline_id, current_id, metrics, settings_differ, route_mismatch}`; constants `PRESENTMON_VERSION = "2.6.0"`, `STARTING_POINT = "Starting point"`.

- [ ] **Step 1: Bump the version and add the crate to the workspace**

`Cargo.toml` (whole file):
```toml
[workspace]
resolver = "2"
members = ["crates/kestrel-core", "crates/kestrel-probe", "crates/kestrel-bench", "app/src-tauri"]

[workspace.package]
edition = "2021"
license = "GPL-3.0-only"
version = "0.2.0"
repository = "https://github.com/gabryelvs/kestrel"
```

In `crates/kestrel-probe/Cargo.toml` change the kestrel-core line to:
```toml
kestrel-core = { version = "0.2.0", path = "../kestrel-core" }
```

In `app/src-tauri/tauri.conf.json` change `"version": "0.1.0"` to `"version": "0.2.0"`.

In `app/src/api/mock-report.json` (the browser-preview report) change `"appVersion": "0.1.0"` to `"appVersion": "0.2.0"`.

Run: `cd app && npm version 0.2.0 --no-git-tag-version && cd ..`
Expected: prints `v0.2.0`; `app/package.json` and `app/package-lock.json` now say `0.2.0`.

`crates/kestrel-bench/Cargo.toml`:
```toml
[package]
name = "kestrel-bench"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.151"

[dev-dependencies]
tempfile = "3.27.0"
```

- [ ] **Step 2: Write the failing tests** — `crates/kestrel-bench/src/lib.rs`:

```rust
//! Kestrel bench: turns PresentMon's output into benchmark results and keeps them. Pure Rust: the app runs the
//! capture; this crate only reads what PresentMon wrote, so it never touches a game.
pub mod model;
pub mod profile;

pub use model::*;
pub use profile::{profile, GameProfile, Route, PROFILES, VALORANT};

/// The PresentMon release Kestrel bundles (spec "PresentMon").
pub const PRESENTMON_VERSION: &str = "2.6.0";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valorant_is_the_only_profile_and_uses_route_v1() {
        assert_eq!(PROFILES.len(), 1);
        let v = profile("valorant").expect("valorant profile");
        assert_eq!(v.process_name, "VALORANT-Win64-Shipping.exe");
        assert_eq!(v.route, Route { version: 1, capture_seconds: 90, switch_at_seconds: 30 });
        assert!(profile("fivem").is_none());
    }

    #[test]
    fn json_uses_camel_case_fields_and_kebab_case_tags() {
        let m = RunMetrics { low1_fps: 1.0, low01_fps: 2.0, pc_latency_ms: Some(3.0), bottleneck: Bottleneck::Gpu, ..Default::default() };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["low1Fps"], json!(1.0));
        assert_eq!(v["low01Fps"], json!(2.0));
        assert_eq!(v["pcLatencyMs"], json!(3.0));
        assert_eq!(v["stuttersPerMin"], json!(0.0));
        assert_eq!(v["durationS"], json!(0.0));
        assert_eq!(v["bottleneck"], json!("gpu"));
        assert_eq!(serde_json::to_value(Flag::BusyBackground { name: "chrome.exe".into() }).unwrap(), json!({"kind": "busy-background", "name": "chrome.exe"}));
        assert_eq!(serde_json::to_value(Flag::NotInFront).unwrap(), json!({"kind": "not-in-front"}));
        assert_eq!(serde_json::to_value(Verdict::RealGain).unwrap(), json!("real-gain"));
        assert_eq!(serde_json::to_value(MetricId::Low01Fps).unwrap(), json!("low01-fps"));
        let p = serde_json::to_value(VALORANT).unwrap();
        assert_eq!(p["processName"], json!("VALORANT-Win64-Shipping.exe"));
        assert_eq!(p["route"]["switchAtSeconds"], json!(30));
    }

    #[test]
    fn a_benchmark_round_trips_through_json() {
        let b = Benchmark {
            id: "20261012-213005".into(),
            name: STARTING_POINT.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            runs: vec![RunRecord { flags: vec![Flag::TooShort], kept: true, ..Default::default() }],
            context: Context { game_settings: Some(ValorantSettings { resolution: Some([1920, 1080]), ..Default::default() }), ..Default::default() },
            ..Default::default()
        };
        let text = serde_json::to_string(&b).unwrap();
        assert_eq!(serde_json::from_str::<Benchmark>(&text).unwrap(), b);
    }
}
```

Run: `cargo test -p kestrel-bench`
Expected: FAIL — `file not found for module model` / `profile`.

- [ ] **Step 3: Implement `crates/kestrel-bench/src/profile.rs`**

```rust
//! Game profiles (decision 55). Stage 2 has one: VALORANT with Route v1.
use serde::Serialize;

/// The routine a benchmark follows. Results from different route versions are never compared.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub version: u32,
    /// How long each capture lasts.
    pub capture_seconds: u32,
    /// When the short beep tells the player to switch from the walk to the bots.
    pub switch_at_seconds: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameProfile {
    pub id: &'static str,
    pub name: &'static str,
    /// The game's process, matched by name only (Kestrel never opens it).
    pub process_name: &'static str,
    pub route: Route,
}

pub const VALORANT: GameProfile = GameProfile {
    id: "valorant",
    name: "VALORANT",
    process_name: "VALORANT-Win64-Shipping.exe",
    route: Route { version: 1, capture_seconds: 90, switch_at_seconds: 30 },
};

pub const PROFILES: &[GameProfile] = &[VALORANT];

pub fn profile(id: &str) -> Option<&'static GameProfile> {
    PROFILES.iter().find(|p| p.id == id)
}
```

- [ ] **Step 4: Implement `crates/kestrel-bench/src/model.rs`**

```rust
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

/// VALORANT's video settings as read from its files (decision 43). None: not found, or never changed from the
/// default (VALORANT only writes settings you changed).
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
    /// (current − baseline) / baseline × 100.
    pub delta_pct: f64,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Comparison {
    pub baseline_id: String,
    pub current_id: String,
    pub metrics: Vec<MetricDelta>,
    /// VALORANT settings that differ between the two (only settings both sides know).
    pub settings_differ: Vec<String>,
    /// Different game or route version: not compared, `metrics` is empty.
    pub route_mismatch: bool,
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `3 passed`; clippy clean.

Run: `cargo test -p kestrel-core -p kestrel-probe`
Expected: all pass (the version bump changes nothing else).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/kestrel-bench crates/kestrel-probe/Cargo.toml app/src-tauri/tauri.conf.json app/package.json app/package-lock.json app/src/api/mock-report.json
git commit -m "feat(bench): kestrel-bench crate with game profiles and the benchmark data model; version 0.2.0"
```

---
### Task 2: PresentMon CSV parser

**Files:**
- Create: `crates/kestrel-bench/src/csv.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod csv;`)

**Interfaces:**
- Consumes: nothing.
- Produces: `kestrel_bench::csv::{Frame {ms: f64, gpu_busy_ms: Option<f64>, pc_latency_ms: Option<f64>, present_mode: String}, CsvError {Empty, MissingColumn(&'static str)}, parse(text: &str) -> Result<Vec<Frame>, CsvError>}`.

- [ ] **Step 1: Write the failing tests** — create `crates/kestrel-bench/src/csv.rs` with only the tests, and add `pub mod csv;` to `lib.rs` after `pub mod model;`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// PresentMon 2.6.0's default columns (verified against its source and test files).
    const HEADER: &str = "Application,ProcessID,SwapChainAddress,PresentRuntime,SyncInterval,PresentFlags,AllowsTearing,PresentMode,TimeInMs,MsBetweenSimulationStart,MsBetweenPresents,MsBetweenDisplayChange,MsInPresentAPI,MsRenderPresentLatency,MsUntilDisplayed,MsPCLatency,CPUStartTimeInMs,MsBetweenAppStart,MsCPUBusy,MsCPUWait,MsGPULatency,MsGPUTime,MsGPUBusy,MsGPUWait,MsAnimationError,AnimationTime,MsFlipDelay,MsAllInputToPhotonLatency,MsClickToPhotonLatency";

    fn row(ms: &str, gpu: &str, latency: &str, mode: &str) -> String {
        HEADER
            .split(',')
            .map(|column| match column {
                "Application" => "VALORANT-Win64-Shipping.exe",
                "PresentMode" => mode,
                "MsBetweenPresents" => ms,
                "MsGPUBusy" => gpu,
                "MsPCLatency" => latency,
                _ => "NA",
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn parses_frames_by_column_name() {
        let text = format!(
            "\u{feff}{HEADER}\n{}\r\n{}\n",
            row("2.5", "2.3", "9.8", "Hardware: Independent Flip"),
            row("3.0", "NA", "NA", "Composed: Flip")
        );
        let frames = parse(&text).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], Frame { ms: 2.5, gpu_busy_ms: Some(2.3), pc_latency_ms: Some(9.8), present_mode: "Hardware: Independent Flip".into() });
        assert_eq!(frames[1], Frame { ms: 3.0, gpu_busy_ms: None, pc_latency_ms: None, present_mode: "Composed: Flip".into() });
    }

    #[test]
    fn reordered_and_missing_optional_columns_still_parse() {
        let frames = parse("PresentMode,MsBetweenPresents\nHardware: Legacy Flip,4.0\n").unwrap();
        assert_eq!(frames, vec![Frame { ms: 4.0, gpu_busy_ms: None, pc_latency_ms: None, present_mode: "Hardware: Legacy Flip".into() }]);
    }

    #[test]
    fn rows_without_a_usable_frame_time_are_skipped() {
        let text = format!("{HEADER}\n{}\n{}\n{}\n{}\n{}\n", row("NA", "1", "1", "x"), row("0", "1", "1", "x"), row("-2", "1", "1", "x"), row("abc", "1", "1", "x"), row("5", "1", "1", "x"));
        let frames = parse(&text).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].ms, 5.0);
    }

    #[test]
    fn a_missing_frame_time_column_or_empty_text_is_an_error() {
        assert_eq!(parse("Application,PresentMode\nx,y\n"), Err(CsvError::MissingColumn("MsBetweenPresents")));
        assert_eq!(parse(""), Err(CsvError::Empty));
        assert_eq!(parse("\u{feff}\n\n"), Err(CsvError::Empty));
        assert_eq!(parse(&format!("{HEADER}\n")), Ok(Vec::new()));
    }
}
```

Run: `cargo test -p kestrel-bench csv`
Expected: FAIL — cannot find `parse`, `Frame`, `CsvError`.

- [ ] **Step 2: Implement** — put this above the tests in `csv.rs`:

```rust
//! PresentMon's CSV → frames. Columns are found by name: their set and order changed between 2.x releases.
//! PresentMon writes a UTF-8 BOM and `NA` where a value doesn't apply.

/// One presented frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame {
    /// Milliseconds since the previous present (`MsBetweenPresents`): the frame time.
    pub ms: f64,
    /// `MsGPUBusy`: how long the GPU worked on this frame.
    pub gpu_busy_ms: Option<f64>,
    /// `MsPCLatency`: NVIDIA Reflex PC latency, when the game reports it.
    pub pc_latency_ms: Option<f64>,
    /// `PresentMode`, e.g. "Hardware: Independent Flip" or "Composed: Flip".
    pub present_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    /// No header line at all.
    Empty,
    MissingColumn(&'static str),
}

pub fn parse(text: &str) -> Result<Vec<Frame>, CsvError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next().ok_or(CsvError::Empty)?;
    let columns: Vec<&str> = header.split(',').map(str::trim).collect();
    let find = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let ms_column = find("MsBetweenPresents").ok_or(CsvError::MissingColumn("MsBetweenPresents"))?;
    let gpu_column = find("MsGPUBusy");
    let latency_column = find("MsPCLatency");
    let mode_column = find("PresentMode");

    let number = |fields: &[&str], column: Option<usize>| -> Option<f64> {
        fields.get(column?)?.trim().parse::<f64>().ok().filter(|v| v.is_finite())
    };
    let mut frames = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        let Some(ms) = number(&fields, Some(ms_column)).filter(|ms| *ms > 0.0) else {
            continue;
        };
        frames.push(Frame {
            ms,
            gpu_busy_ms: number(&fields, gpu_column).filter(|v| *v >= 0.0),
            pc_latency_ms: number(&fields, latency_column).filter(|v| *v > 0.0),
            present_mode: mode_column
                .and_then(|c| fields.get(c))
                .map(|mode| mode.trim().to_string())
                .unwrap_or_default(),
        });
    }
    Ok(frames)
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `7 passed` (3 from Task 1 + 4); clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): parse PresentMon's CSV by column name"
```

---

### Task 3: Run metrics

**Files:**
- Create: `crates/kestrel-bench/src/metrics.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod metrics;`)

**Interfaces:**
- Consumes: `csv::Frame` (Task 2), `model::{RunMetrics, Bottleneck}` (Task 1).
- Produces: `kestrel_bench::metrics::{measure(frames: &[Frame]) -> (RunMetrics, Vec<f32>), percentile(values: &[f64], q: f64) -> f64}`. Definitions (spec "Measurements"): average FPS = 1000 / mean frame time; 1% low = 1000 / mean of the slowest ⌈1% of n⌉ frames; 0.1% low likewise with ⌈0.1%⌉ (at least one frame each); stutters = frames slower than 2× the median, per minute; PC latency = mean of frames that have it; bottleneck from the median of `gpu_busy_ms / ms` (clamped 0–1): ≥ 0.90 GPU, ≤ 0.75 CPU, else mixed, unknown without GPU numbers.

- [ ] **Step 1: Write the failing tests** — create `metrics.rs` with only these tests and add `pub mod metrics;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::Frame;
    use crate::model::Bottleneck;

    fn frames(ms: &[f64]) -> Vec<Frame> {
        ms.iter().map(|&ms| Frame { ms, ..Default::default() }).collect()
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
        assert_eq!((m.pc_latency_ms, m.gpu_busy_ratio, m.bottleneck), (None, None, Bottleneck::Unknown));
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
        assert!(close(m.low1_fps, 1000.0 / ((2_000.0 + 224.0 * 4.0) / 225.0)));
        assert!(close(m.low01_fps, 1000.0 / ((2_000.0 + 22.0 * 4.0) / 23.0)));
        assert!(close(m.stutters_per_min, 1.0 / (91.996 / 60.0)));
        // 1,500 FPS in the Range stays finite too.
        let (fast, _) = measure(&frames(&[1000.0 / 1500.0; 135_000]));
        assert!(close(fast.avg_fps, 1500.0) && close(fast.duration_s, 90.0));
    }

    #[test]
    fn gpu_busy_share_decides_the_bottleneck() {
        let with_gpu = |share: f64| -> Vec<Frame> {
            (0..1000).map(|_| Frame { ms: 4.0, gpu_busy_ms: Some(4.0 * share), ..Default::default() }).collect()
        };
        assert_eq!(measure(&with_gpu(0.95)).0.bottleneck, Bottleneck::Gpu);
        assert_eq!(measure(&with_gpu(0.5)).0.bottleneck, Bottleneck::Cpu);
        assert_eq!(measure(&with_gpu(0.8)).0.bottleneck, Bottleneck::Mixed);
        assert!(close(measure(&with_gpu(0.8)).0.gpu_busy_ratio.unwrap(), 0.8));
        // Busy longer than the frame (overlapping work) counts as fully busy.
        assert!(close(measure(&with_gpu(1.3)).0.gpu_busy_ratio.unwrap(), 1.0));
    }

    #[test]
    fn pc_latency_is_the_mean_of_frames_that_have_it() {
        let f = vec![
            Frame { ms: 4.0, pc_latency_ms: Some(10.0), ..Default::default() },
            Frame { ms: 4.0, pc_latency_ms: Some(20.0), ..Default::default() },
            Frame { ms: 4.0, ..Default::default() },
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
```

Run: `cargo test -p kestrel-bench metrics`
Expected: FAIL — cannot find `measure`, `percentile`.

- [ ] **Step 2: Implement** — above the tests in `metrics.rs`:

```rust
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
    let busy_shares: Vec<f64> = frames
        .iter()
        .filter_map(|f| f.gpu_busy_ms.map(|busy| (busy / f.ms).clamp(0.0, 1.0)))
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
```

(Frames always have `ms > 0` — the parser drops the rest — so `duration_s > 0` whenever `n > 0`.)

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `14 passed`; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): average FPS, 1% and 0.1% lows, stutters, PC latency and bottleneck"
```

---

### Task 4: VALORANT settings (read-only)

**Files:**
- Create: `crates/kestrel-bench/src/settings.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod settings;`)

**Interfaces:**
- Consumes: `model::ValorantSettings` (Task 1). Key names: `docs/superpowers/notes/2026-10-12-valorant-settings-keys.md`.
- Produces: `kestrel_bench::settings::{parse(game_user_settings: &str, riot_user_settings: &str) -> ValorantSettings, find_files(config_root: &Path) -> (Option<PathBuf>, Option<PathBuf>), read(config_root: &Path) -> Option<ValorantSettings>, differences(a: &ValorantSettings, b: &ValorantSettings) -> Vec<String>}`. The app passes `config_root = %LOCALAPPDATA%\VALORANT\Saved\Config`.

- [ ] **Step 1: Write the failing tests** — create `settings.rs` with only these tests and add `pub mod settings;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};

    const GAME_USER_SETTINGS: &str = "[/Script/ShooterGame.ShooterGameUserSettings]\r\nsg.ResolutionQuality=100.000000\r\nsg.ShadowQuality=0\r\nsg.TextureQuality=0\r\nbUseVSync=False\r\nbUseDynamicResolution=False\r\nResolutionSizeX=1920\r\nResolutionSizeY=1080\r\nFullscreenMode=0\r\nFrameRateLimit=0.000000\r\n";
    const RIOT_USER_SETTINGS: &str = "[Settings]\nEAresBoolSettingName::MultithreadedRenderingEnabled=True\nEAresIntSettingName::NvidiaReflexLowLatencySetting=2\nEAresIntSettingName::TextureQuality=0\nEAresBoolSettingName::ShadowsEnabled=False\nEAresStringSettingName::SavedCrosshairProfileData={\"profiles\":[]}\nEAresBoolSettingName::LimitFramerateInMenu=True\nEAresIntSettingName::PlayerPerfShowFrameRate=1\n";

    #[test]
    fn reads_the_settings_that_affect_fps() {
        let s = parse(GAME_USER_SETTINGS, RIOT_USER_SETTINGS);
        assert_eq!(s.resolution, Some([1920, 1080]));
        assert_eq!(s.fullscreen_mode, Some(0));
        assert_eq!(s.vsync, Some(false));
        assert_eq!(s.frame_rate_limit, Some(0.0));
        assert_eq!(s.reflex, Some(2));
        assert_eq!(s.multithreaded_rendering, Some(true));
        assert_eq!(s.quality.get("sg.ShadowQuality").map(String::as_str), Some("0"));
        assert_eq!(s.quality.get("sg.ResolutionQuality").map(String::as_str), Some("100.000000"));
        assert_eq!(s.quality.get("TextureQuality").map(String::as_str), Some("0"));
        assert_eq!(s.quality.get("ShadowsEnabled").map(String::as_str), Some("False"));
        assert_eq!(s.limits.get("LimitFramerateInMenu").map(String::as_str), Some("True"));
        assert_eq!(s.limits.keys().collect::<Vec<_>>(), vec!["LimitFramerateInMenu"], "the FPS counter is not a limit");
        assert!(!s.quality.contains_key("SavedCrosshairProfileData"));
    }

    #[test]
    fn missing_and_odd_files_are_unknown_not_errors() {
        assert_eq!(parse("", ""), ValorantSettings::default());
        let odd = parse("=\nno equals sign\n;comment\n[section]\nResolutionSizeX=wide\nResolutionSizeX=1920\n", "EAresIntSettingName::NvidiaReflexLowLatencySetting=on\n");
        assert_eq!(odd.resolution, None, "needs both X and Y");
        assert_eq!(odd.reflex, None);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(&dir.path().join("does-not-exist")), None);
        assert_eq!(read(dir.path()), None, "a folder without settings files");
        // UTF-16 (with BOM) files are read too.
        let account = dir.path().join("acc").join("Windows");
        fs::create_dir_all(&account).unwrap();
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend("FullscreenMode=1\r\n".encode_utf16().flat_map(u16::to_le_bytes));
        fs::write(account.join("GameUserSettings.ini"), bytes).unwrap();
        assert_eq!(read(dir.path()).unwrap().fullscreen_mode, Some(1));
    }

    #[test]
    fn the_newest_account_folder_wins() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old-account").join("Windows");
        let new = dir.path().join("new-account").join("Windows");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("GameUserSettings.ini"), "ResolutionSizeX=1280\nResolutionSizeY=720\n").unwrap();
        fs::write(new.join("GameUserSettings.ini"), "ResolutionSizeX=1920\nResolutionSizeY=1080\n").unwrap();
        fs::write(dir.path().join("new-account").join("RiotUserSettings.ini"), RIOT_USER_SETTINGS).unwrap();
        let an_hour_ago = SystemTime::now() - Duration::from_secs(3600);
        fs::File::options().write(true).open(old.join("GameUserSettings.ini")).unwrap().set_modified(an_hour_ago).unwrap();
        let (game, riot) = find_files(dir.path());
        assert_eq!(game.unwrap(), new.join("GameUserSettings.ini"));
        assert_eq!(riot.unwrap(), dir.path().join("new-account").join("RiotUserSettings.ini"));
        assert_eq!(read(dir.path()).unwrap().resolution, Some([1920, 1080]));
    }

    #[test]
    fn differences_name_only_settings_both_sides_know() {
        let a = parse(GAME_USER_SETTINGS, RIOT_USER_SETTINGS);
        let mut b = a.clone();
        assert!(differences(&a, &b).is_empty());
        b.resolution = Some([1280, 720]);
        b.quality.insert("sg.ShadowQuality".into(), "2".into());
        b.reflex = None;
        assert_eq!(differences(&a, &b), vec!["Resolution".to_string(), "sg.ShadowQuality".to_string()]);
    }
}
```

Run: `cargo test -p kestrel-bench settings`
Expected: FAIL — cannot find `parse`, `read`, `find_files`, `differences`.

- [ ] **Step 2: Implement** — above the tests in `settings.rs`:

```rust
//! VALORANT's video settings, read from its config files and never written (decision 43). Keys confirmed on the
//! owner's PC: docs/superpowers/notes/2026-10-12-valorant-settings-keys.md. VALORANT only writes settings changed
//! from their default, so a missing key means "default", never an error.
use crate::model::ValorantSettings;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Settings from the text of GameUserSettings.ini and RiotUserSettings.ini (either may be empty).
pub fn parse(game_user_settings: &str, riot_user_settings: &str) -> ValorantSettings {
    let mut s = ValorantSettings::default();
    let (mut width, mut height) = (None, None);
    for (key, value) in pairs(game_user_settings) {
        match key {
            "ResolutionSizeX" => width = value.parse::<u32>().ok(),
            "ResolutionSizeY" => height = value.parse::<u32>().ok(),
            "FullscreenMode" => s.fullscreen_mode = value.parse().ok(),
            "bUseVSync" => s.vsync = boolean(value),
            "FrameRateLimit" => s.frame_rate_limit = value.parse().ok(),
            _ if key.starts_with("sg.") => {
                s.quality.insert(key.to_string(), value.to_string());
            }
            _ => {}
        }
    }
    if let (Some(w), Some(h)) = (width, height) {
        s.resolution = Some([w, h]);
    }
    for (key, value) in pairs(riot_user_settings) {
        // "EAresIntSettingName::NvidiaReflexLowLatencySetting" → "NvidiaReflexLowLatencySetting"
        let name = key.rsplit("::").next().unwrap_or(key);
        let lower = name.to_ascii_lowercase();
        match name {
            "NvidiaReflexLowLatencySetting" => s.reflex = value.parse().ok(),
            "MultithreadedRenderingEnabled" => s.multithreaded_rendering = boolean(value),
            "AnisotropicFiltering" | "AntiAliasing" | "DetailQuality" | "MaterialQuality"
            | "TextureQuality" | "UIQuality" | "ShadowsEnabled" => {
                s.quality.insert(name.to_string(), value.to_string());
            }
            // The in-game FPS counter: its name says "FrameRate" but it limits nothing.
            "PlayerPerfShowFrameRate" => {}
            _ if lower.contains("limit") || lower.contains("maxfps") || lower.contains("framerate") => {
                s.limits.insert(name.to_string(), value.to_string());
            }
            _ => {}
        }
    }
    s
}

/// The most recently written GameUserSettings.ini and RiotUserSettings.ini under `config_root`
/// (`%LOCALAPPDATA%\VALORANT\Saved\Config`). There's one folder per Riot account; the newest is the one in use.
pub fn find_files(config_root: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut files = Vec::new();
    collect_files(config_root, 0, &mut files);
    let newest = |name: &str| {
        files
            .iter()
            .filter(|(path, _)| path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)))
            .max_by_key(|(_, modified)| *modified)
            .map(|(path, _)| path.clone())
    };
    (newest("GameUserSettings.ini"), newest("RiotUserSettings.ini"))
}

/// The settings in use, or None when VALORANT's settings files aren't there.
pub fn read(config_root: &Path) -> Option<ValorantSettings> {
    let (game, riot) = find_files(config_root);
    if game.is_none() && riot.is_none() {
        return None;
    }
    let text = |path: Option<PathBuf>| path.and_then(|p| std::fs::read(p).ok()).map(|b| decode(&b)).unwrap_or_default();
    Some(parse(&text(game), &text(riot)))
}

/// Names of settings that differ. Settings either side doesn't know are skipped, so missing data never counts as
/// a difference.
pub fn differences(a: &ValorantSettings, b: &ValorantSettings) -> Vec<String> {
    let resolution = |s: &ValorantSettings| s.resolution.map(|[w, h]| format!("{w}x{h}"));
    let known = [
        ("Resolution", resolution(a), resolution(b)),
        ("Fullscreen mode", a.fullscreen_mode.map(|v| v.to_string()), b.fullscreen_mode.map(|v| v.to_string())),
        ("V-Sync", a.vsync.map(|v| v.to_string()), b.vsync.map(|v| v.to_string())),
        ("Frame-rate limit", a.frame_rate_limit.map(|v| v.to_string()), b.frame_rate_limit.map(|v| v.to_string())),
        ("NVIDIA Reflex", a.reflex.map(|v| v.to_string()), b.reflex.map(|v| v.to_string())),
        ("Multithreaded rendering", a.multithreaded_rendering.map(|v| v.to_string()), b.multithreaded_rendering.map(|v| v.to_string())),
    ];
    let mut out: Vec<String> = known
        .into_iter()
        .filter_map(|(name, x, y)| match (x, y) {
            (Some(x), Some(y)) if x != y => Some(name.to_string()),
            _ => None,
        })
        .collect();
    for (key, value) in &a.quality {
        if b.quality.get(key).is_some_and(|other| other != value) {
            out.push(key.clone());
        }
    }
    out
}

fn pairs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with(&['[', ';', '#'][..]) {
            return None;
        }
        let (key, value) = line.split_once('=')?;
        let key = key.trim();
        if key.is_empty() {
            None
        } else {
            Some((key, value.trim()))
        }
    })
}

fn boolean(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// Text from UTF-8 (with or without BOM) or UTF-16 LE with BOM.
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (byte_pairs, _) = rest.as_chunks::<2>();
        let units: Vec<u16> = byte_pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
        return String::from_utf16_lossy(&units);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Files up to four folders deep (Config\<account>\Windows\file.ini is three).
fn collect_files(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, SystemTime)>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            collect_files(&path, depth + 1, out);
        } else if let Ok(modified) = meta.modified() {
            out.push((path, modified));
        }
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `18 passed`; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): read VALORANT's video settings without writing them"
```

---
### Task 5: Run flags

**Files:**
- Create: `crates/kestrel-bench/src/flags.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod flags;`)

**Interfaces:**
- Consumes: `csv::Frame`, `metrics::percentile`, `model::{Flag, RunMetrics, ValorantSettings}`.
- Produces: `kestrel_bench::flags::{RunInputs<'a> {frames: &'a [Frame], metrics: &'a RunMetrics, stderr: &'a str, settings: Option<&'a ValorantSettings>, busy: &'a [(String, f32)]}, flags(run: &RunInputs) -> Vec<Flag>}`. Rules (spec table): too short = under 85 s or under 1,000 frames; not in front = more than 5% of frames with a `PresentMode` starting "Composed"; FPS cap = V-Sync on, a frame-rate limit above 0, or flat frame times (95th percentile ≤ 1.05 × 5th); dropped data = PresentMon's stderr mentions "lost" or "overflow"; busy background = an app at ≥ 10% of the whole CPU right after the run (the first such app is named).

- [ ] **Step 1: Write the failing tests** — create `flags.rs` with only the tests and add `pub mod flags;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::Frame;
    use crate::metrics::measure;

    /// A normal uncapped run: frame times alternate between 3.5 and 4.5 ms for 90 s.
    fn varied(n: usize, mode: &str) -> Vec<Frame> {
        (0..n).map(|i| Frame { ms: if i % 2 == 0 { 3.5 } else { 4.5 }, present_mode: mode.into(), ..Default::default() }).collect()
    }

    fn check(frames: &[Frame], stderr: &str, settings: Option<&ValorantSettings>, busy: &[(String, f32)]) -> Vec<Flag> {
        let (metrics, _) = measure(frames);
        flags(&RunInputs { frames, metrics: &metrics, stderr, settings, busy })
    }

    #[test]
    fn a_clean_run_has_no_flags() {
        let settings = ValorantSettings { vsync: Some(false), frame_rate_limit: Some(0.0), ..Default::default() };
        assert!(check(&varied(22_500, "Hardware: Independent Flip"), "", Some(&settings), &[]).is_empty());
    }

    #[test]
    fn empty_capture_is_only_too_short() {
        assert_eq!(check(&[], "", None, &[]), vec![Flag::TooShort]);
    }

    #[test]
    fn short_runs_are_flagged() {
        assert_eq!(check(&varied(10_000, "Hardware: Independent Flip"), "", None, &[]), vec![Flag::TooShort]);
        // 90 s but fewer than 1,000 frames (a slideshow) is too short too.
        let slow: Vec<Frame> = (0..900).map(|i| Frame { ms: if i % 2 == 0 { 90.0 } else { 110.0 }, ..Default::default() }).collect();
        assert_eq!(check(&slow, "", None, &[]), vec![Flag::TooShort]);
    }

    #[test]
    fn composed_frames_mean_the_game_was_not_in_front() {
        let mut frames = varied(20_000, "Hardware: Independent Flip");
        frames.extend(varied(2_500, "Composed: Flip"));
        assert_eq!(check(&frames, "", None, &[]), vec![Flag::NotInFront]);
        let mut five_percent = varied(21_375, "Hardware: Legacy Flip");
        five_percent.extend(varied(1_125, "Composed: Flip"));
        assert!(check(&five_percent, "", None, &[]).is_empty(), "exactly 5% is allowed");
    }

    #[test]
    fn caps_are_found_in_the_settings_or_the_frame_times() {
        let flat: Vec<Frame> = (0..22_500).map(|_| Frame { ms: 4.0, ..Default::default() }).collect();
        assert_eq!(check(&flat, "", None, &[]), vec![Flag::FpsCapped]);
        let vsync = ValorantSettings { vsync: Some(true), ..Default::default() };
        let limit = ValorantSettings { frame_rate_limit: Some(240.0), ..Default::default() };
        let run = varied(22_500, "Hardware: Independent Flip");
        assert_eq!(check(&run, "", Some(&vsync), &[]), vec![Flag::FpsCapped]);
        assert_eq!(check(&run, "", Some(&limit), &[]), vec![Flag::FpsCapped]);
    }

    #[test]
    fn presentmon_warnings_and_busy_apps_are_flagged() {
        let run = varied(22_500, "Hardware: Independent Flip");
        assert_eq!(check(&run, "Warning: 12 events lost", None, &[]), vec![Flag::DroppedData]);
        assert_eq!(check(&run, "ETW buffer OVERFLOW", None, &[]), vec![Flag::DroppedData]);
        let busy = vec![("discord.exe".to_string(), 4.0), ("chrome.exe".to_string(), 12.5)];
        assert_eq!(check(&run, "", None, &busy), vec![Flag::BusyBackground { name: "chrome.exe".into() }]);
        assert!(check(&run, "", None, &[("chrome.exe".to_string(), 9.9)]).is_empty());
    }
}
```

Run: `cargo test -p kestrel-bench flags`
Expected: FAIL — cannot find `flags`, `RunInputs`.

- [ ] **Step 2: Implement** — above the tests in `flags.rs`:

```rust
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
    let composed = run.frames.iter().filter(|f| f.present_mode.starts_with("Composed")).count();
    if !run.frames.is_empty() && composed as f64 / run.frames.len() as f64 > 0.05 {
        out.push(Flag::NotInFront);
    }
    let times: Vec<f64> = run.frames.iter().map(|f| f.ms).collect();
    let flat = !times.is_empty() && {
        let (p5, p95) = (percentile(&times, 0.05), percentile(&times, 0.95));
        p5 > 0.0 && p95 <= p5 * 1.05
    };
    let capped_in_settings = run
        .settings
        .is_some_and(|s| s.vsync == Some(true) || s.frame_rate_limit.is_some_and(|limit| limit > 0.0));
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
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `24 passed`; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): flag runs that can't be trusted"
```

---

### Task 6: Middle-of-three result, comparisons and the graph

**Files:**
- Create: `crates/kestrel-bench/src/result.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod result;`)

**Interfaces:**
- Consumes: `model::*`, `settings::differences`.
- Produces: `kestrel_bench::result::{combine(runs: &[RunRecord]) -> BenchResult, compare(baseline: &Benchmark, current: &Benchmark) -> Comparison, middle_run(runs: &[RunRecord]) -> Option<usize>, graph_points(frame_times: &[f32], points: usize) -> Vec<f32>}`. `combine`: median of each metric, spread = (max − min) / median for average FPS and both lows, bottleneck = the one most runs share, else mixed. `compare`: per metric (average FPS, 1% low, 0.1% low higher is better; stutters, PC latency lower is better) the medians, the difference in %, and a verdict that is real only when every current run is beyond every baseline run; metrics either side lacks are left out; different game or route version → `route_mismatch` and no metrics; `settings_differ` only when both sides have settings. `middle_run`: the run with the middle average FPS (its frames draw the graph). `graph_points`: at most `points` values, each the slowest frame of its bucket.

- [ ] **Step 1: Write the failing tests** — create `result.rs` with only the tests and add `pub mod result;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ValorantSettings;

    fn run(avg: f64, low1: f64, stutters: f64, latency: Option<f64>, bottleneck: Bottleneck) -> RunRecord {
        RunRecord {
            metrics: RunMetrics { frames: 30_000, duration_s: 90.0, avg_fps: avg, low1_fps: low1, low01_fps: low1 / 2.0, stutters_per_min: stutters, pc_latency_ms: latency, bottleneck, ..Default::default() },
            ..Default::default()
        }
    }

    fn bench(id: &str, runs: Vec<RunRecord>) -> Benchmark {
        Benchmark { id: id.into(), profile_id: "valorant".into(), route_version: 1, result: combine(&runs), runs, ..Default::default() }
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
        assert_eq!(r.metrics.pc_latency_ms, Some(11.0), "median of the runs that have it");
        assert_eq!(r.metrics.bottleneck, Bottleneck::Gpu, "two of three runs agree");
        assert!((r.spread.avg_fps - 30.0 / 410.0).abs() < 1e-9);
        let split = vec![run(1.0, 1.0, 0.0, None, Bottleneck::Gpu), run(1.0, 1.0, 0.0, None, Bottleneck::Cpu), run(1.0, 1.0, 0.0, None, Bottleneck::Mixed)];
        assert_eq!(combine(&split).metrics.bottleneck, Bottleneck::Mixed);
        assert_eq!(combine(&[]), BenchResult::default());
    }

    #[test]
    fn a_change_is_real_only_when_the_runs_do_not_overlap() {
        let base = bench("a", vec![run(400.0, 280.0, 2.0, Some(10.0), Bottleneck::Gpu), run(405.0, 290.0, 2.2, Some(10.5), Bottleneck::Gpu), run(410.0, 300.0, 2.4, Some(11.0), Bottleneck::Gpu)]);
        let better = bench("b", vec![run(420.0, 285.0, 1.0, Some(12.0), Bottleneck::Gpu), run(425.0, 295.0, 1.1, Some(12.5), Bottleneck::Gpu), run(430.0, 305.0, 1.2, Some(13.0), Bottleneck::Gpu)]);
        let c = compare(&base, &better);
        let get = |id: MetricId| c.metrics.iter().find(|m| m.metric == id).unwrap().clone();
        assert_eq!(get(MetricId::AvgFps).verdict, Verdict::RealGain);
        assert!((get(MetricId::AvgFps).delta_pct - (425.0 - 405.0) / 405.0 * 100.0).abs() < 1e-9);
        assert_eq!(get(MetricId::Low1Fps).verdict, Verdict::Noise, "ranges overlap");
        assert_eq!(get(MetricId::StuttersPerMin).verdict, Verdict::RealGain, "fewer stutters is better");
        assert_eq!(get(MetricId::PcLatencyMs).verdict, Verdict::RealLoss, "more latency is worse");
        assert_eq!((c.baseline_id.as_str(), c.current_id.as_str(), c.route_mismatch), ("a", "b", false));
    }

    #[test]
    fn metrics_one_side_lacks_and_other_routes_are_not_compared() {
        let base = bench("a", vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        let current = bench("b", vec![run(420.0, 285.0, 1.0, Some(12.0), Bottleneck::Gpu); 3]);
        let compared: Vec<MetricId> = compare(&base, &current).metrics.iter().map(|m| m.metric).collect();
        assert_eq!(compared, vec![MetricId::AvgFps, MetricId::Low1Fps, MetricId::Low01Fps, MetricId::StuttersPerMin]);
        let mut other_route = current.clone();
        other_route.route_version = 2;
        let c = compare(&base, &other_route);
        assert!(c.route_mismatch && c.metrics.is_empty());
    }

    #[test]
    fn unknown_settings_never_differ() {
        let mut base = bench("a", vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        let mut current = bench("b", vec![run(400.0, 280.0, 2.0, None, Bottleneck::Gpu); 3]);
        current.context.game_settings = Some(ValorantSettings { resolution: Some([1920, 1080]), ..Default::default() });
        assert!(compare(&base, &current).settings_differ.is_empty());
        base.context.game_settings = Some(ValorantSettings { resolution: Some([1280, 720]), ..Default::default() });
        assert_eq!(compare(&base, &current).settings_differ, vec!["Resolution".to_string()]);
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
        let runs = vec![run(430.0, 1.0, 0.0, None, Bottleneck::Gpu), run(400.0, 1.0, 0.0, None, Bottleneck::Gpu), run(410.0, 1.0, 0.0, None, Bottleneck::Gpu)];
        assert_eq!(middle_run(&runs), Some(2));
        assert_eq!(middle_run(&[]), None);
    }
}
```

Run: `cargo test -p kestrel-bench result`
Expected: FAIL — cannot find `combine`, `compare`, `graph_points`, `middle_run`.

- [ ] **Step 2: Implement** — above the tests in `result.rs`:

```rust
//! A benchmark's result from its runs (decision 38), comparisons with another benchmark (decision 41), and the
//! frame-time graph's points.
use crate::model::{
    Benchmark, BenchResult, Bottleneck, Comparison, MetricDelta, MetricId, RunMetrics, RunRecord, Spread, Verdict,
};
use crate::settings::differences;

/// The middle value of each metric over the runs, and how far apart the runs were.
pub fn combine(runs: &[RunRecord]) -> BenchResult {
    if runs.is_empty() {
        return BenchResult::default();
    }
    let values = |get: fn(&RunMetrics) -> f64| -> Vec<f64> { runs.iter().map(|r| get(&r.metrics)).collect() };
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
    if baseline.profile_id != current.profile_id || baseline.route_version != current.route_version {
        comparison.route_mismatch = true;
        return comparison;
    }
    for (metric, higher_is_better, get) in METRICS {
        let (Some(base), Some(now)) = (get(&baseline.result.metrics), get(&current.result.metrics)) else {
            continue;
        };
        let base_runs: Vec<f64> = baseline.runs.iter().filter_map(|r| get(&r.metrics)).collect();
        let now_runs: Vec<f64> = current.runs.iter().filter_map(|r| get(&r.metrics)).collect();
        comparison.metrics.push(MetricDelta {
            metric,
            baseline: base,
            current: now,
            delta_pct: if base != 0.0 { (now - base) / base * 100.0 } else { 0.0 },
            verdict: verdict(&base_runs, &now_runs, higher_is_better),
        });
    }
    if let (Some(a), Some(b)) = (&baseline.context.game_settings, &current.context.game_settings) {
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
    frame_times.chunks(size).map(|bucket| bucket.iter().copied().fold(0.0_f32, f32::max)).collect()
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
    let kinds = [Bottleneck::Gpu, Bottleneck::Cpu, Bottleneck::Mixed, Bottleneck::Unknown];
    kinds
        .into_iter()
        .find(|kind| runs.iter().filter(|r| r.metrics.bottleneck == *kind).count() * 2 > runs.len())
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
    values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), &v| (low.min(v), high.max(v)))
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `30 passed`; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): middle-of-three results, real-or-noise comparisons and graph points"
```

---

### Task 7: Saved benchmarks

**Files:**
- Create: `crates/kestrel-bench/src/store.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod store;` and `pub use store::Store;`)

**Interfaces:**
- Consumes: `model::{Benchmark, BenchmarkSummary, STARTING_POINT}`.
- Produces: `kestrel_bench::Store` with `new(dir)`, `dir() -> &Path`, `save(&self, bench: &mut Benchmark, frame_times: &[Vec<f32>]) -> io::Result<String>` (assigns `bench.id`; the first benchmark ever saved, or the first after the baseline was deleted, is renamed "Starting point" and becomes the baseline), `list() -> Vec<BenchmarkSummary>` (newest first, unreadable ones left out), `get(id) -> Option<Benchmark>`, `frame_times(id, run: usize /*1-based*/) -> Option<Vec<f32>>`, `rename(id, name) -> io::Result<()>` (trimmed, blank refused, at most 60 characters), `delete(id) -> io::Result<()>` (clears the baseline if it was it), `set_baseline(id) -> io::Result<()>`, `baseline() -> Option<String>`. Layout: `index.json`, `<id>/summary.json`, `<id>/run-<n>.bin` (little-endian f32). Ids come from `created_at`'s digits ("20261012-213005", then "-2", "-3" on clashes); any id with characters other than ASCII letters, digits and dashes is refused, so the UI can never reach outside the folder. Every change (`save`, `rename`, `delete`, `set_baseline`) holds the store's own lock from reading to writing, so two threads (the capture thread saving, a command deleting) can't lose each other's update to `index.json`.

- [ ] **Step 1: Write the failing tests** — create `store.rs` with only the tests; in `lib.rs` add `pub mod store;` and `pub use store::Store;`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BenchResult, RunMetrics};

    fn bench(created_at: &str, avg: f64) -> Benchmark {
        Benchmark {
            name: format!("Benchmark {created_at}"),
            created_at: created_at.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            result: BenchResult { metrics: RunMetrics { avg_fps: avg, ..Default::default() }, ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn saves_lists_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("benchmarks"));
        let mut first = bench("2026-10-12T21:30:05Z", 410.0);
        let first_id = store.save(&mut first, &[vec![4.0, 4.5], vec![3.9], vec![4.1]]).unwrap();
        assert_eq!(first_id, "20261012-213005");
        assert_eq!((first.id.as_str(), first.name.as_str()), ("20261012-213005", STARTING_POINT));
        let mut second = bench("2026-10-13T08:00:00Z", 430.0);
        let second_id = store.save(&mut second, &[vec![3.0]]).unwrap();
        assert_eq!(second.name, "Benchmark 2026-10-13T08:00:00Z", "only the first is renamed");

        let list = store.list();
        assert_eq!(list.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec![second_id.as_str(), first_id.as_str()]);
        assert!(list[1].is_baseline && !list[0].is_baseline);
        assert_eq!(store.get(&second_id).unwrap(), second);
        assert_eq!(store.frame_times(&first_id, 1), Some(vec![4.0, 4.5]));
        assert_eq!(store.frame_times(&first_id, 3), Some(vec![4.1]));
        assert_eq!(store.frame_times(&first_id, 4), None);
        assert_eq!(store.baseline(), Some(first_id.clone()));
        let leftovers: Vec<_> = walk(store.dir()).into_iter().filter(|p| p.extension().is_some_and(|e| e == "tmp")).collect();
        assert!(leftovers.is_empty(), "no temporary files stay behind: {leftovers:?}");
    }

    #[test]
    fn clashing_ids_get_a_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let ids: Vec<String> = (0..3).map(|_| store.save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[]).unwrap()).collect();
        assert_eq!(ids, vec!["20261012-213005", "20261012-213005-2", "20261012-213005-3"]);
    }

    #[test]
    fn rename_delete_and_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let a = store.save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[]).unwrap();
        let b = store.save(&mut bench("2026-10-13T08:00:00Z", 2.0), &[]).unwrap();
        store.rename(&b, "  After tweaks  ").unwrap();
        assert_eq!(store.get(&b).unwrap().name, "After tweaks");
        assert_eq!(store.rename(&b, "   ").unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
        store.rename(&b, &"x".repeat(80)).unwrap();
        assert_eq!(store.get(&b).unwrap().name.chars().count(), 60);
        store.set_baseline(&b).unwrap();
        assert_eq!(store.baseline(), Some(b.clone()));
        assert_eq!(store.set_baseline("nope").unwrap_err().kind(), std::io::ErrorKind::NotFound);
        store.delete(&b).unwrap();
        assert_eq!(store.baseline(), None);
        assert_eq!(store.list().len(), 1);
        let mut c = bench("2026-10-14T08:00:00Z", 3.0);
        store.save(&mut c, &[]).unwrap();
        assert_eq!(c.name, STARTING_POINT, "with no baseline left, the next benchmark becomes the starting point");
        assert!(store.get(&a).is_some());
    }

    #[test]
    fn damaged_folders_are_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let a = store.save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[]).unwrap();
        let b = store.save(&mut bench("2026-10-13T08:00:00Z", 2.0), &[]).unwrap();
        std::fs::remove_dir_all(dir.path().join(&a)).unwrap();
        std::fs::write(dir.path().join(&b).join("summary.json"), "{not json").unwrap();
        assert!(store.list().is_empty());
        assert_eq!(store.baseline(), None, "the baseline's folder is gone");
        std::fs::write(dir.path().join("index.json"), "garbage").unwrap();
        assert!(store.list().is_empty());
        let c = store.save(&mut bench("2026-10-14T08:00:00Z", 3.0), &[]).unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(store.get("../outside").is_none() && store.get("").is_none());
        assert_eq!(store.delete("../outside").unwrap_err().kind(), std::io::ErrorKind::NotFound);
        assert!(store.get(&c).is_some());
    }

    fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }
}
```

Run: `cargo test -p kestrel-bench store`
Expected: FAIL — cannot find `Store`.

- [ ] **Step 2: Implement** — above the tests in `store.rs`:

```rust
//! Saved benchmarks, in Kestrel's own folder (decision 40):
//!
//! ```text
//! index.json          {"ids": [oldest … newest], "baseline": id}
//! <id>/summary.json   the Benchmark
//! <id>/run-<n>.bin    run n's frame times, little-endian f32
//! ```
//!
//! Each file is written under a temporary name and then renamed, so a crash never leaves half a file.
use crate::model::{Benchmark, BenchmarkSummary, STARTING_POINT};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Index {
    ids: Vec<String>,
    baseline: Option<String>,
}

pub struct Store {
    dir: PathBuf,
    /// Held through every read-change-write, so two callers at once (the capture thread saving, a command deleting)
    /// can't lose each other's change to the index.
    changing: Mutex<()>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into(), changing: Mutex::new(()) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Saves a new benchmark and its runs' frame times; returns (and sets) its id. With no baseline yet, it is
    /// named "Starting point" and becomes the baseline (decision 41).
    pub fn save(&self, bench: &mut Benchmark, frame_times: &[Vec<f32>]) -> io::Result<String> {
        let _changing = self.change();
        std::fs::create_dir_all(&self.dir)?;
        let mut index = self.index();
        let base = id_from(&bench.created_at);
        let mut id = base.clone();
        let mut n = 2;
        while index.ids.contains(&id) || self.dir.join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        bench.id = id.clone();
        if self.baseline_in(&index).is_none() {
            bench.name = STARTING_POINT.into();
            index.baseline = Some(id.clone());
        }
        let folder = self.dir.join(&id);
        std::fs::create_dir_all(&folder)?;
        for (i, times) in frame_times.iter().enumerate() {
            let bytes: Vec<u8> = times.iter().flat_map(|t| t.to_le_bytes()).collect();
            write_atomic(&folder.join(format!("run-{}.bin", i + 1)), &bytes)?;
        }
        write_atomic(&folder.join("summary.json"), &serde_json::to_vec_pretty(bench)?)?;
        index.ids.push(id.clone());
        self.write_index(&index)?;
        Ok(id)
    }

    /// Newest first. Benchmarks whose files can't be read are left out.
    pub fn list(&self) -> Vec<BenchmarkSummary> {
        let index = self.index();
        let baseline = self.baseline_in(&index);
        index
            .ids
            .iter()
            .rev()
            .filter_map(|id| self.get(id))
            .map(|b| summary(&b, baseline.as_deref() == Some(b.id.as_str())))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Benchmark> {
        if !safe_id(id) {
            return None;
        }
        let bytes = std::fs::read(self.dir.join(id).join("summary.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Run `run`'s frame times (runs count from 1).
    pub fn frame_times(&self, id: &str, run: usize) -> Option<Vec<f32>> {
        if !safe_id(id) {
            return None;
        }
        let bytes = std::fs::read(self.dir.join(id).join(format!("run-{run}.bin"))).ok()?;
        let (quads, _) = bytes.as_chunks::<4>();
        Some(quads.iter().map(|&quad| f32::from_le_bytes(quad)).collect())
    }

    pub fn rename(&self, id: &str, name: &str) -> io::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "A name can't be empty."));
        }
        let _changing = self.change();
        let mut bench = self.get(id).ok_or_else(not_found)?;
        bench.name = name.chars().take(60).collect();
        write_atomic(&self.dir.join(id).join("summary.json"), &serde_json::to_vec_pretty(&bench)?)
    }

    pub fn delete(&self, id: &str) -> io::Result<()> {
        if !safe_id(id) {
            return Err(not_found());
        }
        let _changing = self.change();
        let mut index = self.index();
        index.ids.retain(|i| i != id);
        if index.baseline.as_deref() == Some(id) {
            index.baseline = None;
        }
        self.write_index(&index)?;
        match std::fs::remove_dir_all(self.dir.join(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    pub fn set_baseline(&self, id: &str) -> io::Result<()> {
        let _changing = self.change();
        let mut index = self.index();
        if !index.ids.iter().any(|i| i == id) || self.get(id).is_none() {
            return Err(not_found());
        }
        index.baseline = Some(id.to_string());
        self.write_index(&index)
    }

    /// The baseline's id, if it still exists.
    pub fn baseline(&self) -> Option<String> {
        self.baseline_in(&self.index())
    }

    fn baseline_in(&self, index: &Index) -> Option<String> {
        index.baseline.clone().filter(|id| self.get(id).is_some())
    }

    /// A missing or damaged index reads as empty.
    fn index(&self) -> Index {
        std::fs::read(self.dir.join("index.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write_index(&self, index: &Index) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        write_atomic(&self.dir.join("index.json"), &serde_json::to_vec_pretty(index)?)
    }

    /// One change at a time. Every write is atomic, so a panic mid-change leaves nothing half-written and a poisoned
    /// lock is simply taken over.
    fn change(&self) -> MutexGuard<'_, ()> {
        self.changing.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn summary(b: &Benchmark, is_baseline: bool) -> BenchmarkSummary {
    BenchmarkSummary {
        id: b.id.clone(),
        name: b.name.clone(),
        created_at: b.created_at.clone(),
        profile_id: b.profile_id.clone(),
        route_version: b.route_version,
        result: b.result.clone(),
        is_baseline,
        flagged_runs_kept: b.runs.iter().filter(|r| r.kept).count() as u32,
    }
}

/// "2026-10-12T21:30:05Z" → "20261012-213005".
fn id_from(created_at: &str) -> String {
    let digits: String = created_at.chars().filter(char::is_ascii_digit).take(14).collect();
    if digits.len() < 9 {
        return "benchmark".into();
    }
    format!("{}-{}", &digits[..8], &digits[8..])
}

/// Ids are letters, digits and dashes only, so an id from the UI can't point outside the folder.
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "That benchmark doesn't exist.")
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `34 passed`; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): save benchmarks in Kestrel's own folder, safely"
```

---
### Task 8: Session state machine

**Files:**
- Create: `crates/kestrel-bench/src/session.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod session;`)

**Interfaces:**
- Consumes: `model::{Flag, RunRecord}`.
- Produces: `kestrel_bench::session::{Session, Phase {Waiting, Capturing, Review {flags}, Done, Failed {message}, Cancelled}, Action {BeepStart, BeepEnd, StartCapture {pass: u8}, StopCapture, Finish}, SessionView {profile_id, pass, phase (flattened, JSON tag "phase"), game_running, runs: Vec<Option<RunRecord>>, result_id}, RUNS = 3}`. `Session` methods: `new(profile_id)`, `phase()`, `pass()` (0 = warm-up, 1–3 = runs), `is_over()`, `set_game_running(bool)`, `start_pressed() -> Vec<Action>`, `capture_finished(Result<RunRecord, String>) -> Vec<Action>`, `redo()`, `keep()`, `cancel()`, `fail(message)`, `set_result_id(id)`, `runs() -> Vec<RunRecord>`, `view() -> SessionView`. Rules: the start key works only while waiting and the game runs; the warm-up's numbers and flags are never kept; a flagged run waits for redo or keep; the third accepted run returns `Finish`; cancel during a capture returns `StopCapture`; anything after the session is over does nothing, except `fail` (saving the finished result can still fail) and `set_result_id`.

- [ ] **Step 1: Write the failing tests** — create `session.rs` with only the tests and add `pub mod session;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RunMetrics;
    use serde_json::json;

    fn clean(avg: f64) -> RunRecord {
        RunRecord { metrics: RunMetrics { avg_fps: avg, ..Default::default() }, ..Default::default() }
    }

    fn flagged() -> RunRecord {
        RunRecord { flags: vec![Flag::TooShort], ..clean(100.0) }
    }

    fn armed() -> Session {
        let mut s = Session::new("valorant");
        s.set_game_running(true);
        s
    }

    #[test]
    fn warm_up_then_three_runs_then_finish() {
        let mut s = armed();
        assert_eq!(s.start_pressed(), vec![Action::BeepStart, Action::StartCapture { pass: 0 }]);
        assert_eq!(s.capture_finished(Ok(flagged())), vec![Action::BeepEnd], "warm-up flags don't matter");
        assert_eq!((s.pass(), s.phase().clone()), (1, Phase::Waiting));
        for pass in 1..=3u8 {
            assert_eq!(s.start_pressed(), vec![Action::BeepStart, Action::StartCapture { pass }]);
            let actions = s.capture_finished(Ok(clean(400.0 + f64::from(pass))));
            if pass < 3 {
                assert_eq!(actions, vec![Action::BeepEnd]);
                assert_eq!((s.pass(), s.phase().clone()), (pass + 1, Phase::Waiting));
            } else {
                assert_eq!(actions, vec![Action::BeepEnd, Action::Finish]);
                assert_eq!(s.phase(), &Phase::Done);
            }
        }
        assert_eq!(s.runs().iter().map(|r| r.metrics.avg_fps).collect::<Vec<_>>(), vec![401.0, 402.0, 403.0]);
        assert!(s.is_over());
    }

    #[test]
    fn a_flagged_run_is_redone_or_kept() {
        let mut s = armed();
        s.start_pressed();
        s.capture_finished(Ok(clean(1.0)));
        s.start_pressed();
        assert_eq!(s.capture_finished(Ok(flagged())), vec![Action::BeepEnd]);
        assert_eq!(s.phase(), &Phase::Review { flags: vec![Flag::TooShort] });
        assert!(s.redo().is_empty());
        assert_eq!((s.pass(), s.phase().clone()), (1, Phase::Waiting), "redo repeats run 1");
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        assert!(s.keep().is_empty());
        assert_eq!((s.pass(), s.phase().clone()), (2, Phase::Waiting));
        assert!(s.runs()[0].kept);
        s.start_pressed();
        s.capture_finished(Ok(clean(2.0)));
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        assert_eq!(s.keep(), vec![Action::Finish], "keeping the third run finishes");
        assert_eq!(s.phase(), &Phase::Done);
    }

    #[test]
    fn start_key_is_ignored_unless_waiting_with_the_game() {
        let mut s = Session::new("valorant");
        assert!(s.start_pressed().is_empty(), "the game isn't running");
        assert_eq!(s.phase(), &Phase::Waiting);
        s.set_game_running(true);
        assert_eq!(s.start_pressed().len(), 2);
        assert!(s.start_pressed().is_empty(), "a second press during the capture does nothing");
        s.capture_finished(Ok(clean(1.0)));
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        assert!(s.start_pressed().is_empty(), "nothing starts while a flagged run waits for a decision");
        assert!(s.capture_finished(Ok(clean(1.0))).is_empty(), "a stray capture result is ignored");
    }

    #[test]
    fn cancel_stops_a_capture_and_ends_the_session() {
        let mut s = armed();
        s.start_pressed();
        assert_eq!(s.cancel(), vec![Action::StopCapture]);
        assert_eq!(s.phase(), &Phase::Cancelled);
        assert!(s.capture_finished(Ok(clean(1.0))).is_empty());
        assert!(s.start_pressed().is_empty() && s.cancel().is_empty());
        let mut waiting = armed();
        assert!(waiting.cancel().is_empty());
        assert_eq!(waiting.phase(), &Phase::Cancelled);
    }

    #[test]
    fn a_capture_error_fails_the_session() {
        let mut s = armed();
        s.start_pressed();
        assert_eq!(s.capture_finished(Err("PresentMon failed".into())), vec![Action::BeepEnd]);
        assert_eq!(s.phase(), &Phase::Failed { message: "PresentMon failed".into() });
        assert!(s.start_pressed().is_empty() && s.is_over());
        let mut done = armed();
        done.fail("Couldn't save the result".into());
        assert_eq!(done.phase(), &Phase::Failed { message: "Couldn't save the result".into() });
    }

    #[test]
    fn the_view_is_flat_camel_case_json() {
        let mut s = armed();
        s.start_pressed();
        s.capture_finished(Ok(clean(1.0)));
        s.start_pressed();
        s.capture_finished(Ok(flagged()));
        let v = serde_json::to_value(s.view()).unwrap();
        assert_eq!(v["profileId"], json!("valorant"));
        assert_eq!(v["pass"], json!(1));
        assert_eq!(v["phase"], json!("review"));
        assert_eq!(v["flags"], json!([{"kind": "too-short"}]));
        assert_eq!(v["gameRunning"], json!(true));
        assert_eq!(v["runs"], json!([null, null, null]));
        assert_eq!(v["resultId"], json!(null));
    }
}
```

Run: `cargo test -p kestrel-bench session`
Expected: FAIL — cannot find `Session`, `Action`, `Phase`.

- [ ] **Step 2: Implement** — above the tests in `session.rs`:

```rust
//! One benchmark session (spec "What the owner sees", 3): a warm-up pass, then runs 1–3, each started by the
//! owner's start key. A pure state machine: the app performs the actions it returns.
use crate::model::{Flag, RunRecord};
use serde::Serialize;

/// Runs that count (the warm-up comes before them).
pub const RUNS: u8 = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "kebab-case")]
pub enum Phase {
    /// Armed: waiting for the start key (which needs the game running).
    Waiting,
    Capturing,
    /// The last run came back flagged; the owner redoes or keeps it.
    Review { flags: Vec<Flag> },
    Done,
    Failed { message: String },
    Cancelled,
}

/// What the app must do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// One long beep: a capture starts.
    BeepStart,
    /// Two short beeps: a capture ended.
    BeepEnd,
    /// Capture pass `pass` (0 = warm-up, 1–3 = runs).
    StartCapture { pass: u8 },
    /// Stop the running capture (cancelled).
    StopCapture,
    /// All runs are in: combine and save them.
    Finish,
}

/// The session as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub profile_id: String,
    /// 0 = warm-up, 1–3 = runs.
    pub pass: u8,
    #[serde(flatten)]
    pub phase: Phase,
    pub game_running: bool,
    /// Runs 1–3 as accepted so far.
    pub runs: Vec<Option<RunRecord>>,
    pub result_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Session {
    profile_id: String,
    pass: u8,
    phase: Phase,
    game_running: bool,
    runs: Vec<Option<RunRecord>>,
    /// A flagged run waiting for redo or keep.
    pending: Option<RunRecord>,
    result_id: Option<String>,
}

impl Session {
    pub fn new(profile_id: &str) -> Self {
        Self {
            profile_id: profile_id.to_string(),
            pass: 0,
            phase: Phase::Waiting,
            game_running: false,
            runs: vec![None; usize::from(RUNS)],
            pending: None,
            result_id: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn pass(&self) -> u8 {
        self.pass
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::Done | Phase::Failed { .. } | Phase::Cancelled)
    }

    pub fn set_game_running(&mut self, running: bool) {
        self.game_running = running;
    }

    pub fn start_pressed(&mut self) -> Vec<Action> {
        if self.phase != Phase::Waiting || !self.game_running {
            return Vec::new();
        }
        self.phase = Phase::Capturing;
        vec![Action::BeepStart, Action::StartCapture { pass: self.pass }]
    }

    pub fn capture_finished(&mut self, outcome: Result<RunRecord, String>) -> Vec<Action> {
        if self.phase != Phase::Capturing {
            return Vec::new();
        }
        let run = match outcome {
            Ok(run) => run,
            Err(message) => {
                self.phase = Phase::Failed { message };
                return vec![Action::BeepEnd];
            }
        };
        let mut actions = vec![Action::BeepEnd];
        if self.pass == 0 {
            // The warm-up only warms the game up; its numbers and flags aren't kept.
            self.pass = 1;
            self.phase = Phase::Waiting;
        } else if run.flags.is_empty() {
            actions.extend(self.accept(run));
        } else {
            self.phase = Phase::Review { flags: run.flags.clone() };
            self.pending = Some(run);
        }
        actions
    }

    /// Throws the flagged run away and waits to capture it again.
    pub fn redo(&mut self) -> Vec<Action> {
        if matches!(self.phase, Phase::Review { .. }) {
            self.pending = None;
            self.phase = Phase::Waiting;
        }
        Vec::new()
    }

    /// Keeps the flagged run, marked as kept.
    pub fn keep(&mut self) -> Vec<Action> {
        if !matches!(self.phase, Phase::Review { .. }) {
            return Vec::new();
        }
        let Some(mut run) = self.pending.take() else {
            return Vec::new();
        };
        run.kept = true;
        self.accept(run)
    }

    pub fn cancel(&mut self) -> Vec<Action> {
        if self.is_over() {
            return Vec::new();
        }
        let was_capturing = self.phase == Phase::Capturing;
        self.phase = Phase::Cancelled;
        self.pending = None;
        if was_capturing {
            vec![Action::StopCapture]
        } else {
            Vec::new()
        }
    }

    /// Ends the session with an error (for example, saving the result failed).
    pub fn fail(&mut self, message: String) {
        if self.phase != Phase::Cancelled {
            self.phase = Phase::Failed { message };
        }
    }

    pub fn set_result_id(&mut self, id: String) {
        self.result_id = Some(id);
    }

    /// The accepted runs, in order.
    pub fn runs(&self) -> Vec<RunRecord> {
        self.runs.iter().flatten().cloned().collect()
    }

    pub fn view(&self) -> SessionView {
        SessionView {
            profile_id: self.profile_id.clone(),
            pass: self.pass,
            phase: self.phase.clone(),
            game_running: self.game_running,
            runs: self.runs.clone(),
            result_id: self.result_id.clone(),
        }
    }

    fn accept(&mut self, run: RunRecord) -> Vec<Action> {
        self.runs[usize::from(self.pass) - 1] = Some(run);
        if self.pass >= RUNS {
            self.phase = Phase::Done;
            vec![Action::Finish]
        } else {
            self.pass += 1;
            self.phase = Phase::Waiting;
            Vec::new()
        }
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `40 passed`; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): benchmark session state machine"
```

---

### Task 9: "Copy results" text

**Files:**
- Create: `crates/kestrel-bench/src/text.rs`
- Modify: `crates/kestrel-bench/src/lib.rs` (add `pub mod text;`)

**Interfaces:**
- Consumes: `model::*`, `profile::profile`.
- Produces: `kestrel_bench::text::results_text(bench: &Benchmark, comparison: Option<&Comparison>, baseline_name: Option<&str>) -> String` — plain English lines, joined by `\n`, for pasting into chat (decision 45).

- [ ] **Step 1: Write the failing tests** — create `text.rs` with only the tests and add `pub mod text;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BenchResult, Context, MetricDelta, RunMetrics, RunRecord, Spread};

    fn starting_point() -> Benchmark {
        let run = |avg: f64| RunRecord { metrics: RunMetrics { avg_fps: avg, ..Default::default() }, ..Default::default() };
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
                RunRecord { flags: vec![Flag::BusyBackground { name: "chrome.exe".into() }], kept: true, ..run(405.0) },
                run(418.0),
            ],
            result: BenchResult {
                metrics: RunMetrics { avg_fps: 410.0, low1_fps: 288.0, low01_fps: 201.0, stutters_per_min: 0.4, pc_latency_ms: Some(9.84), bottleneck: Bottleneck::Gpu, ..Default::default() },
                spread: Spread { avg_fps: 0.0317, ..Default::default() },
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
                MetricDelta { metric: MetricId::AvgFps, baseline: 410.0, current: 430.24, delta_pct: 4.94, verdict: Verdict::RealGain },
                MetricDelta { metric: MetricId::Low1Fps, baseline: 288.0, current: 290.9, delta_pct: 1.0, verdict: Verdict::Noise },
            ],
            settings_differ: vec!["Resolution".into()],
            ..Default::default()
        };
        let text = results_text(&later, Some(&comparison), Some("Starting point"));
        assert!(text.contains("Stutters 0.4/min | PC latency not reported | CPU-bound\n"), "{text}");
        assert!(text.contains("\nVersus Starting point: average FPS +4.9% (real gain), 1% low +1.0% (within noise)\n"), "{text}");
        assert!(text.contains("\nVALORANT settings that differ from Starting point: Resolution\n"), "{text}");
        let other_route = Comparison { route_mismatch: true, ..Default::default() };
        assert!(results_text(&later, Some(&other_route), Some("Starting point")).contains("\nNot compared with Starting point: different route\n"));
    }
}
```

Run: `cargo test -p kestrel-bench text`
Expected: FAIL — cannot find `results_text`.

- [ ] **Step 2: Implement** — above the tests in `text.rs`:

```rust
//! The text "Copy results" puts on the clipboard (decision 45). It's pasted into chat, so it's plain, short and
//! in English like the rest of the app.
use crate::model::{Benchmark, Bottleneck, Comparison, Flag, MetricId, Verdict};
use crate::profile::profile;

pub fn results_text(bench: &Benchmark, comparison: Option<&Comparison>, baseline_name: Option<&str>) -> String {
    let m = &bench.result.metrics;
    let game = profile(&bench.profile_id).map_or(bench.profile_id.as_str(), |p| p.name);
    let runs: Vec<String> = bench.runs.iter().map(|r| format!("{:.0}", r.metrics.avg_fps)).collect();
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
            m.pc_latency_ms.map_or("not reported".to_string(), |ms| format!("{ms:.1} ms")),
            bottleneck_text(m.bottleneck)
        ),
        format!("Runs: {} FPS", runs.join(" / ")),
    ];
    for (i, run) in bench.runs.iter().enumerate() {
        if run.kept && !run.flags.is_empty() {
            let reasons: Vec<String> = run.flags.iter().map(flag_text).collect();
            lines.push(format!("Run {} kept despite: {}", i + 1, reasons.join(", ")));
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
                .map(|d| format!("{} {:+.1}% ({})", metric_text(d.metric), d.delta_pct, verdict_text(d.verdict)))
                .collect();
            lines.push(format!("Versus {name}: {}", parts.join(", ")));
        }
        if !c.settings_differ.is_empty() {
            lines.push(format!("VALORANT settings that differ from {name}: {}", c.settings_differ.join(", ")));
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
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel-bench && cargo clippy -p kestrel-bench --all-targets -- -D warnings && cargo fmt -p kestrel-bench`
Expected: `42 passed`; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-bench
git commit -m "feat(bench): plain-text results for Copy results"
```

---
### Task 10: Probe — is the game running (no handles), background CPU on demand

**Files:**
- Create: `crates/kestrel-probe/src/process.rs`
- Modify: `crates/kestrel-probe/Cargo.toml` (windows-sys feature), `crates/kestrel-probe/src/lib.rs`, `crates/kestrel-probe/src/parse.rs`, `crates/kestrel-probe/src/win/mod.rs`, `crates/kestrel-probe/src/win/system_info.rs`

**Interfaces:**
- Consumes: the existing `win::system_info::busy_processes()` (PDH `\Process(*)\% Processor Time`, 1.5 s window) and `parse::busy_processes`.
- Produces: `kestrel_probe::process::game_running(process_name: &str) -> bool` (Toolhelp32 snapshot of process names; never opens a process; off Windows it is true only when the `KESTREL_FAKE_GAME` environment variable is set, for development); `kestrel_probe::busy_now() -> Option<Vec<kestrel_core::BusyProcess>>` (Windows: the existing sampler; elsewhere `Some(vec![])`); `kestrel_probe::parse::exe_name(buffer: &[u16]) -> String`.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `crates/kestrel-probe/src/parse.rs`:
```rust
    #[test]
    fn exe_names_stop_at_the_first_nul() {
        let mut buffer = [0u16; 260];
        for (i, unit) in "VALORANT-Win64-Shipping.exe".encode_utf16().enumerate() {
            buffer[i] = unit;
        }
        assert_eq!(exe_name(&buffer), "VALORANT-Win64-Shipping.exe");
        let full: Vec<u16> = "abc".encode_utf16().collect();
        assert_eq!(exe_name(&full), "abc", "no NUL: the whole buffer");
        assert_eq!(exe_name(&[0, 65, 66]), "");
    }
```

Create `crates/kestrel-probe/src/process.rs` with only its tests, and add `pub mod process;` to `lib.rs` after `pub mod parse;`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn a_process_that_does_not_exist_is_not_running() {
        assert!(!game_running("definitely-not-a-real-game-kestrel.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn this_test_process_is_found_by_name_in_any_case() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy().to_uppercase();
        assert!(game_running(&name));
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_game_runs_only_when_faked() {
        // Off Windows, `game_running` only reads KESTREL_FAKE_GAME, and this is the only test that calls it there (the
        // not-running test above is Windows-only), so setting the variable can't race another test.
        std::env::remove_var("KESTREL_FAKE_GAME");
        assert!(!game_running("VALORANT-Win64-Shipping.exe"));
        std::env::set_var("KESTREL_FAKE_GAME", "1");
        assert!(game_running("VALORANT-Win64-Shipping.exe"));
        std::env::remove_var("KESTREL_FAKE_GAME");
        assert_eq!(crate::busy_now(), Some(Vec::new()));
    }
}
```

Run: `cargo test -p kestrel-probe`
Expected: FAIL — cannot find `exe_name`, `game_running`, `busy_now`.

- [ ] **Step 2: Implement**

`crates/kestrel-probe/Cargo.toml`: add `"Win32_System_Diagnostics_ToolHelp"` to the windows-sys features:
```toml
windows-sys = { version = "0.61.2", features = ["Win32_Foundation", "Win32_Graphics_Gdi", "Win32_System_SystemInformation", "Win32_System_Performance", "Win32_System_Diagnostics_ToolHelp"] }
```

`crates/kestrel-probe/src/parse.rs`, above the tests module:
```rust
/// A NUL-terminated UTF-16 buffer (like a Toolhelp32 entry's exe name) → text up to the first NUL.
pub fn exe_name(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}
```

`crates/kestrel-probe/src/process.rs`, above the tests:
```rust
//! Whether a game is running, from the list of process names. Kestrel never opens the game's process: a Toolhelp32
//! snapshot only lists names.

/// True when a process named `process_name` (any letter case) is running.
pub fn game_running(process_name: &str) -> bool {
    #[cfg(windows)]
    {
        running_on_windows(process_name)
    }
    #[cfg(not(windows))]
    {
        // Development off Windows: set KESTREL_FAKE_GAME to pretend the game runs.
        let _ = process_name;
        std::env::var_os("KESTREL_FAKE_GAME").is_some()
    }
}

#[cfg(windows)]
fn running_on_windows(process_name: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    // SAFETY: a process snapshot copies the list of process names; it opens no process.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut found = false;
    // SAFETY: `snapshot` is open and `entry` is a valid PROCESSENTRY32W with dwSize set.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        if crate::parse::exe_name(&entry.szExeFile).eq_ignore_ascii_case(process_name) {
            found = true;
            break;
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: the snapshot was opened above and is closed once.
    unsafe { CloseHandle(snapshot) };
    found
}
```

`crates/kestrel-probe/src/win/mod.rs`: change `mod system_info;` to `pub(crate) mod system_info;`.

`crates/kestrel-probe/src/win/system_info.rs`: change `fn busy_processes() -> Option<Vec<BusyProcess>>` to `pub(crate) fn busy_processes() -> Option<Vec<BusyProcess>>`.

`crates/kestrel-probe/src/lib.rs`: after `collect_report`, add:
```rust
/// Apps keeping the CPU busy right now (measured over about 1.5 s), for flagging a benchmark run. Off Windows:
/// none. None when Windows can't give the counters.
pub fn busy_now() -> Option<Vec<kestrel_core::BusyProcess>> {
    #[cfg(windows)]
    {
        win::system_info::busy_processes()
    }
    #[cfg(not(windows))]
    {
        Some(Vec::new())
    }
}
```

- [ ] **Step 3: Run the tests and the Windows check**

Run: `cargo test -p kestrel-probe && cargo clippy -p kestrel-probe --all-targets -- -D warnings && cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc --all-targets -- -D warnings && cargo fmt -p kestrel-probe`
Expected: all pass (the two Windows-only process tests run on CI); both clippy runs clean.

- [ ] **Step 4: Commit**

```bash
git add crates/kestrel-probe Cargo.lock
git commit -m "feat(probe): find a game by process name without opening it; sample background CPU on demand"
```

---

### Task 11: App — PresentMon capture, beeps, build-time download

**Files:**
- Create: `app/scripts/fetch-presentmon.mjs`, `app/src-tauri/tauri.windows.conf.json`, `app/src-tauri/src/bench/mod.rs`, `app/src-tauri/src/bench/beep.rs`, `app/src-tauri/src/bench/capture.rs`
- Modify: `.gitignore`, `app/package.json` (script), `app/src-tauri/Cargo.toml`, `app/src-tauri/src/lib.rs` (`pub mod bench;`)

**Interfaces:**
- Consumes: `kestrel_bench::csv::parse` (Task 2) in tests.
- Produces: `bench::capture::{CaptureOutput {csv: String, stderr: String}, Capture` trait (`run(&self, process_name: &str, seconds: u32, csv_path: &Path, cancel: &AtomicBool) -> Result<CaptureOutput, String>`, `clean_up_stale_session(&self)`), `PresentMon::new(exe: PathBuf)`, `Simulated { seconds_per_capture: f32 }`, `simulated_csv(seconds: u32) -> String`, `presentmon_args(process_name, seconds, csv_path) -> Vec<String>`, `SESSION_NAME = "KestrelBench"`, `CANCELLED = "cancelled"`, `MISSING = "PresentMon is missing. Reinstall Kestrel and try again."}`; `PresentMon` runs only the exe it was given (a missing one fails with `MISSING`, never a search of PATH) and deletes a cancelled or timed-out capture's CSV; `bench::beep::{Cue {Start, Switch, End, Tick}, tones(Cue) -> &'static [(u32, u32)], play(Cue)}`. Bundled files land at `presentmon/PresentMon.exe` and `presentmon/LICENSE.txt` in the app's resource folder (Windows builds only).

- [ ] **Step 1: Build-time download with hash check** — `app/scripts/fetch-presentmon.mjs`:

```js
// Downloads Intel PresentMon 2.6.0 (MIT licence) for bundling. Build time only: Kestrel never downloads anything
// at runtime. A file whose SHA-256 differs from the pinned one is refused. Run from app/: npm run fetch:presentmon
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri', 'binaries');
const files = [
  {
    name: 'PresentMon-2.6.0-x64.exe',
    url: 'https://github.com/GameTechDev/PresentMon/releases/download/v2.6.0/PresentMon-2.6.0-x64.exe',
    sha256: 'b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af',
  },
  {
    name: 'PresentMon-LICENSE.txt',
    url: 'https://raw.githubusercontent.com/GameTechDev/PresentMon/v2.6.0/LICENSE.txt',
    sha256: '4c949341b1893c8c6ad82f7fb4eedf622cd1fd9c22a9af8f19b2dac19d1947b6',
  },
];

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

await mkdir(out, { recursive: true });
for (const file of files) {
  const path = join(out, file.name);
  const existing = await readFile(path).catch(() => null);
  if (existing && sha256(existing) === file.sha256) {
    console.log(`${file.name}: already here`);
    continue;
  }
  const response = await fetch(file.url);
  if (!response.ok) throw new Error(`${file.name}: download failed (${response.status})`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const actual = sha256(bytes);
  if (actual !== file.sha256) throw new Error(`${file.name}: SHA-256 ${actual} doesn't match the pinned ${file.sha256}`);
  await writeFile(path, bytes);
  console.log(`${file.name}: downloaded and verified (${bytes.length} bytes)`);
}
```

In `app/package.json` `scripts`, `"mock"` is the last entry: give it a trailing comma and add the new script after it, so the two lines read:
```json
    "mock": "cargo run -q -p kestrel-probe --bin kestrel-probe -- --pretty > src/api/mock-report.json",
    "fetch:presentmon": "node scripts/fetch-presentmon.mjs"
```

Append to `.gitignore`:
```
app/src-tauri/binaries/
```

`app/src-tauri/tauri.windows.conf.json` (merged into tauri.conf.json on Windows builds only, so Mac builds don't need the files):
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "bundle": {
    "resources": {
      "binaries/PresentMon-2.6.0-x64.exe": "presentmon/PresentMon.exe",
      "binaries/PresentMon-LICENSE.txt": "presentmon/LICENSE.txt"
    }
  }
}
```

Run: `cd app && npm run fetch:presentmon && npm run fetch:presentmon && cd ..`
Expected: first run prints "downloaded and verified (980320 bytes)" and "downloaded and verified (1067 bytes)"; the second prints "already here" twice. (The download is the build-time step; it's fine on the Mac.)

- [ ] **Step 2: Dependencies** — in `app/src-tauri/Cargo.toml`, `[dependencies]` gains:
```toml
kestrel-bench = { path = "../../crates/kestrel-bench" }
chrono = { version = "0.4.45", default-features = false, features = ["std", "clock"] }
```
and the Windows section becomes:
```toml
[target."cfg(windows)".dependencies]
windows-sys = { version = "0.61.2", features = ["Win32_UI_Shell", "Win32_System_Diagnostics_Debug"] }
```

- [ ] **Step 3: Write the failing tests** — create `app/src-tauri/src/bench/mod.rs`:
```rust
//! Stage 2's benchmark: capturing frames, pacing runs, and the commands the UI calls.
pub mod beep;
pub mod capture;
```
add `pub mod bench;` to `app/src-tauri/src/lib.rs` (after the `use` lines; `pub` so items the app wires up in Task 13 don't trip the dead-code lint before then), and create `beep.rs` and `capture.rs` with only their tests:

`app/src-tauri/src/bench/beep.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_is_one_long_tone_and_the_end_two_short_ones() {
        assert_eq!(tones(Cue::Start).len(), 1);
        assert!(tones(Cue::Start)[0].1 >= 500);
        assert_eq!(tones(Cue::Switch).len(), 1);
        assert_eq!(tones(Cue::End).len(), 2);
        assert!(tones(Cue::End).iter().all(|&(_, ms)| ms < 300));
    }
}
```

`app/src-tauri/src/bench/capture.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_bench::csv::parse;
    use kestrel_bench::metrics::measure;

    #[test]
    fn presentmon_gets_the_flags_from_the_spec() {
        let args = presentmon_args("VALORANT-Win64-Shipping.exe", 90, Path::new("C:\\k\\run-1.csv"));
        assert_eq!(
            args,
            [
                "--process_name", "VALORANT-Win64-Shipping.exe", "--output_file", "C:\\k\\run-1.csv", "--timed", "90",
                "--terminate_after_timed", "--session_name", "KestrelBench", "--stop_existing_session",
                "--no_console_stats", "--set_circular_buffer_size", "8192", "--track_pc_latency",
            ]
        );
    }

    #[test]
    fn the_simulated_capture_looks_like_a_real_run() {
        let frames = parse(&simulated_csv(90)).unwrap();
        let (m, _) = measure(&frames);
        assert!((m.duration_s - 90.0).abs() < 0.1, "{}", m.duration_s);
        assert!(m.avg_fps > 300.0 && m.avg_fps < 500.0, "{}", m.avg_fps);
        assert!(m.pc_latency_ms.is_some() && m.stutters_per_min > 0.0);
    }

    #[test]
    fn cancelling_a_simulated_capture_stops_it() {
        let cancel = AtomicBool::new(true);
        let started = std::time::Instant::now();
        let result = Simulated { seconds_per_capture: 5.0 }.run("x.exe", 90, Path::new("unused.csv"), &cancel);
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        let done = Simulated { seconds_per_capture: 0.0 }.run("x.exe", 2, Path::new("unused.csv"), &AtomicBool::new(false)).unwrap();
        assert!(done.csv.starts_with("Application,PresentMode,MsBetweenPresents"));
    }
}
```

Run: `cd app && npm run build && cd .. && cargo test -p kestrel bench::`
Expected: FAIL — cannot find `tones`, `Cue`, `presentmon_args`, `simulated_csv`, `Simulated`. (`npm run build` first: the Tauri crate needs `app/dist` to compile.)

- [ ] **Step 4: Implement `beep.rs`** — above its tests:

```rust
//! The beeps that pace a run (spec "What the owner sees", 3), through Windows' Beep. Silent off Windows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// One long beep: the capture starts.
    Start,
    /// One short beep at 30 s: switch from the walk to the bots.
    Switch,
    /// Two short beeps: the run is done.
    End,
    /// A short tick, for the fallback countdown.
    Tick,
}

/// (frequency in Hz, duration in ms) for each tone of a cue.
pub fn tones(cue: Cue) -> &'static [(u32, u32)] {
    match cue {
        Cue::Start => &[(880, 600)],
        Cue::Switch => &[(1175, 150)],
        Cue::End => &[(988, 150), (988, 150)],
        Cue::Tick => &[(660, 80)],
    }
}

/// Plays a cue on its own thread (Beep blocks while it sounds).
pub fn play(cue: Cue) {
    std::thread::spawn(move || {
        for (i, &(hz, ms)) in tones(cue).iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(90));
            }
            sound(hz, ms);
        }
    });
}

#[cfg(windows)]
fn sound(hz: u32, ms: u32) {
    // SAFETY: Beep takes two numbers and plays a tone; it touches nothing else.
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::Beep(hz, ms) };
}

#[cfg(not(windows))]
fn sound(_hz: u32, ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
}
```

- [ ] **Step 5: Implement `capture.rs`** — above its tests:

```rust
//! Running a capture. On Windows: the bundled PresentMon, hidden, limited to the game's process, for a fixed
//! time (spec "PresentMon"). Off Windows (development on a Mac): a simulated capture with made-up frames.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// PresentMon's ETW session name; a stale one with this name is cleaned up.
pub const SESSION_NAME: &str = "KestrelBench";
/// The error text of a cancelled capture.
pub const CANCELLED: &str = "cancelled";
/// The error when Kestrel's copy of PresentMon isn't there (spec Errors table: "PresentMon missing").
pub const MISSING: &str = "PresentMon is missing. Reinstall Kestrel and try again.";

#[derive(Debug, Clone, Default)]
pub struct CaptureOutput {
    pub csv: String,
    pub stderr: String,
}

pub trait Capture: Send + Sync {
    /// Captures `process_name` for `seconds`, using `csv_path` for PresentMon's output. Returns
    /// `Err(CANCELLED)` soon after `cancel` is set.
    fn run(&self, process_name: &str, seconds: u32, csv_path: &Path, cancel: &AtomicBool) -> Result<CaptureOutput, String>;
    /// Ends a capture session a crash or a cancel left running.
    fn clean_up_stale_session(&self);
}

/// The arguments of one PresentMon capture, in the order the spec lists them.
pub fn presentmon_args(process_name: &str, seconds: u32, csv_path: &Path) -> Vec<String> {
    [
        "--process_name",
        process_name,
        "--output_file",
        &csv_path.display().to_string(),
        "--timed",
        &seconds.to_string(),
        "--terminate_after_timed",
        "--session_name",
        SESSION_NAME,
        "--stop_existing_session",
        "--no_console_stats",
        "--set_circular_buffer_size",
        "8192",
        "--track_pc_latency",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The bundled PresentMon.
pub struct PresentMon {
    exe: PathBuf,
}

enum Attempt {
    Done(CaptureOutput),
    /// Exit code 6: the session name is in use.
    SessionInUse,
    Failed(String),
}

impl PresentMon {
    pub fn new(exe: PathBuf) -> Self {
        Self { exe }
    }

    fn command(&self) -> Command {
        #[cfg_attr(not(windows), allow(unused_mut))] // only Windows changes it (no console window)
        let mut command = Command::new(&self.exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }

    fn attempt(&self, process_name: &str, seconds: u32, csv_path: &Path, cancel: &AtomicBool) -> Attempt {
        // Only Kestrel's own copy runs: a missing file (or the empty path `capture_engine` passes when the resource
        // can't be found) fails here, before Windows could go looking for another PresentMon.exe.
        if !self.exe.is_file() {
            return Attempt::Failed(MISSING.into());
        }
        let _ = std::fs::remove_file(csv_path);
        let spawned = self
            .command()
            .args(presentmon_args(process_name, seconds, csv_path))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => return Attempt::Failed(format!("Couldn't start PresentMon: {e}")),
        };
        let mut pipe = child.stderr.take();
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(pipe) = pipe.as_mut() {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        });
        let deadline = Instant::now() + Duration::from_secs(u64::from(seconds) + 30);
        let status = loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                self.clean_up_stale_session();
                // A stopped capture's partial CSV is never read: delete it now (spec: temporary CSVs are deleted).
                let _ = std::fs::remove_file(csv_path);
                return Attempt::Failed(CANCELLED.into());
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() > deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    self.clean_up_stale_session();
                    let _ = std::fs::remove_file(csv_path);
                    return Attempt::Failed("PresentMon didn't finish in time.".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                Err(e) => return Attempt::Failed(format!("Lost track of PresentMon: {e}")),
            }
        };
        let stderr = reader.join().unwrap_or_default();
        if status.code() == Some(6) {
            return Attempt::SessionInUse;
        }
        match std::fs::read_to_string(csv_path) {
            Ok(csv) => {
                let _ = std::fs::remove_file(csv_path);
                Attempt::Done(CaptureOutput { csv, stderr })
            }
            Err(_) => Attempt::Failed(format!("PresentMon saved no frames ({status}). {}", stderr.trim())),
        }
    }
}

impl Capture for PresentMon {
    fn run(&self, process_name: &str, seconds: u32, csv_path: &Path, cancel: &AtomicBool) -> Result<CaptureOutput, String> {
        match self.attempt(process_name, seconds, csv_path, cancel) {
            Attempt::Done(output) => Ok(output),
            Attempt::Failed(message) => Err(message),
            Attempt::SessionInUse => {
                // A session named KestrelBench is still running (a crash, say): end it and try once more.
                self.clean_up_stale_session();
                match self.attempt(process_name, seconds, csv_path, cancel) {
                    Attempt::Done(output) => Ok(output),
                    Attempt::Failed(message) => Err(message),
                    Attempt::SessionInUse => Err("PresentMon's capture session is busy. Restart the PC and try again.".into()),
                }
            }
        }
    }

    fn clean_up_stale_session(&self) {
        let _ = self
            .command()
            .args(["--terminate_existing_session", "--session_name", SESSION_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Made-up frames for development off Windows; each capture takes `seconds_per_capture` of real time.
pub struct Simulated {
    pub seconds_per_capture: f32,
}

impl Capture for Simulated {
    fn run(&self, _process_name: &str, seconds: u32, _csv_path: &Path, cancel: &AtomicBool) -> Result<CaptureOutput, String> {
        let end = Instant::now() + Duration::from_secs_f32(self.seconds_per_capture.max(0.0));
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(CANCELLED.into());
            }
            let now = Instant::now();
            if now >= end {
                break;
            }
            std::thread::sleep((end - now).min(Duration::from_millis(50)));
        }
        Ok(CaptureOutput { csv: simulated_csv(seconds), stderr: String::new() })
    }

    fn clean_up_stale_session(&self) {}
}

/// About 400 FPS with small noise and a 12 ms hitch every 500 frames, `seconds` long.
pub fn simulated_csv(seconds: u32) -> String {
    let mut csv = String::from("Application,PresentMode,MsBetweenPresents,MsGPUBusy,MsPCLatency\n");
    let mut seed: u32 = 12_345;
    let (mut elapsed, mut frame) = (0.0_f64, 0_u32);
    while elapsed < f64::from(seconds) * 1000.0 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = f64::from(seed >> 8) / f64::from(1_u32 << 24);
        let ms = if frame % 500 == 499 { 12.0 } else { 2.2 + noise * 0.6 };
        csv.push_str(&format!(
            "VALORANT-Win64-Shipping.exe,Hardware: Independent Flip,{ms:.3},{:.3},{:.2}\n",
            ms * 0.82,
            9.0 + noise * 2.0
        ));
        elapsed += ms;
        frame += 1;
    }
    csv
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p kestrel bench:: && cargo clippy -p kestrel --all-targets -- -D warnings && cargo fmt -p kestrel`
Expected: `4 passed`; clippy clean. (Two pieces are Windows-only and first compile on CI: the `CREATE_NO_WINDOW` block in `PresentMon::command` and `beep::sound`, which calls Windows' `Beep`. Everything else here builds and runs on the Mac.)

- [ ] **Step 7: Commit**

```bash
git add .gitignore app/package.json app/scripts app/src-tauri/Cargo.toml app/src-tauri/tauri.windows.conf.json app/src-tauri/src Cargo.lock
git commit -m "feat(app): bundled PresentMon capture with a pinned download, beeps, and a simulated capture for development"
```

---
### Task 12: Session controller

**Files:**
- Create: `app/src-tauri/src/bench/controller.rs`
- Modify: `app/src-tauri/src/bench/mod.rs` (add `pub mod controller;`), `app/src-tauri/Cargo.toml` (dev-dependency)

**Interfaces:**
- Consumes: `kestrel_bench::{session::*, flags::*, csv, metrics, result, Benchmark, Context, GameProfile, RunRecord, Store, ValorantSettings, PRESENTMON_VERSION, profile}`, `bench::capture::{Capture, CaptureOutput, CANCELLED}`, `bench::beep::Cue`.
- Produces: `bench::controller::{Platform` trait (`register_start_key() -> bool`, `unregister_start_key()`, `game_running(&str) -> bool`, `beep(Cue)`, `busy_now() -> Vec<(String, f32)>`, `game_settings() -> Option<ValorantSettings>`, `emit(Option<&BenchState>)`), `StartMode {Hotkey, Countdown}`, `BenchState {session: SessionView (flattened), start_mode, start_key: &'static str, capture_seconds, switch_at_seconds, seconds_left: Option<u32>, countdown: Option<u32>}`, `Controller` (Clone) with `new(platform, capture, store: Arc<Store>, work_dir, kestrel_version)`, `with_timing(poll, tick)`, `store() -> &Store`, `game_settings()`, `state() -> Option<BenchState>`, `start(profile_id, context) -> Result<BenchState, String>`, `start_pressed()`, `start_countdown() -> Result<(), String>`, `redo()`, `keep()`, `cancel()`, `dismiss()`, `START_KEY = "F10"`, `ACTIVE_MARKER = "capture.active"`}`.
- Rules: the platform is never called while the controller's lock is held (claiming or releasing the key runs on the UI thread, which may be waiting for that lock to handle a key press); one session at a time; the key is given back as soon as a session is over; a capture's results (and a saved result's id) only ever reach the session that started it; the end-of-run double beep sounds as soon as PresentMon stops, before the 1.5 s analysis (spec: at 90 s), so `Action::BeepEnd` plays nothing more; a capture marker file exists in `work_dir` while PresentMon runs (Task 13 cleans up after a crash).

- [ ] **Step 1: Write the failing tests** — add to `app/src-tauri/Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3.27.0"
```
add `pub mod controller;` to `bench/mod.rs`, and create `controller.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::capture::Simulated;
    use std::path::Path;
    use std::sync::atomic::AtomicUsize;

    #[derive(Default)]
    struct Fake {
        key_free: AtomicBool,
        key_held: AtomicBool,
        game: AtomicBool,
        beeps: Mutex<Vec<Cue>>,
        busy: Mutex<Vec<(String, f32)>>,
        emitted: AtomicUsize,
    }

    impl Platform for Fake {
        fn register_start_key(&self) -> bool {
            let free = self.key_free.load(Ordering::SeqCst);
            if free {
                self.key_held.store(true, Ordering::SeqCst);
            }
            free
        }
        fn unregister_start_key(&self) {
            self.key_held.store(false, Ordering::SeqCst);
        }
        fn game_running(&self, _process_name: &str) -> bool {
            self.game.load(Ordering::SeqCst)
        }
        fn beep(&self, cue: Cue) {
            self.beeps.lock().unwrap().push(cue);
        }
        fn busy_now(&self) -> Vec<(String, f32)> {
            self.busy.lock().unwrap().clone()
        }
        fn game_settings(&self) -> Option<ValorantSettings> {
            Some(ValorantSettings { resolution: Some([1920, 1080]), ..Default::default() })
        }
        fn emit(&self, _state: Option<&BenchState>) {
            self.emitted.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Never finishes on its own: returns only when cancelled.
    #[derive(Default)]
    struct Blocking {
        started: AtomicBool,
    }

    impl Capture for Blocking {
        fn run(&self, _p: &str, _s: u32, _c: &Path, cancel: &AtomicBool) -> Result<CaptureOutput, String> {
            self.started.store(true, Ordering::SeqCst);
            while !cancel.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(CANCELLED.into())
        }
        fn clean_up_stale_session(&self) {}
    }

    fn setup(capture: Arc<dyn Capture>) -> (Controller, Arc<Fake>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::default());
        fake.key_free.store(true, Ordering::SeqCst);
        fake.game.store(true, Ordering::SeqCst);
        let store = Arc::new(Store::new(dir.path().join("benchmarks")));
        let controller = Controller::new(fake.clone(), capture, store, dir.path().join("work"), "0.2.0")
            .with_timing(Duration::from_millis(10), Duration::from_millis(1));
        (controller, fake, dir)
    }

    fn fast() -> Arc<dyn Capture> {
        Arc::new(Simulated { seconds_per_capture: 0.0 })
    }

    fn wait_for(c: &Controller, what: &str, check: impl Fn(&BenchState) -> bool) -> BenchState {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(state) = c.state() {
                if check(&state) {
                    return state;
                }
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}: {:?}", c.state());
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn eventually(what: &str, check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !check() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn waiting_at(pass: u8) -> impl Fn(&BenchState) -> bool {
        move |s| s.session.phase == Phase::Waiting && s.session.pass == pass
    }

    #[test]
    fn a_whole_session_saves_the_starting_point() {
        let (c, fake, _dir) = setup(fast());
        let state = c.start("valorant", Context { windows_build: 26200, ..Default::default() }).unwrap();
        assert_eq!((state.session.pass, state.start_mode, state.session.game_running), (0, StartMode::Hotkey, true));
        assert_eq!((state.start_key, state.capture_seconds, state.switch_at_seconds), ("F10", 90, 30));
        assert!(fake.key_held.load(Ordering::SeqCst));
        for pass in 0..3u8 {
            wait_for(&c, "the next pass", waiting_at(pass));
            c.start_pressed();
        }
        wait_for(&c, "run 3", waiting_at(3));
        c.start_pressed();
        let done = wait_for(&c, "the saved result", |s| s.session.phase == Phase::Done && s.session.result_id.is_some());
        let id = done.session.result_id.unwrap();
        let saved = c.store().get(&id).unwrap();
        assert_eq!(saved.name, kestrel_bench::STARTING_POINT);
        assert_eq!(saved.runs.len(), 3);
        assert!(saved.runs.iter().all(|r| r.flags.is_empty() && !r.kept));
        assert_eq!(saved.context.windows_build, 26200);
        assert_eq!(saved.context.game_settings.unwrap().resolution, Some([1920, 1080]));
        assert_eq!((saved.kestrel_version.as_str(), saved.presentmon_version.as_str()), ("0.2.0", "2.6.0"));
        assert!(c.store().frame_times(&id, 3).unwrap().len() > 30_000);
        eventually("the start key to be given back", || !fake.key_held.load(Ordering::SeqCst));
        let beeps = fake.beeps.lock().unwrap();
        assert_eq!(beeps.iter().filter(|&&b| b == Cue::Start).count(), 4);
        assert_eq!(beeps.iter().filter(|&&b| b == Cue::End).count(), 4);
        assert!(fake.emitted.load(Ordering::SeqCst) > 8);
    }

    #[test]
    fn a_flagged_run_waits_and_can_be_kept_or_redone() {
        let (c, fake, _dir) = setup(fast());
        *fake.busy.lock().unwrap() = vec![("chrome.exe".into(), 30.0)];
        c.start("valorant", Context::default()).unwrap();
        c.start_pressed();
        wait_for(&c, "run 1", waiting_at(1));
        c.start_pressed();
        let review = wait_for(&c, "review", |s| matches!(s.session.phase, Phase::Review { .. }));
        assert_eq!(review.session.phase, Phase::Review { flags: vec![kestrel_bench::Flag::BusyBackground { name: "chrome.exe".into() }] });
        c.redo();
        wait_for(&c, "run 1 again", waiting_at(1));
        c.start_pressed();
        wait_for(&c, "review again", |s| matches!(s.session.phase, Phase::Review { .. }));
        c.keep();
        wait_for(&c, "run 2", waiting_at(2));
        fake.busy.lock().unwrap().clear();
        c.start_pressed();
        wait_for(&c, "run 3", waiting_at(3));
        c.start_pressed();
        let done = wait_for(&c, "done", |s| s.session.result_id.is_some());
        let id = done.session.result_id.unwrap();
        let saved = c.store().get(&id).unwrap();
        assert!(saved.runs[0].kept && !saved.runs[1].kept);
        assert!(!c.store().frame_times(&id, 1).unwrap().is_empty(), "the kept run's frames were saved");
    }

    #[test]
    fn cancel_during_capture_stops_it() {
        let blocking = Arc::new(Blocking::default());
        let (c, fake, dir) = setup(blocking.clone());
        c.start("valorant", Context::default()).unwrap();
        c.start_pressed();
        wait_for(&c, "the capture", |s| s.session.phase == Phase::Capturing);
        eventually("the capture to start", || blocking.started.load(Ordering::SeqCst));
        assert!(dir.path().join("work").join(ACTIVE_MARKER).exists(), "a marker exists while capturing");
        c.cancel();
        wait_for(&c, "cancelled", |s| s.session.phase == Phase::Cancelled);
        eventually("the marker to go", || !dir.path().join("work").join(ACTIVE_MARKER).exists());
        eventually("the key to be given back", || !fake.key_held.load(Ordering::SeqCst));
        assert!(c.store().list().is_empty());
        c.dismiss();
        assert!(c.state().is_none());
    }

    #[test]
    fn nothing_starts_until_the_game_runs() {
        let (c, fake, _dir) = setup(fast());
        fake.game.store(false, Ordering::SeqCst);
        assert!(!c.start("valorant", Context::default()).unwrap().session.game_running);
        c.start_pressed();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(c.state().unwrap().session.phase, Phase::Waiting);
        assert!(fake.beeps.lock().unwrap().is_empty());
        fake.game.store(true, Ordering::SeqCst);
        wait_for(&c, "the game to be seen", |s| s.session.game_running);
        c.start_pressed();
        wait_for(&c, "run 1", waiting_at(1));
    }

    #[test]
    fn a_taken_key_falls_back_to_a_countdown() {
        let (c, fake, _dir) = setup(fast());
        fake.key_free.store(false, Ordering::SeqCst);
        assert_eq!(c.start("valorant", Context::default()).unwrap().start_mode, StartMode::Countdown);
        c.start_countdown().unwrap();
        wait_for(&c, "run 1 after the countdown", waiting_at(1));
        assert_eq!(fake.beeps.lock().unwrap().iter().filter(|&&b| b == Cue::Tick).count(), 10);
        assert!(c.state().unwrap().countdown.is_none());
    }

    #[test]
    fn one_session_at_a_time() {
        let (c, _fake, _dir) = setup(fast());
        c.start("valorant", Context::default()).unwrap();
        assert_eq!(c.start("valorant", Context::default()).unwrap_err(), "A benchmark is already running.");
        assert_eq!(c.start("fivem", Context::default()).unwrap_err(), "A benchmark is already running.");
        c.cancel();
        assert!(c.start("fivem", Context::default()).unwrap_err().contains("doesn't know"));
        assert!(c.start("valorant", Context::default()).is_ok(), "a cancelled session can be replaced");
    }
}
```

Run: `cd app && npm run build && cd .. && cargo test -p kestrel bench::controller`
Expected: FAIL — cannot find `Controller`, `Platform`, `BenchState`, …

- [ ] **Step 2: Implement** — above the tests in `controller.rs`:

```rust
//! Runs a benchmark session (decisions 37–49): the start key, the beeps, one capture at a time, and turning each
//! capture into a run. The rules live in kestrel_bench::session; this file performs the actions they return. The
//! outside world (start key, beeps, probes, events) sits behind `Platform`, so whole sessions run in tests with fakes.
use super::beep::Cue;
use super::capture::{Capture, CaptureOutput, CANCELLED};
use kestrel_bench::flags::{flags, RunInputs};
use kestrel_bench::session::{Action, Phase, Session, SessionView};
use kestrel_bench::{
    csv, metrics, result, Benchmark, Context, GameProfile, RunRecord, Store, ValorantSettings, PRESENTMON_VERSION,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The key that starts each pass (decision 37).
pub const START_KEY: &str = "F10";
/// Exists in the work folder while PresentMon runs; found at start-up, it means a crash mid-capture.
pub const ACTIVE_MARKER: &str = "capture.active";
const COUNTDOWN: u32 = 10;
const BUSY: &str = "A benchmark is already running.";

pub trait Platform: Send + Sync + 'static {
    /// Claims the start key; false when another app already holds it.
    fn register_start_key(&self) -> bool;
    fn unregister_start_key(&self);
    fn game_running(&self, process_name: &str) -> bool;
    fn beep(&self, cue: Cue);
    /// Background apps right now: name and share of the whole CPU, in %.
    fn busy_now(&self) -> Vec<(String, f32)>;
    fn game_settings(&self) -> Option<ValorantSettings>;
    /// Tells the UI the session changed; None means there's no session.
    fn emit(&self, state: Option<&BenchState>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartMode {
    /// The start key works.
    Hotkey,
    /// Another app holds the key: the UI's "Start run" button starts a 10-second countdown instead.
    Countdown,
}

/// What the UI shows about the session.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchState {
    #[serde(flatten)]
    pub session: SessionView,
    pub start_mode: StartMode,
    pub start_key: &'static str,
    pub capture_seconds: u32,
    pub switch_at_seconds: u32,
    /// Seconds left in the running capture.
    pub seconds_left: Option<u32>,
    /// Seconds left in the fallback countdown.
    pub countdown: Option<u32>,
}

struct Live {
    session: Session,
    profile: &'static GameProfile,
    start_mode: StartMode,
    seconds_left: Option<u32>,
    countdown: Option<u32>,
    /// Raised by Cancel. Also identifies this session, so a late result from an old capture can't reach a new one.
    cancel: Arc<AtomicBool>,
    /// Frame times of accepted runs (index = run − 1).
    frames: Vec<Option<Vec<f32>>>,
    /// Frame times of a flagged run waiting for redo or keep.
    pending_frames: Option<Vec<f32>>,
    context: Context,
}

#[derive(Clone)]
pub struct Controller {
    live: Arc<Mutex<Option<Live>>>,
    platform: Arc<dyn Platform>,
    capture: Arc<dyn Capture>,
    store: Arc<Store>,
    work_dir: PathBuf,
    kestrel_version: String,
    /// How often the game check repeats while a session waits.
    poll: Duration,
    /// One step of the fallback countdown.
    tick: Duration,
}

impl Controller {
    pub fn new(
        platform: Arc<dyn Platform>,
        capture: Arc<dyn Capture>,
        store: Arc<Store>,
        work_dir: PathBuf,
        kestrel_version: &str,
    ) -> Self {
        Self {
            live: Arc::new(Mutex::new(None)),
            platform,
            capture,
            store,
            work_dir,
            kestrel_version: kestrel_version.to_string(),
            poll: Duration::from_secs(2),
            tick: Duration::from_secs(1),
        }
    }

    pub fn with_timing(mut self, poll: Duration, tick: Duration) -> Self {
        self.poll = poll;
        self.tick = tick;
        self
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn game_settings(&self) -> Option<ValorantSettings> {
        self.platform.game_settings()
    }

    pub fn state(&self) -> Option<BenchState> {
        self.lock().as_ref().map(|live| BenchState {
            session: live.session.view(),
            start_mode: live.start_mode,
            start_key: START_KEY,
            capture_seconds: live.profile.route.capture_seconds,
            switch_at_seconds: live.profile.route.switch_at_seconds,
            seconds_left: live.seconds_left,
            countdown: live.countdown,
        })
    }

    /// Arms a session for `profile_id`. `context` describes the PC (from the last scan).
    pub fn start(&self, profile_id: &str, context: Context) -> Result<BenchState, String> {
        if self.busy() {
            return Err(BUSY.into());
        }
        let profile = kestrel_bench::profile(profile_id).ok_or("Kestrel doesn't know that game.")?;
        // Never call the platform with the lock held (see the module docs).
        self.platform.unregister_start_key();
        let start_mode = if self.platform.register_start_key() { StartMode::Hotkey } else { StartMode::Countdown };
        let mut session = Session::new(profile.id);
        session.set_game_running(self.platform.game_running(profile.process_name));
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut live = self.lock();
            if live.as_ref().is_some_and(|l| !l.session.is_over()) {
                return Err(BUSY.into());
            }
            *live = Some(Live {
                session,
                profile,
                start_mode,
                seconds_left: None,
                countdown: None,
                cancel: cancel.clone(),
                frames: vec![None; usize::from(kestrel_bench::session::RUNS)],
                pending_frames: None,
                context,
            });
        }
        self.watch_game(profile, cancel);
        self.emit();
        self.state().ok_or_else(|| "The benchmark didn't start.".to_string())
    }

    /// The start key was pressed (or the countdown ended).
    pub fn start_pressed(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) if live.countdown.is_none() => live.session.start_pressed(),
            _ => Vec::new(),
        };
        self.perform(actions);
    }

    /// Fallback when another app holds the start key: beeps for 10 seconds, then starts the pass.
    pub fn start_countdown(&self) -> Result<(), String> {
        let cancel = {
            let mut guard = self.lock();
            let live = guard.as_mut().ok_or("No benchmark is running.")?;
            if live.session.phase() != &Phase::Waiting {
                return Err("Wait for the current step to finish.".into());
            }
            if !live.session.view().game_running {
                return Err("Start the game first.".into());
            }
            if live.countdown.is_some() {
                return Ok(());
            }
            live.countdown = Some(COUNTDOWN);
            live.cancel.clone()
        };
        self.emit();
        let me = self.clone();
        std::thread::spawn(move || {
            for left in (1..=COUNTDOWN).rev() {
                if cancel.load(Ordering::SeqCst) {
                    return;
                }
                me.set_countdown(&cancel, Some(left));
                me.platform.beep(Cue::Tick);
                std::thread::sleep(me.tick);
            }
            me.set_countdown(&cancel, None);
            if !cancel.load(Ordering::SeqCst) {
                me.start_pressed();
            }
        });
        Ok(())
    }

    /// Throws away a flagged run; the next press captures it again.
    pub fn redo(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) => {
                live.pending_frames = None;
                live.session.redo()
            }
            None => Vec::new(),
        };
        self.perform(actions);
    }

    /// Keeps a flagged run.
    pub fn keep(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) if matches!(live.session.phase(), Phase::Review { .. }) => {
                let slot = usize::from(live.session.pass()) - 1;
                live.frames[slot] = live.pending_frames.take();
                live.session.keep()
            }
            _ => Vec::new(),
        };
        self.perform(actions);
    }

    pub fn cancel(&self) {
        let actions = match self.lock().as_mut() {
            Some(live) => {
                live.cancel.store(true, Ordering::SeqCst);
                live.countdown = None;
                live.session.cancel()
            }
            None => Vec::new(),
        };
        self.perform(actions);
    }

    /// Clears a finished session, so the page shows the results list again.
    pub fn dismiss(&self) {
        let cleared = {
            let mut guard = self.lock();
            if guard.as_ref().is_some_and(|l| l.session.is_over()) {
                if let Some(live) = guard.take() {
                    live.cancel.store(true, Ordering::SeqCst);
                }
                true
            } else {
                false
            }
        };
        if cleared {
            self.emit();
        }
    }

    fn busy(&self) -> bool {
        self.lock().as_ref().is_some_and(|l| !l.session.is_over())
    }

    fn lock(&self) -> MutexGuard<'_, Option<Live>> {
        // A panic elsewhere must not lock the benchmark forever.
        self.live.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn emit(&self) {
        let state = self.state();
        self.platform.emit(state.as_ref());
    }

    /// Carries out the session's actions, then gives the key back if the session is over, and updates the UI.
    fn perform(&self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::BeepStart => self.platform.beep(Cue::Start),
                // Already played by the capture thread the moment PresentMon stopped (see `spawn_capture`).
                Action::BeepEnd => {}
                Action::StartCapture { pass } => self.spawn_capture(pass),
                Action::StopCapture => {} // cancel() already raised the flag the capture watches
                Action::Finish => self.finish(),
            }
        }
        let over = self.lock().as_ref().is_some_and(|l| l.session.is_over());
        if over {
            self.platform.unregister_start_key();
        }
        self.emit();
    }

    fn spawn_capture(&self, pass: u8) {
        let Some((profile, cancel)) = self.lock().as_ref().map(|l| (l.profile, l.cancel.clone())) else {
            return;
        };
        let me = self.clone();
        std::thread::spawn(move || {
            let done = Arc::new(AtomicBool::new(false));
            let pacer = {
                let (me, done, cancel) = (me.clone(), done.clone(), cancel.clone());
                std::thread::spawn(move || me.pace(profile, &done, &cancel))
            };
            let _ = std::fs::create_dir_all(&me.work_dir);
            let marker = me.work_dir.join(ACTIVE_MARKER);
            let _ = std::fs::write(&marker, b"capturing");
            let csv_path = me.work_dir.join(format!("capture-{pass}.csv"));
            let outcome = me.capture.run(profile.process_name, profile.route.capture_seconds, &csv_path, &cancel);
            let _ = std::fs::remove_file(&marker);
            done.store(true, Ordering::SeqCst);
            let _ = pacer.join();
            // The double beep marks the end of the run as soon as PresentMon stops (spec: at 90 s), not after the
            // analysis below, which samples the CPU for 1.5 s. It sounds in the cases where the session answers with
            // `BeepEnd` (a capture that wasn't cancelled, for the session still capturing it), so `perform` skips that.
            let cancelled = matches!(&outcome, Err(message) if message == CANCELLED);
            if !cancelled && me.capturing(&cancel) {
                me.platform.beep(Cue::End);
            }
            let outcome = outcome.and_then(|output| me.analyse(&output));
            me.capture_finished(&cancel, pass, outcome);
        });
    }

    /// Whether the session `cancel` belongs to is still the live one and still capturing.
    fn capturing(&self, cancel: &Arc<AtomicBool>) -> bool {
        self.lock().as_ref().is_some_and(|l| Arc::ptr_eq(&l.cancel, cancel) && l.session.phase() == &Phase::Capturing)
    }

    /// Counts the capture down for the UI and beeps when it's time to switch to the bots.
    fn pace(&self, profile: &GameProfile, done: &AtomicBool, cancel: &Arc<AtomicBool>) {
        let started = Instant::now();
        let mut switched = false;
        while !done.load(Ordering::SeqCst) && !cancel.load(Ordering::SeqCst) {
            let elapsed = started.elapsed().as_secs() as u32;
            if !switched && elapsed >= profile.route.switch_at_seconds {
                switched = true;
                self.platform.beep(Cue::Switch);
            }
            self.set_seconds_left(cancel, Some(profile.route.capture_seconds.saturating_sub(elapsed)));
            std::thread::sleep(Duration::from_millis(200));
        }
        self.set_seconds_left(cancel, None);
    }

    fn analyse(&self, output: &CaptureOutput) -> Result<(RunRecord, Vec<f32>), String> {
        let frames = csv::parse(&output.csv).map_err(|e| format!("Couldn't read PresentMon's results ({e:?})."))?;
        let (metrics, times) = metrics::measure(&frames);
        let busy = self.platform.busy_now();
        let settings = self.platform.game_settings();
        let flags = flags(&RunInputs {
            frames: &frames,
            metrics: &metrics,
            stderr: &output.stderr,
            settings: settings.as_ref(),
            busy: &busy,
        });
        Ok((RunRecord { metrics, flags, kept: false }, times))
    }

    fn capture_finished(&self, cancel: &Arc<AtomicBool>, pass: u8, outcome: Result<(RunRecord, Vec<f32>), String>) {
        let actions = {
            let mut guard = self.lock();
            let Some(live) = guard.as_mut().filter(|l| Arc::ptr_eq(&l.cancel, cancel)) else {
                return;
            };
            match outcome {
                Err(message) if message == CANCELLED => Vec::new(),
                Err(message) => live.session.capture_finished(Err(message)),
                Ok((run, times)) => {
                    if pass > 0 {
                        let slot = usize::from(pass) - 1;
                        if run.flags.is_empty() {
                            live.frames[slot] = Some(times);
                        } else {
                            live.pending_frames = Some(times);
                        }
                    }
                    live.session.capture_finished(Ok(run))
                }
            }
        };
        self.perform(actions);
    }

    /// Combines the three runs and saves the benchmark (the first one becomes the starting point). Only a done session
    /// without a result is saved, and the id (or a save error) goes back only to that session, even if it was
    /// dismissed and another one started while saving.
    fn finish(&self) {
        let prepared = self.lock().as_ref().filter(|l| l.session.phase() == &Phase::Done && l.session.view().result_id.is_none()).map(|live| {
            let runs = live.session.runs();
            let bench = Benchmark {
                name: format!("Benchmark {}", chrono::Local::now().format("%Y-%m-%d %H:%M")),
                created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                profile_id: live.profile.id.to_string(),
                route_version: live.profile.route.version,
                kestrel_version: self.kestrel_version.clone(),
                presentmon_version: PRESENTMON_VERSION.to_string(),
                result: result::combine(&runs),
                runs,
                context: live.context.clone(),
                ..Default::default()
            };
            let frames: Vec<Vec<f32>> = live.frames.iter().map(|f| f.clone().unwrap_or_default()).collect();
            (bench, frames, live.cancel.clone())
        });
        let Some((mut bench, frames, cancel)) = prepared else {
            return;
        };
        bench.context.game_settings = self.platform.game_settings();
        let saved = self.store.save(&mut bench, &frames);
        if let Some(live) = self.lock().as_mut().filter(|l| Arc::ptr_eq(&l.cancel, &cancel)) {
            match saved {
                Ok(id) => live.session.set_result_id(id),
                Err(e) => live.session.fail(format!("Couldn't save the result: {e}")),
            }
        }
    }

    /// Re-checks whether the game runs until the session ends.
    fn watch_game(&self, profile: &'static GameProfile, cancel: Arc<AtomicBool>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(me.poll);
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            let running = me.platform.game_running(profile.process_name);
            let changed = {
                let mut guard = me.lock();
                match guard.as_mut().filter(|l| Arc::ptr_eq(&l.cancel, &cancel)) {
                    None => return,
                    Some(live) if live.session.is_over() => return,
                    Some(live) if live.session.view().game_running != running => {
                        live.session.set_game_running(running);
                        true
                    }
                    Some(_) => false,
                }
            };
            if changed {
                me.emit();
            }
        });
    }

    fn set_seconds_left(&self, cancel: &Arc<AtomicBool>, value: Option<u32>) {
        let changed = match self.lock().as_mut().filter(|l| Arc::ptr_eq(&l.cancel, cancel)) {
            Some(live) if live.seconds_left != value => {
                live.seconds_left = value;
                true
            }
            _ => false,
        };
        if changed {
            self.emit();
        }
    }

    fn set_countdown(&self, cancel: &Arc<AtomicBool>, value: Option<u32>) {
        if let Some(live) = self.lock().as_mut().filter(|l| Arc::ptr_eq(&l.cancel, cancel)) {
            live.countdown = value;
        }
        self.emit();
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p kestrel bench:: && cargo clippy -p kestrel --all-targets -- -D warnings && cargo fmt -p kestrel`
Expected: `10 passed` (4 from Task 11 + 6); clippy clean. If a timing test is slow on CI it has 20 s of headroom; it should finish in well under a second.

- [ ] **Step 4: Commit**

```bash
git add app/src-tauri Cargo.lock
git commit -m "feat(app): benchmark session controller with start key, beeps, captures and saving"
```

---

### Task 13: Tauri wiring — real platform, commands, diagnostics

**Files:**
- Create: `app/src-tauri/src/bench/commands.rs`
- Modify: `app/src-tauri/src/bench/mod.rs` (add `pub mod commands;`), `app/src-tauri/src/lib.rs`, `app/src-tauri/Cargo.toml` (global-shortcut plugin)

**Interfaces:**
- Consumes: `Controller` and friends (Task 12), `PresentMon`/`Simulated` (Task 11), `kestrel_bench::{result, settings, text, Store, PROFILES, …}`, `kestrel_probe::{process::game_running, busy_now}`, `kestrel_core::{Report, gpu}`.
- Produces: commands (all JSON camelCase) `bench_profiles() -> Vec<GameProfile>`, `bench_state() -> Result<Option<BenchState>, String>`, `bench_start(profile: String) -> Result<BenchState, String>`, `bench_start_run() -> Result<(), String>`, `bench_cancel()`, `bench_redo()`, `bench_keep()`, `bench_dismiss()` (each `-> Result<(), String>`), `bench_list() -> Result<Vec<BenchmarkSummary>, String>`, `bench_get(id) -> Result<BenchDetail, String>` where `BenchDetail {benchmark, graph: Vec<f32> /* ≤ 600 points */, comparison: Option<Comparison>, baselineName: Option<String>}`, `bench_rename(id, name)`, `bench_delete(id)`, `bench_set_baseline(id)` (each `-> Result<(), String>`), `bench_compare(baselineId, currentId) -> Result<Comparison, String>`, `bench_copy_results(id) -> Result<String, String>`; event `bench-state` (payload `BenchState` or `null`); `get_diagnostics` now returns `{"report": …, "benchmarks": [...], "valorantSettings": …}`. Pure helpers for tests: `context_from(&Report) -> Context`, `detail(&Store, id) -> Result<BenchDetail, String>`, `copy_results(&Store, id) -> Result<String, String>`, `start_key() -> Shortcut`. Saved benchmarks live in app-data `benchmarks`; the capture's work files (CSVs and the `capture.active` marker) in its sibling `bench-work`, which no benchmark id can name. Only the bundled `presentmon/PresentMon.exe` ever runs: if it can't be found, every capture fails with "PresentMon is missing" (no search of PATH). F10 presses are handed to a worker thread, never handled on the UI thread.

- [ ] **Step 1: Write the failing tests** — add to `app/src-tauri/Cargo.toml` `[dependencies]`:
```toml
tauri-plugin-global-shortcut = "=2.4.0"
```
(pinned: 2.4.1 came out on 2026-10-10, too new to trust yet), add `pub mod commands;` to `bench/mod.rs`, and create `commands.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_bench::{RunMetrics, RunRecord};

    #[test]
    fn the_context_comes_from_the_scan() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let report = kestrel_core::build_report(kestrel_core::owner_pc_fixture(), date, "test");
        let c = context_from(&report);
        assert_eq!(c.power_plan.as_deref(), Some("Balanced"));
        assert_eq!(c.ram_mts, Some(3600));
        assert_eq!(c.gpu.as_deref(), Some("NVIDIA GeForce RTX 3060 Ti"), "the real GPU, not Parsec's virtual one");
        assert_eq!(c.gpu_driver.as_deref(), Some("610.74"));
        assert_eq!((c.windows_build, c.display_hz), (26200, Some(360)));
        assert!(c.game_settings.is_none(), "added when the benchmark is saved");
    }

    fn saved(store: &Store, created_at: &str, avg: f64) -> String {
        let runs: Vec<RunRecord> = [avg - 5.0, avg, avg + 5.0]
            .iter()
            .map(|&a| RunRecord { metrics: RunMetrics { avg_fps: a, low1_fps: a / 2.0, low01_fps: a / 3.0, ..Default::default() }, ..Default::default() })
            .collect();
        let mut bench = Benchmark {
            name: "Benchmark".into(),
            created_at: created_at.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            result: kestrel_bench::result::combine(&runs),
            runs,
            ..Default::default()
        };
        let frames = vec![vec![4.0_f32; 1_000], vec![4.5_f32; 1_000], vec![5.0_f32; 1_000]];
        store.save(&mut bench, &frames).unwrap()
    }

    #[test]
    fn details_compare_with_the_baseline_and_draw_the_middle_run() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let first = saved(&store, "2026-10-12T21:30:05Z", 400.0);
        let second = saved(&store, "2026-10-13T08:00:00Z", 440.0);
        let d = detail(&store, &second).unwrap();
        assert_eq!(d.baseline_name.as_deref(), Some(kestrel_bench::STARTING_POINT));
        let c = d.comparison.unwrap();
        assert_eq!((c.baseline_id.as_str(), c.current_id.as_str()), (first.as_str(), second.as_str()));
        assert!(!d.graph.is_empty() && d.graph.len() <= 600);
        assert!(d.graph.iter().all(|&t| t == 4.5), "the middle run (run 2, 4.5 ms frames) draws the graph");
        assert!(detail(&store, &first).unwrap().comparison.is_none(), "the baseline isn't compared with itself");
        assert!(detail(&store, "nope").is_err());
        let text = copy_results(&store, &second).unwrap();
        assert!(text.starts_with("Kestrel benchmark: Benchmark ("), "{text}");
        assert!(text.contains("Versus Starting point: average FPS +10.0% (real gain)"), "{text}");
    }
}
```

Run: `cd app && npm run build && cd .. && cargo test -p kestrel bench::commands`
Expected: FAIL — cannot find `context_from`, `detail`, `copy_results`.

- [ ] **Step 2: Implement `commands.rs`** — above the tests:

```rust
//! The Tauri side of the benchmark: the real platform (start key, beeps, probes, events), the commands the UI calls,
//! and helpers the commands share. Every command that touches the controller runs off the UI thread (`async`),
//! because claiming and releasing the start key need the UI thread free.
use super::beep;
use super::capture::Capture;
use super::controller::{BenchState, Controller, Platform, ACTIVE_MARKER};
use kestrel_bench::{
    result, settings, text, Benchmark, BenchmarkSummary, Comparison, Context, GameProfile, Store, ValorantSettings,
    PROFILES,
};
use kestrel_core::Report;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut};

/// Points in a result's frame-time graph.
const GRAPH_POINTS: usize = 600;
const NOT_FOUND: &str = "That benchmark doesn't exist.";

/// The start key, F10 with no modifiers (decision 37).
pub fn start_key() -> Shortcut {
    Shortcut::new(None, Code::F10)
}

struct TauriPlatform {
    app: AppHandle,
}

impl Platform for TauriPlatform {
    fn register_start_key(&self) -> bool {
        self.app.global_shortcut().register(start_key()).is_ok()
    }

    fn unregister_start_key(&self) {
        let _ = self.app.global_shortcut().unregister(start_key());
    }

    fn game_running(&self, process_name: &str) -> bool {
        kestrel_probe::process::game_running(process_name)
    }

    fn beep(&self, cue: beep::Cue) {
        beep::play(cue);
    }

    fn busy_now(&self) -> Vec<(String, f32)> {
        kestrel_probe::busy_now().unwrap_or_default().into_iter().map(|p| (p.name, p.cpu_percent)).collect()
    }

    fn game_settings(&self) -> Option<ValorantSettings> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        settings::read(&PathBuf::from(local).join("VALORANT").join("Saved").join("Config"))
    }

    fn emit(&self, state: Option<&BenchState>) {
        let _ = self.app.emit("bench-state", state);
    }
}

/// The benchmark controller, with saved results in the app's own folder.
pub fn controller(app: &AppHandle) -> tauri::Result<Controller> {
    let data = app.path().app_data_dir()?;
    // Beside the benchmarks folder, not inside it, so no benchmark id (bench_delete("work"), say) can reach it.
    let work = data.join("bench-work");
    let capture = capture_engine(app);
    // A crash during a capture leaves the marker behind: end that capture session first.
    if work.join(ACTIVE_MARKER).exists() {
        capture.clean_up_stale_session();
        let _ = std::fs::remove_file(work.join(ACTIVE_MARKER));
    }
    let platform = Arc::new(TauriPlatform { app: app.clone() });
    let store = Arc::new(Store::new(data.join("benchmarks")));
    Ok(Controller::new(platform, capture, store, work, env!("CARGO_PKG_VERSION")))
}

#[cfg(windows)]
fn capture_engine(app: &AppHandle) -> Arc<dyn Capture> {
    // Only the bundled copy, never a PresentMon.exe found on PATH. If the resource can't be resolved, the empty path
    // makes every capture fail with "PresentMon is missing" (capture::MISSING), and the session shows that error.
    let exe = app
        .path()
        .resolve("presentmon/PresentMon.exe", tauri::path::BaseDirectory::Resource)
        .unwrap_or_default();
    Arc::new(super::capture::PresentMon::new(exe))
}

#[cfg(not(windows))]
fn capture_engine(_app: &AppHandle) -> Arc<dyn Capture> {
    // Development off Windows: made-up frames, three seconds per capture.
    Arc::new(super::capture::Simulated { seconds_per_capture: 3.0 })
}

/// The PC as the last scan saw it, for a benchmark's context.
pub fn context_from(report: &Report) -> Context {
    let s = &report.snapshot;
    let gpu = kestrel_core::gpu::main_gpu(&s.gpus);
    Context {
        power_plan: s.windows.power_plan.as_ref().map(|p| p.name.clone()),
        ram_mts: kestrel_core::ram::configured_mts(&s.memory.modules),
        gpu: gpu.map(|g| g.name.clone()),
        gpu_driver: gpu.map(kestrel_core::gpu::display_driver_version),
        windows_build: s.os.build,
        display_hz: s.displays.iter().find(|d| d.is_primary && !d.is_virtual).map(|d| d.current_hz),
        game_settings: None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchDetail {
    pub benchmark: Benchmark,
    /// The middle run's frame times, squeezed to at most 600 points.
    pub graph: Vec<f32>,
    /// Against the baseline; None for the baseline itself.
    pub comparison: Option<Comparison>,
    pub baseline_name: Option<String>,
}

pub fn detail(store: &Store, id: &str) -> Result<BenchDetail, String> {
    let benchmark = store.get(id).ok_or(NOT_FOUND)?;
    let graph = result::middle_run(&benchmark.runs)
        .and_then(|i| store.frame_times(id, i + 1))
        .map(|times| result::graph_points(&times, GRAPH_POINTS))
        .unwrap_or_default();
    let baseline = store.baseline().filter(|b| b != id).and_then(|b| store.get(&b));
    Ok(BenchDetail {
        comparison: baseline.as_ref().map(|b| result::compare(b, &benchmark)),
        baseline_name: baseline.map(|b| b.name),
        benchmark,
        graph,
    })
}

pub fn copy_results(store: &Store, id: &str) -> Result<String, String> {
    let d = detail(store, id)?;
    Ok(text::results_text(&d.benchmark, d.comparison.as_ref(), d.baseline_name.as_deref()))
}

#[tauri::command]
pub fn bench_profiles() -> Vec<GameProfile> {
    PROFILES.to_vec()
}

#[tauri::command(async)]
pub fn bench_state(controller: State<'_, Controller>) -> Result<Option<BenchState>, String> {
    Ok(controller.state())
}

#[tauri::command]
pub async fn bench_start(app: AppHandle, profile: String) -> Result<BenchState, String> {
    let report = crate::latest_report(&app).await?;
    let controller = app.state::<Controller>();
    controller.start(&profile, context_from(&report))
}

#[tauri::command(async)]
pub fn bench_start_run(controller: State<'_, Controller>) -> Result<(), String> {
    controller.start_countdown()
}

#[tauri::command(async)]
pub fn bench_cancel(controller: State<'_, Controller>) -> Result<(), String> {
    controller.cancel();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_redo(controller: State<'_, Controller>) -> Result<(), String> {
    controller.redo();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_keep(controller: State<'_, Controller>) -> Result<(), String> {
    controller.keep();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_dismiss(controller: State<'_, Controller>) -> Result<(), String> {
    controller.dismiss();
    Ok(())
}

#[tauri::command(async)]
pub fn bench_list(controller: State<'_, Controller>) -> Result<Vec<BenchmarkSummary>, String> {
    Ok(controller.store().list())
}

#[tauri::command(async)]
pub fn bench_get(controller: State<'_, Controller>, id: String) -> Result<BenchDetail, String> {
    detail(controller.store(), &id)
}

#[tauri::command(async)]
pub fn bench_rename(controller: State<'_, Controller>, id: String, name: String) -> Result<(), String> {
    controller.store().rename(&id, &name).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn bench_delete(controller: State<'_, Controller>, id: String) -> Result<(), String> {
    controller.store().delete(&id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn bench_set_baseline(controller: State<'_, Controller>, id: String) -> Result<(), String> {
    controller.store().set_baseline(&id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn bench_compare(controller: State<'_, Controller>, baseline_id: String, current_id: String) -> Result<Comparison, String> {
    let store = controller.store();
    let baseline = store.get(&baseline_id).ok_or(NOT_FOUND)?;
    let current = store.get(&current_id).ok_or(NOT_FOUND)?;
    Ok(result::compare(&baseline, &current))
}

#[tauri::command(async)]
pub fn bench_copy_results(controller: State<'_, Controller>, id: String) -> Result<String, String> {
    copy_results(controller.store(), &id)
}
```

Note: Tauri passes command arguments named in camelCase from JS (`baselineId`, `currentId`) to these snake_case parameters automatically.

- [ ] **Step 3: Wire it into `app/src-tauri/src/lib.rs`** (whole file):

```rust
//! Kestrel's Tauri shell: the scan, live load and the benchmark (stage 2). It writes only inside its own folder.
use kestrel_core::Report;
use kestrel_probe::load::{LoadSample, LoadSampler};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

pub mod bench;

use bench::commands;
use bench::controller::Controller;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct AppState {
    /// The last scan; `None` until the first one finishes.
    report: Mutex<Option<Report>>,
    /// One sampler for the whole run, created and primed in `setup`: its first reading is unreliable.
    load: Mutex<LoadSampler>,
}

/// The last report; scans first if there isn't one yet.
#[tauri::command]
async fn get_report(state: State<'_, AppState>) -> Result<Report, String> {
    let cached = state.report.lock().map_err(|e| e.to_string())?.clone();
    match cached {
        Some(report) => Ok(report),
        None => rescan(state).await,
    }
}

/// Scans the PC again (on a worker thread) and keeps the result.
#[tauri::command]
async fn rescan(state: State<'_, AppState>) -> Result<Report, String> {
    let report = tauri::async_runtime::spawn_blocking(|| kestrel_probe::collect_report(VERSION))
        .await
        .map_err(|e| e.to_string())?;
    *state.report.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(report)
}

/// The last scan, scanning first if needed: a benchmark records the PC's context from it.
pub(crate) async fn latest_report(app: &AppHandle) -> Result<Report, String> {
    get_report(app.state::<AppState>()).await
}

/// One live-load reading; the UI polls this about once a second.
// Keep `async`: the PDH and NVML reads must stay off the UI thread.
#[tauri::command(async)]
fn get_load(state: State<'_, AppState>) -> Result<LoadSample, String> {
    let mut sampler = state.load.lock().map_err(|e| e.to_string())?;
    Ok(sampler.sample())
}

/// Pretty JSON for the "Copy diagnostics" button: the last scan, the saved benchmarks, and VALORANT's settings.
#[tauri::command(async)]
fn get_diagnostics(state: State<'_, AppState>, bench: State<'_, Controller>) -> Result<String, String> {
    let report = state.report.lock().map_err(|e| e.to_string())?.clone().ok_or("Run a scan first.")?;
    let diagnostics = serde_json::json!({
        "report": report,
        "benchmarks": bench.store().list(),
        "valorantSettings": bench.game_settings(),
    });
    serde_json::to_string_pretty(&diagnostics).map_err(|e| e.to_string())
}

/// True when Kestrel runs as administrator, which its manifest asks for at launch.
#[tauri::command]
fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        // SAFETY: IsUserAnAdmin takes no arguments and only reads this process's token.
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed && *shortcut == commands::start_key() {
                        // The plugin holds its shortcut lock while this runs on the UI thread, and the controller
                        // may release the key (which needs both): hand the press to a worker thread.
                        let controller = app.state::<Controller>().inner().clone();
                        std::thread::spawn(move || controller.start_pressed());
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Sampled once here so the UI's first poll already has a baseline to measure against.
            let mut sampler = LoadSampler::new();
            sampler.sample();
            app.manage(AppState {
                report: Mutex::new(None),
                load: Mutex::new(sampler),
            });
            app.manage(commands::controller(app.handle())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_report,
            rescan,
            get_load,
            get_diagnostics,
            is_elevated,
            commands::bench_profiles,
            commands::bench_state,
            commands::bench_start,
            commands::bench_start_run,
            commands::bench_cancel,
            commands::bench_redo,
            commands::bench_keep,
            commands::bench_dismiss,
            commands::bench_list,
            commands::bench_get,
            commands::bench_rename,
            commands::bench_delete,
            commands::bench_set_baseline,
            commands::bench_compare,
            commands::bench_copy_results
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kestrel");
}
```

- [ ] **Step 4: Run the tests and a Mac smoke run**

Run: `cargo test -p kestrel && cargo clippy -p kestrel --all-targets -- -D warnings && cargo fmt -p kestrel && cargo build -p kestrel`
Expected: `12 passed`; clippy clean; build succeeds.

Run (optional manual check on the Mac): `cd app && KESTREL_FAKE_GAME=1 npm run tauri dev`, then in the window's devtools console:
`await window.__TAURI_INTERNALS__.invoke('bench_start', { profile: 'valorant' })` → a state with `"phase": "waiting"`, `"gameRunning": true`. Close the window afterwards.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri Cargo.lock
git commit -m "feat(app): benchmark commands, F10 through the global-shortcut plugin, diagnostics with benchmarks"
```

---
### Task 14: UI — benchmark types, client, browser sample, helpers

**Files:**
- Modify: `app/src/api/types.ts`, `app/src/api/client.ts`, `app/src/strings.ts` (add the `bench` block)
- Create: `app/src/api/sample-bench.ts`, `app/src/api/bench.ts`, `app/src/api/bench.test.ts`

**Interfaces:**
- Consumes: the commands and `bench-state` event of Task 13 (JSON shapes from Tasks 1, 8, 12, 13).
- Produces: TS types `Bottleneck, RunMetrics, Flag, RunRecord, BenchResult, ValorantSettings, BenchContext, Benchmark, BenchmarkSummary, MetricId, Verdict, MetricDelta, Comparison, BenchDetail, GameProfile, SessionPhase, BenchState`; client functions `benchProfiles, benchState, benchStart(profile), benchStartRun, benchCancel, benchRedo, benchKeep, benchDismiss, benchList, benchGet(id), benchRename(id, name), benchDelete(id), benchSetBaseline(id), benchCopyResults(id), onBenchState(handler) -> Promise<() => void>` (browser mode: a sample list and a fake session that runs in seconds); helpers in `bench.ts`: `metricValue(m, id)`, `metricText(id, value)`, `pct(n)`, `spreadText(ratio)`, `verdictTone(v)`, `verdictText(v)`, `flagText(f)`, `passLabel(pass)`, `sessionHeadline(state)`, `graphShape(graph) -> {points, maxMs, medianMs}`, `dateText(iso)`, `settingsText(settings)` (VALORANT's recorded settings in one line), `METRIC_ORDER`.

- [ ] **Step 1: Types** — append to `app/src/api/types.ts`:

```ts
// Stage 2: the benchmark (Rust: crates/kestrel-bench and app/src-tauri/src/bench).
export type Bottleneck = 'gpu' | 'cpu' | 'mixed' | 'unknown';
export interface RunMetrics {
  frames: number;
  durationS: number;
  avgFps: number;
  low1Fps: number;
  low01Fps: number;
  stuttersPerMin: number;
  pcLatencyMs: number | null;
  gpuBusyRatio: number | null;
  bottleneck: Bottleneck;
}
export type Flag =
  | { kind: 'too-short' }
  | { kind: 'not-in-front' }
  | { kind: 'fps-capped' }
  | { kind: 'dropped-data' }
  | { kind: 'busy-background'; name: string };
export interface RunRecord { metrics: RunMetrics; flags: Flag[]; kept: boolean }
export interface BenchResult { metrics: RunMetrics; spread: { avgFps: number; low1Fps: number; low01Fps: number } }
export interface ValorantSettings {
  resolution: [number, number] | null;
  fullscreenMode: number | null;
  vsync: boolean | null;
  frameRateLimit: number | null;
  reflex: number | null;
  multithreadedRendering: boolean | null;
  quality: Record<string, string>;
  limits: Record<string, string>;
}
export interface BenchContext {
  powerPlan: string | null;
  ramMts: number | null;
  gpu: string | null;
  gpuDriver: string | null;
  windowsBuild: number;
  displayHz: number | null;
  gameSettings: ValorantSettings | null;
}
export interface Benchmark {
  id: string;
  name: string;
  createdAt: string;
  profileId: string;
  routeVersion: number;
  kestrelVersion: string;
  presentmonVersion: string;
  runs: RunRecord[];
  result: BenchResult;
  context: BenchContext;
}
export interface BenchmarkSummary {
  id: string;
  name: string;
  createdAt: string;
  profileId: string;
  routeVersion: number;
  result: BenchResult;
  isBaseline: boolean;
  flaggedRunsKept: number;
}
export type MetricId = 'avg-fps' | 'low1-fps' | 'low01-fps' | 'stutters-per-min' | 'pc-latency-ms';
export type Verdict = 'real-gain' | 'real-loss' | 'noise';
export interface MetricDelta { metric: MetricId; baseline: number; current: number; deltaPct: number; verdict: Verdict }
export interface Comparison { baselineId: string; currentId: string; metrics: MetricDelta[]; settingsDiffer: string[]; routeMismatch: boolean }
export interface BenchDetail { benchmark: Benchmark; graph: number[]; comparison: Comparison | null; baselineName: string | null }
export interface GameProfile { id: string; name: string; processName: string; route: { version: number; captureSeconds: number; switchAtSeconds: number } }
export type SessionPhase =
  | { phase: 'waiting' }
  | { phase: 'capturing' }
  | { phase: 'review'; flags: Flag[] }
  | { phase: 'done' }
  | { phase: 'failed'; message: string }
  | { phase: 'cancelled' };
export type BenchState = SessionPhase & {
  profileId: string;
  pass: number;
  gameRunning: boolean;
  runs: (RunRecord | null)[];
  resultId: string | null;
  startMode: 'hotkey' | 'countdown';
  startKey: string;
  captureSeconds: number;
  switchAtSeconds: number;
  secondsLeft: number | null;
  countdown: number | null;
};
```

- [ ] **Step 2: Strings** — in `app/src/strings.ts`, add this block after `diagnostics` (and leave the rest unchanged):

```ts
  bench: {
    title: 'Benchmark',
    intro: 'Measures how smoothly VALORANT runs on a fixed route in the Range. Kestrel only listens to the frames Windows shows; it never touches the game.',
    route: (version: number) => `Route v${version}`,
    setup: 'Before you start: open VALORANT → Practice → The Range. Take a Vandal from the weapon wall. In the bot settings choose Strafe and bot count 5.',
    steps: [
      'Stand on the spawn platform facing the range. Press F10: one long beep.',
      '0–30 s: walk straight down the main path to the bot area without stopping, looking ahead.',
      'At the short beep (30 s): start the strafing bots and shoot them with the Vandal.',
      'At the double beep (90 s): stop and walk back to the spawn platform.',
    ],
    passes: 'A benchmark is a warm-up and 3 runs, about 6 minutes. Kestrel keeps the middle result of the 3.',
    readOnly: 'Kestrel reads VALORANT’s settings to note them with each result. It never changes them.',
    start: 'Start benchmark',
    starting: 'Starting…',
    passLabel: (pass: number) => (pass === 0 ? 'Warm-up' : `Run ${pass} of 3`),
    passChip: (pass: number) => (pass === 0 ? 'Warm-up' : `Run ${pass}`),
    begins: (pass: number) => (pass === 0 ? 'the warm-up' : `run ${pass} of 3`),
    waitingForGame: 'Start VALORANT and open the Range. Kestrel waits for the game.',
    pressKey: (key: string, what: string) => `Stand at the start and press ${key} to begin ${what}.`,
    pressButton: 'Stand at the start, then click Start run: you get 10 seconds to switch to the game. (Another app is using F10.)',
    countdown: (n: number) => `Starting in ${n}…`,
    walk: (pass: string) => `${pass}: walk the main path to the bots.`,
    shoot: (pass: string) => `${pass}: shoot the strafing bots.`,
    secondsLeft: (n: number) => `${n} s`,
    review: (pass: string) => `${pass} can’t be trusted.`,
    done: 'Benchmark saved.',
    failed: 'The benchmark stopped.',
    cancelled: 'Benchmark cancelled.',
    startRun: 'Start run',
    redo: (pass: number) => `Redo run ${pass}`,
    keep: 'Keep anyway',
    cancel: 'Cancel',
    seeResult: 'See the result',
    close: 'Close',
    runOk: 'OK',
    runKept: 'Kept',
    results: 'Results',
    none: 'No benchmarks yet. Your first one becomes your starting point.',
    baseline: 'Baseline',
    kept: (n: number) => `${n} kept run${n === 1 ? '' : 's'}`,
    metrics: { 'avg-fps': 'Average FPS', 'low1-fps': '1% low', 'low01-fps': '0.1% low', 'stutters-per-min': 'Stutters', 'pc-latency-ms': 'PC latency' },
    bottleneckLabel: 'Limited by',
    bottleneck: { gpu: 'GPU-bound', cpu: 'CPU-bound', mixed: 'CPU and GPU both limit', unknown: 'Not measured' },
    spread: (text: string) => `Runs within ${text}`,
    notReported: 'Not reported',
    perMinute: (text: string) => `${text}/min`,
    milliseconds: (text: string) => `${text} ms`,
    versus: (name: string) => `Compared with ${name}`,
    verdict: { 'real-gain': 'Real gain', 'real-loss': 'Real loss', noise: 'Within noise' },
    settingsDiffer: (list: string) => `VALORANT settings changed since then: ${list}. Part of the difference may come from them.`,
    routeMismatch: 'Different route version, so these results aren’t compared.',
    graph: 'Frame times of the middle run',
    graphAxis: 'ms per frame, lower is smoother',
    context: 'PC at the time',
    contextRows: { powerPlan: 'Power plan', ram: 'RAM', gpu: 'Graphics', windows: 'Windows build', display: 'Monitor', game: 'VALORANT' },
    gameSettings: {
      notFound: 'Settings not found',
      vsync: (on: boolean) => (on ? 'V-Sync on' : 'V-Sync off'),
      limit: (fps: number) => (fps > 0 ? `${fps} FPS limit` : 'No FPS limit'),
    },
    rename: 'Rename',
    save: 'Save',
    setBaseline: 'Use as baseline',
    delete: 'Delete',
    confirmDelete: 'Delete this benchmark? This can’t be undone.',
    yesDelete: 'Yes, delete',
    noKeep: 'Keep it',
    copy: 'Copy results',
    copied: 'Copied. Paste it in chat.',
    copyFailed: 'Couldn’t copy. Try again.',
    flags: {
      'too-short': 'Too short: fewer than 85 seconds or 1,000 frames were captured.',
      'not-in-front': 'The game wasn’t in front for part of the run (alt-tab, an overlay or windowed mode).',
      'fps-capped': 'An FPS cap or V-Sync is on, so the run measured the cap.',
      'dropped-data': 'PresentMon dropped data during the run.',
      busy: (name: string) => `${name} was busy in the background.`,
    },
  },
```

- [ ] **Step 3: Write the failing tests** — `app/src/api/bench.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { flagText, graphShape, metricText, passLabel, pct, sessionHeadline, settingsText, spreadText, verdictTone } from './bench';
import type { BenchState, ValorantSettings } from './types';

const base: BenchState = {
  phase: 'waiting', profileId: 'valorant', pass: 0, gameRunning: true, runs: [null, null, null], resultId: null,
  startMode: 'hotkey', startKey: 'F10', captureSeconds: 90, switchAtSeconds: 30, secondsLeft: null, countdown: null,
};

describe('bench helpers', () => {
  it('formats metrics with their units, and the recorded settings', () => {
    expect(metricText('avg-fps', 412.4)).toBe('412');
    expect(metricText('low01-fps', 201.6)).toBe('202');
    expect(metricText('pc-latency-ms', 9.84)).toBe('9.8 ms');
    expect(metricText('pc-latency-ms', null)).toBe('Not reported');
    expect(metricText('stutters-per-min', 0.44)).toBe('0.4/min');
    expect(spreadText(0.0317)).toBe('3.2%');
    const settings: ValorantSettings = {
      resolution: [1920, 1080], fullscreenMode: 0, vsync: false, frameRateLimit: 0, reflex: 2, multithreadedRendering: true,
      quality: { TextureQuality: '0' }, limits: {},
    };
    expect(settingsText(null)).toBe('Settings not found');
    expect(settingsText(settings)).toBe('1920×1080 · V-Sync off · No FPS limit');
    const unknown = { ...settings, resolution: null, fullscreenMode: null, vsync: null, frameRateLimit: null, reflex: null, multithreadedRendering: null };
    expect(settingsText(unknown)).toBe('—');
  });

  it('signs percentages and colours verdicts', () => {
    expect(pct(4.94)).toBe('+4.9%');
    expect(pct(-1.04)).toBe('-1.0%');
    expect(pct(0)).toBe('0.0%');
    expect(verdictTone('real-gain')).toBe('good');
    expect(verdictTone('real-loss')).toBe('bad');
    expect(verdictTone('noise')).toBe('neutral');
  });

  it('explains every flag', () => {
    expect(flagText({ kind: 'busy-background', name: 'chrome.exe' })).toBe('chrome.exe was busy in the background.');
    for (const kind of ['too-short', 'not-in-front', 'fps-capped', 'dropped-data'] as const) {
      expect(flagText({ kind }).length).toBeGreaterThan(10);
    }
  });

  it('names passes', () => {
    expect(passLabel(0)).toBe('Warm-up');
    expect(passLabel(2)).toBe('Run 2 of 3');
  });

  it('tells the player what to do next', () => {
    expect(sessionHeadline({ ...base, gameRunning: false })).toBe('Start VALORANT and open the Range. Kestrel waits for the game.');
    expect(sessionHeadline(base)).toBe('Stand at the start and press F10 to begin the warm-up.');
    expect(sessionHeadline({ ...base, pass: 2 })).toBe('Stand at the start and press F10 to begin run 2 of 3.');
    expect(sessionHeadline({ ...base, startMode: 'countdown' })).toContain('click Start run');
    expect(sessionHeadline({ ...base, countdown: 4 })).toBe('Starting in 4…');
    expect(sessionHeadline({ ...base, phase: 'capturing', pass: 1, secondsLeft: 75 })).toBe('Run 1 of 3: walk the main path to the bots.');
    expect(sessionHeadline({ ...base, phase: 'capturing', pass: 1, secondsLeft: 50 })).toBe('Run 1 of 3: shoot the strafing bots.');
    expect(sessionHeadline({ ...base, phase: 'review', pass: 1, flags: [{ kind: 'too-short' }] })).toBe('Run 1 of 3 can’t be trusted.');
    expect(sessionHeadline({ ...base, phase: 'done', pass: 3 })).toBe('Benchmark saved.');
    expect(sessionHeadline({ ...base, phase: 'failed', message: 'x' })).toBe('The benchmark stopped.');
    expect(sessionHeadline({ ...base, phase: 'cancelled' })).toBe('Benchmark cancelled.');
  });

  it('scales the frame-time graph to its slowest frame', () => {
    const g = graphShape([2, 4, 2, 8]);
    expect(g.points).toBe('0,75 200,50 400,75 600,0');
    expect(g.maxMs).toBe(8);
    expect(g.medianMs).toBe(2);
    expect(graphShape([]).points).toBe('');
    expect(graphShape([5]).points).toBe('0,0 600,0');
  });
});
```

Run: `cd app && npm test; cd ..`
Expected: FAIL — `./bench` can't be resolved.

- [ ] **Step 4: Helpers** — `app/src/api/bench.ts`:

```ts
import { strings } from '../strings';
import type { BenchState, Flag, MetricId, RunMetrics, ValorantSettings, Verdict } from './types';

/** The order metrics appear in tiles and comparisons. */
export const METRIC_ORDER: MetricId[] = ['avg-fps', 'low1-fps', 'low01-fps', 'stutters-per-min', 'pc-latency-ms'];

export function metricValue(m: RunMetrics, id: MetricId): number | null {
  switch (id) {
    case 'avg-fps': return m.avgFps;
    case 'low1-fps': return m.low1Fps;
    case 'low01-fps': return m.low01Fps;
    case 'stutters-per-min': return m.stuttersPerMin;
    case 'pc-latency-ms': return m.pcLatencyMs;
  }
}

export function metricText(id: MetricId, value: number | null): string {
  if (value === null) return strings.bench.notReported;
  if (id === 'pc-latency-ms') return strings.bench.milliseconds(value.toFixed(1));
  if (id === 'stutters-per-min') return strings.bench.perMinute(value.toFixed(1));
  return Math.round(value).toString();
}

/** +4.9% / -1.0% / 0.0% */
export const pct = (n: number): string => `${n > 0 ? '+' : ''}${n.toFixed(1)}%`;

/** A spread ratio (0.0317) as a percentage: "3.2%". */
export const spreadText = (ratio: number): string => `${(ratio * 100).toFixed(1)}%`;

export const verdictTone = (v: Verdict): 'good' | 'bad' | 'neutral' => (v === 'real-gain' ? 'good' : v === 'real-loss' ? 'bad' : 'neutral');
export const verdictText = (v: Verdict): string => strings.bench.verdict[v];

export function flagText(flag: Flag): string {
  return flag.kind === 'busy-background' ? strings.bench.flags.busy(flag.name) : strings.bench.flags[flag.kind];
}

export const passLabel = (pass: number): string => strings.bench.passLabel(pass);

/** One sentence telling the player what happens now or what to do next. */
export function sessionHeadline(s: BenchState): string {
  const b = strings.bench;
  switch (s.phase) {
    case 'waiting':
      if (s.countdown !== null) return b.countdown(s.countdown);
      if (!s.gameRunning) return b.waitingForGame;
      return s.startMode === 'hotkey' ? b.pressKey(s.startKey, b.begins(s.pass)) : b.pressButton;
    case 'capturing': {
      const elapsed = s.captureSeconds - (s.secondsLeft ?? s.captureSeconds);
      return elapsed < s.switchAtSeconds ? b.walk(passLabel(s.pass)) : b.shoot(passLabel(s.pass));
    }
    case 'review':
      return b.review(passLabel(s.pass));
    case 'done':
      return b.done;
    case 'failed':
      return b.failed;
    case 'cancelled':
      return b.cancelled;
  }
}

/** SVG points for frame times in a 600 × 100 box: x across, y scaled so the slowest frame touches the top. */
export function graphShape(graph: number[]): { points: string; maxMs: number; medianMs: number } {
  if (graph.length === 0) return { points: '', maxMs: 0, medianMs: 0 };
  const values = graph.length === 1 ? [graph[0], graph[0]] : graph;
  const maxMs = Math.max(...values);
  const sorted = [...values].sort((a, b) => a - b);
  const medianMs = sorted[Math.max(0, Math.ceil(sorted.length / 2) - 1)];
  const round = (n: number) => Math.round(n * 100) / 100;
  const points = values
    .map((ms, i) => `${round((i * 600) / (values.length - 1))},${round(maxMs > 0 ? 100 - (ms / maxMs) * 100 : 100)}`)
    .join(' ');
  return { points, maxMs, medianMs };
}

/** "2026-10-12T21:30:05Z" → local "2026-10-12 21:30". */
export function dateText(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const two = (n: number) => n.toString().padStart(2, '0');
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

/** VALORANT's recorded settings in one line: "1920×1080 · V-Sync off · No FPS limit". */
export function settingsText(s: ValorantSettings | null): string {
  const g = strings.bench.gameSettings;
  if (!s) return g.notFound;
  const parts: string[] = [];
  if (s.resolution) parts.push(`${s.resolution[0]}×${s.resolution[1]}`);
  if (s.vsync !== null) parts.push(g.vsync(s.vsync));
  if (s.frameRateLimit !== null) parts.push(g.limit(s.frameRateLimit));
  return parts.length > 0 ? parts.join(' · ') : '—';
}
```

Run: `cd app && npm test; cd ..`
Expected: the 6 new tests pass (16 in total).

- [ ] **Step 5: Browser sample and fake session** — `app/src/api/sample-bench.ts`:

```ts
// Sample benchmarks and a fake session for the browser preview (UI work on the Mac). Never used inside Kestrel.
import type { BenchDetail, BenchState, Benchmark, BenchmarkSummary, GameProfile, RunRecord } from './types';

export const sampleProfile: GameProfile = {
  id: 'valorant', name: 'VALORANT', processName: 'VALORANT-Win64-Shipping.exe',
  route: { version: 1, captureSeconds: 90, switchAtSeconds: 30 },
};

const run = (avg: number, low1: number, low01: number, latency: number): RunRecord => ({
  metrics: { frames: Math.round(avg * 90), durationS: 90, avgFps: avg, low1Fps: low1, low01Fps: low01, stuttersPerMin: 0.6, pcLatencyMs: latency, gpuBusyRatio: 0.93, bottleneck: 'gpu' },
  flags: [],
  kept: false,
});

const context = {
  powerPlan: 'EXM Free Power Plan V8', ramMts: 3600, gpu: 'NVIDIA GeForce RTX 3060 Ti', gpuDriver: '610.74',
  windowsBuild: 26200, displayHz: 360, gameSettings: null,
};

const benchmarks: Benchmark[] = [
  {
    id: 'sample-1', name: 'Starting point', createdAt: '2026-10-12T21:30:05Z', profileId: 'valorant', routeVersion: 1,
    kestrelVersion: '0.2.0', presentmonVersion: '2.6.0',
    runs: [run(405, 284, 197, 10.1), run(410, 288, 201, 9.8), run(418, 291, 206, 9.6)],
    result: { metrics: run(410, 288, 201, 9.8).metrics, spread: { avgFps: 0.0317, low1Fps: 0.0243, low01Fps: 0.0448 } },
    context,
  },
  {
    id: 'sample-2', name: 'After Essential tweaks (sample)', createdAt: '2026-10-14T19:05:00Z', profileId: 'valorant', routeVersion: 1,
    kestrelVersion: '0.2.0', presentmonVersion: '2.6.0',
    runs: [run(428, 301, 208, 9.2), run(433, 305, 214, 9.1), run(440, 309, 211, 8.9)],
    result: { metrics: run(433, 305, 214, 9.1).metrics, spread: { avgFps: 0.0277, low1Fps: 0.0262, low01Fps: 0.028 } },
    context,
  },
];
let baselineId = 'sample-1';

const graph = Array.from({ length: 600 }, (_, i) => (i % 97 === 0 ? 6.5 : 2.3 + ((i * 37) % 11) / 30));

export function sampleList(): BenchmarkSummary[] {
  return [...benchmarks].reverse().map((b) => ({
    id: b.id, name: b.name, createdAt: b.createdAt, profileId: b.profileId, routeVersion: b.routeVersion, result: b.result,
    isBaseline: b.id === baselineId, flaggedRunsKept: b.runs.filter((r) => r.kept).length,
  }));
}

export function sampleDetail(id: string): BenchDetail {
  const benchmark = benchmarks.find((b) => b.id === id);
  if (!benchmark) throw new Error('That benchmark doesn’t exist.');
  const baseline = benchmarks.find((b) => b.id === baselineId && b.id !== id) ?? null;
  const delta = (a: number, b: number) => ((b - a) / a) * 100;
  // Fixed verdicts for the sample, except that a ~0% change (the fake session saves a copy of the starting point) is noise.
  const comparison = baseline && {
    baselineId: baseline.id, currentId: id, settingsDiffer: [], routeMismatch: false,
    metrics: [
      { metric: 'avg-fps' as const, baseline: baseline.result.metrics.avgFps, current: benchmark.result.metrics.avgFps, deltaPct: delta(baseline.result.metrics.avgFps, benchmark.result.metrics.avgFps), verdict: 'real-gain' as const },
      { metric: 'low1-fps' as const, baseline: baseline.result.metrics.low1Fps, current: benchmark.result.metrics.low1Fps, deltaPct: delta(baseline.result.metrics.low1Fps, benchmark.result.metrics.low1Fps), verdict: 'real-gain' as const },
      { metric: 'low01-fps' as const, baseline: baseline.result.metrics.low01Fps, current: benchmark.result.metrics.low01Fps, deltaPct: delta(baseline.result.metrics.low01Fps, benchmark.result.metrics.low01Fps), verdict: 'noise' as const },
    ].map((d) => (Math.abs(d.deltaPct) < 0.05 ? { ...d, verdict: 'noise' as const } : d)),
  };
  return { benchmark, graph, comparison, baselineName: baseline?.name ?? null };
}

export function sampleRename(id: string, name: string): void {
  const b = benchmarks.find((x) => x.id === id);
  if (b && name.trim()) b.name = name.trim().slice(0, 60);
}

export function sampleDelete(id: string): void {
  const at = benchmarks.findIndex((b) => b.id === id);
  if (at >= 0) benchmarks.splice(at, 1);
  if (baselineId === id) baselineId = benchmarks[0]?.id ?? '';
}

export function sampleSetBaseline(id: string): void {
  baselineId = id;
}

// A fake session: each capture takes 3 seconds instead of 90.
let state: BenchState | null = null;
const listeners = new Set<(s: BenchState | null) => void>();
const emit = () => listeners.forEach((l) => l(state));
let timer: number | undefined;

export const fakeSession = {
  state: (): BenchState | null => state,
  subscribe(handler: (s: BenchState | null) => void): () => void {
    listeners.add(handler);
    return () => listeners.delete(handler);
  },
  start(): BenchState {
    state = {
      phase: 'waiting', profileId: 'valorant', pass: 0, gameRunning: true, runs: [null, null, null], resultId: null,
      startMode: 'countdown', startKey: 'F10', captureSeconds: 90, switchAtSeconds: 30, secondsLeft: null, countdown: null,
    };
    emit();
    return state;
  },
  startRun(): void {
    if (!state || state.phase !== 'waiting') return;
    const pass = state.pass;
    state = { ...state, phase: 'capturing', secondsLeft: 90 };
    emit();
    timer = window.setInterval(() => {
      if (!state || state.phase !== 'capturing') return window.clearInterval(timer);
      const left = Math.max(0, (state.secondsLeft ?? 90) - 3);
      if (left > 0) {
        state = { ...state, secondsLeft: left };
      } else {
        window.clearInterval(timer);
        const runs = [...state.runs];
        if (pass > 0) runs[pass - 1] = run(410 + pass, 288, 201, 9.8);
        if (pass < 3) {
          state = { ...state, phase: 'waiting', pass: pass + 1, runs, secondsLeft: null };
        } else {
          const id = `sample-${benchmarks.length + 1}`;
          benchmarks.push({ ...benchmarks[0], id, name: `Benchmark ${new Date().toISOString().slice(0, 16).replace('T', ' ')}`, createdAt: new Date().toISOString() });
          state = { ...state, phase: 'done', runs, secondsLeft: null, resultId: id };
        }
      }
      emit();
    }, 100);
  },
  cancel(): void {
    window.clearInterval(timer);
    if (state && state.phase !== 'done') state = { ...state, phase: 'cancelled', secondsLeft: null };
    emit();
  },
  dismiss(): void {
    state = null;
    emit();
  },
};
```

- [ ] **Step 6: Client** — in `app/src/api/client.ts`, change the imports and `call` to:

```ts
import mock from './mock-report.json';
import { fakeSession, sampleDelete, sampleDetail, sampleList, sampleProfile, sampleRename, sampleSetBaseline } from './sample-bench';
import type { BenchDetail, BenchState, BenchmarkSummary, GameProfile, LoadSample, Report } from './types';

const sample = mock as unknown as Report;

/** True inside the Tauri window; false in a plain browser (UI development on the Mac). */
export const inTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}
```

and append:

```ts
// Stage 2: the benchmark. In the browser preview these use sample data and a fake session.
export const benchProfiles = (): Promise<GameProfile[]> => (inTauri() ? call('bench_profiles') : Promise.resolve([sampleProfile]));
export const benchState = (): Promise<BenchState | null> => (inTauri() ? call('bench_state') : Promise.resolve(fakeSession.state()));
export const benchStart = (profile: string): Promise<BenchState> => (inTauri() ? call('bench_start', { profile }) : later(fakeSession.start(), 300));
export const benchStartRun = (): Promise<void> => (inTauri() ? call('bench_start_run') : Promise.resolve(fakeSession.startRun()));
export const benchCancel = (): Promise<void> => (inTauri() ? call('bench_cancel') : Promise.resolve(fakeSession.cancel()));
export const benchRedo = (): Promise<void> => (inTauri() ? call('bench_redo') : Promise.resolve());
export const benchKeep = (): Promise<void> => (inTauri() ? call('bench_keep') : Promise.resolve());
export const benchDismiss = (): Promise<void> => (inTauri() ? call('bench_dismiss') : Promise.resolve(fakeSession.dismiss()));
export const benchList = (): Promise<BenchmarkSummary[]> => (inTauri() ? call('bench_list') : Promise.resolve(sampleList()));
export const benchGet = (id: string): Promise<BenchDetail> => (inTauri() ? call('bench_get', { id }) : Promise.resolve().then(() => sampleDetail(id)));
export const benchRename = (id: string, name: string): Promise<void> => (inTauri() ? call('bench_rename', { id, name }) : Promise.resolve(sampleRename(id, name)));
export const benchDelete = (id: string): Promise<void> => (inTauri() ? call('bench_delete', { id }) : Promise.resolve(sampleDelete(id)));
export const benchSetBaseline = (id: string): Promise<void> => (inTauri() ? call('bench_set_baseline', { id }) : Promise.resolve(sampleSetBaseline(id)));
export const benchCopyResults = (id: string): Promise<string> =>
  inTauri() ? call('bench_copy_results', { id }) : Promise.resolve(`Kestrel benchmark (sample): ${sampleDetail(id).benchmark.name}`);

/** Calls `handler` whenever the benchmark session changes; resolves to an unsubscribe function. */
export async function onBenchState(handler: (state: BenchState | null) => void): Promise<() => void> {
  if (!inTauri()) return fakeSession.subscribe(handler);
  const { listen } = await import('@tauri-apps/api/event');
  return listen<BenchState | null>('bench-state', (event) => handler(event.payload));
}
```

- [ ] **Step 7: Run the checks**

Run: `cd app && npm test && npm run build && cd ..`
Expected: 16 tests pass; tsc and vite build clean.

- [ ] **Step 8: Commit**

```bash
git add app/src
git commit -m "feat(app): benchmark types, client, browser sample and helpers"
```

---

### Task 15: UI — the Benchmark page

**Files:**
- Create: `app/src/pages/Benchmark.tsx`, `app/src/components/bench/RouteCard.tsx`, `app/src/components/bench/SessionPanel.tsx`, `app/src/components/bench/ResultsList.tsx`, `app/src/components/bench/ResultDetail.tsx`, `app/src/components/bench/FrameGraph.tsx`, `app/src/styles/bench.css`
- Modify: `app/src/App.tsx` (render the page), `app/src/main.tsx` (import `bench.css`), `app/src/strings.ts` (drop `comingSoon.benchmark`)

**Interfaces:**
- Consumes: Task 14's client, helpers, strings and types; `CopyDiagnostics` (stage 1).
- Produces: `Benchmark()` page; components `RouteCard({ profile, starting, onStart })`, `SessionPanel({ state, onClose })`, `ResultsList({ items, selected, onSelect })`, `ResultDetail({ id, onChanged })`, `FrameGraph({ graph })`.

- [ ] **Step 1: Components**

`app/src/components/bench/RouteCard.tsx`:
```tsx
import type { GameProfile } from '../../api/types';
import { strings } from '../../strings';

/** Route v1 and the button that arms a session (spec "What the owner sees", 1). */
export function RouteCard({ profile, starting, onStart }: { profile: GameProfile; starting: boolean; onStart: () => void }) {
  const b = strings.bench;
  return (
    <section className="panel route">
      <div className="label">{b.route(profile.route.version)}</div>
      <p>{b.setup}</p>
      <ol className="steps">
        {b.steps.map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
      <p className="muted small">{b.passes}</p>
      <p className="muted small">{b.readOnly}</p>
      <div>
        <button className="btn btn-primary cut-s" disabled={starting} onClick={onStart}>
          {starting ? b.starting : b.start}
        </button>
      </div>
    </section>
  );
}
```

`app/src/components/bench/SessionPanel.tsx`:
```tsx
import { benchCancel, benchKeep, benchRedo, benchStartRun } from '../../api/client';
import { flagText, sessionHeadline } from '../../api/bench';
import type { BenchState } from '../../api/types';
import { strings } from '../../strings';
import { CopyDiagnostics } from '../CopyDiagnostics';

/** The running session: what to do now, the countdown, each pass's status, and the session's buttons. */
export function SessionPanel({ state, onClose }: { state: BenchState; onClose: (resultId: string | null) => void }) {
  const b = strings.bench;
  const over = state.phase === 'done' || state.phase === 'failed' || state.phase === 'cancelled';
  const big = state.countdown ?? (state.phase === 'capturing' ? state.secondsLeft : null);
  return (
    <section className="panel session">
      <div className="session-head">
        <h2 aria-live="polite">{sessionHeadline(state)}</h2>
        {big !== null && <span className="num session-clock">{b.secondsLeft(big)}</span>}
      </div>
      <ol className="pass-chips">
        {[0, 1, 2, 3].map((pass) => {
          const record = pass > 0 ? state.runs[pass - 1] : null;
          const finished = pass < state.pass || state.phase === 'done';
          return (
            <li key={pass} className={`cut-s${pass === state.pass && !over ? ' current' : ''}${finished ? ' finished' : ''}`}>
              <span>{b.passChip(pass)}</span>
              {finished && <span className="muted small"> · {record?.kept ? b.runKept : b.runOk}</span>}
            </li>
          );
        })}
      </ol>
      {state.phase === 'review' && (
        <ul className="flags">
          {state.flags.map((flag) => (
            <li key={flag.kind}>{flagText(flag)}</li>
          ))}
        </ul>
      )}
      {state.phase === 'failed' && <p className="caution">{state.message}</p>}
      {state.phase === 'failed' && <CopyDiagnostics />}
      <div className="actions">
        {state.phase === 'waiting' && state.startMode === 'countdown' && state.countdown === null && state.gameRunning && (
          <button className="btn btn-primary cut-s" onClick={() => void benchStartRun()}>{b.startRun}</button>
        )}
        {state.phase === 'review' && (
          <>
            <button className="btn btn-primary cut-s" onClick={() => void benchRedo()}>{b.redo(state.pass)}</button>
            <button className="btn btn-ghost cut-s" onClick={() => void benchKeep()}>{b.keep}</button>
          </>
        )}
        {!over && <button className="btn btn-ghost cut-s" onClick={() => void benchCancel()}>{b.cancel}</button>}
        {state.phase === 'done' && <button className="btn btn-primary cut-s" onClick={() => onClose(state.resultId)}>{b.seeResult}</button>}
        {(state.phase === 'failed' || state.phase === 'cancelled') && <button className="btn btn-ghost cut-s" onClick={() => onClose(null)}>{b.close}</button>}
      </div>
    </section>
  );
}
```

`app/src/components/bench/ResultsList.tsx`:
```tsx
import { dateText, metricText } from '../../api/bench';
import type { BenchmarkSummary } from '../../api/types';
import { strings } from '../../strings';

export function ResultsList({ items, selected, onSelect }: { items: BenchmarkSummary[]; selected: string | null; onSelect: (id: string) => void }) {
  const b = strings.bench;
  return (
    <section className="panel results">
      <div className="label">{b.results}</div>
      {items.length === 0 && <p className="muted small">{b.none}</p>}
      <ul className="results-list">
        {items.map((item) => (
          <li key={item.id}>
            <button className={`result-row${item.id === selected ? ' selected' : ''}`} aria-current={item.id === selected ? 'true' : undefined} onClick={() => onSelect(item.id)}>
              <span className="result-name">
                {item.name}
                {item.isBaseline && <span className="tag tag-info cut-s">{b.baseline}</span>}
                {item.flaggedRunsKept > 0 && <span className="tag tag-high cut-s">{b.kept(item.flaggedRunsKept)}</span>}
              </span>
              <span className="muted small num">{dateText(item.createdAt)}</span>
              <span className="num">{metricText('avg-fps', item.result.metrics.avgFps)} <span className="muted small">FPS</span></span>
              <span className="num">{metricText('low1-fps', item.result.metrics.low1Fps)} <span className="muted small">{b.metrics['low1-fps']}</span></span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
```

`app/src/components/bench/FrameGraph.tsx`:
```tsx
import { graphShape } from '../../api/bench';
import { strings } from '../../strings';

/** The middle run's frame times: spikes are hitches. */
export function FrameGraph({ graph }: { graph: number[] }) {
  const { points, maxMs, medianMs } = graphShape(graph);
  if (!points) return null;
  const medianY = maxMs > 0 ? 100 - (medianMs / maxMs) * 100 : 100;
  return (
    <figure className="frame-graph">
      <figcaption className="label">{strings.bench.graph}</figcaption>
      <div className="frame-graph-box">
        <span className="num muted small axis-top">{maxMs.toFixed(1)} ms</span>
        <svg viewBox="0 0 600 100" preserveAspectRatio="none" role="img" aria-label={strings.bench.graph}>
          <line x1={0} x2={600} y1={medianY} y2={medianY} stroke="var(--line-strong)" strokeDasharray="4 4" vectorEffect="non-scaling-stroke" />
          <polyline points={points} fill="none" stroke="var(--accent)" strokeWidth={1.25} vectorEffect="non-scaling-stroke" />
        </svg>
        <span className="num muted small axis-bottom">0 ms</span>
      </div>
      <p className="muted small">{strings.bench.graphAxis}</p>
    </figure>
  );
}
```

`app/src/components/bench/ResultDetail.tsx`:
```tsx
import { useEffect, useState } from 'react';
import { benchCopyResults, benchDelete, benchGet, benchRename, benchSetBaseline } from '../../api/client';
import { dateText, flagText, METRIC_ORDER, metricText, metricValue, pct, settingsText, spreadText, verdictText, verdictTone } from '../../api/bench';
import type { BenchDetail } from '../../api/types';
import { strings } from '../../strings';
import { FrameGraph } from './FrameGraph';

/** One saved benchmark: its numbers, the comparison with the baseline, the graph, the context and the actions. */
export function ResultDetail({ id, onChanged }: { id: string; onChanged: () => void }) {
  const b = strings.bench;
  const [detail, setDetail] = useState<BenchDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [naming, setNaming] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const load = () => benchGet(id).then(setDetail, (e) => setError(String(e)));
  useEffect(() => {
    void benchGet(id).then(setDetail, (e) => setError(String(e)));
  }, [id]);
  useEffect(() => {
    if (!note) return;
    const timer = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(timer);
  }, [note]);

  if (error) return <p className="notice cut-s">{error}</p>;
  if (!detail) return null;
  const { benchmark: bench, comparison } = detail;
  const m = bench.result.metrics;
  const act = (work: Promise<void>) => void work.then(() => { onChanged(); return load(); }, (e) => setError(String(e)));

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await benchCopyResults(id));
      setNote(b.copied);
    } catch {
      setNote(b.copyFailed);
    }
  };

  const ctx = bench.context;
  const contextRows: [string, string][] = [
    [b.contextRows.powerPlan, ctx.powerPlan ?? '—'],
    [b.contextRows.ram, ctx.ramMts ? `${ctx.ramMts} MT/s` : '—'],
    [b.contextRows.gpu, ctx.gpu ? `${ctx.gpu}${ctx.gpuDriver ? ` · ${ctx.gpuDriver}` : ''}` : '—'],
    [b.contextRows.windows, ctx.windowsBuild ? String(ctx.windowsBuild) : '—'],
    [b.contextRows.display, ctx.displayHz ? `${ctx.displayHz} Hz` : '—'],
    [b.contextRows.game, settingsText(ctx.gameSettings)],
  ];

  return (
    <section className="panel detail">
      <header className="detail-head">
        {naming === null ? (
          <h2>{bench.name}</h2>
        ) : (
          <form className="rename" onSubmit={(e) => { e.preventDefault(); act(benchRename(id, naming)); setNaming(null); }}>
            <input id="bench-name" className="cut-s" value={naming} maxLength={60} onChange={(e) => setNaming(e.target.value)} aria-label={b.rename} autoFocus />
            <button className="btn btn-primary cut-s" type="submit" disabled={!naming.trim()}>{b.save}</button>
          </form>
        )}
        <span className="muted small num">{dateText(bench.createdAt)} · {b.route(bench.routeVersion)}</span>
      </header>

      <div className="tiles">
        {METRIC_ORDER.map((id) => (
          <div key={id} className="tile cut-s">
            <div className="label">{b.metrics[id]}</div>
            <div className="num tile-value">{metricText(id, metricValue(m, id))}</div>
            {id === 'avg-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.avgFps))}</div>}
            {id === 'low1-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.low1Fps))}</div>}
            {id === 'low01-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.low01Fps))}</div>}
          </div>
        ))}
        <div className="tile cut-s">
          <div className="label">{b.bottleneckLabel}</div>
          <div className="tile-value tile-word">{b.bottleneck[m.bottleneck]}</div>
        </div>
      </div>

      {comparison && detail.baselineName && (
        <div className="comparison">
          <div className="label">{b.versus(detail.baselineName)}</div>
          {comparison.routeMismatch ? (
            <p className="muted small">{b.routeMismatch}</p>
          ) : (
            <ul>
              {comparison.metrics.map((d) => (
                <li key={d.metric}>
                  <span>{b.metrics[d.metric]}</span>
                  <span className="num">{metricText(d.metric, d.baseline)} → {metricText(d.metric, d.current)}</span>
                  <span className={`num verdict-${verdictTone(d.verdict)}`}>{pct(d.deltaPct)} · {verdictText(d.verdict)}</span>
                </li>
              ))}
            </ul>
          )}
          {comparison.settingsDiffer.length > 0 && <p className="caution">{b.settingsDiffer(comparison.settingsDiffer.join(', '))}</p>}
        </div>
      )}

      {bench.runs.some((r) => r.kept) && (
        <ul className="flags">
          {bench.runs.map((r, i) => r.kept && r.flags.map((f) => <li key={`${i}-${f.kind}`}>{b.passLabel(i + 1)}: {flagText(f)}</li>))}
        </ul>
      )}

      <FrameGraph graph={detail.graph} />

      <div>
        <div className="label">{b.context}</div>
        <dl className="hw num">
          {contextRows.map(([name, value]) => (
            <div key={name} className="hw-row">
              <dt>{name}</dt>
              <dd title={value}>{value}</dd>
            </div>
          ))}
        </dl>
      </div>

      <div className="actions">
        <button className="btn btn-primary cut-s" onClick={() => void copy()}>{b.copy}</button>
        <button className="btn btn-ghost cut-s" onClick={() => setNaming(bench.name)}>{b.rename}</button>
        <button className="btn btn-ghost cut-s" onClick={() => act(benchSetBaseline(id))}>{b.setBaseline}</button>
        {!confirming && <button className="btn btn-ghost cut-s" onClick={() => setConfirming(true)}>{b.delete}</button>}
        {note && <span className="toast" role="status">{note}</span>}
      </div>
      {confirming && (
        <div className="confirm">
          <p>{b.confirmDelete}</p>
          <button className="btn btn-primary cut-s" onClick={() => { setConfirming(false); void benchDelete(id).then(onChanged, (e) => setError(String(e))); }}>{b.yesDelete}</button>
          <button className="btn btn-ghost cut-s" onClick={() => setConfirming(false)}>{b.noKeep}</button>
        </div>
      )}
    </section>
  );
}
```

`app/src/pages/Benchmark.tsx`:
```tsx
import { useCallback, useEffect, useState } from 'react';
import { benchDismiss, benchList, benchProfiles, benchStart, benchState, onBenchState } from '../api/client';
import type { BenchmarkSummary, BenchState, GameProfile } from '../api/types';
import { ResultDetail } from '../components/bench/ResultDetail';
import { ResultsList } from '../components/bench/ResultsList';
import { RouteCard } from '../components/bench/RouteCard';
import { SessionPanel } from '../components/bench/SessionPanel';
import { strings } from '../strings';

/** Stage 2: start a benchmark, follow the session, and look through saved results (newest first). */
export function Benchmark() {
  const [profile, setProfile] = useState<GameProfile | null>(null);
  const [items, setItems] = useState<BenchmarkSummary[]>([]);
  const [session, setSession] = useState<BenchState | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async (select?: string | null) => {
    const list = await benchList();
    setItems(list);
    setSelected((current) => select ?? (current && list.some((i) => i.id === current) ? current : list[0]?.id ?? null));
  }, []);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void benchProfiles().then((profiles) => alive && setProfile(profiles[0] ?? null));
    void benchState().then((state) => alive && setSession(state));
    void refresh();
    void onBenchState((state) => {
      setSession(state);
      if (state?.phase === 'done' && state.resultId) void refresh(state.resultId);
    }).then((stop) => (alive ? (unlisten = stop) : stop()));
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [refresh]);

  const start = async () => {
    if (!profile) return;
    setStarting(true);
    setError(null);
    try {
      setSession(await benchStart(profile.id));
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(false);
    }
  };

  const close = (resultId: string | null) => {
    void benchDismiss().then(() => {
      setSession(null);
      void refresh(resultId);
    });
  };

  return (
    <div className="page bench">
      <header className="page-head">
        <h1>{strings.bench.title}</h1>
        <p className="muted">{strings.bench.intro}</p>
      </header>
      {error && <p className="notice cut-s">{error}</p>}
      {session ? <SessionPanel state={session} onClose={close} /> : profile && <RouteCard profile={profile} starting={starting} onStart={() => void start()} />}
      <ResultsList items={items} selected={selected} onSelect={setSelected} />
      {selected && <ResultDetail key={selected} id={selected} onChanged={() => void refresh()} />}
    </div>
  );
}
```

- [ ] **Step 2: Styles** — `app/src/styles/bench.css` (import it in `main.tsx` after `pages.css`):

```css
.bench h2 { font-size: 17px; font-weight: 600; }
.route { display: grid; gap: 10px; }
.session { display: grid; gap: 12px; }
.session-head { display: flex; align-items: baseline; gap: 16px; }
.session-head h2 { flex: 1; min-width: 0; }
.session-clock { font-size: 36px; color: var(--accent); }
.pass-chips { list-style: none; display: flex; gap: 6px; padding: 0; margin: 0; flex-wrap: wrap; }
.pass-chips li { padding: 6px 12px; background: var(--panel-2); font-size: 12.5px; color: var(--muted); }
.pass-chips li.current { background: var(--accent); color: var(--accent-ink); font-weight: 600; }
.pass-chips li.finished { color: var(--text); }
.flags { margin: 0; padding-left: 18px; color: var(--warn); font-size: 13px; display: grid; gap: 4px; }
.results-list { list-style: none; padding: 0; margin: 6px 0 0; display: grid; }
.result-row { width: 100%; display: grid; grid-template-columns: minmax(0, 1fr) auto auto auto; gap: 14px; align-items: baseline; padding: 9px 4px; border-top: 1px solid var(--line); text-align: left; }
.result-row:hover { background: var(--panel-2); }
.result-row.selected { box-shadow: inset 3px 0 0 var(--accent); }
.result-name { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; min-width: 0; }
.detail { display: grid; gap: 16px; }
.detail-head { display: flex; align-items: baseline; gap: 12px; flex-wrap: wrap; }
.rename { display: flex; gap: 8px; }
.rename input { background: var(--panel-2); border: 1px solid var(--line-strong); color: var(--text); font: inherit; padding: 6px 10px; }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: 8px; }
.tile { background: var(--panel-2); padding: 10px 12px; display: grid; gap: 2px; }
.tile-value { font-size: 22px; }
.tile-word { font-size: 15px; }
.comparison ul { list-style: none; padding: 0; margin: 6px 0 0; display: grid; }
.comparison li { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 14px; padding: 7px 0; border-top: 1px solid var(--line); font-size: 13px; }
.verdict-good { color: var(--accent); }
.verdict-bad { color: var(--warn); }
.verdict-neutral { color: var(--muted); }
.frame-graph { margin: 0; display: grid; gap: 6px; }
.frame-graph-box { position: relative; padding: 14px 0; }
.frame-graph-box svg { width: 100%; height: 110px; display: block; background: var(--panel-2); }
.axis-top { position: absolute; top: 0; left: 0; }
.axis-bottom { position: absolute; bottom: 0; left: 0; }
.hw-row { display: contents; }
.detail .hw { grid-template-columns: 110px minmax(0, 1fr); }
.confirm { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
.confirm p { margin: 0; }
```

- [ ] **Step 3: Show the page** — in `app/src/App.tsx`, import `{ Benchmark } from './pages/Benchmark'` and replace the benchmark line with:
```tsx
          {report && tab === 'benchmark' && <Benchmark />}
```
In `app/src/strings.ts`, delete the `benchmark: { stage: 2, … }` entry inside `comingSoon` (nothing uses it now). In `app/src/main.tsx`, add `import './styles/bench.css';` after `import './styles/pages.css';`.

- [ ] **Step 4: Verify**

Run: `cd app && npm test && npm run build && cd ..`
Expected: tests pass; tsc and vite build clean.
Start `npm run dev` in `app/` in the background, `curl -s http://localhost:1420/ | grep -c 'id="root"'` → `1`, then stop it (nothing left on port 1420). The controller checks the page visually in the browser preview (sample data): Route v1 card, results list with two sample benchmarks, the detail with tiles, comparison, graph and context, and a fake session (Start benchmark → Start run → passes advance every few seconds → See the result).

- [ ] **Step 5: Commit**

```bash
git add app/src
git commit -m "feat(app): Benchmark page with route, live session, results and comparisons"
```

---

### Task 16: UI — Home card, first-run step, About

**Files:**
- Create: `app/src/components/LastBenchmark.tsx`
- Modify: `app/src/pages/Home.tsx`, `app/src/first-run/FirstRun.tsx`, `app/src/App.tsx`, `app/src/pages/Toolbox.tsx`, `app/src/strings.ts`, `app/src/styles/home.css`

**Interfaces:**
- Consumes: `benchList`, `benchGet`, helpers (Task 14); `Tab` (stage 1).
- Produces: `LastBenchmark({ onTab })`; `FirstRun({ onDone })` where `onDone(report: Report, next: 'home' | 'benchmark')`; Toolbox `About Kestrel` section taking `appVersion`.

- [ ] **Step 1: Strings** — in `app/src/strings.ts`:
  - in `home`, replace `noBenchmark: 'Benchmarks arrive in stage 2'` with:
    ```ts
    noBenchmark: 'No benchmark yet. Measure your starting point in the Range: about 6 minutes.',
    runStartingPoint: 'Run starting point',
    seeBenchmarks: 'See benchmarks',
    ```
  - in `firstRun`, replace `later: 'Benchmark and tweaks arrive in later stages.'` and `toHome: 'Go to home'` with:
    ```ts
    later: 'Next, measure your starting point: a warm-up and 3 runs in the Range, about 6 minutes. Every later change is compared with it.',
    benchmarkNow: 'Run starting-point benchmark',
    notNow: 'Later',
    ```
  - after `about`, add:
    ```ts
    aboutKestrel: {
      title: 'About Kestrel',
      version: (v: string) => `Kestrel ${v}. Free and open source (GPL-3.0).`,
      presentmon: 'Benchmarks use Intel PresentMon 2.6.0 (MIT licence). It ships inside Kestrel with its licence file.',
      licence: 'PresentMon licence',
      licenceUrl: 'https://github.com/GameTechDev/PresentMon/blob/v2.6.0/LICENSE.txt',
    },
    ```

- [ ] **Step 2: Home card** — `app/src/components/LastBenchmark.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { benchGet, benchList } from '../api/client';
import { metricText, metricValue, pct, verdictTone } from '../api/bench';
import type { BenchDetail } from '../api/types';
import { strings } from '../strings';
import type { Tab } from './TitleBar';

/** The latest benchmark against the baseline, or a nudge to measure the starting point. */
export function LastBenchmark({ onTab }: { onTab: (tab: Tab) => void }) {
  const [detail, setDetail] = useState<BenchDetail | null | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    void benchList()
      .then((list) => (list[0] ? benchGet(list[0].id) : null))
      .then((d) => alive && setDetail(d), () => alive && setDetail(null));
    return () => {
      alive = false;
    };
  }, []);

  const h = strings.home;
  if (detail === undefined) return <section className="panel home-bench" />;
  if (detail === null) {
    return (
      <section className="panel home-bench">
        <div className="label">{h.lastBenchmark}</div>
        <p className="muted small">{h.noBenchmark}</p>
        <div>
          <button className="btn btn-primary cut-s" onClick={() => onTab('benchmark')}>{h.runStartingPoint}</button>
        </div>
      </section>
    );
  }
  const m = detail.benchmark.result.metrics;
  const delta = (id: 'avg-fps' | 'low1-fps') => detail.comparison?.metrics.find((d) => d.metric === id);
  return (
    <section className="panel home-bench">
      <div className="label">{h.lastBenchmark}</div>
      <div className="last-bench">
        <span>{detail.benchmark.name}</span>
        {(['avg-fps', 'low1-fps'] as const).map((id) => {
          const d = delta(id);
          return (
            <span key={id} className="num">
              {metricText(id, metricValue(m, id))} <span className="muted small">{strings.bench.metrics[id]}</span>
              {d && <span className={`small verdict-${verdictTone(d.verdict)}`}> {pct(d.deltaPct)}</span>}
            </span>
          );
        })}
        <button className="link" onClick={() => onTab('benchmark')}>{h.seeBenchmarks}</button>
      </div>
    </section>
  );
}
```

In `app/src/pages/Home.tsx`, import `{ LastBenchmark } from '../components/LastBenchmark'` and replace the whole `<section className="panel home-bench">…</section>` with `<LastBenchmark onTab={onTab} />`.

Append to `app/src/styles/home.css`:
```css
.last-bench { display: flex; gap: 18px; align-items: baseline; flex-wrap: wrap; margin-top: 6px; }
```

- [ ] **Step 3: First-run step** — in `app/src/first-run/FirstRun.tsx`:
  - the prop type becomes `{ onDone: (report: Report, next: 'home' | 'benchmark') => void }`;
  - in the `result` phase, replace the `<div>` holding the "Go to home" button with:
    ```tsx
            <div className="actions">
              <button className="btn btn-primary cut-s" onClick={() => onDone(phase.report, 'benchmark')}>{strings.firstRun.benchmarkNow}</button>
              <button className="btn btn-ghost cut-s" onClick={() => onDone(phase.report, 'home')}>{strings.firstRun.notNow}</button>
            </div>
    ```
  In `app/src/App.tsx`, `finishFirstRun` becomes:
  ```tsx
  const finishFirstRun = (report: Report, next: 'home' | 'benchmark') => {
    markFirstRunDone();
    setScan({ state: 'ready', report });
    setFirstRun(false);
    setTab(next);
  };
  ```

- [ ] **Step 4: About section** — in `app/src/pages/Toolbox.tsx`, the props become `{ guide, focus, appVersion }: { guide: BiosGuide; focus: BiosTopic | null; appVersion: string }`; replace the closing `<p className="muted small">{strings.toolbox.later}</p>` with:
```tsx
      <p className="muted small">{strings.toolbox.later}</p>
      <section className="panel guide">
        <h2>{strings.aboutKestrel.title}</h2>
        <p className="muted">{strings.aboutKestrel.version(appVersion)}</p>
        <p className="muted">{strings.aboutKestrel.presentmon}</p>
        <button className="link" onClick={() => void openLink(strings.aboutKestrel.licenceUrl)}>{strings.aboutKestrel.licence}</button>
      </section>
```
and in `App.tsx` pass it: `<Toolbox guide={report.guide} focus={guideTopic} appVersion={report.appVersion} />`.

- [ ] **Step 5: Verify**

Run: `cd app && npm test && npm run build && cd ..`
Expected: tests pass; tsc and vite build clean. Dev-server curl check as in Task 15, then stop it. The controller checks Home (sample "After Essential tweaks (sample)" with deltas), the first run (two buttons) and Toolbox (About section) in the browser preview.

- [ ] **Step 6: Commit**

```bash
git add app/src
git commit -m "feat(app): last benchmark on Home, starting-point step in the first run, About Kestrel"
```

---
### Task 17: CI, README, push and pull request

**Files:**
- Modify: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`

**Interfaces:**
- Consumes: everything above; `npm run fetch:presentmon` (Task 11).
- Produces: green `core`, `ui` and `windows` checks on the `stage-2` pull request; the Windows job proves the pinned PresentMon downloads, runs a 3-second capture with the app's own flags (exit code 0 and a valid CSV header), and lands at `target/release/presentmon/PresentMon.exe` next to `Kestrel.exe`, and it lints and tests the app crate on Windows; the release workflow fetches PresentMon before `tauri-action`.

- [ ] **Step 1: `ci.yml`** — apply these changes (the rest stays as it is):
  - `core` job: `cargo clippy -p kestrel-core -p kestrel-probe -p kestrel-bench --locked -- -D warnings` and `cargo test -p kestrel-core -p kestrel-probe -p kestrel-bench --locked`.
  - `windows` job: the same `-p kestrel-bench` additions to its clippy (`cargo clippy -p kestrel-probe -p kestrel-bench --locked -- -D warnings`) and test steps; then, right after the `npm ci` step and before `npx tauri build`, add:
    ```yaml
      - run: npm run fetch:presentmon
        working-directory: app
      - name: PresentMon smoke capture
        shell: pwsh
        run: |
          # The flags the app passes (presentmon_args), on dwm.exe for 3 seconds under a session name of its own.
          $pm = Resolve-Path "app/src-tauri/binaries/PresentMon-2.6.0-x64.exe"
          $capture = Start-Process -FilePath $pm -NoNewWindow -PassThru -ArgumentList '--process_name dwm.exe --output_file pm-smoke.csv --timed 3 --terminate_after_timed --session_name KestrelCI --stop_existing_session --no_console_stats --set_circular_buffer_size 8192 --track_pc_latency'
          $null = $capture.Handle
          # Keep the desktop changing so DWM presents frames while PresentMon records.
          Add-Type -AssemblyName System.Windows.Forms
          $form = New-Object System.Windows.Forms.Form
          $form.Show()
          for ($i = 0; $i -lt 60; $i++) { $form.Left = 100 + ($i % 20) * 10; [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 100 }
          $form.Close()
          if (-not $capture.WaitForExit(30000)) { $capture.Kill(); throw "PresentMon didn't finish" }
          if ($capture.ExitCode -ne 0) { throw "PresentMon exited with code $($capture.ExitCode)" }
          if (-not (Test-Path pm-smoke.csv)) { throw "PresentMon wrote no CSV" }
          $header = Get-Content pm-smoke.csv -TotalCount 1
          if (-not $header.Contains("MsBetweenPresents")) { throw "Unexpected PresentMon header: $header" }
          Write-Host "PresentMon header OK; frames captured: $((Get-Content pm-smoke.csv).Count - 1)"
    ```
  - Right after the `npx tauri build` step, add the app crate's own lint, which is the only check of its Windows-only code. It needs `app/dist` (which `npx tauri build` produces and the app embeds) and the fetched PresentMon (which Windows builds bundle), and like the other cargo steps it runs from the repo root, so it takes no `working-directory`. The app's tests run on the Mac (they use fakes, not Windows code); a Windows test binary can fail to load without the app's common-controls manifest, so CI doesn't run them:
    ```yaml
      - run: cargo clippy -p kestrel --locked --all-targets -- -D warnings
    ```
  - Replace the "Check the installer was built" step with:
    ```yaml
      - name: Check the installer and the bundled PresentMon
        shell: pwsh
        run: |
          $installer = Get-ChildItem "target/release/bundle/nsis/Kestrel_*_x64-setup.exe" -ErrorAction SilentlyContinue
          if (-not $installer) { throw "The installer was not built" }
          if (-not (Test-Path "target/release/presentmon/PresentMon.exe")) { throw "PresentMon isn't next to Kestrel.exe" }
          if (-not (Test-Path "target/release/presentmon/LICENSE.txt")) { throw "PresentMon's licence isn't bundled" }
    ```

- [ ] **Step 2: `release.yml`** — after its `npm ci` step add:
```yaml
      - run: npm run fetch:presentmon
        working-directory: app
```
and change the first line of `releaseBody` to:
```
            Test build. It changes nothing on your PC; it only saves benchmark results in its own folder.
```

- [ ] **Step 3: `README.md`** — replace the `## Status` paragraph and the install step 2 with:
```markdown
## Status

Stage 2 (v0.2.0): the benchmark. A warm-up and 3 runs on a fixed route in VALORANT's Range measure average FPS,
1% and 0.1% lows, stutters and PC latency, and compare every later result with your starting point. Kestrel
changes nothing on your PC; it only saves benchmark results in its own folder.

Stage 1 (v0.1.0): hardware detection, Health Check and BIOS guide.
```
```markdown
2. Download `Kestrel_<version>_x64-setup.exe` and run it. Installing over an older version is fine.
```
and add this line to "What Kestrel will never do":
```markdown
- Download anything while it runs (PresentMon, which measures frames, ships inside the installer)
```

- [ ] **Step 4: Local checks before pushing** (the Linux jobs' commands, plus the Windows-target lint)

Run, from the repo root:
```bash
PATH="$HOME/.cargo/bin:$PATH" cargo fmt --all --check
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel-core -p kestrel-probe -p kestrel-bench --locked --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel-core -p kestrel-probe -p kestrel-bench --locked
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel-probe -p kestrel-bench --target x86_64-pc-windows-msvc --locked --all-targets -- -D warnings
(cd app && npm test && npx tsc --noEmit && npm run build)
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel --locked && PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel --locked --all-targets -- -D warnings
```
Expected: all clean. Validate both workflow files parse (`ruby -ryaml -e 'YAML.load_file(ARGV[0])' .github/workflows/ci.yml`, same for release.yml).

- [ ] **Step 5: Commit, push, open the PR**

```bash
git add .github README.md
git commit -m "ci: fetch and smoke-test PresentMon, check it's bundled; README for stage 2"
git push -u origin stage-2
gh pr create --base main --head stage-2 --title "Stage 2: benchmark" --body "Stage 2 of Kestrel (see docs/superpowers/plans/2026-10-12-kestrel-stage-2-benchmark.md and docs/superpowers/specs/2026-10-12-kestrel-stage-2-benchmark-design.md). Kestrel still changes nothing on the PC; it only saves benchmark results in its own folder.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

- [ ] **Step 6: Wait for CI** through the app's PR monitor (don't poll). Expected: `core`, `ui` and `windows` pass, including the PresentMon smoke capture, the app crate's clippy on Windows, and the bundle check.

---

### Task 18: Publish v0.2.0 and hand it to the owner

**Files:** none in the repo; updates the tracker doc.

- [ ] **Step 1: Tag and publish** (only after the owner agrees to publish)

```bash
git tag -a v0.2.0 -m "Kestrel v0.2.0 — stage 2 test build (benchmark)" HEAD
git push origin v0.2.0
```
When the Release workflow finishes:
```bash
gh release view v0.2.0 --json isPrerelease,isDraft,assets --jq '{pre: .isPrerelease, draft: .isDraft, files: [.assets[].name]}'
```
Expected: `pre: true`, `draft: false`, a file named `Kestrel_0.2.0_x64-setup.exe`.

- [ ] **Step 2: Update the tracker** — Build plan: stage 2 → "Ready to test" with the release link; the "Now:" line points at the stage 2 checklist; add under "How testing works":

```markdown
### What to test in stage 2

- [ ] Install v0.2.0 over v0.1.0; Kestrel opens as before
- [ ] Benchmark tab: Route v1 is shown; Start benchmark, then open VALORANT and the Range
- [ ] F10 starts each pass and the beeps are audible in the game (long at the start, short at 30 s, double at the end)
- [ ] Warm-up and 3 runs complete; the average FPS is close to VALORANT's own FPS counter in the Range
- [ ] PC latency shows a number (Reflex On)
- [ ] The result says CPU- or GPU-bound
- [ ] Close and reopen Kestrel: the result is still there, and Home shows it
- [ ] Copy results, then paste the text in chat
- [ ] Route v1's steps were clear (note anything to change)
- [ ] Nothing outside Kestrel's folder changed
```

- [ ] **Step 3: Tell the owner** — one short message: the release link, "install over the current version, then work through 'What to test in stage 2' in the tracker", and "paste Copy results when you're done". Merge the PR into `main` only after the owner's test passes and they say so.

---

## Self-review (done while writing)

- **Spec coverage:** decisions 36–50 and 55 → Tasks 1–16 (36 starting point = first saved, Task 7; 37 F10 + beeps, Tasks 11–13; 38 three runs + middle value, Tasks 6 and 8; 39 metrics, Task 3; 40 saved summary + frame times, Task 7; 41 baseline + real-or-noise, Tasks 6–7; 42 first-run step, Task 16; 43 settings read-only + warnings, Tasks 4 and 6; 44 flags, Task 5; 45 Copy results + diagnostics, Tasks 9 and 13; 46 PresentMon 2.6.0 pinned, Tasks 11 and 17; 47 Reflex PC latency, Tasks 2–3; 48 Route v1 text, Task 14; 49 warm-up, Task 8; 50 no free capture: none built; 55 game profiles, Task 1). Errors table → Tasks 11–13 (game not running waits; F10 taken → countdown; PresentMon failure → session fails; no frames → "Too short"; crash mid-session → marker cleanup; AMD/Intel → "Not reported"; save failure → session fails). CI items → Task 17. Owner checklist → Task 18.
- **Placeholders:** none; every code step has its code.
- **Type consistency:** Rust `Benchmark/BenchmarkSummary/Comparison/RunMetrics/Flag/SessionView/BenchState` ↔ TS types of Task 14 (camelCase fields, kebab-case tags, `phase` flattened into `BenchState`); command names in `lib.rs` (Task 13) ↔ `client.ts` (Task 14); `Store::save` assigns ids used by `bench_get`.
- **Review Focus:** each line has its pinned test (Tasks 3, 4, 5, 6, 7, 8, 12).
