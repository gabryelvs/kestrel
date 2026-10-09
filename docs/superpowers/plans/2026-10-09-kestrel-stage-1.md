# Kestrel Stage 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Kestrel v0.1.0 as a GitHub pre-release: a Windows app that detects the PC's hardware, runs a read-only Health Check, shows a BIOS guide, and copies a diagnostics report — changing nothing on the PC.

**Architecture:** A Cargo workspace with a pure-Rust `kestrel-core` crate (data model, RAM/GPU analysis, Health Check rules, score, BIOS guide — all unit-tested on macOS), a `kestrel-probe` crate that fills the data model (real Windows probes via WMI/registry/Win32 behind `cfg(windows)`, a fixture-backed mock elsewhere), and a Tauri 2 app (`app/`) with a React + TypeScript UI. Windows builds run on GitHub Actions; the UI is developed and previewed on the Mac against the mock.

**Tech Stack:** Rust stable (2021 edition), Tauri 2, React 19 + TypeScript + Vite, `wmi`, `winreg`, `windows-sys`, `sysinfo`, `nvml-wrapper`, `regex`, `chrono`, `serde`, Vitest, GitHub Actions, NSIS.

**Spec:** `docs/superpowers/specs/2026-10-09-kestrel-design.md`

## Global Constraints

- Stage 1 is read-only: no code path may write the registry, change services, power plans, files outside Kestrel's own app-data folder, or anything else on the PC.
- Never read or write VALORANT's process memory, never inject, never load a kernel driver, never start/stop/modify Vanguard (`vgc`, `vgk`) — querying their state is allowed.
- No network calls at runtime except opening links in the user's browser. No telemetry.
- Windows x64 only for the shipped app. Windows 11 fully supported; Windows 10 22H2 must run (banner, untested).
- App identifier `io.github.gabryelvs.kestrel`; product name `Kestrel`; executable `Kestrel.exe`; version `0.1.0`.
- Installer: NSIS, `perMachine`; manifest `requireAdministrator`.
- UI copy: English, sentence case, no exclamation marks, all user-facing strings in `app/src/strings.ts`.
- Visual tokens: background `#0C0E11`, panel `#15181D`, line `#232830`, text `#E9ECEF`, muted `#8C949E`, accent `#C8F53C`, warning `#FFB547`, danger `#FF6B6B`; fonts Chakra Petch (UI) and JetBrains Mono (numbers), bundled locally.
- Every Rust type crossing to the UI is `serde` with `rename_all = "camelCase"`; enums `rename_all = "kebab-case"`.
- Licence GPL-3.0. README and About text say: "Kestrel is not affiliated with Riot Games. VALORANT is a trademark of Riot Games, Inc."
- Commits end with: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A probe fails (WMI namespace missing, access denied, empty result) — the app must still open and show every other check, with the failed ones as "Unknown" and the error listed in diagnostics. Pinned by `health::tests::empty_snapshot_gives_unknowns_not_problems` (Task 5) and `probe` error collection in Task 9.
2. VALORANT installed somewhere custom (e.g. `D:/Games/Riot Games/VALORANT/live`) or not installed — storage checks use the real drive or report Unknown. Pinned by `valorant::tests::*` (Task 6).
3. Several GPUs including virtual (Parsec, Microsoft Basic Display) and hybrid laptops — "main GPU" must be the real discrete one; virtual adapters never trigger driver-age problems. Pinned by `gpu::tests::main_gpu_*` (Task 4).
4. Several monitors (360 Hz main + 60 Hz side) and active virtual displays — the refresh check names the display that is below its maximum and ignores virtual ones. Pinned by `health::tests::refresh_flags_only_the_slow_real_display` (Task 5).
5. Unusual or mixed RAM kits, or SMBIOS speeds of 0 — never a false "XMP off" alarm; unknown part numbers give Unknown. Pinned by `ram::tests::*unknown*` and `ram::tests::mixed_kits_use_slowest_rating` (Task 3).

## File Structure

```
/                                   repo root (/Users/gabs/Optimisation)
├─ Cargo.toml                       workspace: crates/*, app/src-tauri
├─ rust-toolchain.toml              stable + clippy, rustfmt, x86_64-pc-windows-msvc target
├─ README.md, LICENSE, .gitignore
├─ docs/superpowers/{specs,plans}/  this plan and the spec
├─ crates/kestrel-core/             pure logic, no OS calls
│  ├─ src/lib.rs                    module list + re-exports
│  ├─ src/snapshot.rs               SystemSnapshot and its parts (the data model)
│  ├─ src/ram.rs                    rated speed from part numbers, channel layout
│  ├─ src/gpu.rs                    vendor detection, NVIDIA version decode, main GPU
│  ├─ src/valorant.rs               install path parsing from Riot metadata
│  ├─ src/health.rs                 Finding model + all Health Check rules
│  ├─ src/score.rs                  score from findings
│  ├─ src/bios_guide.rs             vendor detection + BIOS steps
│  ├─ src/report.rs                 Report = snapshot + findings + score + guide
│  └─ fixtures/owner-pc.json        sample snapshot of the owner's PC (mock + tests)
├─ crates/kestrel-probe/            fills SystemSnapshot
│  ├─ src/lib.rs                    collect_snapshot(), LoadSampler
│  ├─ src/mock.rs                   non-Windows: fixture snapshot
│  ├─ src/load.rs                   live CPU/RAM/GPU load sampler
│  ├─ src/win/mod.rs                Windows orchestration + error collection
│  ├─ src/win/wmi_probe.rs          WMI queries
│  ├─ src/win/registry_probe.rs     registry reads
│  ├─ src/win/display.rs            EnumDisplayDevices/EnumDisplaySettings
│  ├─ src/win/firmware.rs           IOMMU via ACPI DMAR/IVRS tables
│  ├─ src/win/gpu_usage.rs          PDH "GPU Engine" counters + NVML temperature
│  └─ src/bin/kestrel-probe.rs      CLI: prints Report JSON (CI smoke test, mock regeneration)
├─ app/                             Tauri app
│  ├─ package.json, vite.config.ts, tsconfig.json, index.html
│  ├─ src/main.tsx, App.tsx, strings.ts
│  ├─ src/styles/{tokens.css,base.css}
│  ├─ src/api/{types.ts,client.ts,mock-report.json}
│  ├─ src/components/…              TitleBar, Logo, Panel, ScoreCard, FindingRow, HardwarePanel, LoadGraph, LockInCard, CopyDiagnostics, Toast
│  ├─ src/pages/…                   Home, Health, Toolbox (BIOS guide), ComingSoon
│  ├─ src/first-run/FirstRun.tsx
│  └─ src-tauri/                    Rust shell: commands, manifest, icons, tauri.conf.json
└─ .github/workflows/{ci.yml,release.yml}
```

Responsibilities: `kestrel-core` decides (pure functions, tested on any OS); `kestrel-probe` observes (OS calls, compile-checked on the Mac with `--target x86_64-pc-windows-msvc`, run on the Windows CI runner); the app presents. The UI never computes findings itself.

## Conventions for every task

- Load Rust into the shell first: `source "$HOME/.cargo/env"`.
- Crate versions: add dependencies with `cargo add`; if the resolved version's API differs from a snippet, keep the behaviour described in the step and follow the compiler.
- Windows-only code must pass `cargo check -p kestrel-probe --target x86_64-pc-windows-msvc` on the Mac before commit.
- Work happens on branch `stage-1`; `main` only receives the bootstrap commit and the reviewed merge.

---
### Task 1: Bootstrap the repo, toolchain and workspace

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `README.md`, `LICENSE`, `crates/kestrel-core/Cargo.toml`, `crates/kestrel-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a Cargo workspace whose `members` list later tasks extend; branch `stage-1`; GitHub remote `origin` = `github.com/gabryelvs/kestrel`.

- [ ] **Step 1: Install Rust (owner approved this on 2026-10-09)**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
source "$HOME/.cargo/env"
rustup component add clippy rustfmt
rustup target add x86_64-pc-windows-msvc
cargo --version
```
Expected: `cargo 1.x.y` printed.

- [ ] **Step 2: Create workspace files**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/kestrel-core"]

[workspace.package]
edition = "2021"
license = "GPL-3.0-only"
version = "0.1.0"
repository = "https://github.com/gabryelvs/kestrel"
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
components = ["clippy", "rustfmt"]
targets = ["x86_64-pc-windows-msvc"]
```

`.gitignore`:
```
target/
node_modules/
app/dist/
app/src-tauri/gen/
.DS_Store
*.log
```

`crates/kestrel-core/Cargo.toml`:
```toml
[package]
name = "kestrel-core"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
```

`crates/kestrel-core/src/lib.rs`:
```rust
//! Kestrel core: the data model and every decision Kestrel makes. Pure Rust, no OS calls.
```

- [ ] **Step 3: README and licence**

`README.md`:
```markdown
# Kestrel

A free, honest FPS optimiser for VALORANT on Windows. Kestrel detects your hardware, finds what's
holding your frames back, and (in later versions) applies reversible tweaks picked for your PC.

> Kestrel is not affiliated with Riot Games. VALORANT is a trademark of Riot Games, Inc.

## Status

Stage 1 (v0.1.0): hardware detection, Health Check and BIOS guide. It only reads your PC; it changes nothing.

## Install a test build

1. Open the latest pre-release on the [Releases page](https://github.com/gabryelvs/kestrel/releases).
2. Download `Kestrel_0.1.0_x64-setup.exe` and run it.
3. Test builds aren't signed yet, so Windows may show "Windows protected your PC". Click **More info → Run anyway**.

## What Kestrel will never do

- Read or change VALORANT's memory, or inject anything into it
- Load its own kernel driver
- Touch Riot Vanguard
- Tell you to turn off TPM or Secure Boot
- Permanently disable Defender, UAC or Windows Update

## Development

Plans live in `docs/superpowers/plans/`. Windows builds run on GitHub Actions.

## Licence

GPL-3.0. See [LICENSE](LICENSE).
```

```bash
gh api licenses/gpl-3.0 --jq .body > LICENSE
head -3 LICENSE
```
Expected: `GNU GENERAL PUBLIC LICENSE` / `Version 3, 29 June 2007`.

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo test --workspace`
Expected: `test result: ok. 0 passed` (no tests yet), exit 0.

- [ ] **Step 5: Commit, create the public repo, branch**

```bash
git init -b main
git add .
git commit -m "chore: bootstrap Kestrel workspace

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
gh repo create gabryelvs/kestrel --public --description "Kestrel: a free, honest FPS optimiser for VALORANT on Windows" --source . --remote origin --push
git switch -c stage-1
```
Expected: repo URL printed; `git branch --show-current` → `stage-1`.

---

### Task 2: The data model (`SystemSnapshot`) and the owner-PC fixture

**Files:**
- Modify: `crates/kestrel-core/Cargo.toml`, `crates/kestrel-core/src/lib.rs`
- Create: `crates/kestrel-core/src/snapshot.rs`, `crates/kestrel-core/fixtures/owner-pc.json`

**Interfaces:**
- Consumes: nothing.
- Produces (all `pub`, `serde` camelCase, enums kebab-case, all `Default`):
  `SystemSnapshot { schema_version: u32, collected_at: String, os: OsInfo, machine: MachineInfo, cpu: CpuInfo, gpus: Vec<GpuInfo>, memory: MemoryInfo, displays: Vec<DisplayInfo>, board: BoardInfo, storage: StorageInfo, windows: WindowsSettings, security: SecurityInfo, network: NetworkInfo, background: BackgroundInfo, probe_errors: Vec<ProbeError> }`;
  `OsInfo { caption: String, build: u32, display_version: Option<String> }`;
  `FormFactor { Desktop, Laptop, Unknown }`; `MachineInfo { form_factor, manufacturer: String, model: String, on_battery: Option<bool> }`;
  `CpuVendor { Intel, Amd, Other }`; `CpuInfo { name: String, vendor, cores: u32, threads: u32 }`;
  `GpuVendor { Nvidia, Amd, Intel, Virtual, Other }`; `GpuInfo { name: String, vendor, driver_version: String, driver_date: Option<String>, vram_mb: Option<u64>, is_integrated: bool }`;
  `MemoryInfo { total_mb: u64, ddr_generation: Option<u8>, modules: Vec<MemoryModule> }`; `MemoryModule { capacity_mb: u64, configured_mts: Option<u32>, max_mts: Option<u32>, part_number: String, manufacturer: String, bank_label: String, device_locator: String }`;
  `DisplayInfo { name: String, adapter: String, current_hz: u32, max_hz: u32, width: u32, height: u32, is_primary: bool, is_virtual: bool }`;
  `BoardInfo { manufacturer: String, product: String, bios_version: String, bios_date: Option<String> }`;
  `DriveMedia { Nvme, Ssd, Hdd, Unknown }`; `ValorantInstall { path: String, drive_letter: String, media, free_gb: f64, total_gb: f64 }`; `StorageInfo { valorant: Option<ValorantInstall> }`;
  `PowerPlan { guid: String, name: String }`; `WindowsSettings { power_plan: Option<PowerPlan>, game_mode: Option<bool>, background_recording: Option<bool>, hags: Option<bool>, valorant_gpu_preference: Option<u8>, pending_restart: Option<bool>, startup_app_count: Option<u32> }`;
  `ServiceState { Running, Stopped, NotInstalled, Unknown }`; `SecurityInfo { tpm2: Option<bool>, secure_boot: Option<bool>, vbs_running: Option<bool>, hvci_running: Option<bool>, iommu: Option<bool>, vanguard: ServiceState }`;
  `NetKind { Ethernet, Wifi, Other }`; `NetAdapter { name: String, kind, link_mbps: Option<u64>, connected: bool, power_saving: Option<bool> }`; `NetworkInfo { adapters: Vec<NetAdapter>, delivery_optimization_internet: Option<bool> }`;
  `BusyProcess { name: String, cpu_percent: f32 }`; `BackgroundInfo { busy_processes: Option<Vec<BusyProcess>> }`;
  `ProbeError { probe: String, message: String }`;
  `pub const SCHEMA_VERSION: u32 = 1;` `pub fn owner_pc_fixture() -> SystemSnapshot`.
  Dates are `"YYYY-MM-DD"` strings. `None` always means "couldn't tell".

- [ ] **Step 1: Add dependencies**

```bash
cd crates/kestrel-core
cargo add serde --features derive
cargo add serde_json
cargo add chrono --no-default-features --features std,serde
cargo add regex
cd ../..
```

- [ ] **Step 2: Write the fixture** `crates/kestrel-core/fixtures/owner-pc.json` (sample values for the owner's PC; used by the mock probe and tests):

```json
{
  "schemaVersion": 1,
  "collectedAt": "2026-10-09T16:30:00Z",
  "os": { "caption": "Microsoft Windows 11 Pro", "build": 26200, "displayVersion": "25H2" },
  "machine": { "formFactor": "desktop", "manufacturer": "ASUS", "model": "System Product Name", "onBattery": null },
  "cpu": { "name": "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz", "vendor": "intel", "cores": 8, "threads": 16 },
  "gpus": [
    { "name": "Parsec Virtual Display Adapter", "vendor": "virtual", "driverVersion": "0.45.0.0", "driverDate": null, "vramMb": null, "isIntegrated": false },
    { "name": "NVIDIA GeForce RTX 3060 Ti", "vendor": "nvidia", "driverVersion": "32.0.16.1074", "driverDate": "2026-09-15", "vramMb": 8192, "isIntegrated": false }
  ],
  "memory": { "totalMb": 32768, "ddrGeneration": 4, "modules": [
    { "capacityMb": 16384, "configuredMts": 3600, "maxMts": 2133, "partNumber": "CMG32GX4M2D3600C18", "manufacturer": "Corsair", "bankLabel": "BANK 1", "deviceLocator": "ChannelA-DIMM2" },
    { "capacityMb": 16384, "configuredMts": 3600, "maxMts": 2133, "partNumber": "CMG32GX4M2D3600C18", "manufacturer": "Corsair", "bankLabel": "BANK 3", "deviceLocator": "ChannelB-DIMM2" }
  ] },
  "displays": [
    { "name": "\\\\.\\DISPLAY1", "adapter": "NVIDIA GeForce RTX 3060 Ti", "currentHz": 360, "maxHz": 360, "width": 1920, "height": 1080, "isPrimary": true, "isVirtual": false }
  ],
  "board": { "manufacturer": "ASUSTeK COMPUTER INC.", "product": "ROG MAXIMUS XIII HERO", "biosVersion": "2103", "biosDate": "2024-05-20" },
  "storage": { "valorant": { "path": "C:/Riot Games/VALORANT/live", "driveLetter": "C", "media": "nvme", "freeGb": 210.5, "totalGb": 931.5 } },
  "windows": { "powerPlan": { "guid": "381b4222-f694-41f0-9685-ff5bb260df2e", "name": "Balanced" }, "gameMode": true, "backgroundRecording": false, "hags": true, "valorantGpuPreference": null, "pendingRestart": false, "startupAppCount": 7 },
  "security": { "tpm2": true, "secureBoot": true, "vbsRunning": false, "hvciRunning": false, "iommu": false, "vanguard": "running" },
  "network": { "adapters": [ { "name": "Intel(R) Ethernet Controller (3) I225-V", "kind": "ethernet", "linkMbps": 2500, "connected": true, "powerSaving": true } ], "deliveryOptimizationInternet": false },
  "background": { "busyProcesses": [] },
  "probeErrors": []
}
```

- [ ] **Step 3: Write the failing tests** at the bottom of `crates/kestrel-core/src/snapshot.rs` (create the file with only this block and `use super::*` will fail to compile — that is the expected failure):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_fixture_parses() {
        let s = owner_pc_fixture();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.cpu.threads, 16);
        assert_eq!(s.memory.modules.len(), 2);
        assert_eq!(s.security.vanguard, ServiceState::Running);
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let s = owner_pc_fixture();
        let json = serde_json::to_string(&s).unwrap();
        let again: SystemSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, again);
    }

    #[test]
    fn enums_use_kebab_case_and_fields_camel_case() {
        assert_eq!(serde_json::to_string(&DriveMedia::Nvme).unwrap(), "\"nvme\"");
        assert_eq!(serde_json::to_string(&ServiceState::NotInstalled).unwrap(), "\"not-installed\"");
        let json = serde_json::to_string(&OsInfo { caption: "x".into(), build: 1, display_version: None }).unwrap();
        assert!(json.contains("displayVersion"));
    }

    #[test]
    fn default_snapshot_means_unknown_everywhere() {
        let s = SystemSnapshot::default();
        assert_eq!(s.machine.form_factor, FormFactor::Unknown);
        assert_eq!(s.security.vanguard, ServiceState::Unknown);
        assert!(s.storage.valorant.is_none());
    }
}
```

Add to `crates/kestrel-core/src/lib.rs`:
```rust
pub mod snapshot;
pub use snapshot::*;
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p kestrel-core`
Expected: compile errors (`cannot find function owner_pc_fixture`, missing types).

- [ ] **Step 5: Implement** — put this above the tests in `snapshot.rs`:

```rust
//! The data model: everything Kestrel knows about a PC. `None` always means "couldn't tell".
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

macro_rules! model {
    ($($item:item)*) => { $(
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        #[serde(rename_all = "camelCase", default)]
        $item
    )* };
}

macro_rules! choice {
    ($($item:item)*) => { $(
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "kebab-case")]
        $item
    )* };
}

model! {
    pub struct SystemSnapshot {
        pub schema_version: u32,
        pub collected_at: String,
        pub os: OsInfo,
        pub machine: MachineInfo,
        pub cpu: CpuInfo,
        pub gpus: Vec<GpuInfo>,
        pub memory: MemoryInfo,
        pub displays: Vec<DisplayInfo>,
        pub board: BoardInfo,
        pub storage: StorageInfo,
        pub windows: WindowsSettings,
        pub security: SecurityInfo,
        pub network: NetworkInfo,
        pub background: BackgroundInfo,
        pub probe_errors: Vec<ProbeError>,
    }
    pub struct OsInfo { pub caption: String, pub build: u32, pub display_version: Option<String> }
    pub struct MachineInfo { pub form_factor: FormFactor, pub manufacturer: String, pub model: String, pub on_battery: Option<bool> }
    pub struct CpuInfo { pub name: String, pub vendor: CpuVendor, pub cores: u32, pub threads: u32 }
    pub struct GpuInfo { pub name: String, pub vendor: GpuVendor, pub driver_version: String, pub driver_date: Option<String>, pub vram_mb: Option<u64>, pub is_integrated: bool }
    pub struct MemoryInfo { pub total_mb: u64, pub ddr_generation: Option<u8>, pub modules: Vec<MemoryModule> }
    pub struct MemoryModule { pub capacity_mb: u64, pub configured_mts: Option<u32>, pub max_mts: Option<u32>, pub part_number: String, pub manufacturer: String, pub bank_label: String, pub device_locator: String }
    pub struct DisplayInfo { pub name: String, pub adapter: String, pub current_hz: u32, pub max_hz: u32, pub width: u32, pub height: u32, pub is_primary: bool, pub is_virtual: bool }
    pub struct BoardInfo { pub manufacturer: String, pub product: String, pub bios_version: String, pub bios_date: Option<String> }
    pub struct StorageInfo { pub valorant: Option<ValorantInstall> }
    pub struct ValorantInstall { pub path: String, pub drive_letter: String, pub media: DriveMedia, pub free_gb: f64, pub total_gb: f64 }
    pub struct PowerPlan { pub guid: String, pub name: String }
    pub struct WindowsSettings { pub power_plan: Option<PowerPlan>, pub game_mode: Option<bool>, pub background_recording: Option<bool>, pub hags: Option<bool>, pub valorant_gpu_preference: Option<u8>, pub pending_restart: Option<bool>, pub startup_app_count: Option<u32> }
    pub struct SecurityInfo { pub tpm2: Option<bool>, pub secure_boot: Option<bool>, pub vbs_running: Option<bool>, pub hvci_running: Option<bool>, pub iommu: Option<bool>, pub vanguard: ServiceState }
    pub struct NetAdapter { pub name: String, pub kind: NetKind, pub link_mbps: Option<u64>, pub connected: bool, pub power_saving: Option<bool> }
    pub struct NetworkInfo { pub adapters: Vec<NetAdapter>, pub delivery_optimization_internet: Option<bool> }
    pub struct BusyProcess { pub name: String, pub cpu_percent: f32 }
    pub struct BackgroundInfo { pub busy_processes: Option<Vec<BusyProcess>> }
    pub struct ProbeError { pub probe: String, pub message: String }
}

choice! {
    pub enum FormFactor { Desktop, Laptop, #[default] Unknown }
    pub enum CpuVendor { Intel, Amd, #[default] Other }
    pub enum GpuVendor { Nvidia, Amd, Intel, Virtual, #[default] Other }
    pub enum DriveMedia { Nvme, Ssd, Hdd, #[default] Unknown }
    pub enum ServiceState { Running, Stopped, NotInstalled, #[default] Unknown }
    pub enum NetKind { Ethernet, Wifi, #[default] Other }
}

pub const OWNER_PC_JSON: &str = include_str!("../fixtures/owner-pc.json");

/// The owner's PC as a sample snapshot. Used by the mock probe on non-Windows machines and by tests.
pub fn owner_pc_fixture() -> SystemSnapshot {
    serde_json::from_str(OWNER_PC_JSON).expect("bundled fixture is valid JSON for SystemSnapshot")
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p kestrel-core`
Expected: `4 passed`.

- [ ] **Step 7: Commit**

```bash
git add crates/kestrel-core
git commit -m "feat(core): add SystemSnapshot data model and owner-PC fixture

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: RAM analysis — rated speed and channel layout

**Files:**
- Create: `crates/kestrel-core/src/ram.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (add `pub mod ram;`)

**Interfaces:**
- Consumes: `MemoryModule` (Task 2).
- Produces: `pub fn rated_mts_from_part(part: &str) -> Option<u32>`; `pub fn rated_mts(modules: &[MemoryModule]) -> Option<u32>` (slowest module's part rating; else SMBIOS max only if every module reports one > 0); `pub fn configured_mts(modules: &[MemoryModule]) -> Option<u32>` (slowest configured speed; `None` if any module reports 0/None); `pub enum ChannelLayout { Single, Dual, Unknown }` (Copy, serde kebab-case); `pub fn channel_layout(modules: &[MemoryModule]) -> ChannelLayout`.

- [ ] **Step 1: Write the failing tests** in `crates/kestrel-core/src/ram.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn module(part: &str, configured: Option<u32>, max: Option<u32>, locator: &str, bank: &str) -> MemoryModule {
        MemoryModule {
            capacity_mb: 16384,
            configured_mts: configured,
            max_mts: max,
            part_number: part.into(),
            manufacturer: String::new(),
            bank_label: bank.into(),
            device_locator: locator.into(),
        }
    }

    #[test]
    fn corsair_parts() {
        assert_eq!(rated_mts_from_part("CMG32GX4M2D3600C18"), Some(3600));
        assert_eq!(rated_mts_from_part("CMK16GX4M2B3200C16"), Some(3200));
        assert_eq!(rated_mts_from_part("CMH32GX5M2B6000C30"), Some(6000));
    }

    #[test]
    fn gskill_parts() {
        assert_eq!(rated_mts_from_part("F4-3600C16-8GVKC"), Some(3600));
        assert_eq!(rated_mts_from_part("F5-6000J3038F16GX2-TZ5RK"), Some(6000));
    }

    #[test]
    fn kingston_parts() {
        assert_eq!(rated_mts_from_part("KF436C17BBK2/16"), Some(3600));
        assert_eq!(rated_mts_from_part("HX432C16PB3K2/16"), Some(3200));
        assert_eq!(rated_mts_from_part("KF560C36BBEK2-32"), Some(6000));
    }

    #[test]
    fn crucial_parts() {
        assert_eq!(rated_mts_from_part("BL8G32C16U4B.M8FE1"), Some(3200));
        assert_eq!(rated_mts_from_part("BL2K16G36C16U4B"), Some(3600));
        assert_eq!(rated_mts_from_part("CP16G56C46U5.M8B1"), Some(5600));
    }

    #[test]
    fn teamgroup_adata_patriot_parts() {
        assert_eq!(rated_mts_from_part("TF13D432G3600HC18JDC01"), Some(3600));
        assert_eq!(rated_mts_from_part("AX4U360038G18A-DT60"), Some(3600));
        assert_eq!(rated_mts_from_part("PVS416G360C8K"), Some(3600));
    }

    #[test]
    fn part_numbers_are_trimmed_and_case_insensitive() {
        assert_eq!(rated_mts_from_part("  cmg32gx4m2d3600c18   "), Some(3600));
    }

    #[test]
    fn unknown_part_gives_none() {
        assert_eq!(rated_mts_from_part("CT16G4DFRA32A"), None);
        assert_eq!(rated_mts_from_part(""), None);
        assert_eq!(rated_mts_from_part("0000"), None);
    }

    #[test]
    fn mixed_kits_use_slowest_rating() {
        let m = [
            module("CMG32GX4M2D3600C18", Some(3200), None, "", ""),
            module("F4-3200C16-8GVKB", Some(3200), None, "", ""),
        ];
        assert_eq!(rated_mts(&m), Some(3200));
    }

    #[test]
    fn unknown_parts_fall_back_to_smbios_max_only_when_every_module_reports_it() {
        assert_eq!(rated_mts(&[module("CT16G4DFRA32A", Some(2400), Some(3200), "", "")]), Some(3200));
        assert_eq!(rated_mts(&[module("CT16G4DFRA32A", Some(2400), None, "", "")]), None);
        assert_eq!(rated_mts(&[module("CT16G4DFRA32A", Some(2400), Some(0), "", "")]), None);
        assert_eq!(rated_mts(&[]), None);
    }

    #[test]
    fn configured_speed_ignores_zero_and_missing() {
        assert_eq!(configured_mts(&[module("", Some(3600), None, "", ""), module("", Some(0), None, "", "")]), None);
        assert_eq!(configured_mts(&[module("", Some(3600), None, "", ""), module("", Some(3600), None, "", "")]), Some(3600));
        assert_eq!(configured_mts(&[]), None);
    }

    #[test]
    fn channel_layouts() {
        let dual = [module("", None, None, "ChannelA-DIMM2", "BANK 1"), module("", None, None, "ChannelB-DIMM2", "BANK 3")];
        assert_eq!(channel_layout(&dual), ChannelLayout::Dual);
        let same = [module("", None, None, "ChannelA-DIMM1", ""), module("", None, None, "ChannelA-DIMM2", "")];
        assert_eq!(channel_layout(&same), ChannelLayout::Single);
        let msi = [module("", None, None, "DIMM_A2", ""), module("", None, None, "DIMM_B2", "")];
        assert_eq!(channel_layout(&msi), ChannelLayout::Dual);
        let amd = [module("", None, None, "DIMM 0", "P0 CHANNEL A"), module("", None, None, "DIMM 1", "P0 CHANNEL B")];
        assert_eq!(channel_layout(&amd), ChannelLayout::Dual);
        let unknown = [module("", None, None, "DIMM 0", "BANK 0"), module("", None, None, "DIMM 1", "BANK 2")];
        assert_eq!(channel_layout(&unknown), ChannelLayout::Unknown);
        assert_eq!(channel_layout(&dual[..1]), ChannelLayout::Single);
        assert_eq!(channel_layout(&[]), ChannelLayout::Unknown);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kestrel-core ram`
Expected: compile errors (functions not defined).

- [ ] **Step 3: Implement** above the tests in `ram.rs`:

```rust
//! RAM analysis: what the kit is rated for, and whether it runs in dual channel.
use crate::snapshot::MemoryModule;
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelLayout {
    Single,
    Dual,
    Unknown,
}

struct Rule {
    re: Regex,
    scale: u32,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let rule = |pattern: &str, scale: u32| Rule { re: Regex::new(pattern).expect("valid pattern"), scale };
        vec![
            rule(r"^CM[A-Z]{1,3}\d+GX[45]M\d[A-Z]?(\d{4})C\d{2}", 1), // Corsair
            rule(r"^F[45]-(\d{4})[A-Z]", 1),                         // G.Skill
            rule(r"^(?:KF|HX)[45](\d{2})C\d{2}", 100),               // Kingston FURY / HyperX
            rule(r"^(?:BL|CP)(?:\dK)?\d+G(\d{2})C\d{2}", 100),       // Crucial Ballistix / Pro
            rule(r"(\d{4})HC\d{2}", 1),                              // TeamGroup
            rule(r"^AX[45]U(\d{4})", 1),                             // ADATA XPG
            rule(r"^PV[A-Z]*[45]\d{1,3}G(\d{3})C", 10),              // Patriot Viper
        ]
    })
}

/// Rated speed in MT/s read from a module's part number, if the brand's format is known.
pub fn rated_mts_from_part(part: &str) -> Option<u32> {
    let part = part.trim().to_ascii_uppercase();
    if part.is_empty() {
        return None;
    }
    rules().iter().find_map(|rule| {
        let digits = rule.re.captures(&part)?.get(1)?.as_str();
        let mts = digits.parse::<u32>().ok()? * rule.scale;
        (1600..=12_000).contains(&mts).then_some(mts)
    })
}

/// The kit's rated speed: the slowest module's part-number rating. When a part number isn't
/// recognised, fall back to the SMBIOS maximum, but only if every module reports one.
pub fn rated_mts(modules: &[MemoryModule]) -> Option<u32> {
    if modules.is_empty() {
        return None;
    }
    let from_parts: Vec<u32> = modules.iter().filter_map(|m| rated_mts_from_part(&m.part_number)).collect();
    if from_parts.len() == modules.len() {
        return from_parts.into_iter().min();
    }
    let maxima: Vec<u32> = modules.iter().filter_map(|m| m.max_mts.filter(|&v| v > 0)).collect();
    (maxima.len() == modules.len()).then(|| maxima.into_iter().min()).flatten()
}

/// The speed the RAM actually runs at (slowest module). `None` if any module doesn't report it.
pub fn configured_mts(modules: &[MemoryModule]) -> Option<u32> {
    let speeds: Vec<u32> = modules.iter().filter_map(|m| m.configured_mts.filter(|&v| v > 0)).collect();
    (!speeds.is_empty() && speeds.len() == modules.len()).then(|| speeds.into_iter().min()).flatten()
}

fn channel_letter(module: &MemoryModule) -> Option<char> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)channel\s*([a-h])\b|\bdimm[_\s]?([a-h])\d").expect("valid pattern"));
    [&module.device_locator, &module.bank_label].into_iter().find_map(|text| {
        let caps = re.captures(text)?;
        let letter = caps.get(1).or_else(|| caps.get(2))?;
        letter.as_str().chars().next().map(|c| c.to_ascii_uppercase())
    })
}

/// Whether the modules sit on more than one memory channel.
pub fn channel_layout(modules: &[MemoryModule]) -> ChannelLayout {
    match modules.len() {
        0 => ChannelLayout::Unknown,
        1 => ChannelLayout::Single,
        _ => match modules.iter().map(channel_letter).collect::<Option<Vec<char>>>() {
            Some(mut letters) => {
                letters.sort_unstable();
                letters.dedup();
                if letters.len() >= 2 { ChannelLayout::Dual } else { ChannelLayout::Single }
            }
            None => ChannelLayout::Unknown,
        },
    }
}
```

Add `pub mod ram;` to `lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kestrel-core ram`
Expected: `11 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core
git commit -m "feat(core): detect RAM rated speed and channel layout

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 4: GPU helpers — vendor, integrated vs discrete, main GPU, NVIDIA version

**Files:**
- Create: `crates/kestrel-core/src/gpu.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (add `pub mod gpu;`)

**Interfaces:**
- Consumes: `GpuInfo`, `GpuVendor` (Task 2).
- Produces: `pub fn vendor_from(pnp_device_id: &str, name: &str) -> GpuVendor`; `pub fn looks_integrated(vendor: GpuVendor, name: &str, vram_mb: Option<u64>) -> bool`; `pub fn main_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo>` (largest-VRAM real discrete GPU, else first real integrated, never virtual); `pub fn is_hybrid(gpus: &[GpuInfo]) -> bool`; `pub fn nvidia_version(windows_version: &str) -> Option<String>`; `pub fn display_driver_version(gpu: &GpuInfo) -> String` (NVIDIA → "610.74", others → raw).

- [ ] **Step 1: Write the failing tests** in `crates/kestrel-core/src/gpu.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(name: &str, vendor: GpuVendor, integrated: bool, vram: Option<u64>) -> GpuInfo {
        GpuInfo { name: name.into(), vendor, driver_version: "1.0.0.0".into(), driver_date: None, vram_mb: vram, is_integrated: integrated }
    }

    #[test]
    fn vendors_from_pci_ids_and_names() {
        assert_eq!(vendor_from("PCI\\VEN_10DE&DEV_2489&SUBSYS_00000000", "NVIDIA GeForce RTX 3060 Ti"), GpuVendor::Nvidia);
        assert_eq!(vendor_from("ROOT\\DISPLAY\\0000", "Parsec Virtual Display Adapter"), GpuVendor::Virtual);
        assert_eq!(vendor_from("PCI\\VEN_8086&DEV_4C8A", "Intel(R) UHD Graphics 750"), GpuVendor::Intel);
        assert_eq!(vendor_from("PCI\\VEN_1002&DEV_73BF", "AMD Radeon RX 6800 XT"), GpuVendor::Amd);
        assert_eq!(vendor_from("ROOT\\BasicDisplay\\0000", "Microsoft Basic Display Adapter"), GpuVendor::Virtual);
        assert_eq!(vendor_from("", "Mystery GPU"), GpuVendor::Other);
    }

    #[test]
    fn integrated_detection() {
        assert!(looks_integrated(GpuVendor::Intel, "Intel(R) UHD Graphics 750", Some(128)));
        assert!(looks_integrated(GpuVendor::Intel, "Intel(R) Arc(TM) Graphics", Some(128)));
        assert!(!looks_integrated(GpuVendor::Intel, "Intel(R) Arc(TM) A770 Graphics", Some(16384)));
        assert!(looks_integrated(GpuVendor::Amd, "AMD Radeon(TM) Graphics", Some(512)));
        assert!(looks_integrated(GpuVendor::Amd, "AMD Radeon 780M Graphics", Some(2048)));
        assert!(!looks_integrated(GpuVendor::Amd, "AMD Radeon RX 7800 XT", Some(16384)));
        assert!(!looks_integrated(GpuVendor::Nvidia, "NVIDIA GeForce RTX 4060 Laptop GPU", Some(8192)));
    }

    #[test]
    fn main_gpu_skips_virtual_adapters() {
        let gpus = [gpu("Parsec Virtual Display Adapter", GpuVendor::Virtual, false, None), gpu("NVIDIA GeForce RTX 3060 Ti", GpuVendor::Nvidia, false, Some(8192))];
        assert_eq!(main_gpu(&gpus).unwrap().name, "NVIDIA GeForce RTX 3060 Ti");
    }

    #[test]
    fn main_gpu_prefers_discrete_on_hybrid() {
        let gpus = [gpu("Intel(R) UHD Graphics", GpuVendor::Intel, true, Some(128)), gpu("NVIDIA GeForce RTX 4060 Laptop GPU", GpuVendor::Nvidia, false, Some(8192))];
        assert_eq!(main_gpu(&gpus).unwrap().vendor, GpuVendor::Nvidia);
        assert!(is_hybrid(&gpus));
    }

    #[test]
    fn main_gpu_falls_back_to_integrated_and_never_virtual() {
        let igpu = [gpu("Intel(R) UHD Graphics", GpuVendor::Intel, true, Some(128))];
        assert_eq!(main_gpu(&igpu).unwrap().vendor, GpuVendor::Intel);
        assert!(!is_hybrid(&igpu));
        let only_virtual = [gpu("Microsoft Basic Display Adapter", GpuVendor::Virtual, false, None)];
        assert!(main_gpu(&only_virtual).is_none());
    }

    #[test]
    fn nvidia_versions_decode() {
        assert_eq!(nvidia_version("32.0.16.1074").as_deref(), Some("610.74"));
        assert_eq!(nvidia_version("31.0.15.5222").as_deref(), Some("552.22"));
        assert_eq!(nvidia_version("27.21.14.5671").as_deref(), Some("456.71"));
        assert_eq!(nvidia_version("1.2.3"), None);
        assert_eq!(nvidia_version("abc"), None);
        let g = gpu("NVIDIA GeForce RTX 3060 Ti", GpuVendor::Nvidia, false, None);
        assert_eq!(display_driver_version(&GpuInfo { driver_version: "32.0.16.1074".into(), ..g }), "610.74");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kestrel-core gpu`
Expected: compile errors (functions not defined).

- [ ] **Step 3: Implement** above the tests:

```rust
//! GPU helpers: who made it, whether it's built into the CPU, and which one games use.
use crate::snapshot::{GpuInfo, GpuVendor};
use regex::Regex;
use std::sync::OnceLock;

const VIRTUAL_HINTS: [&str; 5] = ["virtual", "parsec", "basic display", "remote display", "indirect display"];

/// Vendor from the PCI id inside a PNP device id ("PCI\\VEN_10DE&…"), falling back to the name.
pub fn vendor_from(pnp_device_id: &str, name: &str) -> GpuVendor {
    let id = pnp_device_id.to_ascii_uppercase();
    let lower = name.to_ascii_lowercase();
    if VIRTUAL_HINTS.iter().any(|h| lower.contains(h)) || id.contains("VEN_1414") {
        GpuVendor::Virtual
    } else if id.contains("VEN_10DE") || lower.contains("nvidia") {
        GpuVendor::Nvidia
    } else if id.contains("VEN_1002") || lower.contains("radeon") || lower.contains("amd") {
        GpuVendor::Amd
    } else if id.contains("VEN_8086") || lower.contains("intel") {
        GpuVendor::Intel
    } else {
        GpuVendor::Other
    }
}

/// Integrated graphics: Intel without a discrete Arc model number, or AMD "…Graphics" without "RX".
pub fn looks_integrated(vendor: GpuVendor, name: &str, vram_mb: Option<u64>) -> bool {
    static DISCRETE_ARC: OnceLock<Regex> = OnceLock::new();
    let lower = name.to_ascii_lowercase();
    match vendor {
        GpuVendor::Intel => !DISCRETE_ARC
            .get_or_init(|| Regex::new(r"arc(\(tm\))?\s+[ab]\d{3}").expect("valid pattern"))
            .is_match(&lower),
        GpuVendor::Amd => !lower.contains("rx") && (lower.ends_with("graphics") || vram_mb.is_some_and(|v| v < 512)),
        _ => false,
    }
}

fn real(gpus: &[GpuInfo]) -> impl Iterator<Item = &GpuInfo> {
    gpus.iter().filter(|g| g.vendor != GpuVendor::Virtual)
}

/// The GPU that matters for gaming: the biggest real discrete GPU, else the first real integrated one.
pub fn main_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo> {
    real(gpus)
        .filter(|g| !g.is_integrated)
        .max_by_key(|g| g.vram_mb.unwrap_or(0))
        .or_else(|| real(gpus).next())
}

/// True when a PC has both a real integrated and a real discrete GPU (typical gaming laptop).
pub fn is_hybrid(gpus: &[GpuInfo]) -> bool {
    real(gpus).any(|g| g.is_integrated) && real(gpus).any(|g| !g.is_integrated)
}

/// Windows driver version "32.0.16.1074" → NVIDIA's "610.74".
pub fn nvidia_version(windows_version: &str) -> Option<String> {
    let parts: Vec<&str> = windows_version.trim().split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let third: u32 = parts[2].parse().ok()?;
    let fourth: u32 = parts[3].parse().ok()?;
    if fourth > 9999 {
        return None;
    }
    let digits = format!("{}{:04}", third % 10, fourth);
    Some(format!("{}.{}", &digits[..3], &digits[3..]))
}

/// The driver version as the vendor's own tools show it.
pub fn display_driver_version(gpu: &GpuInfo) -> String {
    match gpu.vendor {
        GpuVendor::Nvidia => nvidia_version(&gpu.driver_version).unwrap_or_else(|| gpu.driver_version.clone()),
        _ => gpu.driver_version.clone(),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kestrel-core gpu`
Expected: `6 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core
git commit -m "feat(core): GPU vendor, integrated detection, main GPU and NVIDIA version

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Health Check part 1 — finding model, hardware, display and GPU checks

**Files:**
- Create: `crates/kestrel-core/src/health.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (add `pub mod health;`)

**Interfaces:**
- Consumes: `SystemSnapshot` and parts (Task 2), `ram::{configured_mts, rated_mts, channel_layout, ChannelLayout}` (Task 3), `gpu::{main_gpu, is_hybrid, display_driver_version}` (Task 4).
- Produces (all `Serialize`, camelCase / kebab-case):
  `pub enum Severity { High, Medium, Low, Info }` (derives `Ord`; High sorts first);
  `pub enum Category { Hardware, Display, Gpu, Windows, Storage, Background, Vanguard, Network }`;
  `pub enum Status { Pass, Problem, Info, Unknown }`;
  `pub enum BiosTopic { Xmp, MemoryGear, Iommu, ResizableBar, UpdateBios }`;
  `pub enum Fix { BiosGuide { topic: BiosTopic }, LaterStage { stage: u8 }, Steps { steps: Vec<String> }, Link { label: String, url: String }, Nothing }` serialized as `{"kind": "bios-guide", "topic": "xmp"}` etc.;
  `pub struct Finding { id: String, category: Category, severity: Severity, status: Status, title: String, detail: String, fix: Fix }`;
  `pub fn run_health_check(s: &SystemSnapshot, today: chrono::NaiveDate) -> Vec<Finding>` — one finding per check, always, in a fixed order. Check ids in this task: `ram-speed`, `ram-channels`, `bios-age`, `display-refresh`, `virtual-display`, `gpu-driver-age`, `valorant-gpu`, `on-battery`.

- [ ] **Step 1: Write the failing tests** in `crates/kestrel-core/src/health.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()
    }

    fn get<'a>(findings: &'a [Finding], id: &str) -> &'a Finding {
        findings.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("no finding {id}"))
    }

    fn display(name: &str, current: u32, max: u32, primary: bool, is_virtual: bool, adapter: &str) -> DisplayInfo {
        DisplayInfo { name: name.into(), adapter: adapter.into(), current_hz: current, max_hz: max, width: 1920, height: 1080, is_primary: primary, is_virtual }
    }

    fn gpu(name: &str, vendor: GpuVendor, integrated: bool, date: Option<&str>) -> GpuInfo {
        GpuInfo { name: name.into(), vendor, driver_version: "32.0.16.1074".into(), driver_date: date.map(Into::into), vram_mb: Some(if integrated { 128 } else { 8192 }), is_integrated: integrated }
    }

    #[test]
    fn owner_pc_part_one_results() {
        let f = run_health_check(&owner_pc_fixture(), today());
        for id in ["ram-speed", "ram-channels", "display-refresh", "virtual-display", "gpu-driver-age", "valorant-gpu", "on-battery"] {
            assert_eq!(get(&f, id).status, Status::Pass, "{id}");
        }
        assert_eq!(get(&f, "bios-age").status, Status::Problem);
        assert_eq!(get(&f, "bios-age").fix, Fix::BiosGuide { topic: BiosTopic::UpdateBios });
    }

    #[test]
    fn ram_below_rated_is_a_high_problem() {
        let mut s = owner_pc_fixture();
        for m in &mut s.memory.modules {
            m.configured_mts = Some(2133);
        }
        let f = run_health_check(&s, today());
        let ram = get(&f, "ram-speed");
        assert_eq!((ram.status, ram.severity), (Status::Problem, Severity::High));
        assert!(ram.title.contains("2133") && ram.title.contains("3600"), "{}", ram.title);
        assert_eq!(ram.fix, Fix::BiosGuide { topic: BiosTopic::Xmp });
    }

    #[test]
    fn single_stick_is_a_problem() {
        let mut s = owner_pc_fixture();
        s.memory.modules.truncate(1);
        assert_eq!(get(&run_health_check(&s, today()), "ram-channels").status, Status::Problem);
    }

    #[test]
    fn empty_snapshot_gives_unknowns_not_problems() {
        let f = run_health_check(&SystemSnapshot::default(), today());
        assert!(!f.is_empty());
        for finding in &f {
            assert_ne!(finding.status, Status::Problem, "{} should not be a problem on an empty snapshot", finding.id);
        }
        assert_eq!(get(&f, "ram-speed").status, Status::Unknown);
        assert_eq!(get(&f, "display-refresh").status, Status::Unknown);
    }

    #[test]
    fn refresh_flags_only_the_slow_real_display() {
        let mut s = owner_pc_fixture();
        s.displays = vec![
            display("\\\\.\\DISPLAY1", 360, 360, true, false, "NVIDIA GeForce RTX 3060 Ti"),
            display("\\\\.\\DISPLAY2", 60, 144, false, false, "NVIDIA GeForce RTX 3060 Ti"),
            display("\\\\.\\DISPLAY3", 30, 240, false, true, "Parsec Virtual Display Adapter"),
        ];
        let refresh = get(&run_health_check(&s, today()), "display-refresh").clone();
        assert_eq!(refresh.status, Status::Problem);
        assert_eq!(refresh.title, "Display 2 runs at 60 Hz but supports 144 Hz");
    }

    #[test]
    fn active_virtual_display_is_a_low_problem() {
        let mut s = owner_pc_fixture();
        s.displays.push(display("\\\\.\\DISPLAY9", 60, 60, false, true, "Parsec Virtual Display Adapter"));
        let v = get(&run_health_check(&s, today()), "virtual-display").clone();
        assert_eq!((v.status, v.severity), (Status::Problem, Severity::Low));
    }

    #[test]
    fn old_driver_links_to_the_vendor_and_virtual_gpus_are_ignored() {
        let mut s = owner_pc_fixture();
        s.gpus = vec![gpu("Parsec Virtual Display Adapter", GpuVendor::Virtual, false, Some("2019-01-01")), gpu("NVIDIA GeForce RTX 3060 Ti", GpuVendor::Nvidia, false, Some("2026-09-15"))];
        assert_eq!(get(&run_health_check(&s, today()), "gpu-driver-age").status, Status::Pass);
        s.gpus[1].driver_date = Some("2026-01-01".into());
        let old = get(&run_health_check(&s, today()), "gpu-driver-age").clone();
        assert_eq!(old.status, Status::Problem);
        assert!(matches!(old.fix, Fix::Link { ref url, .. } if url.contains("nvidia.com")));
    }

    #[test]
    fn monitor_plugged_into_motherboard_on_a_desktop() {
        let mut s = owner_pc_fixture();
        s.gpus = vec![gpu("Intel(R) UHD Graphics 750", GpuVendor::Intel, true, Some("2026-09-01")), gpu("NVIDIA GeForce RTX 3060 Ti", GpuVendor::Nvidia, false, Some("2026-09-15"))];
        s.displays = vec![display("\\\\.\\DISPLAY1", 60, 60, true, false, "Intel(R) UHD Graphics 750")];
        let v = get(&run_health_check(&s, today()), "valorant-gpu").clone();
        assert_eq!((v.status, v.severity), (Status::Problem, Severity::High));
    }

    #[test]
    fn hybrid_laptop_needs_high_performance_preference() {
        let mut s = owner_pc_fixture();
        s.machine.form_factor = FormFactor::Laptop;
        s.machine.on_battery = Some(false);
        s.gpus = vec![gpu("Intel(R) UHD Graphics", GpuVendor::Intel, true, Some("2026-09-01")), gpu("NVIDIA GeForce RTX 4060 Laptop GPU", GpuVendor::Nvidia, false, Some("2026-09-15"))];
        s.displays = vec![display("\\\\.\\DISPLAY1", 165, 165, true, false, "Intel(R) UHD Graphics")];
        assert_eq!(get(&run_health_check(&s, today()), "valorant-gpu").status, Status::Problem);
        s.windows.valorant_gpu_preference = Some(2);
        assert_eq!(get(&run_health_check(&s, today()), "valorant-gpu").status, Status::Pass);
    }

    #[test]
    fn laptop_on_battery_is_a_problem() {
        let mut s = owner_pc_fixture();
        s.machine.form_factor = FormFactor::Laptop;
        s.machine.on_battery = Some(true);
        assert_eq!(get(&run_health_check(&s, today()), "on-battery").status, Status::Problem);
    }

    #[test]
    fn fix_serializes_with_a_kind_tag() {
        let json = serde_json::to_string(&Fix::BiosGuide { topic: BiosTopic::Xmp }).unwrap();
        assert_eq!(json, r#"{"kind":"bios-guide","topic":"xmp"}"#);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kestrel-core health`
Expected: compile errors (types and `run_health_check` not defined).

- [ ] **Step 3: Implement** above the tests in `health.rs`:

```rust
//! The Health Check: one finding per check, in plain words, each with a way to fix it.
use crate::gpu;
use crate::ram::{self, ChannelLayout};
use crate::snapshot::*;
use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity { High, Medium, Low, Info }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category { Hardware, Display, Gpu, Windows, Storage, Background, Vanguard, Network }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status { Pass, Problem, Info, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BiosTopic { Xmp, MemoryGear, Iommu, ResizableBar, UpdateBios }

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Fix {
    BiosGuide { topic: BiosTopic },
    LaterStage { stage: u8 },
    Steps { steps: Vec<String> },
    Link { label: String, url: String },
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub category: Category,
    pub severity: Severity,
    pub status: Status,
    pub title: String,
    pub detail: String,
    pub fix: Fix,
}

/// A check's fixed identity; `pass`/`problem`/`unknown`/`info` fill in the result.
struct Check {
    id: &'static str,
    category: Category,
    severity: Severity,
}

impl Check {
    fn result(&self, status: Status, title: impl Into<String>, detail: impl Into<String>, fix: Fix) -> Finding {
        Finding { id: self.id.into(), category: self.category, severity: self.severity, status, title: title.into(), detail: detail.into(), fix }
    }
    fn pass(&self, title: impl Into<String>, detail: impl Into<String>) -> Finding {
        self.result(Status::Pass, title, detail, Fix::Nothing)
    }
    fn problem(&self, title: impl Into<String>, detail: impl Into<String>, fix: Fix) -> Finding {
        self.result(Status::Problem, title, detail, fix)
    }
    fn unknown(&self, title: impl Into<String>, detail: impl Into<String>) -> Finding {
        self.result(Status::Unknown, title, detail, Fix::Nothing)
    }
}

pub(crate) fn steps(items: &[&str]) -> Fix {
    Fix::Steps { steps: items.iter().map(|s| s.to_string()).collect() }
}

pub(crate) fn parse_date(date: &Option<String>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date.as_deref()?, "%Y-%m-%d").ok()
}

/// Runs every check. Always returns one finding per check, in a fixed order.
pub fn run_health_check(s: &SystemSnapshot, today: NaiveDate) -> Vec<Finding> {
    vec![
        ram_speed(s),
        ram_channels(s),
        bios_age(s),
        display_refresh(s),
        virtual_display(s),
        gpu_driver_age(s, today),
        valorant_gpu(s),
        on_battery(s),
    ]
}

fn ram_speed(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "ram-speed", category: Category::Hardware, severity: Severity::High };
    let modules = &s.memory.modules;
    match (ram::configured_mts(modules), ram::rated_mts(modules)) {
        (Some(now), Some(rated)) if now + 50 < rated => c.problem(
            format!("RAM runs at {now} MT/s, but the kit is rated {rated}"),
            "XMP (Intel) or EXPO (AMD) is probably off in the BIOS. Turning it on is often the biggest single FPS gain. If you lowered the speed on purpose for stability, you can ignore this.",
            Fix::BiosGuide { topic: BiosTopic::Xmp },
        ),
        (Some(now), Some(rated)) => c.pass(format!("RAM runs at {now} MT/s, its rated speed"), format!("The kit is rated {rated} MT/s.")),
        _ => c.unknown(
            "Couldn't tell your RAM's rated speed",
            "Kestrel reads the rated speed from the module part number, and this kit's format isn't one it knows yet. The BIOS guide shows how to check XMP yourself.",
        ),
    }
}

fn ram_channels(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "ram-channels", category: Category::Hardware, severity: Severity::High };
    let modules = &s.memory.modules;
    match ram::channel_layout(modules) {
        ChannelLayout::Dual => c.pass("RAM runs in dual channel", "Your sticks sit on separate memory channels, which doubles memory bandwidth."),
        ChannelLayout::Single if modules.len() == 1 => c.problem(
            "Only one RAM stick, so it runs in single channel",
            "Single-channel memory halves bandwidth and can cost a lot of FPS in CPU-heavy games like VALORANT.",
            steps(&["Buy a second stick that matches the first (same size and speed).", "Install it in the slot your motherboard manual recommends for two sticks (usually A2 and B2)."]),
        ),
        ChannelLayout::Single => c.problem(
            "Your RAM sticks share one memory channel",
            "They're in slots on the same channel, which halves memory bandwidth. Moving them fixes it for free.",
            steps(&["Shut down and unplug the PC.", "Move the sticks to the two slots your motherboard manual recommends for two sticks (usually A2 and B2, the second and fourth from the CPU).", "Start the PC and run the Health Check again."]),
        ),
        ChannelLayout::Unknown => c.unknown("Couldn't tell whether your RAM runs in dual channel", "Windows didn't report which slots the sticks are in."),
    }
}

fn bios_age(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "bios-age", category: Category::Hardware, severity: Severity::Low };
    let cutoff = NaiveDate::from_ymd_opt(2025, 12, 1).expect("valid date");
    match parse_date(&s.board.bios_date) {
        Some(date) if date < cutoff => c.problem(
            format!("BIOS is from {date}"),
            "Riot has asked some players to install a BIOS update released after December 2025 to fix a security flaw. Updates also improve memory stability. Updating resets BIOS settings, so turn XMP back on afterwards.",
            Fix::BiosGuide { topic: BiosTopic::UpdateBios },
        ),
        Some(date) => c.pass(format!("BIOS is from {date}"), "Recent enough for Riot's December 2025 security fix."),
        None => c.unknown("Couldn't read your BIOS date", "Windows didn't report when your BIOS was released."),
    }
}

fn display_label(d: &DisplayInfo) -> String {
    if d.is_primary {
        return "Main display".into();
    }
    let digits: String = d.name.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() { "A display".into() } else { format!("Display {}", digits.trim_start_matches('0')) }
}

fn display_refresh(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "display-refresh", category: Category::Display, severity: Severity::High };
    let real: Vec<&DisplayInfo> = s.displays.iter().filter(|d| !d.is_virtual && d.max_hz > 0).collect();
    if real.is_empty() {
        return c.unknown("Couldn't read your monitor's refresh rate", "Windows didn't report any connected monitors.");
    }
    match real.iter().find(|d| d.current_hz + 1 < d.max_hz) {
        Some(slow) => {
            let who = if real.len() == 1 { "Monitor".to_string() } else { display_label(slow) };
            c.problem(
                format!("{who} runs at {} Hz but supports {} Hz", slow.current_hz, slow.max_hz),
                "VALORANT can only show as many frames per second as the monitor's refresh rate. Choose the highest rate in Windows display settings.",
                steps(&["Open Settings → System → Display → Advanced display.", "Pick the display, then choose the highest refresh rate."]),
            )
        }
        None if real.len() == 1 => c.pass(format!("Monitor runs at its top refresh rate ({} Hz)", real[0].max_hz), ""),
        None => c.pass("All monitors run at their top refresh rate", ""),
    }
}

fn virtual_display(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "virtual-display", category: Category::Display, severity: Severity::Low };
    match s.displays.iter().find(|d| d.is_virtual) {
        Some(v) => c.problem(
            "A virtual display is active",
            format!("{} has added a virtual monitor. While it's active, Windows can render to it or limit your real monitor.", v.adapter),
            steps(&["End any remote-play session (Parsec or similar) before playing.", "If it stays on, turn the virtual display off in that app's settings."]),
        ),
        None => c.pass("No virtual displays active", ""),
    }
}

fn driver_link(vendor: GpuVendor) -> Fix {
    let (label, url) = match vendor {
        GpuVendor::Nvidia => ("Get the latest NVIDIA driver", "https://www.nvidia.com/en-us/drivers/"),
        GpuVendor::Amd => ("Get the latest AMD driver", "https://www.amd.com/en/support/download/drivers.html"),
        GpuVendor::Intel => ("Get the latest Intel driver", "https://www.intel.com/content/www/us/en/download-center/home.html"),
        _ => return steps(&["Download the latest driver from your graphics card maker's website."]),
    };
    Fix::Link { label: label.into(), url: url.into() }
}

fn gpu_driver_age(s: &SystemSnapshot, today: NaiveDate) -> Finding {
    let c = Check { id: "gpu-driver-age", category: Category::Gpu, severity: Severity::Medium };
    let Some(main) = gpu::main_gpu(&s.gpus) else {
        return c.unknown("Couldn't find your graphics card", "Windows didn't report a real graphics card.");
    };
    let version = gpu::display_driver_version(main);
    match parse_date(&main.driver_date) {
        Some(date) if (today - date).num_days() > 183 => c.problem(
            format!("Graphics driver is {} months old", (today - date).num_days() / 30),
            format!("{} driver {version} is from {date}. Newer drivers often fix stutter and add game optimisations.", main.name),
            driver_link(main.vendor),
        ),
        Some(date) => c.pass(format!("Graphics driver is recent ({version}, from {date})"), main.name.clone()),
        None => c.unknown("Couldn't read your graphics driver's date", format!("{} driver {version}.", main.name)),
    }
}

fn valorant_gpu(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "valorant-gpu", category: Category::Gpu, severity: Severity::High };
    let Some(main) = gpu::main_gpu(&s.gpus) else {
        return c.unknown("Couldn't tell which graphics chip VALORANT uses", "Windows didn't report a real graphics card.");
    };
    if s.machine.form_factor != FormFactor::Laptop && !main.is_integrated {
        let primary = s.displays.iter().find(|d| d.is_primary && !d.is_virtual);
        let on_integrated = primary.is_some_and(|d| s.gpus.iter().any(|g| g.is_integrated && g.name == d.adapter));
        if on_integrated {
            return c.problem(
                "Your monitor is plugged into the motherboard",
                format!("It's using the processor's built-in graphics instead of your {}.", main.name),
                steps(&["Shut down the PC.", "Move the monitor cable from the motherboard's video port to a port on the graphics card (lower down at the back).", "Start the PC."]),
            );
        }
        return c.pass(format!("VALORANT runs on your {}", main.name), "");
    }
    if !gpu::is_hybrid(&s.gpus) {
        return c.pass(format!("VALORANT runs on your {}", main.name), "");
    }
    match s.windows.valorant_gpu_preference {
        Some(2) => c.pass(format!("VALORANT is set to use your {}", main.name), ""),
        _ => c.problem(
            "VALORANT may run on the integrated graphics",
            "This PC has two graphics chips. Windows can pick the weaker one unless VALORANT is set to High performance.",
            steps(&["Open Settings → System → Display → Graphics.", "Find VALORANT (add VALORANT-Win64-Shipping.exe if it isn't listed).", "Choose Options → High performance → Save."]),
        ),
    }
}

fn on_battery(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "on-battery", category: Category::Gpu, severity: Severity::Medium };
    match (s.machine.form_factor, s.machine.on_battery) {
        (FormFactor::Laptop, Some(true)) => c.problem(
            "Laptop is running on battery",
            "On battery, Windows and the graphics card cut power, which can cut FPS sharply.",
            steps(&["Plug in the charger before you play."]),
        ),
        (FormFactor::Laptop, Some(false)) => c.pass("Laptop is plugged in", ""),
        (FormFactor::Desktop, _) => c.pass("Running on mains power", ""),
        _ => c.unknown("Couldn't tell whether this PC runs on battery", ""),
    }
}
```

Add `pub mod health;` to `lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kestrel-core health`
Expected: `11 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core
git commit -m "feat(core): Health Check model plus hardware, display and GPU checks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 6: Health Check part 2 — Windows, storage, background, Vanguard and network checks; VALORANT path parsing; score

**Files:**
- Modify: `crates/kestrel-core/src/health.rs`
- Create: `crates/kestrel-core/src/valorant.rs`, `crates/kestrel-core/src/score.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (add `pub mod valorant; pub mod score;`)

**Interfaces:**
- Consumes: everything from Task 5 (`Check`, `steps`, `Fix`, `Finding`, …).
- Produces: 19 more check ids, in this order after Task 5's eight: `power-plan`, `game-mode`, `background-recording`, `pending-restart`, `windows-build`, `valorant-drive`, `drive-space`, `background-cpu`, `startup-apps`, `tpm`, `secure-boot`, `memory-integrity`, `iommu`, `vanguard`, `on-demand`, `wired`, `link-speed`, `adapter-power`, `delivery-optimization` (27 total).
  `valorant::{install_path_from_product_settings(yaml: &str) -> Option<String>, drive_letter(path: &str) -> Option<String>, game_exe_path(install: &str) -> String, DEFAULT_INSTALL: &str, PRODUCT_SETTINGS: &str}` — paths use forward slashes.
  `score::{Score { value: u8, problems: u32, potential_gain: u8 }, score(findings: &[Finding]) -> Score}` — High −15, Medium −8, Low −3, floor 0; `potential_gain = 100 − value`.

- [ ] **Step 1: Write the failing tests** — add to the `tests` module in `health.rs`:

```rust
    #[test]
    fn owner_pc_full_results() {
        let f = run_health_check(&owner_pc_fixture(), today());
        assert_eq!(f.len(), 27);
        let with = |status: Status| -> Vec<&str> { f.iter().filter(|x| x.status == status).map(|x| x.id.as_str()).collect() };
        assert_eq!(with(Status::Problem), vec!["bios-age", "power-plan", "adapter-power"]);
        assert_eq!(with(Status::Info), vec!["memory-integrity", "iommu", "on-demand"]);
        assert!(with(Status::Unknown).is_empty());
    }

    #[test]
    fn every_check_has_a_unique_id() {
        let f = run_health_check(&owner_pc_fixture(), today());
        let mut ids: Vec<&str> = f.iter().map(|x| x.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), f.len());
    }

    #[test]
    fn windows_versions() {
        let mut s = owner_pc_fixture();
        s.os.build = 19045;
        let w10 = get(&run_health_check(&s, today()), "windows-build").clone();
        assert_eq!((w10.status, w10.severity), (Status::Problem, Severity::Low));
        assert!(matches!(w10.fix, Fix::Link { .. }));
        s.os.build = 22631;
        let old11 = get(&run_health_check(&s, today()), "windows-build").clone();
        assert_eq!((old11.status, old11.severity), (Status::Problem, Severity::Medium));
        s.os.build = 18363;
        assert_eq!(get(&run_health_check(&s, today()), "windows-build").severity, Severity::High);
    }

    #[test]
    fn valorant_on_a_hard_drive_is_a_high_problem() {
        let mut s = owner_pc_fixture();
        s.storage.valorant.as_mut().unwrap().media = DriveMedia::Hdd;
        let d = get(&run_health_check(&s, today()), "valorant-drive").clone();
        assert_eq!((d.status, d.severity), (Status::Problem, Severity::High));
    }

    #[test]
    fn valorant_missing_makes_storage_checks_unknown() {
        let mut s = owner_pc_fixture();
        s.storage.valorant = None;
        let f = run_health_check(&s, today());
        assert_eq!(get(&f, "valorant-drive").status, Status::Unknown);
        assert_eq!(get(&f, "drive-space").status, Status::Unknown);
    }

    #[test]
    fn nearly_full_drive() {
        let mut s = owner_pc_fixture();
        s.storage.valorant.as_mut().unwrap().free_gb = 12.0;
        assert_eq!(get(&run_health_check(&s, today()), "drive-space").status, Status::Problem);
    }

    #[test]
    fn busy_background_apps_are_named() {
        let mut s = owner_pc_fixture();
        s.background.busy_processes = Some(vec![
            BusyProcess { name: "chrome.exe".into(), cpu_percent: 12.0 },
            BusyProcess { name: "discord.exe".into(), cpu_percent: 2.0 },
        ]);
        let b = get(&run_health_check(&s, today()), "background-cpu").clone();
        assert_eq!(b.status, Status::Problem);
        assert_eq!(b.title, "1 app is using CPU in the background");
        assert!(b.detail.contains("chrome.exe (12%)") && !b.detail.contains("discord"));
    }

    #[test]
    fn wifi_is_a_problem_and_link_speed_is_skipped() {
        let mut s = owner_pc_fixture();
        s.network.adapters = vec![NetAdapter { name: "Wi-Fi".into(), kind: NetKind::Wifi, link_mbps: Some(866), connected: true, power_saving: Some(false) }];
        let f = run_health_check(&s, today());
        assert_eq!(get(&f, "wired").status, Status::Problem);
        assert_eq!(get(&f, "link-speed").status, Status::Info);
    }

    #[test]
    fn slow_ethernet_link_is_a_problem() {
        let mut s = owner_pc_fixture();
        s.network.adapters[0].link_mbps = Some(100);
        assert_eq!(get(&run_health_check(&s, today()), "link-speed").status, Status::Problem);
    }

    #[test]
    fn on_demand_eligible_when_everything_is_on() {
        let mut s = owner_pc_fixture();
        s.security.vbs_running = Some(true);
        s.security.hvci_running = Some(true);
        s.security.iommu = Some(true);
        let o = get(&run_health_check(&s, today()), "on-demand").clone();
        assert_eq!(o.status, Status::Info);
        assert!(o.title.contains("qualifies"), "{}", o.title);
    }

    #[test]
    fn missing_vanguard_is_a_high_problem() {
        let mut s = owner_pc_fixture();
        s.security.vanguard = ServiceState::NotInstalled;
        let v = get(&run_health_check(&s, today()), "vanguard").clone();
        assert_eq!((v.status, v.severity), (Status::Problem, Severity::High));
    }

    #[test]
    fn tpm_off_is_high_on_windows_11_and_low_on_windows_10() {
        let mut s = owner_pc_fixture();
        s.security.tpm2 = Some(false);
        assert_eq!(get(&run_health_check(&s, today()), "tpm").severity, Severity::High);
        s.os.build = 19045;
        assert_eq!(get(&run_health_check(&s, today()), "tpm").severity, Severity::Low);
    }
```

Create `crates/kestrel-core/src/valorant.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_standard_install_path() {
        let yaml = "product_install_full_path: \"C:/Riot Games/VALORANT/live\"\nproduct_install_root: \"C:/Riot Games\"\n";
        assert_eq!(install_path_from_product_settings(yaml).as_deref(), Some("C:/Riot Games/VALORANT/live"));
    }

    #[test]
    fn reads_a_custom_path_with_backslashes_and_single_quotes() {
        let yaml = "  product_install_full_path: 'D:\\Games\\Riot Games\\VALORANT\\live'\n";
        assert_eq!(install_path_from_product_settings(yaml).as_deref(), Some("D:/Games/Riot Games/VALORANT/live"));
    }

    #[test]
    fn missing_or_empty_path_is_none() {
        assert_eq!(install_path_from_product_settings("product_install_root: \"C:/Riot Games\""), None);
        assert_eq!(install_path_from_product_settings("product_install_full_path: \"\""), None);
    }

    #[test]
    fn drive_letters() {
        assert_eq!(drive_letter("d:/Games/VALORANT").as_deref(), Some("D"));
        assert_eq!(drive_letter("relative/path"), None);
        assert_eq!(drive_letter(""), None);
    }

    #[test]
    fn game_exe_path_is_built_from_the_install_folder() {
        assert_eq!(game_exe_path("C:/Riot Games/VALORANT/live/"), "C:/Riot Games/VALORANT/live/ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe");
    }
}
```

Create `crates/kestrel-core/src/score.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::run_health_check;
    use crate::snapshot::owner_pc_fixture;
    use chrono::NaiveDate;

    #[test]
    fn no_problems_scores_100() {
        assert_eq!(score(&[]), Score { value: 100, problems: 0, potential_gain: 0 });
    }

    #[test]
    fn owner_pc_scores_86() {
        let f = run_health_check(&owner_pc_fixture(), NaiveDate::from_ymd_opt(2026, 10, 9).unwrap());
        assert_eq!(score(&f), Score { value: 86, problems: 3, potential_gain: 14 });
    }

    #[test]
    fn score_never_goes_below_zero() {
        let mut f = run_health_check(&owner_pc_fixture(), NaiveDate::from_ymd_opt(2026, 10, 9).unwrap());
        for finding in &mut f {
            finding.status = Status::Problem;
            finding.severity = Severity::High;
        }
        assert_eq!(score(&f).value, 0);
        assert_eq!(score(&f).potential_gain, 100);
    }
}
```

Add `pub mod valorant; pub mod score;` to `lib.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kestrel-core`
Expected: compile errors in `valorant.rs`/`score.rs`; once those compile, health tests fail on `f.len() == 27`.

- [ ] **Step 3: Implement `valorant.rs`** (above its tests):

```rust
//! Where VALORANT lives on disk, read from Riot's own install records.

/// Riot's install record for VALORANT.
pub const PRODUCT_SETTINGS: &str = "C:/ProgramData/Riot Games/Metadata/valorant.live/valorant.live.product_settings.yaml";
/// Where the Riot Client installs VALORANT by default.
pub const DEFAULT_INSTALL: &str = "C:/Riot Games/VALORANT/live";
const GAME_EXE_RELATIVE: &str = "ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe";

/// Reads `product_install_full_path` from the product settings file. Returns forward slashes.
pub fn install_path_from_product_settings(yaml: &str) -> Option<String> {
    yaml.lines().find_map(|line| {
        let value = line.trim().strip_prefix("product_install_full_path:")?;
        let path = value.trim().trim_matches(|c| c == '"' || c == '\'').replace('\\', "/");
        (!path.is_empty()).then_some(path)
    })
}

/// "D:/Games/…" → "D".
pub fn drive_letter(path: &str) -> Option<String> {
    let mut chars = path.chars();
    let letter = chars.next()?;
    (letter.is_ascii_alphabetic() && chars.next() == Some(':')).then(|| letter.to_ascii_uppercase().to_string())
}

/// Full path of the game's executable inside an install folder.
pub fn game_exe_path(install: &str) -> String {
    format!("{}/{GAME_EXE_RELATIVE}", install.trim_end_matches('/'))
}
```

- [ ] **Step 4: Implement `score.rs`** (above its tests):

```rust
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
    let problems: Vec<&Finding> = findings.iter().filter(|f| f.status == Status::Problem).collect();
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
    Score { value, problems: problems.len() as u32, potential_gain: 100 - value }
}
```

- [ ] **Step 5: Implement the new checks in `health.rs`**

Change `struct Check` to derive copies and add an `info` helper:

```rust
#[derive(Clone, Copy)]
struct Check {
    id: &'static str,
    category: Category,
    severity: Severity,
}
```
and inside `impl Check` add:
```rust
    fn info(&self, title: impl Into<String>, detail: impl Into<String>) -> Finding {
        self.result(Status::Info, title, detail, Fix::Nothing)
    }
```

Replace `run_health_check` with:

```rust
pub fn run_health_check(s: &SystemSnapshot, today: NaiveDate) -> Vec<Finding> {
    vec![
        ram_speed(s),
        ram_channels(s),
        bios_age(s),
        display_refresh(s),
        virtual_display(s),
        gpu_driver_age(s, today),
        valorant_gpu(s),
        on_battery(s),
        power_plan(s),
        game_mode(s),
        background_recording(s),
        pending_restart(s),
        windows_build(s),
        valorant_drive(s),
        drive_space(s),
        background_cpu(s),
        startup_apps(s),
        tpm(s),
        secure_boot(s),
        memory_integrity(s),
        iommu(s),
        vanguard(s),
        on_demand(s),
        wired(s),
        link_speed(s),
        adapter_power(s),
        delivery_optimization(s),
    ]
}
```

Append these functions after `on_battery`:

```rust
const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const ULTIMATE_PERFORMANCE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

fn power_plan(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "power-plan", category: Category::Windows, severity: Severity::Medium };
    let Some(plan) = &s.windows.power_plan else {
        return c.unknown("Couldn't read your power plan", "");
    };
    let guid = plan.guid.to_ascii_lowercase();
    if guid == HIGH_PERFORMANCE || guid == ULTIMATE_PERFORMANCE || plan.name.contains("Kestrel") {
        c.pass(format!("Power plan: {}", plan.name), "")
    } else {
        c.problem(
            format!("Power plan is {}", plan.name),
            "This plan lets the CPU slow down between frames, which hurts 1% lows. Kestrel's own plan arrives in stage 3; until then you can pick High performance in Control Panel → Power Options.",
            Fix::LaterStage { stage: 3 },
        )
    }
}

fn game_mode(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "game-mode", category: Category::Windows, severity: Severity::Medium };
    match s.windows.game_mode {
        Some(true) => c.pass("Game Mode is on", ""),
        Some(false) => c.problem("Game Mode is off", "Game Mode tells Windows to favour the game you're playing. Ryzen X3D chips also need it to pick the right cores.", Fix::LaterStage { stage: 3 }),
        None => c.unknown("Couldn't read the Game Mode setting", ""),
    }
}

fn background_recording(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "background-recording", category: Category::Windows, severity: Severity::Medium };
    match s.windows.background_recording {
        Some(true) => c.problem("Background recording is on", "Xbox Game Bar keeps recording gameplay in the background (\"Record what happened\"), which costs FPS.", Fix::LaterStage { stage: 3 }),
        Some(false) => c.pass("Background recording is off", ""),
        None => c.unknown("Couldn't read the background recording setting", ""),
    }
}

fn pending_restart(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "pending-restart", category: Category::Windows, severity: Severity::Low };
    match s.windows.pending_restart {
        Some(true) => c.problem("Windows is waiting to restart", "Updates stay half-installed until you restart, and can keep working in the background while you play.", steps(&["Save your work and restart the PC."])),
        Some(false) => c.pass("No restart pending", ""),
        None => c.unknown("Couldn't tell whether a restart is pending", ""),
    }
}

fn windows_build(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "windows-build", category: Category::Windows, severity: Severity::Medium };
    let build = s.os.build;
    let update = || steps(&["Open Settings → Windows Update.", "Install the latest version Windows offers."]);
    match build {
        0 => c.unknown("Couldn't read your Windows version", ""),
        b if b >= 26100 => c.pass(format!("Windows 11 {}", s.os.display_version.clone().unwrap_or_default()).trim_end().to_string(), format!("Build {b}.")),
        b if b >= 22000 => c.problem(format!("Windows 11 is out of date (build {b})"), "Riot can require Windows 11 24H2 (build 26100) or newer for some accounts.", update()),
        b if b >= 19045 => Check { severity: Severity::Low, ..c }.problem(
            "You're on Windows 10",
            "VALORANT still runs on Windows 10, but Riot can require Windows 11 for some accounts, and Windows 10's extended security updates for home PCs are reported to end on 13 October 2026. Kestrel runs here but isn't tested on Windows 10.",
            Fix::Link { label: "About upgrading to Windows 11".into(), url: "https://www.microsoft.com/windows/get-windows-11".into() },
        ),
        b if b >= 19041 => c.problem(format!("Windows 10 needs updating (build {b})"), "Riot can require Windows 10 22H2 (build 19045) or newer for some accounts.", update()),
        b => Check { severity: Severity::High, ..c }.problem(format!("Windows is too old for VALORANT (build {b})"), "VALORANT needs Windows 10 build 19041 or newer.", update()),
    }
}

fn valorant_drive(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "valorant-drive", category: Category::Storage, severity: Severity::High };
    let Some(v) = &s.storage.valorant else {
        return c.unknown("Couldn't find VALORANT", "Kestrel looks in Riot's install records. If VALORANT isn't installed, the drive checks are skipped.");
    };
    match v.media {
        DriveMedia::Hdd => c.problem(
            format!("VALORANT is on a hard drive ({}:)", v.drive_letter),
            "Hard drives load maps and textures slowly, which causes stutter. Moving VALORANT to an SSD fixes it.",
            steps(&["In the Riot Client, uninstall VALORANT.", "Install it again and choose a folder on your SSD."]),
        ),
        DriveMedia::Nvme => c.pass(format!("VALORANT is on an NVMe SSD ({}:)", v.drive_letter), ""),
        DriveMedia::Ssd => c.pass(format!("VALORANT is on an SSD ({}:)", v.drive_letter), ""),
        DriveMedia::Unknown => c.unknown(format!("Couldn't tell what kind of drive {}: is", v.drive_letter), ""),
    }
}

fn drive_space(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "drive-space", category: Category::Storage, severity: Severity::Low };
    let Some(v) = &s.storage.valorant else {
        return c.unknown("Couldn't check free space", "VALORANT wasn't found.");
    };
    if v.total_gb <= 0.0 {
        return c.unknown(format!("Couldn't read free space on {}:", v.drive_letter), "");
    }
    if v.free_gb < 20.0 || v.free_gb / v.total_gb < 0.10 {
        c.problem(
            format!("Drive {}: is nearly full ({:.0} GB free)", v.drive_letter, v.free_gb),
            "Windows and VALORANT need free space for updates and the shader cache. Keep at least 20 GB free.",
            steps(&["Open Settings → System → Storage → Cleanup recommendations."]),
        )
    } else {
        c.pass(format!("Drive {}: has {:.0} GB free", v.drive_letter, v.free_gb), "")
    }
}

fn background_cpu(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "background-cpu", category: Category::Background, severity: Severity::Medium };
    let Some(list) = &s.background.busy_processes else {
        return c.unknown("Couldn't measure background activity", "");
    };
    let busy: Vec<&BusyProcess> = list.iter().filter(|p| p.cpu_percent >= 5.0).collect();
    if busy.is_empty() {
        return c.pass("Nothing heavy is running in the background", "");
    }
    let names = busy.iter().map(|p| format!("{} ({:.0}%)", p.name, p.cpu_percent)).collect::<Vec<_>>().join(", ");
    let who = if busy.len() == 1 { "app is" } else { "apps are" };
    c.problem(
        format!("{} {who} using CPU in the background", busy.len()),
        format!("Busiest: {names}. Close them before you play."),
        steps(&["Close these apps before you play, or stop them starting with Windows (Task Manager → Startup apps)."]),
    )
}

fn startup_apps(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "startup-apps", category: Category::Background, severity: Severity::Low };
    match s.windows.startup_app_count {
        Some(n) if n > 10 => c.problem(
            format!("{n} apps start with Windows"),
            "Each one uses memory and CPU in the background. Kestrel's startup manager arrives in stage 4; until then, turn off what you don't need in Task Manager → Startup apps.",
            Fix::LaterStage { stage: 4 },
        ),
        Some(n) => c.pass(format!("{n} apps start with Windows"), ""),
        None => c.unknown("Couldn't count startup apps", ""),
    }
}

/// On Windows 11 Vanguard requires TPM and Secure Boot; on Windows 10 only flagged accounts do.
fn vanguard_requirement(s: &SystemSnapshot, id: &'static str) -> Check {
    let severity = if s.os.build >= 22000 { Severity::High } else { Severity::Low };
    Check { id, category: Category::Vanguard, severity }
}

fn tpm(s: &SystemSnapshot) -> Finding {
    let c = vanguard_requirement(s, "tpm");
    match s.security.tpm2 {
        Some(true) => c.pass("TPM 2.0 is on", ""),
        Some(false) => c.problem(
            "TPM 2.0 is off or missing",
            "VALORANT on Windows 11 needs TPM 2.0. In the BIOS it's often called fTPM (AMD) or PTT (Intel).",
            steps(&["Restart into the BIOS.", "Turn on fTPM (AMD) or Intel PTT (Intel), usually under Advanced or Security.", "Save and restart."]),
        ),
        None => c.unknown("Couldn't check TPM 2.0", "This check needs Kestrel to run as administrator."),
    }
}

fn secure_boot(s: &SystemSnapshot) -> Finding {
    let c = vanguard_requirement(s, "secure-boot");
    match s.security.secure_boot {
        Some(true) => c.pass("Secure Boot is on", ""),
        Some(false) => c.problem(
            "Secure Boot is off",
            "VALORANT on Windows 11 needs Secure Boot.",
            steps(&["Restart into the BIOS.", "In the Boot section, set Secure Boot to Enabled (Windows UEFI mode).", "Save and restart."]),
        ),
        None => c.unknown("Couldn't check Secure Boot", ""),
    }
}

fn memory_integrity(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "memory-integrity", category: Category::Vanguard, severity: Severity::Info };
    match s.security.hvci_running {
        Some(true) => c.info("Memory Integrity is on", "Good for security, and needed for Vanguard's On-Demand mode. It can cost a few percent of FPS; Kestrel's Extreme tier will offer to turn it off, with a warning."),
        Some(false) => c.info("Memory Integrity is off", "VALORANT works fine like this. Riot can require it on for flagged accounts, and Vanguard's On-Demand mode needs it."),
        None => c.unknown("Couldn't check Memory Integrity", ""),
    }
}

fn iommu(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "iommu", category: Category::Vanguard, severity: Severity::Info };
    match s.security.iommu {
        Some(true) => c.info("IOMMU is on", "Needed for Vanguard's On-Demand mode."),
        Some(false) => c.result(
            Status::Info,
            "IOMMU is off",
            "Only needed for Vanguard's On-Demand mode, or if Riot asks for it. It's called VT-d on Intel and IOMMU or AMD-Vi on AMD.",
            Fix::BiosGuide { topic: BiosTopic::Iommu },
        ),
        None => c.unknown("Couldn't check IOMMU", ""),
    }
}

fn vanguard(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "vanguard", category: Category::Vanguard, severity: Severity::High };
    match s.security.vanguard {
        ServiceState::Running => c.pass("Vanguard is installed and running", ""),
        ServiceState::Stopped => c.pass("Vanguard is installed", "It starts when needed."),
        ServiceState::NotInstalled => c.problem(
            "Vanguard isn't installed",
            "VALORANT won't start without Riot Vanguard.",
            steps(&["Open the Riot Client.", "Launch VALORANT; the Riot Client reinstalls Vanguard and asks you to restart."]),
        ),
        ServiceState::Unknown => c.unknown("Couldn't check Vanguard", ""),
    }
}

fn on_demand(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "on-demand", category: Category::Vanguard, severity: Severity::Info };
    if s.os.build == 0 {
        return c.unknown("Couldn't check Vanguard On-Demand eligibility", "");
    }
    let sec = &s.security;
    let needs = [
        ("Windows 11 25H2", Some(s.os.build >= 26200)),
        ("Secure Boot", sec.secure_boot),
        ("TPM 2.0", sec.tpm2),
        ("VBS", sec.vbs_running),
        ("Memory Integrity", sec.hvci_running),
        ("IOMMU", sec.iommu),
    ];
    let missing: Vec<&str> = needs.iter().filter(|(_, v)| *v == Some(false)).map(|(name, _)| *name).collect();
    if !missing.is_empty() {
        c.info("Not eligible for Vanguard On-Demand mode", format!("Missing: {}. On-Demand is optional; VALORANT works without it.", missing.join(", ")))
    } else if needs.iter().any(|(_, v)| v.is_none()) {
        c.unknown("Couldn't check Vanguard On-Demand eligibility", "")
    } else {
        c.info("This PC qualifies for Vanguard On-Demand mode", "On-Demand runs Vanguard only while you play.")
    }
}

fn connected(s: &SystemSnapshot, kind: NetKind) -> Option<&NetAdapter> {
    s.network.adapters.iter().find(|a| a.connected && a.kind == kind)
}

fn wired(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "wired", category: Category::Network, severity: Severity::Medium };
    if connected(s, NetKind::Ethernet).is_some() {
        c.pass("Connected by cable", "")
    } else if connected(s, NetKind::Wifi).is_some() {
        c.problem("Playing over Wi-Fi", "Wi-Fi adds latency spikes and packet loss. Use an Ethernet cable if you can.", steps(&["Connect the PC to the router with an Ethernet cable."]))
    } else {
        c.unknown("Couldn't find an active network connection", "")
    }
}

fn link_speed(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "link-speed", category: Category::Network, severity: Severity::Low };
    match connected(s, NetKind::Ethernet).map(|a| a.link_mbps) {
        Some(Some(mbps)) if mbps < 1000 => c.problem(
            format!("Network link is only {mbps} Mbps"),
            "Gigabit Ethernet should connect at 1000 Mbps or more. A slower link usually means a damaged cable or a bad port.",
            steps(&["Try another Ethernet cable (Cat 5e or better) or another port on the router."]),
        ),
        Some(Some(mbps)) => c.pass(format!("Network link: {mbps} Mbps"), ""),
        Some(None) => c.unknown("Couldn't read the network link speed", ""),
        None => c.info("No cable connected, so link speed isn't checked", ""),
    }
}

fn adapter_power(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "adapter-power", category: Category::Network, severity: Severity::Low };
    let active: Vec<&NetAdapter> = s.network.adapters.iter().filter(|a| a.connected).collect();
    if active.iter().any(|a| a.power_saving == Some(true)) {
        c.problem("Network adapter power saving is on", "Windows can let the adapter doze, which causes small latency spikes. Kestrel's Competitive tier turns this off in stage 4.", Fix::LaterStage { stage: 4 })
    } else if !active.is_empty() && active.iter().all(|a| a.power_saving == Some(false)) {
        c.pass("Network adapter power saving is off", "")
    } else {
        c.unknown("Couldn't check network power saving", "")
    }
}

fn delivery_optimization(s: &SystemSnapshot) -> Finding {
    let c = Check { id: "delivery-optimization", category: Category::Network, severity: Severity::Low };
    match s.network.delivery_optimization_internet {
        Some(true) => c.problem("Windows Update uploads to other PCs over the internet", "Your PC shares updates with strangers, which uses upload bandwidth while you play.", Fix::LaterStage { stage: 4 }),
        Some(false) => c.pass("Windows Update doesn't upload to the internet", ""),
        None => c.unknown("Couldn't read the Windows Update sharing setting", ""),
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p kestrel-core`
Expected: all tests pass (4 snapshot + 11 ram + 6 gpu + 23 health + 5 valorant + 3 score = 52). Then `cargo clippy -p kestrel-core -- -D warnings` → no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/kestrel-core
git commit -m "feat(core): complete the Health Check, VALORANT path parsing and score

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 7: BIOS guide and the full report

**Files:**
- Create: `crates/kestrel-core/src/bios_guide.rs`, `crates/kestrel-core/src/report.rs`
- Modify: `crates/kestrel-core/src/lib.rs`

**Interfaces:**
- Consumes: `SystemSnapshot` (Task 2), `health::{BiosTopic, Finding, run_health_check}` (Tasks 5–6), `score::{Score, score}` (Task 6).
- Produces: `pub enum BoardVendor { Asus, Msi, Gigabyte, Asrock, Other }`; `pub struct GuideLink { label: String, url: String }`; `pub struct GuideSection { topic: BiosTopic, title: String, why: String, steps: Vec<String>, caution: Option<String>, link: Option<GuideLink> }`; `pub struct BiosGuide { vendor: BoardVendor, board: String, enter_key: String, tip: String, sections: Vec<GuideSection> }`; `pub fn board_vendor(manufacturer: &str) -> BoardVendor`; `pub fn build_guide(s: &SystemSnapshot) -> BiosGuide` (section order: Xmp, MemoryGear (11th-gen Intel only), UpdateBios, Iommu, ResizableBar);
  `pub struct Report { app_version: String, snapshot: SystemSnapshot, findings: Vec<Finding>, score: Score, guide: BiosGuide }`; `pub fn build_report(snapshot: SystemSnapshot, today: NaiveDate, app_version: &str) -> Report`. Re-exported from the crate root: `kestrel_core::{build_report, Report}`.

- [ ] **Step 1: Write the failing tests**

`crates/kestrel-core/src/bios_guide.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{owner_pc_fixture, CpuVendor};

    fn section(g: &BiosGuide, topic: BiosTopic) -> &GuideSection {
        g.sections.iter().find(|s| s.topic == topic).unwrap()
    }

    #[test]
    fn owner_pc_gets_asus_extreme_tweaker_steps() {
        let g = build_guide(&owner_pc_fixture());
        assert_eq!(g.vendor, BoardVendor::Asus);
        assert_eq!(g.board, "ROG MAXIMUS XIII HERO");
        assert_eq!(g.enter_key, "Del");
        let topics: Vec<BiosTopic> = g.sections.iter().map(|s| s.topic).collect();
        assert_eq!(topics, vec![BiosTopic::Xmp, BiosTopic::MemoryGear, BiosTopic::UpdateBios, BiosTopic::Iommu, BiosTopic::ResizableBar]);
        let xmp = section(&g, BiosTopic::Xmp).steps.join(" ");
        assert!(xmp.contains("Extreme Tweaker") && xmp.contains("XMP I"), "{xmp}");
        assert!(section(&g, BiosTopic::MemoryGear).steps.join(" ").contains("1:1"));
    }

    #[test]
    fn asus_non_rog_board_uses_ai_tweaker() {
        let mut s = owner_pc_fixture();
        s.board.product = "PRIME Z590-A".into();
        assert!(section(&build_guide(&s), BiosTopic::Xmp).steps.join(" ").contains("Ai Tweaker"));
    }

    #[test]
    fn amd_uses_expo_and_skips_gear() {
        let mut s = owner_pc_fixture();
        s.cpu.vendor = CpuVendor::Amd;
        s.cpu.name = "AMD Ryzen 7 7800X3D 8-Core Processor".into();
        let g = build_guide(&s);
        let xmp = section(&g, BiosTopic::Xmp);
        assert_eq!(xmp.title, "Turn on EXPO (or DOCP)");
        assert!(xmp.steps.join(" ").contains("EXPO I"));
        assert!(g.sections.iter().all(|s| s.topic != BiosTopic::MemoryGear));
        assert!(section(&g, BiosTopic::Iommu).steps.join(" ").contains("AMD CBS"));
    }

    #[test]
    fn vendor_detection() {
        assert_eq!(board_vendor("ASUSTeK COMPUTER INC."), BoardVendor::Asus);
        assert_eq!(board_vendor("Micro-Star International Co., Ltd."), BoardVendor::Msi);
        assert_eq!(board_vendor("Gigabyte Technology Co., Ltd."), BoardVendor::Gigabyte);
        assert_eq!(board_vendor("ASRock"), BoardVendor::Asrock);
        assert_eq!(board_vendor("Dell Inc."), BoardVendor::Other);
    }

    #[test]
    fn every_vendor_and_cpu_gets_complete_sections() {
        for maker in ["ASUSTeK COMPUTER INC.", "Micro-Star International Co., Ltd.", "Gigabyte Technology Co., Ltd.", "ASRock", "Dell Inc."] {
            for (vendor, name) in [(CpuVendor::Intel, "11th Gen Intel(R) Core(TM) i5-11400F"), (CpuVendor::Amd, "AMD Ryzen 5 5600X")] {
                let mut s = owner_pc_fixture();
                s.board.manufacturer = maker.into();
                s.cpu.vendor = vendor;
                s.cpu.name = name.into();
                for sec in build_guide(&s).sections {
                    assert!(sec.steps.len() >= 2, "{maker} {name} {:?}", sec.topic);
                    assert!(!sec.title.is_empty() && !sec.why.is_empty());
                }
            }
        }
    }

    #[test]
    fn update_section_names_the_board_and_links_support() {
        let g = build_guide(&owner_pc_fixture());
        let update = section(&g, BiosTopic::UpdateBios);
        assert!(update.steps[0].contains("ROG MAXIMUS XIII HERO"));
        assert!(update.link.as_ref().unwrap().url.contains("asus.com"));
    }

    #[test]
    fn unknown_board_name_reads_naturally() {
        let mut s = owner_pc_fixture();
        s.board.product = String::new();
        assert_eq!(build_guide(&s).board, "your motherboard");
    }
}
```

`crates/kestrel-core/src/report.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    #[test]
    fn owner_report_has_everything() {
        let r = build_report(owner_pc_fixture(), NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(), "0.1.0");
        assert_eq!(r.findings.len(), 27);
        assert_eq!(r.score.value, 86);
        assert_eq!(r.app_version, "0.1.0");
        let json = serde_json::to_string(&r).unwrap();
        for key in ["\"appVersion\"", "\"snapshot\"", "\"findings\"", "\"score\"", "\"guide\""] {
            assert!(json.contains(key), "missing {key}");
        }
    }
}
```

Set `crates/kestrel-core/src/lib.rs` to:
```rust
//! Kestrel core: the data model and every decision Kestrel makes. Pure Rust, no OS calls.
pub mod bios_guide;
pub mod gpu;
pub mod health;
pub mod ram;
pub mod report;
pub mod score;
pub mod snapshot;
pub mod valorant;

pub use report::{build_report, Report};
pub use snapshot::*;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kestrel-core`
Expected: compile errors (`build_guide`, `build_report` not defined).

- [ ] **Step 3: Implement `bios_guide.rs`** (above its tests):

```rust
//! The BIOS guide: step-by-step help for settings Windows can't change, in this board's words.
use crate::health::BiosTopic;
use crate::snapshot::{CpuVendor, FormFactor, SystemSnapshot};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoardVendor { Asus, Msi, Gigabyte, Asrock, Other }

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideLink { pub label: String, pub url: String }

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideSection {
    pub topic: BiosTopic,
    pub title: String,
    pub why: String,
    pub steps: Vec<String>,
    pub caution: Option<String>,
    pub link: Option<GuideLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosGuide {
    pub vendor: BoardVendor,
    pub board: String,
    pub enter_key: String,
    pub tip: String,
    pub sections: Vec<GuideSection>,
}

pub fn board_vendor(manufacturer: &str) -> BoardVendor {
    let m = manufacturer.to_ascii_lowercase();
    if m.contains("asus") {
        BoardVendor::Asus
    } else if m.contains("micro-star") || m.contains("msi") {
        BoardVendor::Msi
    } else if m.contains("gigabyte") {
        BoardVendor::Gigabyte
    } else if m.contains("asrock") {
        BoardVendor::Asrock
    } else {
        BoardVendor::Other
    }
}

struct Ctx<'a> {
    vendor: BoardVendor,
    board: &'a str,
    amd: bool,
    tweaker: &'static str,
    profile: &'static str,
}

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn build_guide(s: &SystemSnapshot) -> BiosGuide {
    let vendor = board_vendor(&s.board.manufacturer);
    let product = s.board.product.trim();
    let board = if product.is_empty() { "your motherboard" } else { product };
    let upper = product.to_ascii_uppercase();
    let rog = ["MAXIMUS", "CROSSHAIR", "RAMPAGE", "ZENITH"].iter().any(|n| upper.contains(n));
    let amd = s.cpu.vendor == CpuVendor::Amd;
    let ctx = Ctx { vendor, board, amd, tweaker: if rog { "Extreme Tweaker" } else { "Ai Tweaker" }, profile: if amd { "EXPO" } else { "XMP" } };

    let mut sections = vec![xmp(&ctx)];
    if s.cpu.vendor == CpuVendor::Intel && s.cpu.name.contains("11th Gen") {
        sections.push(memory_gear(&ctx));
    }
    sections.extend([update_bios(&ctx), iommu(&ctx), resizable_bar(&ctx)]);

    BiosGuide { vendor, board: board.into(), enter_key: enter_key(vendor, s.machine.form_factor).into(), tip: tip(vendor).into(), sections }
}

fn enter_key(vendor: BoardVendor, form: FormFactor) -> &'static str {
    match (vendor, form) {
        (BoardVendor::Asus, FormFactor::Laptop) => "F2",
        (BoardVendor::Asus | BoardVendor::Msi | BoardVendor::Gigabyte, _) => "Del",
        (BoardVendor::Asrock, _) => "F2 or Del",
        (BoardVendor::Other, _) => "Del, F2 or F10 (the start-up screen shows which)",
    }
}

fn tip(vendor: BoardVendor) -> &'static str {
    match vendor {
        BoardVendor::Asus => "Press F7 to switch between EZ Mode and Advanced Mode. The Search button at the top finds any setting by name.",
        BoardVendor::Msi => "Press F7 to switch between EZ Mode and Advanced mode.",
        BoardVendor::Gigabyte => "Press F2 to switch between Easy Mode and Advanced Mode.",
        BoardVendor::Asrock => "Press F6 to switch between Easy Mode and Advanced Mode.",
        BoardVendor::Other => "Menu names differ between makers. Look for each setting by name in the BIOS menus.",
    }
}

fn xmp(c: &Ctx) -> GuideSection {
    let profile = c.profile;
    let steps = match c.vendor {
        BoardVendor::Asus => vec![
            "Restart and press Del to open the BIOS.".into(),
            "Press F7 for Advanced Mode.".into(),
            format!("Open the {} tab.", c.tweaker),
            if c.amd { "Set Ai Overclock Tuner to EXPO I (older boards call it D.O.C.P.).".into() } else { "Set Ai Overclock Tuner to XMP I.".into() },
            "Check that DRAM Frequency shows your kit's speed, then press F10 and choose Yes.".into(),
        ],
        BoardVendor::Msi => vec![
            "Restart and press Del to open the BIOS.".into(),
            format!("On the EZ Mode screen, switch on {profile} and pick Profile 1."),
            "Press F10 and choose Yes.".into(),
        ],
        BoardVendor::Gigabyte => vec![
            "Restart and press Del to open the BIOS.".into(),
            "Press F2 for Advanced Mode if needed, then open the Tweaker tab.".into(),
            if c.amd { "Set Extreme Memory Profile (X.M.P.) or EXPO to Profile1.".into() } else { "Set Extreme Memory Profile (X.M.P.) to Profile1.".into() },
            "Press F10 and choose Yes.".into(),
        ],
        BoardVendor::Asrock => v(&[
            "Restart and press F2 or Del to open the BIOS.",
            "Open OC Tweaker → DRAM Configuration.",
            "Set Load XMP Setting (EXPO on newer AMD boards) to Profile 1.",
            "Press F10 and choose Yes.",
        ]),
        BoardVendor::Other => vec![
            "Restart and open the BIOS (usually Del or F2 at start-up).".into(),
            format!("Find the memory overclocking or {profile} setting."),
            "Choose Profile 1, then save and exit.".into(),
        ],
    };
    GuideSection {
        topic: BiosTopic::Xmp,
        title: if c.amd { "Turn on EXPO (or DOCP)".into() } else { "Turn on XMP".into() },
        why: format!("Most RAM starts at a safe, slow default speed. {profile} runs it at the speed on the box, which can add a lot of FPS in CPU-heavy games like VALORANT."),
        steps,
        caution: Some("If the PC won't start, it usually retries with safe settings by itself. If it doesn't, clear the BIOS settings (Clear CMOS) and try a lower speed.".into()),
        link: None,
    }
}

fn memory_gear(c: &Ctx) -> GuideSection {
    let steps = match c.vendor {
        BoardVendor::Asus => vec![
            format!("On the {} tab, set Memory Controller : DRAM Frequency Ratio to 1:1.", c.tweaker),
            "If the PC is unstable at your kit's speed in 1:1, set DRAM Frequency to DDR4-3200 and keep 1:1.".into(),
            "Press F10 and choose Yes.".into(),
        ],
        _ => v(&[
            "Look for Gear Mode or Memory Controller : DRAM Frequency Ratio in the memory or overclocking settings.",
            "Choose Gear 1 (1:1).",
            "If the PC is unstable, lower DRAM Frequency to DDR4-3200 and keep Gear 1.",
            "Save and exit (usually F10).",
        ]),
    };
    GuideSection {
        topic: BiosTopic::MemoryGear,
        title: "Keep the memory controller in Gear 1".into(),
        why: "On 11th-gen Intel CPUs, Gear 1 runs the memory controller at full speed. Gear 2 adds delay that usually costs more FPS than the extra RAM speed gains.".into(),
        steps,
        caution: Some("Crashes or blue screens after this change mean the memory can't hold that speed in Gear 1. Lower the frequency.".into()),
        link: None,
    }
}

fn update_bios(c: &Ctx) -> GuideSection {
    let (maker, tool, url) = match c.vendor {
        BoardVendor::Asus => ("ASUS", "Tool → ASUS EZ Flash 3 Utility", Some("https://www.asus.com/support/download-center/")),
        BoardVendor::Msi => ("MSI", "M-FLASH", Some("https://www.msi.com/support/download")),
        BoardVendor::Gigabyte => ("GIGABYTE", "Q-Flash (F8)", Some("https://www.gigabyte.com/Support")),
        BoardVendor::Asrock => ("ASRock", "Tool → Instant Flash", Some("https://www.asrock.com/support/index.asp")),
        BoardVendor::Other => ("your motherboard maker", "the BIOS's built-in update tool", None),
    };
    GuideSection {
        topic: BiosTopic::UpdateBios,
        title: "Update the BIOS".into(),
        why: "Riot has asked some players to install BIOS updates released after December 2025 to fix a security flaw. Updates also improve memory compatibility.".into(),
        steps: vec![
            format!("Download the latest BIOS for {} from {maker} support and copy it to a USB stick.", c.board),
            format!("In the BIOS, open {tool}."),
            "Pick the file on the USB stick and confirm.".into(),
        ],
        caution: Some(format!("Don't turn off the PC while it updates. Updating resets BIOS settings, so turn {} back on afterwards.", c.profile)),
        link: url.map(|u| GuideLink { label: format!("Open {maker} support"), url: u.into() }),
    }
}

fn iommu(c: &Ctx) -> GuideSection {
    let steps = match (c.vendor, c.amd) {
        (BoardVendor::Asus, false) => v(&["Press F7 for Advanced Mode.", "Open Advanced → System Agent (SA) Configuration.", "Set VT-d to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Msi, false) => v(&["Press F7 for Advanced mode.", "Open OC → CPU Features.", "Set Intel VT-D Tech to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Gigabyte, false) => v(&["Press F2 for Advanced Mode.", "Open Settings → Miscellaneous.", "Set VT-d to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Asrock, false) => v(&["Open Advanced → Chipset Configuration.", "Set VT-d to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Other, false) => v(&["Find VT-d in the chipset or CPU settings.", "Set it to Enabled, then save and exit."]),
        (BoardVendor::Asus, true) => v(&["Press F7 for Advanced Mode.", "Open Advanced → AMD CBS → NBIO Common Options.", "Set IOMMU to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Msi, true) => v(&["Press F7 for Advanced mode.", "Open Settings → Advanced → AMD CBS → NBIO Common Options.", "Set IOMMU to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Gigabyte, true) => v(&["Press F2 for Advanced Mode.", "Open Settings → Miscellaneous.", "Set IOMMU to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Asrock, true) => v(&["Open Advanced → AMD CBS → NBIO Common Options.", "Set IOMMU to Enabled.", "Press F10 and choose Yes."]),
        (BoardVendor::Other, true) => v(&["Find IOMMU (sometimes called AMD-Vi) in the CPU or chipset settings.", "Set it to Enabled, then save and exit."]),
    };
    GuideSection {
        topic: BiosTopic::Iommu,
        title: "Turn on IOMMU (only if you need it)".into(),
        why: "Vanguard's optional On-Demand mode needs it, and Riot can ask for it on some accounts. It doesn't change FPS.".into(),
        steps,
        caution: Some("Menu names vary between boards and BIOS versions. If you can't find it, search the BIOS for VT-d or IOMMU.".into()),
        link: None,
    }
}

fn resizable_bar(c: &Ctx) -> GuideSection {
    let steps = match c.vendor {
        BoardVendor::Asus => v(&["Press F7 for Advanced Mode.", "Open Advanced → PCI Subsystem Settings.", "Set Above 4G Decoding to Enabled, then Re-Size BAR Support to Auto.", "Press F10 and choose Yes."]),
        BoardVendor::Msi => v(&["Press F7 for Advanced mode.", "Open Settings → Advanced → PCI Subsystem Settings.", "Enable Above 4G memory, then Re-Size BAR Support.", "Press F10 and choose Yes."]),
        BoardVendor::Gigabyte => v(&["Press F2 for Advanced Mode.", "Open Settings → IO Ports.", "Set Above 4G Decoding and Re-Size BAR Support to Enabled.", "Press F10 and choose Yes."]),
        BoardVendor::Asrock => v(&["Open Advanced → Chipset Configuration.", "Enable Above 4G Decoding and Re-Size BAR Support (C.A.M. on some AMD boards).", "Press F10 and choose Yes."]),
        BoardVendor::Other => v(&["Find Above 4G Decoding and Resizable BAR in the PCI or chipset settings.", "Enable both, then save and exit."]),
    };
    GuideSection {
        topic: BiosTopic::ResizableBar,
        title: "Resizable BAR (optional)".into(),
        why: "Lets the CPU reach all of the graphics card's memory at once. VALORANT gains little, but some other games do.".into(),
        steps,
        caution: Some("Windows must start in UEFI mode (CSM off) for this to work.".into()),
        link: None,
    }
}
```

- [ ] **Step 4: Implement `report.rs`** (above its tests):

```rust
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
    Report { app_version: app_version.into(), snapshot, findings, score, guide }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p kestrel-core && cargo clippy -p kestrel-core -- -D warnings`
Expected: 60 tests pass (52 + 7 guide + 1 report); clippy clean.

- [ ] **Step 6: Commit**

```bash
git add crates/kestrel-core
git commit -m "feat(core): BIOS guide per board maker and the full report

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 8: Probe crate — parsing helpers, mock probe, report CLI

**Files:**
- Create: `crates/kestrel-probe/Cargo.toml`, `crates/kestrel-probe/src/lib.rs`, `crates/kestrel-probe/src/parse.rs`, `crates/kestrel-probe/src/mock.rs`, `crates/kestrel-probe/src/bin/kestrel-probe.rs`, `crates/kestrel-probe/tests/smoke.rs`
- Modify: root `Cargo.toml` (members += `crates/kestrel-probe`)

**Interfaces:**
- Consumes: `kestrel_core::{SystemSnapshot, owner_pc_fixture, build_report, Report, ProbeError, …}`.
- Produces: `kestrel_probe::collect_snapshot() -> SystemSnapshot` (never panics; failures go to `probe_errors`); `kestrel_probe::collect_report(app_version: &str) -> Report`; `kestrel_probe::parse::{cim_date, power_plan_name, gpu_preference, delivery_optimization_internet, startup_entry_enabled, net_kind, drive_media, form_factor, cpu_vendor, ddr_generation, busy_processes}` (signatures below); binary `kestrel-probe [--pretty]` printing the report JSON.

- [ ] **Step 1: Create the crate**

```bash
cargo new --lib crates/kestrel-probe
cd crates/kestrel-probe
cargo add kestrel-core --path ../kestrel-core
cargo add serde_json
cargo add chrono --no-default-features --features std,clock
cargo add sysinfo
cd ../..
```
Set the `[package]` block of `crates/kestrel-probe/Cargo.toml` to use workspace values:
```toml
[package]
name = "kestrel-probe"
edition.workspace = true
license.workspace = true
version.workspace = true
```
Add `"crates/kestrel-probe"` to the root `members`.

- [ ] **Step 2: Write the failing tests** — `crates/kestrel-probe/src/parse.rs` (tests only):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cim_dates() {
        assert_eq!(cim_date("20240520000000.000000+000").as_deref(), Some("2024-05-20"));
        assert_eq!(cim_date("2024"), None);
        assert_eq!(cim_date("20241340000000.000000+000"), None);
        assert_eq!(cim_date(""), None);
    }

    #[test]
    fn power_plan_names() {
        assert_eq!(power_plan_name("@%SystemRoot%\\system32\\powrprof.dll,-14,Balanced"), "Balanced");
        assert_eq!(power_plan_name("Kestrel"), "Kestrel");
        assert_eq!(power_plan_name("  My plan, tuned "), "My plan, tuned");
    }

    #[test]
    fn gpu_preferences() {
        assert_eq!(gpu_preference("GpuPreference=2;"), Some(2));
        assert_eq!(gpu_preference("SwapEffectUpgradeEnable=1;GpuPreference=1;"), Some(1));
        assert_eq!(gpu_preference("SwapEffectUpgradeEnable=1;"), None);
    }

    #[test]
    fn delivery_optimization_modes() {
        assert!(delivery_optimization_internet(Some(3)));
        assert!(!delivery_optimization_internet(Some(1)));
        assert!(!delivery_optimization_internet(None));
    }

    #[test]
    fn startup_approved_bytes() {
        assert!(startup_entry_enabled(None));
        assert!(startup_entry_enabled(Some(&[0x02, 0, 0, 0])));
        assert!(startup_entry_enabled(Some(&[0x06, 0, 0, 0])));
        assert!(!startup_entry_enabled(Some(&[0x03, 0, 0, 0])));
    }

    #[test]
    fn hardware_codes() {
        assert_eq!(net_kind(14), NetKind::Ethernet);
        assert_eq!(net_kind(9), NetKind::Wifi);
        assert_eq!(net_kind(0), NetKind::Other);
        assert_eq!(drive_media(Some(17), Some(4)), DriveMedia::Nvme);
        assert_eq!(drive_media(Some(11), Some(4)), DriveMedia::Ssd);
        assert_eq!(drive_media(Some(11), Some(3)), DriveMedia::Hdd);
        assert_eq!(drive_media(None, None), DriveMedia::Unknown);
        assert_eq!(form_factor(Some(2), false), FormFactor::Laptop);
        assert_eq!(form_factor(Some(1), true), FormFactor::Laptop);
        assert_eq!(form_factor(Some(1), false), FormFactor::Desktop);
        assert_eq!(cpu_vendor("GenuineIntel"), CpuVendor::Intel);
        assert_eq!(cpu_vendor("AuthenticAMD"), CpuVendor::Amd);
        assert_eq!(ddr_generation(26), Some(4));
        assert_eq!(ddr_generation(34), Some(5));
        assert_eq!(ddr_generation(0), None);
    }

    #[test]
    fn busy_process_list() {
        let samples = vec![
            ("chrome.exe".to_string(), 160.0),
            ("System Idle Process".to_string(), 1500.0),
            ("Kestrel.exe".to_string(), 80.0),
            ("explorer.exe".to_string(), 4.0),
        ];
        let busy = busy_processes(samples, 16);
        assert_eq!(busy.len(), 1);
        assert_eq!(busy[0].name, "chrome.exe");
        assert!((busy[0].cpu_percent - 10.0).abs() < 0.01);
    }
}
```

`crates/kestrel-probe/tests/smoke.rs`:
```rust
#[test]
fn report_has_every_check() {
    let report = kestrel_probe::collect_report("test");
    assert_eq!(report.findings.len(), 27);
    assert_eq!(report.app_version, "test");
}

#[cfg(not(windows))]
#[test]
fn mock_says_it_is_a_sample() {
    let s = kestrel_probe::collect_snapshot();
    assert!(s.probe_errors.iter().any(|e| e.probe == "mock"));
    assert_eq!(s.cpu.threads, 16);
}

#[cfg(windows)]
#[test]
fn windows_probe_reads_the_basics() {
    let s = kestrel_probe::collect_snapshot();
    assert!(!s.cpu.name.is_empty(), "probe errors: {:?}", s.probe_errors);
    assert!(s.os.build >= 19041, "probe errors: {:?}", s.probe_errors);
    assert!(s.memory.total_mb > 0, "probe errors: {:?}", s.probe_errors);
}
```

`crates/kestrel-probe/src/lib.rs`:
```rust
//! Kestrel probe: looks at the PC and fills a `SystemSnapshot`. Read-only by design.
pub mod parse;

#[cfg(not(windows))]
mod mock;

use kestrel_core::{Report, SystemSnapshot};

/// Looks at this PC. Never panics: anything unreadable becomes `None` plus a `probe_errors` entry.
pub fn collect_snapshot() -> SystemSnapshot {
    #[cfg(not(windows))]
    {
        mock::collect()
    }
    #[cfg(windows)]
    {
        unimplemented!("Windows probes arrive in Task 9")
    }
}

/// Snapshot, Health Check, score and BIOS guide, dated today.
pub fn collect_report(app_version: &str) -> Report {
    kestrel_core::build_report(collect_snapshot(), chrono::Local::now().date_naive(), app_version)
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p kestrel-probe`
Expected: compile errors (`parse` functions and `mock` missing).

- [ ] **Step 4: Implement `parse.rs`** (above its tests):

```rust
//! Pure helpers that turn raw Windows values into Kestrel's model. No OS calls, so they're tested anywhere.
use kestrel_core::{BusyProcess, CpuVendor, DriveMedia, FormFactor, NetKind};

/// CIM datetime "20240520000000.000000+000" → "2024-05-20".
pub fn cim_date(raw: &str) -> Option<String> {
    let digits = raw.get(0..8)?;
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (&digits[0..4], &digits[4..6], &digits[6..8]);
    chrono::NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)?;
    Some(format!("{y}-{m}-{d}"))
}

/// Built-in plan names are stored as "@…powrprof.dll,-14,Balanced"; custom names are plain.
pub fn power_plan_name(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with('@') {
        raw.rsplit(',').next().unwrap_or(raw).trim().to_string()
    } else {
        raw.to_string()
    }
}

/// "SwapEffectUpgradeEnable=1;GpuPreference=2;" → Some(2).
pub fn gpu_preference(raw: &str) -> Option<u8> {
    raw.split(';').find_map(|part| part.trim().strip_prefix("GpuPreference=")?.parse().ok())
}

/// Delivery Optimization download mode 3 means "PCs on my local network and on the internet".
pub fn delivery_optimization_internet(mode: Option<u32>) -> bool {
    mode == Some(3)
}

/// StartupApproved entries: even first byte (0x02, 0x06) = enabled, odd (0x03, 0x07) = disabled. Missing = enabled.
pub fn startup_entry_enabled(approved: Option<&[u8]>) -> bool {
    approved.and_then(|bytes| bytes.first()).is_none_or(|first| first % 2 == 0)
}

/// MSFT_NetAdapter.NdisPhysicalMedium: 14 = 802.3 Ethernet, 9 = native 802.11 Wi-Fi.
pub fn net_kind(ndis_physical_medium: u64) -> NetKind {
    match ndis_physical_medium {
        14 => NetKind::Ethernet,
        9 => NetKind::Wifi,
        _ => NetKind::Other,
    }
}

/// MSFT_PhysicalDisk: BusType 17 = NVMe; MediaType 4 = SSD, 3 = HDD.
pub fn drive_media(bus_type: Option<u64>, media_type: Option<u64>) -> DriveMedia {
    if bus_type == Some(17) {
        return DriveMedia::Nvme;
    }
    match media_type {
        Some(4) => DriveMedia::Ssd,
        Some(3) => DriveMedia::Hdd,
        _ => DriveMedia::Unknown,
    }
}

/// Win32_ComputerSystem.PCSystemType 2 = mobile. A battery also means laptop.
pub fn form_factor(pc_system_type: Option<u64>, has_battery: bool) -> FormFactor {
    if pc_system_type == Some(2) || has_battery { FormFactor::Laptop } else { FormFactor::Desktop }
}

pub fn cpu_vendor(manufacturer: &str) -> CpuVendor {
    if manufacturer.contains("Intel") {
        CpuVendor::Intel
    } else if manufacturer.contains("AMD") {
        CpuVendor::Amd
    } else {
        CpuVendor::Other
    }
}

/// SMBIOSMemoryType: 24 = DDR3, 26 = DDR4, 34 = DDR5.
pub fn ddr_generation(smbios_memory_type: u64) -> Option<u8> {
    match smbios_memory_type {
        24 => Some(3),
        26 => Some(4),
        34 => Some(5),
        _ => None,
    }
}

/// Kestrel's own work (its window, its probe, and the WMI host answering its queries) isn't background load.
const NOT_BACKGROUND: [&str; 6] = ["system idle process", "idle", "kestrel.exe", "kestrel-probe.exe", "wmiprvse.exe", "msedgewebview2.exe"];

/// Per-process CPU samples (100 = one full core) → share of the whole CPU, ≥ 5%, busiest first, at most 5.
pub fn busy_processes(samples: Vec<(String, f32)>, logical_cpus: u32) -> Vec<BusyProcess> {
    let cores = logical_cpus.max(1) as f32;
    let mut busy: Vec<BusyProcess> = samples
        .into_iter()
        .filter(|(name, _)| !NOT_BACKGROUND.contains(&name.to_ascii_lowercase().as_str()))
        .map(|(name, usage)| BusyProcess { name, cpu_percent: usage / cores })
        .filter(|p| p.cpu_percent >= 5.0)
        .collect();
    busy.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
    busy.truncate(5);
    busy
}
```

- [ ] **Step 5: Implement `mock.rs`, the CLI**

`crates/kestrel-probe/src/mock.rs`:
```rust
//! Off Windows (development on a Mac), the probe returns the owner's PC as sample data.
use kestrel_core::{owner_pc_fixture, ProbeError, SystemSnapshot};

pub fn collect() -> SystemSnapshot {
    let mut s = owner_pc_fixture();
    s.collected_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    s.probe_errors.push(ProbeError {
        probe: "mock".into(),
        message: "Not running on Windows, so this is the sample PC from fixtures/owner-pc.json.".into(),
    });
    s
}
```
Note: `chrono::Utc::now()` needs the `clock` feature added in Step 1.

`crates/kestrel-probe/src/bin/kestrel-probe.rs`:
```rust
//! Prints Kestrel's full report as JSON. Used by CI and to regenerate the UI's sample data.
fn main() {
    let pretty = std::env::args().any(|a| a == "--pretty");
    let report = kestrel_probe::collect_report(env!("CARGO_PKG_VERSION"));
    let json = if pretty { serde_json::to_string_pretty(&report) } else { serde_json::to_string(&report) };
    println!("{}", json.expect("report serializes"));
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p kestrel-probe && cargo run -q -p kestrel-probe --bin kestrel-probe -- --pretty | head -5`
Expected: `7 passed` (parse) + `2 passed` (smoke); the CLI prints `{`, `"appVersion": "0.1.0",` …

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/kestrel-probe
git commit -m "feat(probe): parsing helpers, mock probe and report CLI

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 9: Windows probes part 1 — orchestration and WMI

**Files:**
- Modify: `crates/kestrel-probe/Cargo.toml`, `crates/kestrel-probe/src/lib.rs`
- Create: `crates/kestrel-probe/src/win/mod.rs`, `crates/kestrel-probe/src/win/wmi_probe.rs`, stub files `registry_probe.rs`, `display.rs`, `firmware.rs`, `system_info.rs` (filled in Task 10)

**Interfaces:**
- Consumes: `parse::*` (Task 8), `kestrel_core::{gpu, valorant, …}`.
- Produces: `win::collect() -> SystemSnapshot` (runs on its own thread so COM starts clean); `win::problem(probe, message) -> ProbeError`; `wmi_probe::Wmi::{connect, fill}` filling os, cpu, machine, gpus, memory, board/BIOS, TPM, VBS/HVCI, Vanguard service state, network adapters, VALORANT drive media. Task 10 provides `registry_probe::{fill, gpu_memory_mb, adapter_power_saving}`, `display::fill`, `firmware::iommu_enabled`, `system_info::fill`.

Testing note: these are OS calls. The gate on the Mac is a clean `cargo check --target x86_64-pc-windows-msvc`; the behaviour gate is `windows_probe_reads_the_basics` (Task 8 smoke test) on the Windows CI runner (Task 16) and the owner's test run.

- [ ] **Step 1: Add Windows-only dependencies**

```bash
cd crates/kestrel-probe
cargo add wmi --target 'cfg(windows)'
cargo add winreg --target 'cfg(windows)'
cargo add windows-sys --target 'cfg(windows)' --features Win32_Foundation,Win32_Graphics_Gdi,Win32_System_SystemInformation,Win32_System_Performance
cargo add nvml-wrapper --target 'cfg(windows)'
cd ../..
```

- [ ] **Step 2: Wire the module in** — in `lib.rs` add `#[cfg(windows)] mod win;` and replace the `unimplemented!` branch with `win::collect()`.

- [ ] **Step 3: Write `win/mod.rs`**

```rust
//! Real probes for Windows. Each one is independent: a failure becomes `None` plus a ProbeError.
mod display;
mod firmware;
mod registry_probe;
mod system_info;
mod wmi_probe;

use kestrel_core::{valorant, ProbeError, SystemSnapshot, ValorantInstall, SCHEMA_VERSION};

pub fn collect() -> SystemSnapshot {
    // WMI needs a thread whose COM state we control (WebView2 owns the UI thread's), so probe on a fresh one.
    std::thread::spawn(collect_here).join().unwrap_or_else(|_| {
        let mut s = SystemSnapshot::default();
        s.probe_errors.push(problem("probe", "the probe thread crashed"));
        s
    })
}

fn collect_here() -> SystemSnapshot {
    let mut s = SystemSnapshot {
        schema_version: SCHEMA_VERSION,
        collected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ..Default::default()
    };
    let mut errors = Vec::new();
    s.storage.valorant = find_valorant_install().map(|path| ValorantInstall {
        drive_letter: valorant::drive_letter(&path).unwrap_or_else(|| "C".into()),
        path,
        ..Default::default()
    });
    match wmi_probe::Wmi::connect() {
        Ok(wmi) => wmi.fill(&mut s, &mut errors),
        Err(e) => errors.push(problem("wmi", e)),
    }
    registry_probe::fill(&mut s, &mut errors);
    display::fill(&mut s);
    s.security.iommu = Some(firmware::iommu_enabled());
    system_info::fill(&mut s);
    s.probe_errors = errors;
    s
}

/// VALORANT's install folder from Riot's install record, else the default folder, if it exists.
fn find_valorant_install() -> Option<String> {
    let program_data = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let record = format!("{program_data}\\Riot Games\\Metadata\\valorant.live\\valorant.live.product_settings.yaml");
    std::fs::read_to_string(record)
        .ok()
        .and_then(|yaml| valorant::install_path_from_product_settings(&yaml))
        .or_else(|| Some(valorant::DEFAULT_INSTALL.to_string()))
        .filter(|path| std::path::Path::new(path).exists())
}

pub(crate) fn problem(probe: &str, message: impl std::fmt::Display) -> ProbeError {
    ProbeError { probe: probe.into(), message: message.to_string() }
}
```

Create the four Task 10 modules as stubs so this compiles:
```rust
// registry_probe.rs
use kestrel_core::{ProbeError, SystemSnapshot};
pub fn fill(_s: &mut SystemSnapshot, _errors: &mut Vec<ProbeError>) {}
pub fn gpu_memory_mb(_name: &str) -> Option<u64> { None }
pub fn adapter_power_saving(_device_id: &str) -> Option<bool> { None }
// display.rs
pub fn fill(_s: &mut kestrel_core::SystemSnapshot) {}
// firmware.rs
pub fn iommu_enabled() -> bool { false }
// system_info.rs
pub fn fill(_s: &mut kestrel_core::SystemSnapshot) {}
```

- [ ] **Step 4: Write `win/wmi_probe.rs`**

```rust
//! WMI queries. Each query is independent; a failing one adds a ProbeError and leaves its fields as None.
use super::{problem, registry_probe};
use crate::parse;
use kestrel_core::{gpu, GpuInfo, MemoryModule, NetAdapter, ProbeError, ServiceState, SystemSnapshot};
use std::collections::HashMap;
use wmi::{COMLibrary, Variant, WMIConnection};

type Row = HashMap<String, Variant>;
type Outcome = Result<(), Box<dyn std::error::Error>>;

pub struct Wmi {
    com: COMLibrary,
    cimv2: WMIConnection,
}

impl Wmi {
    pub fn connect() -> Result<Self, wmi::WMIError> {
        let com = COMLibrary::new().or_else(|_| COMLibrary::without_security())?;
        let cimv2 = WMIConnection::new(com)?;
        Ok(Self { com, cimv2 })
    }

    fn rows(&self, query: &str) -> Result<Vec<Row>, wmi::WMIError> {
        self.cimv2.raw_query(query)
    }

    fn rows_in(&self, namespace: &str, query: &str) -> Result<Vec<Row>, wmi::WMIError> {
        WMIConnection::with_namespace_path(namespace, self.com)?.raw_query(query)
    }

    pub fn fill(&self, s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
        let mut attempt = |name: &str, result: Outcome| {
            if let Err(e) = result {
                errors.push(problem(name, e));
            }
        };
        attempt("os", self.os(s));
        attempt("cpu", self.cpu(s));
        attempt("machine", self.machine(s));
        attempt("gpus", self.gpus(s));
        attempt("memory", self.memory(s));
        attempt("board", self.board(s));
        attempt("tpm", self.tpm(s));
        attempt("device-guard", self.device_guard(s));
        attempt("vanguard", self.vanguard(s));
        attempt("network", self.network(s));
        attempt("storage", self.storage(s));
    }

    fn os(&self, s: &mut SystemSnapshot) -> Outcome {
        let row = first(self.rows("SELECT Caption, BuildNumber FROM Win32_OperatingSystem")?)?;
        s.os.caption = text(&row, "Caption");
        s.os.build = num(&row, "BuildNumber").unwrap_or(0) as u32;
        Ok(())
    }

    fn cpu(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows("SELECT Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors FROM Win32_Processor")?;
        let row = rows.first().ok_or("no processor reported")?;
        s.cpu.name = text(row, "Name");
        s.cpu.vendor = parse::cpu_vendor(&text(row, "Manufacturer"));
        s.cpu.cores = rows.iter().filter_map(|r| num(r, "NumberOfCores")).sum::<u64>() as u32;
        s.cpu.threads = rows.iter().filter_map(|r| num(r, "NumberOfLogicalProcessors")).sum::<u64>() as u32;
        Ok(())
    }

    fn machine(&self, s: &mut SystemSnapshot) -> Outcome {
        let row = first(self.rows("SELECT Manufacturer, Model, PCSystemType FROM Win32_ComputerSystem")?)?;
        let batteries = self.rows("SELECT BatteryStatus FROM Win32_Battery").unwrap_or_default();
        s.machine.manufacturer = text(&row, "Manufacturer");
        s.machine.model = text(&row, "Model");
        s.machine.form_factor = parse::form_factor(num(&row, "PCSystemType"), !batteries.is_empty());
        // BatteryStatus 1 = discharging (on battery); anything else = on AC power.
        s.machine.on_battery = batteries.first().and_then(|b| num(b, "BatteryStatus")).map(|status| status == 1);
        Ok(())
    }

    fn gpus(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows("SELECT Name, PNPDeviceID, DriverVersion, DriverDate FROM Win32_VideoController")?;
        s.gpus = rows
            .iter()
            .map(|row| {
                let name = text(row, "Name");
                let vendor = gpu::vendor_from(&text(row, "PNPDeviceID"), &name);
                let vram_mb = registry_probe::gpu_memory_mb(&name);
                GpuInfo {
                    is_integrated: gpu::looks_integrated(vendor, &name, vram_mb),
                    driver_version: text(row, "DriverVersion"),
                    driver_date: parse::cim_date(&text(row, "DriverDate")),
                    vram_mb,
                    vendor,
                    name,
                }
            })
            .collect();
        Ok(())
    }

    fn memory(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows("SELECT Capacity, ConfiguredClockSpeed, Speed, PartNumber, Manufacturer, BankLabel, DeviceLocator, SMBIOSMemoryType FROM Win32_PhysicalMemory")?;
        s.memory.modules = rows
            .iter()
            .map(|row| MemoryModule {
                capacity_mb: num(row, "Capacity").unwrap_or(0) / (1024 * 1024),
                configured_mts: num(row, "ConfiguredClockSpeed").map(|v| v as u32),
                max_mts: num(row, "Speed").map(|v| v as u32),
                part_number: text(row, "PartNumber"),
                manufacturer: text(row, "Manufacturer"),
                bank_label: text(row, "BankLabel"),
                device_locator: text(row, "DeviceLocator"),
            })
            .collect();
        s.memory.total_mb = s.memory.modules.iter().map(|m| m.capacity_mb).sum();
        s.memory.ddr_generation = rows.first().and_then(|r| num(r, "SMBIOSMemoryType")).and_then(parse::ddr_generation);
        Ok(())
    }

    fn board(&self, s: &mut SystemSnapshot) -> Outcome {
        let board = first(self.rows("SELECT Manufacturer, Product FROM Win32_BaseBoard")?)?;
        s.board.manufacturer = text(&board, "Manufacturer");
        s.board.product = text(&board, "Product");
        let bios = first(self.rows("SELECT SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS")?)?;
        s.board.bios_version = text(&bios, "SMBIOSBIOSVersion");
        s.board.bios_date = parse::cim_date(&text(&bios, "ReleaseDate"));
        Ok(())
    }

    fn tpm(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows_in("root\\CIMV2\\Security\\MicrosoftTpm", "SELECT IsEnabled_InitialValue, SpecVersion FROM Win32_Tpm")?;
        s.security.tpm2 = Some(rows.first().is_some_and(|r| flag(r, "IsEnabled_InitialValue") == Some(true) && text(r, "SpecVersion").starts_with("2.0")));
        Ok(())
    }

    fn device_guard(&self, s: &mut SystemSnapshot) -> Outcome {
        let row = first(self.rows_in("root\\Microsoft\\Windows\\DeviceGuard", "SELECT VirtualizationBasedSecurityStatus, SecurityServicesRunning FROM Win32_DeviceGuard")?)?;
        s.security.vbs_running = num(&row, "VirtualizationBasedSecurityStatus").map(|status| status == 2);
        s.security.hvci_running = Some(nums(&row, "SecurityServicesRunning").contains(&2));
        Ok(())
    }

    fn vanguard(&self, s: &mut SystemSnapshot) -> Outcome {
        // Read-only: we only ask whether Vanguard's service exists and runs. Kestrel never starts, stops or changes it.
        let rows = self.rows("SELECT State FROM Win32_Service WHERE Name = 'vgc'")?;
        s.security.vanguard = match rows.first().map(|r| text(r, "State")) {
            None => ServiceState::NotInstalled,
            Some(state) if state == "Running" => ServiceState::Running,
            Some(_) => ServiceState::Stopped,
        };
        Ok(())
    }

    fn network(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows_in("root\\StandardCimv2", "SELECT Name, DeviceID, NdisPhysicalMedium, Speed, MediaConnectState, HardwareInterface, Virtual FROM MSFT_NetAdapter")?;
        s.network.adapters = rows
            .iter()
            .filter(|r| flag(r, "HardwareInterface") == Some(true) && flag(r, "Virtual") != Some(true))
            .map(|r| NetAdapter {
                name: text(r, "Name"),
                kind: parse::net_kind(num(r, "NdisPhysicalMedium").unwrap_or(0)),
                link_mbps: num(r, "Speed").filter(|&bps| bps > 0).map(|bps| bps / 1_000_000),
                connected: num(r, "MediaConnectState") == Some(1),
                power_saving: registry_probe::adapter_power_saving(&text(r, "DeviceID")),
            })
            .collect();
        Ok(())
    }

    fn storage(&self, s: &mut SystemSnapshot) -> Outcome {
        let Some(install) = s.storage.valorant.as_mut() else { return Ok(()) };
        let letter = install.drive_letter.chars().next().unwrap_or('C');
        let namespace = "root\\Microsoft\\Windows\\Storage";
        let partitions = self.rows_in(namespace, "SELECT DriveLetter, DiskNumber FROM MSFT_Partition")?;
        let disk = partitions
            .iter()
            .find(|p| drive_letter_of(p) == Some(letter))
            .and_then(|p| num(p, "DiskNumber"))
            .ok_or("VALORANT's drive has no partition record")?;
        let disks = self.rows_in(namespace, "SELECT DeviceId, MediaType, BusType FROM MSFT_PhysicalDisk")?;
        let row = disks.iter().find(|d| text(d, "DeviceId") == disk.to_string()).ok_or("VALORANT's disk has no physical disk record")?;
        install.media = parse::drive_media(num(row, "BusType"), num(row, "MediaType"));
        Ok(())
    }
}

fn first(mut rows: Vec<Row>) -> Result<Row, Box<dyn std::error::Error>> {
    if rows.is_empty() { Err("no rows returned".into()) } else { Ok(rows.swap_remove(0)) }
}

fn text(row: &Row, key: &str) -> String {
    match row.get(key) {
        Some(Variant::String(v)) => v.trim().to_string(),
        _ => String::new(),
    }
}

fn variant_num(v: &Variant) -> Option<u64> {
    match v {
        Variant::UI1(x) => Some(u64::from(*x)),
        Variant::UI2(x) => Some(u64::from(*x)),
        Variant::UI4(x) => Some(u64::from(*x)),
        Variant::UI8(x) => Some(*x),
        Variant::I1(x) => u64::try_from(*x).ok(),
        Variant::I2(x) => u64::try_from(*x).ok(),
        Variant::I4(x) => u64::try_from(*x).ok(),
        Variant::I8(x) => u64::try_from(*x).ok(),
        Variant::String(x) => x.trim().parse().ok(),
        _ => None,
    }
}

fn num(row: &Row, key: &str) -> Option<u64> {
    variant_num(row.get(key)?)
}

fn nums(row: &Row, key: &str) -> Vec<u64> {
    match row.get(key) {
        Some(Variant::Array(items)) => items.iter().filter_map(variant_num).collect(),
        _ => Vec::new(),
    }
}

fn flag(row: &Row, key: &str) -> Option<bool> {
    match row.get(key)? {
        Variant::Bool(v) => Some(*v),
        _ => None,
    }
}

/// MSFT_Partition.DriveLetter is a CIM char16 (a number), or a string on some systems.
fn drive_letter_of(row: &Row) -> Option<char> {
    match row.get("DriveLetter")? {
        Variant::String(v) => v.chars().next(),
        other => variant_num(other).and_then(|n| char::from_u32(n as u32)),
    }
    .map(|c| c.to_ascii_uppercase())
}
```

- [ ] **Step 5: Compile-check for Windows and re-run the Mac tests**

Run: `cargo check -p kestrel-probe --target x86_64-pc-windows-msvc && cargo test -p kestrel-probe`
Expected: check finishes with no errors (fix any API drift in `wmi` the compiler reports, keeping behaviour); Mac tests still pass.

- [ ] **Step 6: Commit**

```bash
git add crates/kestrel-probe
git commit -m "feat(probe): Windows orchestration and WMI probes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Windows probes part 2 — registry, displays, IOMMU, storage space, background apps

**Files:**
- Modify (replace stubs): `crates/kestrel-probe/src/win/registry_probe.rs`, `display.rs`, `firmware.rs`, `system_info.rs`

**Interfaces:**
- Consumes: `parse::*` (Task 8), `win::problem` (Task 9).
- Produces: `registry_probe::fill(s, errors)` (display version, power plan, Game Mode, background recording, HAGS, pending restart, startup apps, VALORANT GPU preference, Secure Boot, Delivery Optimization), `registry_probe::gpu_memory_mb(name) -> Option<u64>`, `registry_probe::adapter_power_saving(device_id) -> Option<bool>`, `display::fill(s)`, `firmware::iommu_enabled() -> bool`, `system_info::fill(s)` (VALORANT drive space, busy processes). All read-only.

- [ ] **Step 1: `registry_probe.rs`**

```rust
//! Registry reads. Keys are opened read-only (KEY_READ) and never written.
use super::problem;
use crate::parse;
use kestrel_core::{valorant, PowerPlan, ProbeError, SystemSnapshot};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ};
use winreg::RegKey;

const DISPLAY_CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const NET_CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";
const PLAN_NAMES: [(&str, &str); 4] = [
    ("381b4222-f694-41f0-9685-ff5bb260df2e", "Balanced"),
    ("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c", "High performance"),
    ("a1841308-3541-4fab-bc81-f71556f20b4a", "Power saver"),
    ("e9a42b02-d5df-448d-aa00-03f14749eb61", "Ultimate Performance"),
];

fn open(root: winreg::HKEY, path: &str) -> Option<RegKey> {
    RegKey::predef(root).open_subkey_with_flags(path, KEY_READ).ok()
}
fn hklm(path: &str) -> Option<RegKey> {
    open(HKEY_LOCAL_MACHINE, path)
}
fn hkcu(path: &str) -> Option<RegKey> {
    open(HKEY_CURRENT_USER, path)
}
fn dword(key: &Option<RegKey>, name: &str) -> Option<u32> {
    key.as_ref()?.get_value::<u32, _>(name).ok()
}
fn string(key: &Option<RegKey>, name: &str) -> Option<String> {
    key.as_ref()?.get_value::<String, _>(name).ok()
}

pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    s.os.display_version = string(&hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"), "DisplayVersion");
    s.windows.power_plan = power_plan();
    if s.windows.power_plan.is_none() {
        errors.push(problem("power-plan", "couldn't read the active power plan"));
    }
    // Missing value = Windows default (Game Mode on).
    s.windows.game_mode = Some(dword(&hkcu(r"Software\Microsoft\GameBar"), "AutoGameModeEnabled") != Some(0));
    let dvr = hkcu(r"Software\Microsoft\Windows\CurrentVersion\GameDVR");
    s.windows.background_recording = Some(dword(&dvr, "AppCaptureEnabled") != Some(0) && dword(&dvr, "HistoricalCaptureEnabled") == Some(1));
    s.windows.hags = match dword(&hklm(r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers"), "HwSchMode") {
        Some(2) => Some(true),
        Some(1) => Some(false),
        _ => None,
    };
    s.windows.pending_restart = Some(
        hklm(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending").is_some()
            || hklm(r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired").is_some(),
    );
    s.windows.startup_app_count = Some(startup_app_count());
    s.windows.valorant_gpu_preference = s.storage.valorant.as_ref().and_then(|v| gpu_preference_for(&valorant::game_exe_path(&v.path)));
    s.security.secure_boot = Some(dword(&hklm(r"SYSTEM\CurrentControlSet\Control\SecureBoot\State"), "UEFISecureBootEnabled") == Some(1));
    s.network.delivery_optimization_internet = Some(parse::delivery_optimization_internet(delivery_optimization_mode()));
}

fn power_plan() -> Option<PowerPlan> {
    let base = r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes";
    let guid = string(&hklm(base), "ActivePowerScheme")?.to_ascii_lowercase();
    let known = PLAN_NAMES.iter().find(|(g, _)| *g == guid).map(|(_, n)| n.to_string());
    let name = string(&hklm(&format!(r"{base}\{guid}")), "FriendlyName")
        .map(|raw| parse::power_plan_name(&raw))
        .filter(|n| !n.is_empty())
        .or(known)
        .unwrap_or_else(|| "Custom plan".into());
    Some(PowerPlan { guid, name })
}

fn startup_app_count() -> u32 {
    let run_keys = [
        (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run", "Run"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "Run"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run", "Run32"),
    ];
    let mut count = 0;
    for (root, path, approved_name) in run_keys {
        let Some(run) = open(root, path) else { continue };
        let approved = open(root, &format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\{approved_name}"));
        for (name, _) in run.enum_values().filter_map(Result::ok) {
            let bytes = approved.as_ref().and_then(|a| a.get_raw_value(&name).ok()).map(|v| v.bytes.to_vec());
            if parse::startup_entry_enabled(bytes.as_deref()) {
                count += 1;
            }
        }
    }
    count + startup_folder_count()
}

fn startup_folder_count() -> u32 {
    let folders = [
        std::env::var("APPDATA").ok().map(|a| format!(r"{a}\Microsoft\Windows\Start Menu\Programs\Startup")),
        std::env::var("ProgramData").ok().map(|p| format!(r"{p}\Microsoft\Windows\Start Menu\Programs\Startup")),
    ];
    folders
        .into_iter()
        .flatten()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|e| !e.file_name().to_string_lossy().eq_ignore_ascii_case("desktop.ini"))
        .count() as u32
}

fn gpu_preference_for(exe: &str) -> Option<u8> {
    let prefs = hkcu(r"Software\Microsoft\DirectX\UserGpuPreferences")?;
    let wanted = exe.replace('/', "\\").to_ascii_lowercase();
    let name = prefs.enum_values().filter_map(Result::ok).map(|(name, _)| name).find(|name| name.to_ascii_lowercase() == wanted)?;
    parse::gpu_preference(&prefs.get_value::<String, _>(&name).ok()?)
}

fn delivery_optimization_mode() -> Option<u32> {
    let policy = dword(&hklm(r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization"), "DODownloadMode");
    let settings = open(HKEY_USERS, r"S-1-5-20\Software\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Settings");
    policy
        .or_else(|| dword(&settings, "DownloadMode"))
        .or_else(|| dword(&hklm(r"SOFTWARE\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Config"), "DODownloadMode"))
}

fn class_entries(class: &str) -> Vec<RegKey> {
    let Some(root) = hklm(class) else { return Vec::new() };
    root.enum_keys()
        .filter_map(Result::ok)
        .filter(|k| k.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|k| root.open_subkey_with_flags(&k, KEY_READ).ok())
        .collect()
}

/// Dedicated video memory (MB) of the display adapter whose driver description is `name`.
pub fn gpu_memory_mb(name: &str) -> Option<u64> {
    let key = class_entries(DISPLAY_CLASS).into_iter().find(|k| k.get_value::<String, _>("DriverDesc").ok().as_deref() == Some(name))?;
    let bytes = key
        .get_value::<u64, _>("HardwareInformation.qwMemorySize")
        .ok()
        .or_else(|| key.get_value::<u32, _>("HardwareInformation.MemorySize").ok().map(u64::from))
        .or_else(|| {
            let raw = key.get_raw_value("HardwareInformation.MemorySize").ok()?;
            let b = raw.bytes.get(0..4)?;
            Some(u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
        })?;
    Some(bytes / (1024 * 1024))
}

/// Whether Energy-Efficient Ethernet or a similar power-saving option is on, for the adapter with this NetCfgInstanceId.
pub fn adapter_power_saving(device_id: &str) -> Option<bool> {
    const KEYWORDS: [&str; 4] = ["*EEE", "EEE", "EnableGreenEthernet", "PowerSavingMode"];
    let key = class_entries(NET_CLASS)
        .into_iter()
        .find(|k| k.get_value::<String, _>("NetCfgInstanceId").is_ok_and(|v| v.eq_ignore_ascii_case(device_id)))?;
    Some(KEYWORDS.iter().any(|kw| key.get_value::<String, _>(*kw).is_ok_and(|v| v.trim() == "1")))
}
```

- [ ] **Step 2: `display.rs`**

```rust
//! Monitors: current and highest refresh rate at the current resolution, via EnumDisplaySettings.
use kestrel_core::{DisplayInfo, SystemSnapshot};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
    DISPLAY_DEVICE_PRIMARY_DEVICE, ENUM_CURRENT_SETTINGS,
};

const VIRTUAL_HINTS: [&str; 4] = ["virtual", "parsec", "indirect", "remote display"];

fn wide(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
}

fn mode(device: &[u16], index: u32) -> Option<DEVMODEW> {
    // SAFETY: DEVMODEW is plain data; dmSize tells Windows how much it may write.
    let mut m: DEVMODEW = unsafe { std::mem::zeroed() };
    m.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    (unsafe { EnumDisplaySettingsW(device.as_ptr(), index, &mut m) } != 0).then_some(m)
}

pub fn fill(s: &mut SystemSnapshot) {
    let mut displays = Vec::new();
    for index in 0u32.. {
        // SAFETY: DISPLAY_DEVICEW is plain data; cb tells Windows its size.
        let mut dev: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        dev.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut dev, 0) } == 0 {
            break;
        }
        if dev.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        let Some(current) = mode(&dev.DeviceName, ENUM_CURRENT_SETTINGS) else { continue };
        let max_hz = (0u32..)
            .map_while(|i| mode(&dev.DeviceName, i))
            .filter(|m| m.dmPelsWidth == current.dmPelsWidth && m.dmPelsHeight == current.dmPelsHeight)
            .map(|m| m.dmDisplayFrequency)
            .max()
            .unwrap_or(current.dmDisplayFrequency);
        let adapter = wide(&dev.DeviceString);
        let lower = adapter.to_ascii_lowercase();
        displays.push(DisplayInfo {
            name: wide(&dev.DeviceName),
            is_virtual: VIRTUAL_HINTS.iter().any(|h| lower.contains(h)),
            adapter,
            current_hz: current.dmDisplayFrequency,
            max_hz: max_hz.max(current.dmDisplayFrequency),
            width: current.dmPelsWidth,
            height: current.dmPelsHeight,
            is_primary: dev.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0,
        });
    }
    s.displays = displays;
}
```

- [ ] **Step 3: `firmware.rs`**

```rust
//! IOMMU: firmware publishes an ACPI DMAR table (Intel VT-d) or IVRS table (AMD-Vi) only when it's enabled.
use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;

pub fn iommu_enabled() -> bool {
    let acpi = u32::from_be_bytes(*b"ACPI");
    [*b"DMAR", *b"IVRS"].iter().any(|signature| {
        // SAFETY: a null buffer of size 0 only asks for the table's size; nothing is written.
        unsafe { GetSystemFirmwareTable(acpi, u32::from_le_bytes(*signature), std::ptr::null_mut(), 0) > 0 }
    })
}
```

- [ ] **Step 4: `system_info.rs`**

```rust
//! Free space on VALORANT's drive and busy background apps, via the cross-platform `sysinfo` crate.
use crate::parse;
use kestrel_core::SystemSnapshot;
use std::time::Duration;
use sysinfo::{Disks, ProcessesToUpdate, System};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn fill(s: &mut SystemSnapshot) {
    if let Some(install) = s.storage.valorant.as_mut() {
        let prefix = format!("{}:", install.drive_letter);
        let disks = Disks::new_with_refreshed_list();
        if let Some(d) = disks.list().iter().find(|d| d.mount_point().to_string_lossy().to_ascii_uppercase().starts_with(&prefix)) {
            install.free_gb = d.available_space() as f64 / GIB;
            install.total_gb = d.total_space() as f64 / GIB;
        }
    }
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    std::thread::sleep(Duration::from_millis(1500));
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let samples = sys.processes().values().map(|p| (p.name().to_string_lossy().into_owned(), p.cpu_usage())).collect();
    let threads = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
    s.background.busy_processes = Some(parse::busy_processes(samples, threads));
}
```

- [ ] **Step 5: Compile-check and test**

Run: `cargo check -p kestrel-probe --target x86_64-pc-windows-msvc && cargo test -p kestrel-probe && cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc -- -D warnings`
Expected: no errors or warnings (adapt to `windows-sys`/`winreg`/`sysinfo` API drift the compiler reports, keeping behaviour).

- [ ] **Step 6: Commit**

```bash
git add crates/kestrel-probe
git commit -m "feat(probe): registry, display, IOMMU, storage and background probes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 11: Live load sampler (CPU, RAM, GPU %, GPU temperature)

**Files:**
- Create: `crates/kestrel-probe/src/load.rs`, `crates/kestrel-probe/src/win/gpu_usage.rs`
- Modify: `crates/kestrel-probe/src/lib.rs` (`pub mod load;`), `crates/kestrel-probe/src/win/mod.rs` (`pub(crate) mod gpu_usage;`), `crates/kestrel-probe/Cargo.toml` (`cargo add serde --features derive`)

**Interfaces:**
- Produces: `kestrel_probe::load::LoadSample { cpu_percent: f32, ram_percent: f32, gpu_percent: Option<f32>, gpu_temp_c: Option<f32> }` (Serialize camelCase → `cpuPercent`, `ramPercent`, `gpuPercent`, `gpuTempC`); `kestrel_probe::load::LoadSampler::{new() -> Self, sample(&mut self) -> LoadSample}` (`LoadSampler: Send`); Windows-only `win::gpu_usage::GpuUsage::{new() -> Option<Self>, percent(&mut self) -> Option<f32>, temperature(&self) -> Option<f32>}`.

- [ ] **Step 1: Write the failing tests** — `crates/kestrel-probe/src/load.rs` (tests only):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_percentages() {
        let mut sampler = LoadSampler::new();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let s = sampler.sample();
        assert!((0.0..=100.0).contains(&s.cpu_percent), "{s:?}");
        assert!(s.ram_percent > 0.0 && s.ram_percent <= 100.0, "{s:?}");
        #[cfg(not(windows))]
        assert!(s.gpu_percent.is_none() && s.gpu_temp_c.is_none());
    }

    #[test]
    fn serializes_camel_case() {
        let json = serde_json::to_string(&LoadSample { cpu_percent: 1.0, ram_percent: 2.0, gpu_percent: None, gpu_temp_c: Some(60.0) }).unwrap();
        assert_eq!(json, r#"{"cpuPercent":1.0,"ramPercent":2.0,"gpuPercent":null,"gpuTempC":60.0}"#);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kestrel-probe load`
Expected: compile errors (`LoadSampler` not defined).

- [ ] **Step 3: Implement `load.rs`** (above its tests):

```rust
//! Live load for the home screen graph: CPU and RAM everywhere, GPU usage and temperature on Windows.
use serde::Serialize;
use sysinfo::System;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSample {
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub gpu_percent: Option<f32>,
    pub gpu_temp_c: Option<f32>,
}

pub struct LoadSampler {
    sys: System,
    #[cfg(windows)]
    gpu: Option<crate::win::gpu_usage::GpuUsage>,
}

impl Default for LoadSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadSampler {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self {
            sys,
            #[cfg(windows)]
            gpu: crate::win::gpu_usage::GpuUsage::new(),
        }
    }

    /// Call about once a second. The first CPU reading after `new` can be 0.
    pub fn sample(&mut self) -> LoadSample {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        let total = self.sys.total_memory();
        let ram = if total == 0 { 0.0 } else { self.sys.used_memory() as f32 / total as f32 * 100.0 };
        #[cfg(windows)]
        let (gpu_percent, gpu_temp_c) = match self.gpu.as_mut() {
            Some(g) => (g.percent(), g.temperature()),
            None => (None, None),
        };
        #[cfg(not(windows))]
        let (gpu_percent, gpu_temp_c) = (None, None);
        LoadSample { cpu_percent: percent(self.sys.global_cpu_usage()), ram_percent: percent(ram), gpu_percent: gpu_percent.map(percent), gpu_temp_c }
    }
}

fn percent(value: f32) -> f32 {
    (value.clamp(0.0, 100.0) * 10.0).round() / 10.0
}
```

- [ ] **Step 4: Implement `win/gpu_usage.rs`**

```rust
//! GPU load from Windows performance counters (what Task Manager shows) and NVIDIA temperature via NVML.
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

const PDH_MORE_DATA: u32 = 0x8000_07D2;

pub struct GpuUsage {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    nvml: Option<nvml_wrapper::Nvml>,
}

// SAFETY: PDH handles can be used from any thread; the sampler sits behind a Mutex, so use is serialized.
unsafe impl Send for GpuUsage {}

impl GpuUsage {
    pub fn new() -> Option<Self> {
        let path: Vec<u16> = "\\GPU Engine(*engtype_3D)\\Utilization Percentage\0".encode_utf16().collect();
        // SAFETY: out-pointers are valid locals and the counter path is NUL-terminated.
        unsafe {
            let mut query: PDH_HQUERY = std::mem::zeroed();
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != 0 {
                return None;
            }
            let mut counter: PDH_HCOUNTER = std::mem::zeroed();
            if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            PdhCollectQueryData(query);
            Some(Self { query, counter, nvml: nvml_wrapper::Nvml::init().ok() })
        }
    }

    /// Total 3D-engine utilisation across all processes, 0–100.
    pub fn percent(&mut self) -> Option<f32> {
        // SAFETY: handles live as long as self; the buffer is sized by the first call and 8-byte aligned.
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let (mut bytes, mut count) = (0u32, 0u32);
            match PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, std::ptr::null_mut()) {
                0 => return Some(0.0),
                PDH_MORE_DATA => {}
                _ => return None,
            }
            let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
            let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, items) != 0 {
                return None;
            }
            let total: f64 = std::slice::from_raw_parts(items, count as usize).iter().map(|item| item.FmtValue.Anonymous.doubleValue).sum();
            Some(total.clamp(0.0, 100.0) as f32)
        }
    }

    /// GPU temperature in °C on NVIDIA cards (NVML ships with the driver); None elsewhere.
    pub fn temperature(&self) -> Option<f32> {
        use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
        let device = self.nvml.as_ref()?.device_by_index(0).ok()?;
        device.temperature(TemperatureSensor::Gpu).ok().map(|t| t as f32)
    }
}

impl Drop for GpuUsage {
    fn drop(&mut self) {
        // SAFETY: the query was opened in `new` and is closed exactly once.
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}
```

- [ ] **Step 5: Run tests and the Windows check**

Run: `cargo test -p kestrel-probe && cargo check -p kestrel-probe --target x86_64-pc-windows-msvc`
Expected: tests pass (`load` 2 more); Windows check clean.

- [ ] **Step 6: Commit**

```bash
git add crates/kestrel-probe
git commit -m "feat(probe): live CPU, RAM and GPU load sampler

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Tauri app shell — commands, admin manifest, installer config, icons

**Files:**
- Create: `app/package.json`, `app/vite.config.ts`, `app/tsconfig.json`, `app/index.html`, `app/src/main.tsx` (placeholder until Task 13)
- Create: `app/src-tauri/Cargo.toml`, `build.rs`, `app.manifest`, `tauri.conf.json`, `capabilities/default.json`, `src/main.rs`, `src/lib.rs`, `icons/kestrel.svg` (+ generated icons)
- Modify: root `Cargo.toml` (members += `app/src-tauri`)

**Interfaces:**
- Consumes: `kestrel_probe::{collect_report, load::{LoadSampler, LoadSample}}`, `kestrel_core::Report`.
- Produces Tauri commands (invoked from TypeScript with these exact names): `get_report() -> Report` (cached; scans on first call), `rescan() -> Report`, `get_load() -> LoadSample`, `get_diagnostics() -> String` (pretty JSON of the last report), `is_elevated() -> bool`. Window label `main`, frameless, 1180×760 (min 1024×680).

- [ ] **Step 1: Frontend skeleton**

`app/package.json`:
```json
{
  "name": "kestrel-app",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "test": "vitest run",
    "tauri": "tauri",
    "mock": "cargo run -q -p kestrel-probe --bin kestrel-probe -- --pretty > src/api/mock-report.json"
  }
}
```

```bash
cd app
npm install react react-dom @tauri-apps/api @tauri-apps/plugin-opener @fontsource/chakra-petch @fontsource/jetbrains-mono @tabler/icons-react
npm install -D vite @vitejs/plugin-react typescript @types/react @types/react-dom @tauri-apps/cli vitest
cd ..
```

`app/vite.config.ts`:
```ts
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: { target: 'chrome105', outDir: 'dist' },
  test: { environment: 'node' },
});
```

`app/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2021",
    "lib": ["ES2021", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "noEmit": true,
    "types": ["vite/client"]
  },
  "include": ["src", "vite.config.ts"]
}
```

`app/index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Kestrel</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`app/src/main.tsx` (placeholder, replaced in Task 13):
```tsx
import { createRoot } from 'react-dom/client';

createRoot(document.getElementById('root')!).render(<p>Kestrel</p>);
```

- [ ] **Step 2: Tauri crate files**

`app/src-tauri/Cargo.toml`:
```toml
[package]
name = "kestrel"
description = "Kestrel: a free, honest FPS optimiser for VALORANT"
edition.workspace = true
license.workspace = true
version.workspace = true

[lib]
name = "kestrel_app"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
kestrel-core = { path = "../../crates/kestrel-core" }
kestrel-probe = { path = "../../crates/kestrel-probe" }
```
Then `cd app/src-tauri && cargo add windows-sys --target 'cfg(windows)' --features Win32_UI_Shell && cd ../..`, and add `"app/src-tauri"` to the root `members`.

`app/src-tauri/build.rs`:
```rust
fn main() {
    // Kestrel asks for administrator rights once at launch (spec decision 11).
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("failed to run tauri-build");
}
```

`app/src-tauri/app.manifest`:
```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}" />
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
```

`app/src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Kestrel",
  "mainBinaryName": "Kestrel",
  "version": "0.1.0",
  "identifier": "io.github.gabryelvs.kestrel",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "Kestrel",
        "width": 1180,
        "height": 760,
        "minWidth": 1024,
        "minHeight": 680,
        "center": true,
        "decorations": false,
        "backgroundColor": "#0C0E11"
      }
    ],
    "security": {
      "csp": "default-src 'self' ipc: http://ipc.localhost; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "publisher": "Kestrel contributors",
    "shortDescription": "Free FPS optimiser for VALORANT",
    "license": "GPL-3.0-only",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"],
    "windows": {
      "nsis": { "installMode": "perMachine" },
      "webviewInstallMode": { "type": "embedBootstrapper" }
    }
  }
}
```

`app/src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Main window: window controls and opening links in the browser.",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-start-dragging",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-close",
    "opener:default"
  ]
}
```

`app/src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    kestrel_app::run()
}
```

`app/src-tauri/src/lib.rs`:
```rust
//! Kestrel's Tauri shell: hands the probe's report and live load to the UI. Read-only in stage 1.
use kestrel_core::Report;
use kestrel_probe::load::{LoadSample, LoadSampler};
use std::sync::Mutex;
use tauri::State;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Default)]
struct AppState {
    report: Mutex<Option<Report>>,
    load: Mutex<Option<LoadSampler>>,
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
    let report = tauri::async_runtime::spawn_blocking(|| kestrel_probe::collect_report(VERSION)).await.map_err(|e| e.to_string())?;
    *state.report.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(report)
}

#[tauri::command]
fn get_load(state: State<'_, AppState>) -> Result<LoadSample, String> {
    let mut sampler = state.load.lock().map_err(|e| e.to_string())?;
    Ok(sampler.get_or_insert_with(LoadSampler::new).sample())
}

/// Pretty JSON of the last report, for the "Copy diagnostics" button.
#[tauri::command]
fn get_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.report.lock().map_err(|e| e.to_string())?;
    let report = guard.as_ref().ok_or("Run a scan first.")?;
    serde_json::to_string_pretty(report).map_err(|e| e.to_string())
}

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
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![get_report, rescan, get_load, get_diagnostics, is_elevated])
        .run(tauri::generate_context!())
        .expect("error while running Kestrel");
}
```

- [ ] **Step 3: Icon** — `app/src-tauri/icons/kestrel.svg` (the "Hover" mark from decision 24 on a dark tile):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <rect width="1024" height="1024" rx="200" fill="#0C0E11"/>
  <g transform="translate(112 112) scale(8)" fill="#C8F53C">
    <polygon points="50,18 56,28 50,38 44,28"/>
    <polygon points="50,42 94,26 88,39 50,57 12,39 6,26"/>
    <polygon points="45,61 55,61 60,82 50,77 40,82"/>
  </g>
</svg>
```

Run: `cd app && npx tauri icon src-tauri/icons/kestrel.svg && cd ..`
Expected: `icons/icon.ico`, `icons/32x32.png`, … written.

- [ ] **Step 4: Verify it builds on the Mac**

Run: `cd app && npm run build && cd .. && cargo build -p kestrel && cargo test --workspace`
Expected: Vite builds `app/dist`; the Tauri crate compiles; all workspace tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock app
git commit -m "feat(app): Tauri shell with report, load and diagnostics commands

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 13: Frontend foundation — tokens, strings, API client, title bar, tabs

**Files:**
- Create: `app/src/strings.ts`, `app/src/styles/tokens.css`, `app/src/styles/base.css`, `app/src/api/types.ts`, `app/src/api/client.ts`, `app/src/api/findings.ts`, `app/src/api/findings.test.ts`, `app/src/api/mock-report.json` (generated), `app/src/App.tsx`, `app/src/components/{TitleBar,Logo,Banner}.tsx`, `app/src/pages/ComingSoon.tsx`
- Modify: `app/src/main.tsx`

**Interfaces:**
- Consumes: Tauri commands from Task 12; the JSON shape of `Report`/`LoadSample` (Tasks 2–11).
- Produces: TS types `Report, Finding, Fix, Score, BiosGuide, GuideSection, SystemSnapshot, GpuInfo, LoadSample, Severity, Category, Status, BiosTopic`; `client.ts`: `inTauri(): boolean`, `getReport()`, `rescan()`, `getLoad()`, `getDiagnostics()`, `isElevated()`, `openLink(url)` (all Promise-returning; browser mode uses `mock-report.json` and fake load); `findings.ts`: `topProblems(findings, limit = 3)`, `groupByCategory(findings)`, `CATEGORY_ORDER`, `mainGpu(gpus)`, `driverLabel(gpu)`, `cpuLabel(snapshot)`, `ramLabel(snapshot)`, `displayLabel(snapshot)`; components `TitleBar({ tab, onTab, admin })`, `type Tab = 'home' | 'health' | 'tweaks' | 'lockIn' | 'benchmark' | 'toolbox'`, `Logo({ size })`, `Banner({ children })`, `ComingSoon({ stage, title, body })`; `strings` object.

- [ ] **Step 1: Generate the sample report the browser preview and tests use**

Run: `cd app && mkdir -p src/api && npm run mock && head -3 src/api/mock-report.json && cd ..`
Expected: `{`, `"appVersion": "0.1.0",`, `"snapshot": {`.

- [ ] **Step 2: Types** — `app/src/api/types.ts`:

```ts
// Mirrors the Rust types in crates/kestrel-core (serde camelCase / kebab-case).
export type Severity = 'high' | 'medium' | 'low' | 'info';
export type Category = 'hardware' | 'display' | 'gpu' | 'windows' | 'storage' | 'background' | 'vanguard' | 'network';
export type Status = 'pass' | 'problem' | 'info' | 'unknown';
export type BiosTopic = 'xmp' | 'memory-gear' | 'iommu' | 'resizable-bar' | 'update-bios';

export type Fix =
  | { kind: 'bios-guide'; topic: BiosTopic }
  | { kind: 'later-stage'; stage: number }
  | { kind: 'steps'; steps: string[] }
  | { kind: 'link'; label: string; url: string }
  | { kind: 'nothing' };

export interface Finding { id: string; category: Category; severity: Severity; status: Status; title: string; detail: string; fix: Fix }
export interface Score { value: number; problems: number; potentialGain: number }
export interface GuideSection { topic: BiosTopic; title: string; why: string; steps: string[]; caution: string | null; link: { label: string; url: string } | null }
export interface BiosGuide { vendor: 'asus' | 'msi' | 'gigabyte' | 'asrock' | 'other'; board: string; enterKey: string; tip: string; sections: GuideSection[] }
export interface GpuInfo { name: string; vendor: 'nvidia' | 'amd' | 'intel' | 'virtual' | 'other'; driverVersion: string; driverDate: string | null; vramMb: number | null; isIntegrated: boolean }
export interface MemoryModule { capacityMb: number; configuredMts: number | null; maxMts: number | null; partNumber: string; manufacturer: string; bankLabel: string; deviceLocator: string }
export interface DisplayInfo { name: string; adapter: string; currentHz: number; maxHz: number; width: number; height: number; isPrimary: boolean; isVirtual: boolean }

export interface SystemSnapshot {
  schemaVersion: number;
  collectedAt: string;
  os: { caption: string; build: number; displayVersion: string | null };
  machine: { formFactor: 'desktop' | 'laptop' | 'unknown'; manufacturer: string; model: string; onBattery: boolean | null };
  cpu: { name: string; vendor: 'intel' | 'amd' | 'other'; cores: number; threads: number };
  gpus: GpuInfo[];
  memory: { totalMb: number; ddrGeneration: number | null; modules: MemoryModule[] };
  displays: DisplayInfo[];
  board: { manufacturer: string; product: string; biosVersion: string; biosDate: string | null };
  storage: { valorant: { path: string; driveLetter: string; media: 'nvme' | 'ssd' | 'hdd' | 'unknown'; freeGb: number; totalGb: number } | null };
  probeErrors: { probe: string; message: string }[];
}

export interface Report { appVersion: string; snapshot: SystemSnapshot; findings: Finding[]; score: Score; guide: BiosGuide }
export interface LoadSample { cpuPercent: number; ramPercent: number; gpuPercent: number | null; gpuTempC: number | null }
```

- [ ] **Step 3: Write the failing tests** — `app/src/api/findings.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import mock from './mock-report.json';
import type { Report } from './types';
import { cpuLabel, displayLabel, driverLabel, groupByCategory, mainGpu, ramLabel, topProblems } from './findings';

const report = mock as unknown as Report;

describe('findings helpers', () => {
  it('lists problems by severity, keeping check order within a severity', () => {
    expect(topProblems(report.findings).map((f) => f.id)).toEqual(['power-plan', 'bios-age', 'adapter-power']);
    expect(topProblems(report.findings, 1)).toHaveLength(1);
  });

  it('groups checks in a fixed category order and drops empty groups', () => {
    expect(groupByCategory(report.findings).map((g) => g.category)).toEqual([
      'hardware', 'display', 'gpu', 'windows', 'storage', 'background', 'vanguard', 'network',
    ]);
    expect(groupByCategory([])).toEqual([]);
  });

  it('picks the real GPU and shows NVIDIA driver versions the NVIDIA way', () => {
    const gpu = mainGpu(report.snapshot.gpus)!;
    expect(gpu.name).toBe('NVIDIA GeForce RTX 3060 Ti');
    expect(driverLabel(gpu)).toBe('610.74');
    expect(mainGpu([{ ...gpu, vendor: 'virtual' }])).toBeUndefined();
  });

  it('formats hardware labels', () => {
    expect(cpuLabel(report.snapshot)).toBe('Intel Core i9-11900K · 8C/16T');
    expect(ramLabel(report.snapshot)).toBe('32 GB DDR4 · 3600 MT/s');
    expect(displayLabel(report.snapshot)).toBe('360 Hz · 1920×1080');
  });
});
```

Run: `cd app && npm test; cd ..`
Expected: FAIL — `./findings` can't be resolved.

- [ ] **Step 4: Implement `app/src/api/findings.ts`**

```ts
import type { Category, Finding, GpuInfo, Severity, SystemSnapshot } from './types';

const SEVERITY_RANK: Record<Severity, number> = { high: 0, medium: 1, low: 2, info: 3 };
export const CATEGORY_ORDER: Category[] = ['hardware', 'display', 'gpu', 'windows', 'storage', 'background', 'vanguard', 'network'];

/** Problems only, most severe first; equal severities keep the check order. */
export function topProblems(findings: Finding[], limit = 3): Finding[] {
  return findings
    .filter((f) => f.status === 'problem')
    .sort((a, b) => SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity])
    .slice(0, limit);
}

export function groupByCategory(findings: Finding[]): { category: Category; findings: Finding[] }[] {
  return CATEGORY_ORDER.map((category) => ({ category, findings: findings.filter((f) => f.category === category) })).filter(
    (group) => group.findings.length > 0,
  );
}

/** Same rule as kestrel-core's gpu::main_gpu: biggest real discrete GPU, else first real one. */
export function mainGpu(gpus: GpuInfo[]): GpuInfo | undefined {
  const real = gpus.filter((g) => g.vendor !== 'virtual');
  const discrete = real.filter((g) => !g.isIntegrated).sort((a, b) => (b.vramMb ?? 0) - (a.vramMb ?? 0));
  return discrete[0] ?? real[0];
}

/** Windows driver version "32.0.16.1074" → NVIDIA's "610.74"; other vendors unchanged. */
export function driverLabel(gpu: GpuInfo): string {
  const parts = gpu.driverVersion.split('.');
  if (gpu.vendor !== 'nvidia' || parts.length !== 4 || parts.some((p) => !/^\d+$/.test(p))) return gpu.driverVersion;
  const digits = `${Number(parts[2]) % 10}${parts[3].padStart(4, '0')}`;
  return `${digits.slice(0, 3)}.${digits.slice(3)}`;
}

export function cpuLabel(s: SystemSnapshot): string {
  const name = s.cpu.name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/@.*$/, '')
    .replace(/^\d+(st|nd|rd|th) Gen\s+/i, '')
    .replace(/\s+\d+-Core Processor$/i, '')
    .replace(/\s+/g, ' ')
    .trim();
  return s.cpu.threads ? `${name} · ${s.cpu.cores}C/${s.cpu.threads}T` : name;
}

export function ramLabel(s: SystemSnapshot): string {
  const gb = Math.round(s.memory.totalMb / 1024);
  const ddr = s.memory.ddrGeneration ? ` DDR${s.memory.ddrGeneration}` : '';
  const speeds = s.memory.modules.map((m) => m.configuredMts ?? 0).filter((v) => v > 0);
  return `${gb} GB${ddr}${speeds.length ? ` · ${Math.min(...speeds)} MT/s` : ''}`;
}

export function displayLabel(s: SystemSnapshot): string {
  const main = s.displays.find((d) => d.isPrimary && !d.isVirtual) ?? s.displays.find((d) => !d.isVirtual);
  return main ? `${main.currentHz} Hz · ${main.width}×${main.height}` : '—';
}
```

Run: `cd app && npm test; cd ..`
Expected: `4 passed`.

- [ ] **Step 5: Client** — `app/src/api/client.ts`:

```ts
import mock from './mock-report.json';
import type { LoadSample, Report } from './types';

const sample = mock as unknown as Report;

/** True inside the Tauri window; false in a plain browser (UI development on the Mac). */
export const inTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(command: string): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command);
}

const later = <T,>(value: T, ms: number) => new Promise<T>((resolve) => setTimeout(() => resolve(value), ms));

export const getReport = (): Promise<Report> => (inTauri() ? call<Report>('get_report') : later(sample, 600));
export const rescan = (): Promise<Report> => (inTauri() ? call<Report>('rescan') : later(sample, 1800));
export const getDiagnostics = (): Promise<string> => (inTauri() ? call<string>('get_diagnostics') : Promise.resolve(JSON.stringify(sample, null, 2)));
export const isElevated = (): Promise<boolean> => (inTauri() ? call<boolean>('is_elevated') : Promise.resolve(false));

let tick = 0;
export const getLoad = (): Promise<LoadSample> => {
  if (inTauri()) return call<LoadSample>('get_load');
  tick += 1;
  const wave = (period: number, base: number, swing: number) => Math.round((base + swing * Math.sin(tick / period) + Math.random() * 3) * 10) / 10;
  return Promise.resolve({ cpuPercent: wave(3, 9, 5), ramPercent: 41, gpuPercent: wave(5, 4, 2), gpuTempC: 44 });
};

export async function openLink(url: string): Promise<void> {
  if (!inTauri()) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  const { openUrl } = await import('@tauri-apps/plugin-opener');
  await openUrl(url);
}
```

- [ ] **Step 6: Strings** — `app/src/strings.ts`:

```ts
/** Every user-facing string lives here (spec decision 17). Sentence case, no exclamation marks. */
export const strings = {
  app: { name: 'Kestrel' },
  tabs: { home: 'Home', health: 'Health', tweaks: 'Tweaks', lockIn: 'Lock In', benchmark: 'Benchmark', toolbox: 'Toolbox' },
  titleBar: { admin: 'admin', notAdmin: 'not admin', minimize: 'Minimise', maximize: 'Maximise', close: 'Close' },
  banners: {
    sample: 'Sample data: Kestrel isn’t running on Windows, so this is the test PC from the fixture.',
    windows10: 'You’re on Windows 10. Kestrel runs here, but it’s only tested on Windows 11.',
  },
  loading: { scanning: 'Scanning your PC…', failed: 'The scan didn’t finish.', retry: 'Try again' },
  home: {
    score: 'System score',
    fixes: (n: number) => (n === 0 ? 'Nothing to fix' : n === 1 ? '1 fix found' : `${n} fixes found`),
    gain: (n: number) => `Fixes could add up to ${n} points`,
    applyPlan: 'Apply plan',
    seeAll: 'See all checks',
    hardware: 'Hardware',
    lockIn: 'Lock In',
    lockInSoon: 'Arrives in stage 5',
    liveLoad: 'Live load',
    lastBenchmark: 'Last benchmark',
    noBenchmark: 'Benchmarks arrive in stage 2',
    rescan: 'Scan again',
    help: 'Help on GitHub',
  },
  hardware: { cpu: 'CPU', gpu: 'GPU', ram: 'RAM', display: 'Panel', os: 'OS', board: 'Board' },
  health: {
    title: 'Health Check',
    intro: 'Every check only reads your PC. Nothing is changed.',
    categories: {
      hardware: 'Hardware and BIOS', display: 'Display', gpu: 'Graphics', windows: 'Windows',
      storage: 'Storage', background: 'Background', vanguard: 'Vanguard readiness', network: 'Network',
    },
    status: { pass: 'OK', problem: 'Fix', info: 'Info', unknown: 'Unknown' },
    severity: { high: 'High', medium: 'Med', low: 'Low', info: 'Info' },
    biosGuide: 'Open BIOS guide',
    laterStage: (n: number) => `Kestrel fixes this in stage ${n}`,
    howToFix: 'How to fix',
    probeErrors: (n: number) => `${n} probe${n === 1 ? '' : 's'} couldn’t read everything. Copy diagnostics to share the details.`,
  },
  toolbox: {
    title: 'BIOS guide',
    intro: (board: string) => `Steps for ${board}. Kestrel can’t change BIOS settings, so these are the menus to look for.`,
    enterKey: (key: string) => `To open the BIOS, restart and press ${key}.`,
    why: 'Why',
    careful: 'Careful',
    later: 'Fixes and the VALORANT troubleshooter arrive in stage 6.',
  },
  comingSoon: {
    arrives: (n: number) => `Arrives in stage ${n}`,
    tweaks: { stage: 3, title: 'Tweaks', body: 'Reversible tweaks picked for your hardware, with Preview and one-click undo.' },
    lockIn: { stage: 5, title: 'Lock In', body: 'A tiny helper that boosts your PC while VALORANT runs and undoes everything when you quit.' },
    benchmark: { stage: 2, title: 'Benchmark', body: 'A 90-second run in the Range that measures average FPS and 1% lows, before and after.' },
  },
  diagnostics: { copy: 'Copy diagnostics', copied: 'Copied. Paste it in chat.', failed: 'Couldn’t copy. Try again.' },
  firstRun: {
    title: 'Let’s check your PC',
    body: 'Kestrel looks at your hardware and Windows settings. The scan only reads; it changes nothing.',
    start: 'Scan my PC',
    scanning: 'Scanning',
    steps: ['Hardware', 'Displays', 'Graphics', 'Windows', 'Storage', 'Vanguard', 'Network'],
    resultTitle: 'Here’s what Kestrel found',
    later: 'Benchmark and tweaks arrive in later stages.',
    toHome: 'Go to home',
  },
  about: { disclaimer: 'Kestrel is not affiliated with Riot Games. VALORANT is a trademark of Riot Games, Inc.' },
  links: { github: 'https://github.com/gabryelvs/kestrel/issues' },
} as const;
```

- [ ] **Step 7: Styles**

`app/src/styles/tokens.css`:
```css
:root {
  --bg: #0c0e11;
  --panel: #15181d;
  --panel-2: #1b1f25;
  --line: #232830;
  --line-strong: #3a414b;
  --text: #e9ecef;
  --muted: #8c949e;
  --accent: #c8f53c;
  --accent-ink: #0c0e11;
  --warn: #ffb547;
  --warn-ink: #2a1d05;
  --danger: #ff6b6b;
  --font-ui: 'Chakra Petch', system-ui, sans-serif;
  --font-num: 'JetBrains Mono', ui-monospace, monospace;
  --cut: 10px;
  --cut-s: 5px;
}
```

`app/src/styles/base.css`:
```css
* { box-sizing: border-box; }
html, body, #root { height: 100%; margin: 0; }
body { background: var(--bg); color: var(--text); font-family: var(--font-ui); font-size: 14px; line-height: 1.45; -webkit-font-smoothing: antialiased; user-select: none; overflow: hidden; }
button { font: inherit; color: inherit; background: none; border: 0; cursor: pointer; padding: 0; }
h1, h2, h3, p { margin: 0; }
:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

.app { display: flex; flex-direction: column; height: 100%; }
.content { flex: 1; min-height: 0; padding: 14px; }
.scroll { overflow-y: auto; }
.scroll::-webkit-scrollbar { width: 8px; }
.scroll::-webkit-scrollbar-thumb { background: var(--line-strong); }
.center { display: grid; place-items: center; align-content: center; gap: 10px; height: 100%; text-align: center; }
.muted { color: var(--muted); }
.num { font-family: var(--font-num); font-variant-numeric: tabular-nums; }
.label { font-size: 11px; letter-spacing: 0.14em; color: var(--muted); }

.panel { background: var(--panel); padding: 14px 16px; clip-path: polygon(var(--cut) 0, 100% 0, 100% calc(100% - var(--cut)), calc(100% - var(--cut)) 100%, 0 100%, 0 var(--cut)); }
.cut-s { clip-path: polygon(var(--cut-s) 0, 100% 0, 100% calc(100% - var(--cut-s)), calc(100% - var(--cut-s)) 100%, 0 100%, 0 var(--cut-s)); }

.btn { display: inline-flex; align-items: center; gap: 8px; padding: 8px 14px; font-size: 13px; font-weight: 500; }
.btn-primary { background: var(--accent); color: var(--accent-ink); }
.btn-primary:hover { filter: brightness(1.08); }
.btn-ghost { border: 1px solid var(--line-strong); }
.btn-ghost:hover { border-color: var(--muted); }
.link { color: var(--accent); font-size: 12px; }
.link:hover { text-decoration: underline; }

.tag { font-size: 11px; padding: 2px 7px; white-space: nowrap; }
.tag-high { background: var(--warn); color: var(--warn-ink); }
.tag-medium, .tag-low, .tag-info { background: #2a3038; color: var(--text); }

.titlebar { display: flex; align-items: center; gap: 10px; height: 44px; padding-left: 14px; border-bottom: 1px solid var(--line); flex: none; }
.brand { display: flex; align-items: center; gap: 8px; font-weight: 600; letter-spacing: 0.08em; }
.tabs { display: flex; gap: 2px; margin-left: 8px; }
.tab { padding: 5px 10px; font-size: 12.5px; letter-spacing: 0.04em; color: var(--muted); }
.tab:hover { color: var(--text); }
.tab-active { background: var(--accent); color: var(--accent-ink); font-weight: 600; }
.admin { margin-left: auto; display: flex; align-items: center; gap: 5px; font-size: 11px; color: var(--muted); }
.window-buttons { display: flex; align-self: stretch; }
.window-buttons button { width: 44px; display: grid; place-items: center; color: var(--muted); }
.window-buttons button:hover { background: var(--panel-2); color: var(--text); }
.window-buttons .close:hover { background: #c42b1c; color: #fff; }
.banner { padding: 6px 16px; font-size: 12px; background: var(--panel-2); color: var(--warn); border-bottom: 1px solid var(--line); flex: none; }

.coming-soon { max-width: 560px; margin: 48px auto; padding: 28px 32px; }
.coming-soon h1 { font-size: 28px; font-weight: 600; margin: 6px 0 8px; }

@media (prefers-reduced-motion: reduce) { * { animation: none !important; transition: none !important; } }
```

- [ ] **Step 8: Components and app shell**

`app/src/components/Logo.tsx`:
```tsx
/** The "Hover" mark (spec decision 24). */
export function Logo({ size = 16 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 100 100" fill="var(--accent)" aria-hidden="true">
      <polygon points="50,18 56,28 50,38 44,28" />
      <polygon points="50,42 94,26 88,39 50,57 12,39 6,26" />
      <polygon points="45,61 55,61 60,82 50,77 40,82" />
    </svg>
  );
}
```

`app/src/components/Banner.tsx`:
```tsx
import type { ReactNode } from 'react';

export function Banner({ children }: { children: ReactNode }) {
  return (
    <div className="banner" role="status">
      {children}
    </div>
  );
}
```

`app/src/components/TitleBar.tsx`:
```tsx
import { IconMinus, IconShieldCheck, IconShieldOff, IconSquare, IconX } from '@tabler/icons-react';
import { inTauri } from '../api/client';
import { strings } from '../strings';
import { Logo } from './Logo';

export type Tab = 'home' | 'health' | 'tweaks' | 'lockIn' | 'benchmark' | 'toolbox';
const TABS: Tab[] = ['home', 'health', 'tweaks', 'lockIn', 'benchmark', 'toolbox'];

async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  if (!inTauri()) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow()[action]();
}

export function TitleBar({ tab, onTab, admin }: { tab: Tab; onTab: (tab: Tab) => void; admin: boolean }) {
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <Logo size={16} />
        <span>{strings.app.name}</span>
      </div>
      <nav className="tabs" aria-label="Sections">
        {TABS.map((t) => (
          <button key={t} className={`tab cut-s${t === tab ? ' tab-active' : ''}`} aria-current={t === tab ? 'page' : undefined} onClick={() => onTab(t)}>
            {strings.tabs[t]}
          </button>
        ))}
      </nav>
      <span className="admin num">
        {admin ? <IconShieldCheck size={14} color="var(--accent)" aria-hidden /> : <IconShieldOff size={14} aria-hidden />}
        {admin ? strings.titleBar.admin : strings.titleBar.notAdmin}
      </span>
      <div className="window-buttons">
        <button aria-label={strings.titleBar.minimize} onClick={() => void windowAction('minimize')}>
          <IconMinus size={14} />
        </button>
        <button aria-label={strings.titleBar.maximize} onClick={() => void windowAction('toggleMaximize')}>
          <IconSquare size={12} />
        </button>
        <button aria-label={strings.titleBar.close} className="close" onClick={() => void windowAction('close')}>
          <IconX size={14} />
        </button>
      </div>
    </header>
  );
}
```

`app/src/pages/ComingSoon.tsx`:
```tsx
import { strings } from '../strings';

export function ComingSoon({ stage, title, body }: { stage: number; title: string; body: string }) {
  return (
    <section className="panel coming-soon">
      <div className="label">{strings.comingSoon.arrives(stage)}</div>
      <h1>{title}</h1>
      <p className="muted">{body}</p>
    </section>
  );
}
```

`app/src/App.tsx` (pages are filled in by Tasks 14–15; this version shows the score so every prop is used):
```tsx
import { useCallback, useEffect, useState } from 'react';
import { getReport, inTauri, isElevated, rescan } from './api/client';
import type { Report } from './api/types';
import { Banner } from './components/Banner';
import { TitleBar, type Tab } from './components/TitleBar';
import { ComingSoon } from './pages/ComingSoon';
import { strings } from './strings';

type Scan = { state: 'loading' } | { state: 'ready'; report: Report } | { state: 'failed'; message: string };

export function App() {
  const [tab, setTab] = useState<Tab>('home');
  const [scan, setScan] = useState<Scan>({ state: 'loading' });
  const [admin, setAdmin] = useState(false);

  const run = useCallback(async (fresh: boolean) => {
    setScan({ state: 'loading' });
    try {
      setScan({ state: 'ready', report: await (fresh ? rescan() : getReport()) });
    } catch (error) {
      setScan({ state: 'failed', message: String(error) });
    }
  }, []);

  useEffect(() => {
    void run(false);
    void isElevated().then(setAdmin);
  }, [run]);

  const report = scan.state === 'ready' ? scan.report : null;
  const build = report?.snapshot.os.build ?? 0;

  return (
    <div className="app">
      <TitleBar tab={tab} onTab={setTab} admin={admin} />
      {!inTauri() && <Banner>{strings.banners.sample}</Banner>}
      {build > 0 && build < 22000 && <Banner>{strings.banners.windows10}</Banner>}
      <main className="content scroll">
        {scan.state === 'loading' && <p className="center muted">{strings.loading.scanning}</p>}
        {scan.state === 'failed' && (
          <div className="center">
            <p>{strings.loading.failed}</p>
            <p className="muted">{scan.message}</p>
            <button className="btn btn-ghost cut-s" onClick={() => void run(true)}>
              {strings.loading.retry}
            </button>
          </div>
        )}
        {report && <Page tab={tab} report={report} onTab={setTab} onRescan={() => void run(true)} />}
      </main>
    </div>
  );
}

type PageProps = { tab: Tab; report: Report; onTab: (tab: Tab) => void; onRescan: () => void };

function Page({ tab, report, onTab, onRescan }: PageProps) {
  switch (tab) {
    case 'tweaks':
      return <ComingSoon {...strings.comingSoon.tweaks} />;
    case 'lockIn':
      return <ComingSoon {...strings.comingSoon.lockIn} />;
    case 'benchmark':
      return <ComingSoon {...strings.comingSoon.benchmark} />;
    default:
      return (
        <section className="panel">
          <div className="label">{strings.home.score}</div>
          <p className="num">{report.score.value}/100</p>
          <button className="link" onClick={() => onTab('health')}>{strings.home.seeAll}</button>{' '}
          <button className="link" onClick={onRescan}>{strings.home.rescan}</button>
        </section>
      );
  }
}
```

`app/src/main.tsx`:
```tsx
import '@fontsource/chakra-petch/400.css';
import '@fontsource/chakra-petch/500.css';
import '@fontsource/chakra-petch/600.css';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import './styles/tokens.css';
import './styles/base.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
```

- [ ] **Step 9: Verify**

Run: `cd app && npm test && npm run build && cd ..`
Expected: tests pass; build succeeds.
Then start `npm run dev` in `app/` and open http://localhost:1420 in a browser. Expected: dark window, lime logo and tabs, "Sample data" banner, score `86/100`; Tweaks/Lock In/Benchmark tabs show "Arrives in stage N" panels. Stop the dev server.

- [ ] **Step 10: Commit**

```bash
git add app
git commit -m "feat(app): design tokens, strings, API client and title bar

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 14: Home screen — score, top fixes, hardware, Lock In card, live load, diagnostics

**Files:**
- Create: `app/src/api/format.ts`, `app/src/api/format.test.ts`, `app/src/pages/Home.tsx`, `app/src/components/{ScoreCard,FindingRow,HardwarePanel,LockInCard,LoadGraph,CopyDiagnostics}.tsx`, `app/src/styles/home.css`
- Modify: `app/src/strings.ts` (add `home.stage`), `app/src/main.tsx` (import `home.css`), `app/src/App.tsx` (render `Home`)

**Interfaces:**
- Consumes: `client.{getLoad, getDiagnostics, openLink}`, `findings.{topProblems, mainGpu, driverLabel, cpuLabel, ramLabel, displayLabel}`, `strings`, `Tab`.
- Produces: `format.ts`: `filledSegments(score: number): number` (0–20), `osLabel(s: SystemSnapshot): string`, `toPoints(values: number[], window: number): string`, `loadSummary(sample: LoadSample): string`; `Home({ report, onTab, onGuide, onRescan })` where `onGuide: (topic: BiosTopic) => void`; `FindingRow({ finding, onTab, onGuide })`.

- [ ] **Step 1: Write the failing tests** — `app/src/api/format.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import mock from './mock-report.json';
import type { Report } from './types';
import { filledSegments, loadSummary, osLabel, toPoints } from './format';

const report = mock as unknown as Report;

describe('format helpers', () => {
  it('fills one segment per 5 points', () => {
    expect(filledSegments(86)).toBe(17);
    expect(filledSegments(0)).toBe(0);
    expect(filledSegments(100)).toBe(20);
    expect(filledSegments(140)).toBe(20);
  });

  it('labels Windows without the vendor prefix', () => {
    expect(osLabel(report.snapshot)).toBe('Windows 11 Pro 25H2');
  });

  it('right-aligns graph points and flips y', () => {
    expect(toPoints([10, 20], 60)).toBe('58,90 59,80');
    expect(toPoints([], 60)).toBe('');
  });

  it('summarises load, adding GPU and temperature only when known', () => {
    expect(loadSummary({ cpuPercent: 6.24, ramPercent: 41, gpuPercent: 3, gpuTempC: 44 })).toBe('CPU 6% · GPU 3% · 44°C');
    expect(loadSummary({ cpuPercent: 6.24, ramPercent: 41, gpuPercent: null, gpuTempC: null })).toBe('CPU 6% · RAM 41%');
  });
});
```

Run: `cd app && npm test; cd ..`
Expected: FAIL — `./format` can't be resolved.

- [ ] **Step 2: Implement `app/src/api/format.ts`**

```ts
import type { LoadSample, SystemSnapshot } from './types';

export const filledSegments = (score: number): number => Math.max(0, Math.min(20, Math.round(score / 5)));

export const osLabel = (s: SystemSnapshot): string =>
  `${s.os.caption.replace(/^Microsoft\s+/, '')} ${s.os.displayVersion ?? ''}`.trim();

/** SVG polyline points for 0–100 values, newest at the right edge of a `window`-wide graph. */
export function toPoints(values: number[], window: number): string {
  const offset = window - values.length;
  return values.map((v, i) => `${offset + i},${100 - Math.max(0, Math.min(100, v))}`).join(' ');
}

export function loadSummary(s: LoadSample): string {
  const parts = [`CPU ${Math.round(s.cpuPercent)}%`];
  if (s.gpuPercent !== null) parts.push(`GPU ${Math.round(s.gpuPercent)}%`);
  if (s.gpuTempC !== null) parts.push(`${Math.round(s.gpuTempC)}°C`);
  if (s.gpuPercent === null && s.gpuTempC === null) parts.push(`RAM ${Math.round(s.ramPercent)}%`);
  return parts.join(' · ');
}
```

Run: `cd app && npm test; cd ..`
Expected: all tests pass (4 findings + 4 format).

- [ ] **Step 3: Strings** — in `strings.ts`, inside `home`, add: `stage: (n: number) => \`Stage ${n}\`,`

- [ ] **Step 4: Components**

`app/src/components/FindingRow.tsx`:
```tsx
import { openLink } from '../api/client';
import type { BiosTopic, Finding } from '../api/types';
import { strings } from '../strings';
import type { Tab } from './TitleBar';

type Props = { finding: Finding; onTab: (tab: Tab) => void; onGuide: (topic: BiosTopic) => void };

/** One problem on the home screen: severity tag, what's wrong, and the quickest way to fix it. */
export function FindingRow({ finding, onTab, onGuide }: Props) {
  const { fix } = finding;
  return (
    <div className="finding-row">
      <span className={`tag tag-${finding.severity} cut-s`}>{strings.health.severity[finding.severity]}</span>
      <span className="title">{finding.title}</span>
      {fix.kind === 'bios-guide' && (
        <button className="link" onClick={() => onGuide(fix.topic)}>{strings.health.biosGuide}</button>
      )}
      {fix.kind === 'later-stage' && <span className="muted small">{strings.home.stage(fix.stage)}</span>}
      {fix.kind === 'steps' && (
        <button className="link" onClick={() => onTab('health')}>{strings.health.howToFix}</button>
      )}
      {fix.kind === 'link' && (
        <button className="link" onClick={() => void openLink(fix.url)}>{fix.label}</button>
      )}
    </div>
  );
}
```

`app/src/components/ScoreCard.tsx`:
```tsx
import { topProblems } from '../api/findings';
import { filledSegments } from '../api/format';
import type { BiosTopic, Report } from '../api/types';
import { strings } from '../strings';
import { FindingRow } from './FindingRow';
import type { Tab } from './TitleBar';

type Props = { report: Report; onTab: (tab: Tab) => void; onGuide: (topic: BiosTopic) => void };

export function ScoreCard({ report, onTab, onGuide }: Props) {
  const { score } = report;
  const filled = filledSegments(score.value);
  return (
    <section className="panel score-card">
      <div className="label">{strings.home.score}</div>
      <div className="score-line">
        <span className="num score-number">{score.value}</span>
        <span className="num muted">/100</span>
        <span className="fixes">{strings.home.fixes(score.problems)}</span>
      </div>
      <div className="segments" aria-hidden="true">
        {Array.from({ length: 20 }, (_, i) => (
          <span key={i} className={i < filled ? 'on' : undefined} />
        ))}
      </div>
      {score.potentialGain > 0 && <p className="muted small">{strings.home.gain(score.potentialGain)}</p>}
      <div className="score-findings">
        {topProblems(report.findings).map((f) => (
          <FindingRow key={f.id} finding={f} onTab={onTab} onGuide={onGuide} />
        ))}
      </div>
      <div className="actions">
        <button className="btn btn-primary cut-s" onClick={() => onTab('tweaks')}>{strings.home.applyPlan}</button>
        <button className="btn btn-ghost cut-s" onClick={() => onTab('health')}>{strings.home.seeAll}</button>
      </div>
    </section>
  );
}
```

`app/src/components/HardwarePanel.tsx`:
```tsx
import { Fragment } from 'react';
import { cpuLabel, displayLabel, driverLabel, mainGpu, ramLabel } from '../api/findings';
import { osLabel } from '../api/format';
import type { SystemSnapshot } from '../api/types';
import { strings } from '../strings';

export function HardwarePanel({ snapshot: s }: { snapshot: SystemSnapshot }) {
  const gpu = mainGpu(s.gpus);
  const rows: [string, string][] = [
    [strings.hardware.cpu, cpuLabel(s)],
    [strings.hardware.gpu, gpu ? `${gpu.name.replace(/^(NVIDIA GeForce|AMD Radeon) /, '')} · ${driverLabel(gpu)}` : '—'],
    [strings.hardware.ram, ramLabel(s)],
    [strings.hardware.display, displayLabel(s)],
    [strings.hardware.os, osLabel(s)],
    [strings.hardware.board, s.board.product || '—'],
  ];
  return (
    <section className="panel">
      <div className="label">{strings.home.hardware}</div>
      <dl className="hw num">
        {rows.map(([name, value]) => (
          <Fragment key={name}>
            <dt>{name}</dt>
            <dd title={value}>{value}</dd>
          </Fragment>
        ))}
      </dl>
    </section>
  );
}
```

`app/src/components/LockInCard.tsx`:
```tsx
import { strings } from '../strings';

/** Placeholder until stage 5: shows where Lock In will live, switched off. */
export function LockInCard() {
  return (
    <section className="panel lock-in">
      <div>
        <div className="label">{strings.home.lockIn}</div>
        <div className="muted small">{strings.home.lockInSoon}</div>
      </div>
      <span className="toggle cut-s" aria-hidden="true" />
    </section>
  );
}
```

`app/src/components/LoadGraph.tsx`:
```tsx
import { useEffect, useState } from 'react';
import { getLoad } from '../api/client';
import { loadSummary, toPoints } from '../api/format';
import type { LoadSample } from '../api/types';
import { strings } from '../strings';

const WINDOW = 60;

/** Polls live load once a second (skipping while the window is hidden) and draws the last minute. */
export function LoadGraph() {
  const [samples, setSamples] = useState<LoadSample[]>([]);

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      if (document.hidden) return;
      try {
        const sample = await getLoad();
        if (alive) setSamples((previous) => [...previous.slice(-(WINDOW - 1)), sample]);
      } catch {
        // Keep the last values; the next tick tries again.
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  const last = samples.at(-1);
  const hasGpu = samples.some((s) => s.gpuPercent !== null);
  return (
    <section className="panel">
      <div className="graph-head">
        <span className="label">{strings.home.liveLoad}</span>
        <span className="num muted small">{last ? loadSummary(last) : '…'}</span>
      </div>
      <svg className="graph" viewBox={`0 0 ${WINDOW - 1} 100`} preserveAspectRatio="none" aria-hidden="true">
        {[25, 50, 75].map((y) => (
          <line key={y} x1={0} x2={WINDOW - 1} y1={y} y2={y} stroke="var(--line)" strokeWidth={1} vectorEffect="non-scaling-stroke" />
        ))}
        {hasGpu && (
          <polyline points={toPoints(samples.map((s) => s.gpuPercent ?? 0), WINDOW)} fill="none" stroke="var(--muted)" strokeWidth={1.25} vectorEffect="non-scaling-stroke" />
        )}
        <polyline points={toPoints(samples.map((s) => s.cpuPercent), WINDOW)} fill="none" stroke="var(--accent)" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
      </svg>
    </section>
  );
}
```

`app/src/components/CopyDiagnostics.tsx`:
```tsx
import { IconClipboard } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { getDiagnostics } from '../api/client';
import { strings } from '../strings';

/** Copies the full report as JSON so a friend can paste it to the owner (spec decision 10). */
export function CopyDiagnostics() {
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    if (!note) return;
    const id = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(id);
  }, [note]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await getDiagnostics());
      setNote(strings.diagnostics.copied);
    } catch {
      setNote(strings.diagnostics.failed);
    }
  };

  return (
    <span className="copy">
      <button className="btn btn-ghost cut-s" onClick={() => void copy()}>
        <IconClipboard size={14} aria-hidden /> {strings.diagnostics.copy}
      </button>
      {note && <span className="toast" role="status">{note}</span>}
    </span>
  );
}
```

`app/src/pages/Home.tsx`:
```tsx
import { IconRefresh } from '@tabler/icons-react';
import { openLink } from '../api/client';
import type { BiosTopic, Report } from '../api/types';
import { CopyDiagnostics } from '../components/CopyDiagnostics';
import { HardwarePanel } from '../components/HardwarePanel';
import { LoadGraph } from '../components/LoadGraph';
import { LockInCard } from '../components/LockInCard';
import { ScoreCard } from '../components/ScoreCard';
import type { Tab } from '../components/TitleBar';
import { strings } from '../strings';

type Props = { report: Report; onTab: (tab: Tab) => void; onGuide: (topic: BiosTopic) => void; onRescan: () => void };

export function Home({ report, onTab, onGuide, onRescan }: Props) {
  return (
    <div className="home">
      <ScoreCard report={report} onTab={onTab} onGuide={onGuide} />
      <div className="home-side">
        <HardwarePanel snapshot={report.snapshot} />
        <LockInCard />
        <LoadGraph />
      </div>
      <section className="panel home-bench">
        <div className="label">{strings.home.lastBenchmark}</div>
        <p className="muted small">{strings.home.noBenchmark}</p>
      </section>
      <footer className="home-footer">
        <CopyDiagnostics />
        <button className="btn btn-ghost cut-s" onClick={onRescan}>
          <IconRefresh size={14} aria-hidden /> {strings.home.rescan}
        </button>
        <button className="link" onClick={() => void openLink(strings.links.github)}>{strings.home.help}</button>
        <span className="muted disclaimer">{strings.about.disclaimer}</span>
      </footer>
    </div>
  );
}
```

`app/src/styles/home.css` (import it in `main.tsx` after `base.css`):
```css
.home { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr); gap: 10px; }
.home-side { display: grid; gap: 10px; align-content: start; }
.home-bench, .home-footer { grid-column: 1 / -1; }
.home-footer { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; padding-top: 2px; }
.disclaimer { margin-left: auto; font-size: 11px; }
.score-line { display: flex; align-items: baseline; gap: 6px; margin-top: 2px; }
.score-number { font-size: 52px; line-height: 1; }
.fixes { margin-left: auto; font-size: 12px; color: var(--accent); }
.segments { display: grid; grid-template-columns: repeat(20, 1fr); gap: 3px; margin: 12px 0 8px; }
.segments span { height: 6px; background: var(--line); }
.segments span.on { background: var(--accent); }
.score-findings { margin-top: 10px; }
.finding-row { display: flex; align-items: center; gap: 10px; padding: 9px 0; border-top: 1px solid var(--line); font-size: 13px; }
.finding-row .title { flex: 1; min-width: 0; }
.actions { display: flex; gap: 8px; margin-top: 12px; }
.small { font-size: 12px; }
.hw { display: grid; grid-template-columns: 56px minmax(0, 1fr); row-gap: 5px; margin: 8px 0 0; font-size: 12px; }
.hw dt { color: var(--muted); }
.hw dd { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.lock-in { display: flex; align-items: center; gap: 10px; }
.toggle { margin-left: auto; width: 34px; height: 18px; background: var(--line); position: relative; flex: none; }
.toggle::after { content: ''; position: absolute; left: 3px; top: 3px; width: 12px; height: 12px; background: var(--muted); }
.graph-head { display: flex; align-items: baseline; gap: 8px; }
.graph-head .num { margin-left: auto; }
.graph { width: 100%; height: 64px; display: block; margin-top: 8px; }
.copy { display: inline-flex; align-items: center; gap: 10px; }
.toast { font-size: 12px; color: var(--accent); }
```

- [ ] **Step 5: Show Home in `App.tsx`** — import `Home` and replace the `default:` branch of `Page` with:

```tsx
    default:
      return <Home report={report} onTab={onTab} onGuide={() => onTab('toolbox')} onRescan={onRescan} />;
```
(Task 15 replaces `App.tsx` entirely, adding the guide focus and the Health/Toolbox pages.)

- [ ] **Step 6: Verify**

Run: `cd app && npm test && npm run build && cd ..`
Expected: tests pass; build succeeds.
Start `npm run dev` in `app/`, open http://localhost:1420. Expected: score card with `86`, 17 lime segments, "3 fixes found", rows "Power plan is Balanced" (Med, Stage 3), "BIOS is from 2024-05-20" (Low, Open BIOS guide), "Network adapter power saving is on" (Low, Stage 4); hardware panel with `Intel Core i9-11900K · 8C/16T`, `RTX 3060 Ti · 610.74`, `32 GB DDR4 · 3600 MT/s`, `360 Hz · 1920×1080`, `Windows 11 Pro 25H2`, `ROG MAXIMUS XIII HERO`; a moving lime line in Live load; "Copy diagnostics" shows "Copied. Paste it in chat." Take a screenshot for the owner.

- [ ] **Step 7: Commit**

```bash
git add app
git commit -m "feat(app): home screen with score, fixes, hardware and live load

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 15: Health page, BIOS guide page, guided first run

**Files:**
- Create: `app/src/pages/Health.tsx`, `app/src/pages/Toolbox.tsx`, `app/src/first-run/FirstRun.tsx`, `app/src/first-run/flag.ts`, `app/src/first-run/flag.test.ts`, `app/src/styles/pages.css`
- Modify: `app/src/App.tsx` (replace), `app/src/main.tsx` (import `pages.css`)

**Interfaces:**
- Consumes: `groupByCategory`, `topProblems`, `client.{rescan, openLink}`, `strings`, `Home` (Task 14), `ComingSoon`, `TitleBar`, `Banner`.
- Produces: `Health({ report, onGuide })`; `Toolbox({ guide, focus })` (scrolls to and marks the `focus` section); `FirstRun({ onDone })` where `onDone(report: Report)`; `flag.ts`: `firstRunDone(): boolean`, `markFirstRunDone(): void` (localStorage key `kestrel.firstRunDone`, safe when storage throws).

- [ ] **Step 1: Write the failing test** — `app/src/first-run/flag.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest';
import { firstRunDone, markFirstRunDone } from './flag';

afterEach(() => vi.unstubAllGlobals());

describe('first-run flag', () => {
  it('is false until marked, then true', () => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
    expect(firstRunDone()).toBe(false);
    markFirstRunDone();
    expect(firstRunDone()).toBe(true);
  });

  it('treats unavailable storage as a first run and never throws', () => {
    vi.stubGlobal('localStorage', { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('blocked'); } });
    expect(firstRunDone()).toBe(false);
    expect(() => markFirstRunDone()).not.toThrow();
  });
});
```

Run: `cd app && npm test; cd ..`
Expected: FAIL — `./flag` can't be resolved.

- [ ] **Step 2: Implement `app/src/first-run/flag.ts`**

```ts
const KEY = 'kestrel.firstRunDone';

export function firstRunDone(): boolean {
  try {
    return localStorage.getItem(KEY) === '1';
  } catch {
    return false;
  }
}

export function markFirstRunDone(): void {
  try {
    localStorage.setItem(KEY, '1');
  } catch {
    // Storage can be unavailable; the guided scan will simply show again next time.
  }
}
```

Run: `cd app && npm test; cd ..`
Expected: all tests pass.

- [ ] **Step 3: Health page** — `app/src/pages/Health.tsx`:

```tsx
import { IconAlertTriangle, IconCheck, IconHelpCircle, IconInfoCircle } from '@tabler/icons-react';
import { useState, type ReactElement } from 'react';
import { openLink } from '../api/client';
import { groupByCategory } from '../api/findings';
import type { BiosTopic, Finding, Report, Status } from '../api/types';
import { strings } from '../strings';

const ICONS: Record<Status, ReactElement> = {
  pass: <IconCheck size={16} color="var(--accent)" aria-hidden />,
  problem: <IconAlertTriangle size={16} color="var(--warn)" aria-hidden />,
  info: <IconInfoCircle size={16} color="var(--muted)" aria-hidden />,
  unknown: <IconHelpCircle size={16} color="var(--muted)" aria-hidden />,
};

export function Health({ report, onGuide }: { report: Report; onGuide: (topic: BiosTopic) => void }) {
  const errors = report.snapshot.probeErrors;
  return (
    <div className="page">
      <header className="page-head">
        <h1>{strings.health.title}</h1>
        <p className="muted">{strings.health.intro}</p>
      </header>
      {errors.length > 0 && <p className="notice cut-s">{strings.health.probeErrors(errors.length)}</p>}
      {groupByCategory(report.findings).map((group) => (
        <section key={group.category} className="panel group">
          <h2 className="label">{strings.health.categories[group.category]}</h2>
          {group.findings.map((finding) => (
            <CheckRow key={finding.id} finding={finding} onGuide={onGuide} />
          ))}
        </section>
      ))}
    </div>
  );
}

function CheckRow({ finding, onGuide }: { finding: Finding; onGuide: (topic: BiosTopic) => void }) {
  const [open, setOpen] = useState(false);
  const { fix } = finding;
  return (
    <div className={`check check-${finding.status}`}>
      <span className="status-icon" title={strings.health.status[finding.status]}>
        {ICONS[finding.status]}
      </span>
      <div>
        <div className="check-title">
          {finding.title}
          {finding.status === 'problem' && (
            <span className={`tag tag-${finding.severity} cut-s`}>{strings.health.severity[finding.severity]}</span>
          )}
        </div>
        {finding.detail && <p className="muted small">{finding.detail}</p>}
        {fix.kind === 'steps' && open && (
          <ol className="steps">
            {fix.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        )}
      </div>
      <div className="check-action">
        {fix.kind === 'bios-guide' && <button className="link" onClick={() => onGuide(fix.topic)}>{strings.health.biosGuide}</button>}
        {fix.kind === 'later-stage' && <span className="muted small">{strings.health.laterStage(fix.stage)}</span>}
        {fix.kind === 'steps' && (
          <button className="link" aria-expanded={open} onClick={() => setOpen(!open)}>{strings.health.howToFix}</button>
        )}
        {fix.kind === 'link' && <button className="link" onClick={() => void openLink(fix.url)}>{fix.label}</button>}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: BIOS guide page** — `app/src/pages/Toolbox.tsx`:

```tsx
import { useEffect, useRef } from 'react';
import { openLink } from '../api/client';
import type { BiosGuide, BiosTopic } from '../api/types';
import { strings } from '../strings';

export function Toolbox({ guide, focus }: { guide: BiosGuide; focus: BiosTopic | null }) {
  const sections = useRef(new Map<BiosTopic, HTMLElement>());

  useEffect(() => {
    if (focus) sections.current.get(focus)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, [focus]);

  return (
    <div className="page">
      <header className="page-head">
        <h1>{strings.toolbox.title}</h1>
        <p className="muted">{strings.toolbox.intro(guide.board)}</p>
        <p>
          {strings.toolbox.enterKey(guide.enterKey)} <span className="muted">{guide.tip}</span>
        </p>
      </header>
      {guide.sections.map((section) => (
        <section
          key={section.topic}
          ref={(el) => {
            if (el) sections.current.set(section.topic, el);
          }}
          className={`panel guide${section.topic === focus ? ' guide-focus' : ''}`}
        >
          <h2>{section.title}</h2>
          <p className="muted">
            <span className="label">{strings.toolbox.why}</span> {section.why}
          </p>
          <ol className="steps">
            {section.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
          {section.caution && (
            <p className="caution">
              <span className="label">{strings.toolbox.careful}</span> {section.caution}
            </p>
          )}
          {section.link && (
            <button className="link" onClick={() => void openLink(section.link!.url)}>{section.link.label}</button>
          )}
        </section>
      ))}
      <p className="muted small">{strings.toolbox.later}</p>
    </div>
  );
}
```

- [ ] **Step 5: First run** — `app/src/first-run/FirstRun.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { rescan } from '../api/client';
import { topProblems } from '../api/findings';
import type { Report } from '../api/types';
import { Logo } from '../components/Logo';
import { strings } from '../strings';

type Phase = { name: 'welcome'; error?: string } | { name: 'scanning' } | { name: 'result'; report: Report };

/** Spec decision 22, stage-1 version: welcome → scan → what Kestrel found → home. */
export function FirstRun({ onDone }: { onDone: (report: Report) => void }) {
  const [phase, setPhase] = useState<Phase>({ name: 'welcome' });
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (phase.name !== 'scanning') return;
    const id = window.setInterval(() => setStep((s) => (s + 1) % strings.firstRun.steps.length), 450);
    return () => window.clearInterval(id);
  }, [phase.name]);

  const start = async () => {
    setPhase({ name: 'scanning' });
    try {
      setPhase({ name: 'result', report: await rescan() });
    } catch (error) {
      setPhase({ name: 'welcome', error: String(error) });
    }
  };

  return (
    <div className="first-run">
      <section className="panel">
        <Logo size={40} />
        {phase.name === 'welcome' && (
          <>
            <h1>{strings.firstRun.title}</h1>
            <p className="muted">{strings.firstRun.body}</p>
            {phase.error && <p className="caution">{strings.loading.failed} {phase.error}</p>}
            <div>
              <button className="btn btn-primary cut-s" onClick={() => void start()}>{strings.firstRun.start}</button>
            </div>
          </>
        )}
        {phase.name === 'scanning' && (
          <>
            <h1>{strings.firstRun.scanning}</h1>
            <p className="scan-step" aria-live="polite">{strings.firstRun.steps[step]}…</p>
          </>
        )}
        {phase.name === 'result' && (
          <>
            <h1>{strings.firstRun.resultTitle}</h1>
            <p>
              <span className="num score-number">{phase.report.score.value}</span>
              <span className="num muted">/100</span> <span className="fixes">{strings.home.fixes(phase.report.score.problems)}</span>
            </p>
            <ul className="steps">
              {topProblems(phase.report.findings).map((f) => (
                <li key={f.id}>{f.title}</li>
              ))}
            </ul>
            <p className="muted small">{strings.firstRun.later}</p>
            <div>
              <button className="btn btn-primary cut-s" onClick={() => onDone(phase.report)}>{strings.firstRun.toHome}</button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
```

- [ ] **Step 6: Styles** — `app/src/styles/pages.css` (import in `main.tsx` after `home.css`):

```css
.page { max-width: 880px; margin: 0 auto; display: grid; gap: 10px; }
.page-head { display: grid; gap: 4px; padding: 6px 2px 4px; }
.page-head h1 { font-size: 24px; font-weight: 600; }
.notice { background: var(--panel-2); color: var(--warn); padding: 8px 12px; font-size: 12px; }
.group h2 { margin-bottom: 2px; }
.check { display: grid; grid-template-columns: 22px minmax(0, 1fr) auto; gap: 10px; padding: 10px 0; border-top: 1px solid var(--line); }
.group h2 + .check { border-top: 0; }
.check-title { font-size: 13.5px; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.check-action { align-self: start; text-align: right; max-width: 220px; }
.status-icon { padding-top: 2px; }
.steps { margin: 8px 0 0; padding-left: 18px; display: grid; gap: 4px; font-size: 13px; }
.guide { display: grid; gap: 8px; }
.guide h2 { font-size: 16px; font-weight: 600; }
.guide-focus { box-shadow: inset 3px 0 0 var(--accent); }
.caution { font-size: 12.5px; color: var(--warn); }
.first-run { flex: 1; display: grid; place-items: center; }
.first-run .panel { width: min(560px, 92%); padding: 28px 32px; display: grid; gap: 12px; }
.first-run h1 { font-size: 26px; font-weight: 600; }
.scan-step { font-family: var(--font-num); color: var(--accent); }
```

- [ ] **Step 7: Final `App.tsx`** (replace the whole file):

```tsx
import { useCallback, useEffect, useRef, useState } from 'react';
import { getReport, inTauri, isElevated, rescan } from './api/client';
import type { BiosTopic, Report } from './api/types';
import { Banner } from './components/Banner';
import { TitleBar, type Tab } from './components/TitleBar';
import { FirstRun } from './first-run/FirstRun';
import { firstRunDone, markFirstRunDone } from './first-run/flag';
import { ComingSoon } from './pages/ComingSoon';
import { Health } from './pages/Health';
import { Home } from './pages/Home';
import { Toolbox } from './pages/Toolbox';
import { strings } from './strings';

type Scan = { state: 'loading' } | { state: 'ready'; report: Report } | { state: 'failed'; message: string };

export function App() {
  const [tab, setTab] = useState<Tab>('home');
  const [scan, setScan] = useState<Scan>({ state: 'loading' });
  const [admin, setAdmin] = useState(false);
  const [guideTopic, setGuideTopic] = useState<BiosTopic | null>(null);
  const [firstRun, setFirstRun] = useState(() => !firstRunDone());
  const skippedInitialScan = useRef(firstRun);

  const run = useCallback(async (fresh: boolean) => {
    setScan({ state: 'loading' });
    try {
      setScan({ state: 'ready', report: await (fresh ? rescan() : getReport()) });
    } catch (error) {
      setScan({ state: 'failed', message: String(error) });
    }
  }, []);

  useEffect(() => {
    if (!skippedInitialScan.current) void run(false);
    void isElevated().then(setAdmin);
  }, [run]);

  const openGuide = (topic: BiosTopic) => {
    setGuideTopic(topic);
    setTab('toolbox');
  };

  const chooseTab = (next: Tab) => {
    if (next !== 'toolbox') setGuideTopic(null);
    setTab(next);
  };

  const finishFirstRun = (report: Report) => {
    markFirstRunDone();
    setScan({ state: 'ready', report });
    setFirstRun(false);
  };

  const report = scan.state === 'ready' ? scan.report : null;
  const build = report?.snapshot.os.build ?? 0;

  return (
    <div className="app">
      <TitleBar tab={tab} onTab={chooseTab} admin={admin} />
      {!inTauri() && <Banner>{strings.banners.sample}</Banner>}
      {build > 0 && build < 22000 && <Banner>{strings.banners.windows10}</Banner>}
      {firstRun ? (
        <FirstRun onDone={finishFirstRun} />
      ) : (
        <main className="content scroll">
          {scan.state === 'loading' && <p className="center muted">{strings.loading.scanning}</p>}
          {scan.state === 'failed' && (
            <div className="center">
              <p>{strings.loading.failed}</p>
              <p className="muted">{scan.message}</p>
              <button className="btn btn-ghost cut-s" onClick={() => void run(true)}>{strings.loading.retry}</button>
            </div>
          )}
          {report && tab === 'home' && <Home report={report} onTab={chooseTab} onGuide={openGuide} onRescan={() => void run(true)} />}
          {report && tab === 'health' && <Health report={report} onGuide={openGuide} />}
          {report && tab === 'toolbox' && <Toolbox guide={report.guide} focus={guideTopic} />}
          {tab === 'tweaks' && <ComingSoon {...strings.comingSoon.tweaks} />}
          {tab === 'lockIn' && <ComingSoon {...strings.comingSoon.lockIn} />}
          {tab === 'benchmark' && <ComingSoon {...strings.comingSoon.benchmark} />}
        </main>
      )}
    </div>
  );
}
```

- [ ] **Step 8: Verify**

Run: `cd app && npm test && npm run build && cd ..`
Expected: tests pass (10 total); build succeeds.
Start `npm run dev` and open http://localhost:1420 in a fresh browser profile (or clear site data). Expected:
1. First run: "Let's check your PC" → Scan my PC → cycling step names → "Here's what Kestrel found" with 86/100 and three problems → Go to home.
2. Reload: goes straight to Home.
3. Health tab: 8 groups, 27 rows; "How to fix" expands steps; the sample-data note about 1 probe shows at the top.
4. Home → "Open BIOS guide" on the BIOS row → Toolbox scrolls to "Update the BIOS", marked with a lime bar; the guide shows ASUS steps with "Extreme Tweaker".
Take screenshots of Home, Health and Toolbox for the owner.

- [ ] **Step 9: Commit**

```bash
git add app
git commit -m "feat(app): Health Check page, BIOS guide and guided first run

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 16: CI — Linux logic tests, UI tests, Windows build with probe smoke test

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`

**Interfaces:**
- Consumes: everything above.
- Produces: green checks on the `stage-1` pull request; artifact `kestrel-windows` containing `Kestrel_0.1.0_x64-setup.exe` and `probe-report.json` (the probe's view of the CI runner); a release workflow that publishes a GitHub pre-release for any `v*` tag.

- [ ] **Step 1: Look up current action versions** (use these majors in the YAML below if they differ)

```bash
for a in actions/checkout actions/setup-node actions/upload-artifact Swatinem/rust-cache tauri-apps/tauri-action; do
  gh api "repos/$a/releases/latest" --jq ".tag_name + \"  $a\""
done
```

- [ ] **Step 2: Write `.github/workflows/ci.yml`**

```yaml
name: CI
on:
  push:
    branches: [main, "stage-*"]
  pull_request:

permissions:
  contents: read

jobs:
  core:
    name: Core logic (Linux)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy -p kestrel-core -p kestrel-probe -- -D warnings
      - run: cargo test -p kestrel-core -p kestrel-probe

  ui:
    name: UI tests
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: app
    steps:
      - uses: actions/checkout@v5
      - uses: actions/setup-node@v5
        with:
          node-version: lts/*
          cache: npm
          cache-dependency-path: app/package-lock.json
      - run: npm ci
      - run: npm test
      - run: npx tsc --noEmit

  windows:
    name: Windows build
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - uses: actions/setup-node@v5
        with:
          node-version: lts/*
          cache: npm
          cache-dependency-path: app/package-lock.json
      - run: cargo clippy -p kestrel-probe -- -D warnings
      - run: cargo test -p kestrel-core -p kestrel-probe
      - name: Probe this runner (smoke test)
        run: cargo run -q -p kestrel-probe --bin kestrel-probe -- --pretty > probe-report.json
      - run: npm ci
        working-directory: app
      - run: npx tauri build
        working-directory: app
      - uses: actions/upload-artifact@v5
        with:
          name: kestrel-windows
          path: |
            target/release/bundle/nsis/*.exe
            probe-report.json
```

- [ ] **Step 3: Write `.github/workflows/release.yml`**

```yaml
name: Release
on:
  push:
    tags: ["v*"]

permissions:
  contents: write

jobs:
  release:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: actions/setup-node@v5
        with:
          node-version: lts/*
          cache: npm
          cache-dependency-path: app/package-lock.json
      - run: npm ci
        working-directory: app
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          projectPath: app
          tagName: ${{ github.ref_name }}
          releaseName: Kestrel ${{ github.ref_name }} (test build)
          releaseBody: |
            Test build. It only reads your PC; it changes nothing.

            Install: download `Kestrel_*_x64-setup.exe` below and run it. Test builds aren't signed yet, so Windows may show "Windows protected your PC". Click More info → Run anyway.

            Kestrel is not affiliated with Riot Games. VALORANT is a trademark of Riot Games, Inc.
          prerelease: true
          releaseDraft: false
```

- [ ] **Step 4: Format, push, open the pull request**

```bash
cargo fmt --all
git add .github Cargo.lock
git commit -am "ci: Linux, UI and Windows build workflows plus tagged pre-releases

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git push -u origin stage-1
gh pr create --base main --head stage-1 --title "Stage 1: hardware detection, Health Check, BIOS guide" --body "Stage 1 of Kestrel (see docs/superpowers/plans/2026-10-09-kestrel-stage-1.md). Read-only: nothing on the PC is changed.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

- [ ] **Step 5: Wait for CI and read the runner's probe report**

Let the app's PR monitor report the checks (don't poll). Expected: `core`, `ui` and `windows` all pass. Then:
```bash
gh run download --name kestrel-windows --dir ci-artifact
/usr/bin/python3 -c "import json,sys; r=json.load(open('ci-artifact/probe-report.json', encoding='utf-8-sig')); print(r['snapshot']['cpu']['name'], r['snapshot']['os']['build']); print(r['snapshot']['probeErrors'])"
```
Expected: a CPU name and a build ≥ 20348. Probe errors on a CI VM (no TPM, no GPU temperature) are fine; a missing CPU name or build is not — fix those before continuing. Delete `ci-artifact/` afterwards (it's not committed).

---

### Task 17: Publish v0.1.0 as a pre-release and hand it to the owner

**Files:** none in the repo; updates the tracker doc.

- [ ] **Step 1: Tag and publish**

```bash
git tag v0.1.0
git push origin v0.1.0
```
Wait for the Release workflow (the app reports it). Then:
```bash
gh release view v0.1.0 --json isPrerelease,assets --jq '{pre: .isPrerelease, files: [.assets[].name]}'
```
Expected: `pre: true` and a file named `Kestrel_0.1.0_x64-setup.exe`.

- [ ] **Step 2: Update the tracker** — Build plan: stage 1 → "Ready to test", with the release link. Add this checklist under "How testing works on your PC":

```markdown
### What to test in stage 1

- [ ] Install from the v0.1.0 pre-release; Windows asks for admin once when Kestrel opens
- [ ] First run: "Scan my PC" finishes in about 10 seconds
- [ ] Hardware: i9-11900K, RTX 3060 Ti, 32 GB DDR4 at 3600 MT/s, 360 Hz, Windows 11 Pro 25H2, ROG MAXIMUS XIII HERO
- [ ] Health Check: RAM shows 3600 MT/s and dual channel; note anything that looks wrong or says Unknown
- [ ] Toolbox: the BIOS guide mentions Extreme Tweaker and XMP I
- [ ] Live load moves, and shows GPU % and temperature
- [ ] Copy diagnostics, then paste the text in chat
- [ ] Nothing on your PC changed
```

- [ ] **Step 3: Tell the owner** — one short message: the release link, "install it and work through the checklist in the tracker", and "paste the Copy diagnostics text when you're done". Merge the PR into `main` only after the owner's test passes and they say so.

---

## Self-review (done while writing)

- Spec coverage (stage 1): app shell and tabs → Tasks 12–13; hardware detection → 8–11; Health Check list → 5–6 (all 27 checks map to the spec's list) and 15 (page); BIOS guide → 7 and 15; Copy diagnostics → 14; admin manifest and NSIS per-machine installer → 12; Windows 10 banner → 13; sample-data banner off Windows → 13; guided first run (stage-1 version) → 15; Lock In and benchmark placeholders → 13–14; disclaimer → 1 and 14; CI and pre-release → 16–17. Later stages (tweaks, Lock In, benchmark, updater) are intentionally out of scope.
- Placeholders: none; Task 9's stubs are replaced in full by Task 10.
- Type consistency: Rust `Report { app_version, snapshot, findings, score, guide }` ↔ TS `Report { appVersion, snapshot, findings, score, guide }`; `Fix` tag `kind` with kebab-case values on both sides; `LoadSample` fields `cpuPercent/ramPercent/gpuPercent/gpuTempC` on both sides; Tauri command names identical in `lib.rs` and `client.ts`.
- Review Focus: each item has a named test in its owning task (Tasks 3, 4, 5, 6 and the Task 8 smoke tests).
