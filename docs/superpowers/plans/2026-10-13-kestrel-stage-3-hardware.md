# Kestrel Stage 3 — Hardware & BIOS Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Hardware tab that lists every part of the PC Windows reports without a kernel driver (with live per-core clocks and Copy specs), four BIOS-related Health Check items with two new BIOS-guide topics, and the graphics card's PCIe link measured during benchmark runs — shipped as v0.3.0.

**Architecture:** Pure decisions and decoders go in `crates/kestrel-core` (CPUID decoding, the codename table, topology and clock maths, the PCIe card-to-slot walk, the Resizable BAR rule, chipset and placeholder rules, drive grouping, the four checks, the guide topics), tested on macOS. `crates/kestrel-probe` gains Windows readers (CPUID, `GetLogicalProcessorInformationEx`, Configuration Manager, display config, firmware type, new WMI queries) and a PDH clock sampler, each failure a `ProbeError`. `crates/kestrel-bench` records the link sampled during a benchmark; the Tauri app exposes the clocks, samples the link while a pass runs and applies the latest measurement to every report it returns; the React UI adds the Hardware page.

**Tech Stack:** Rust 2021 workspace (serde, regex, chrono, windows-sys 0.61.2, wmi 0.18.4, sysinfo 0.39.6), Tauri 2 (crate `kestrel`, lib `kestrel_app`), React 19 + TypeScript + Vite + Vitest.

**Spec:** `docs/superpowers/specs/2026-10-13-kestrel-stage-3-hardware-design.md` (main spec: `docs/superpowers/specs/2026-10-09-kestrel-design.md`, decisions 51–54 and 72–78; owner's screen brief: `docs/superpowers/notes/2026-10-13-hardware-tab-brief.md`).

## Global Constraints

- Never read or write a game's memory, never inject, never load a kernel driver, never start, stop or change Vanguard or any anti-cheat.
- No network calls at runtime except opening links; no telemetry.
- Kestrel writes only inside its own app-data folder. Stage 3 adds no writes; the only new stored data is `context.gpuPnpDeviceId` and `context.gpuLink` inside the existing benchmark files.
- Kestrel's own code never opens a handle to a game. During benchmark passes it reads only device properties of the graphics card and the PCI bridges above it, or NVML's link reading for an NVIDIA card (final-review ruling FR2).
- Every new Windows read is user-mode: the `CPUID` instruction and `XGETBV`, `GetLogicalProcessorInformationEx`, PDH through `PdhAddEnglishCounterW`, Configuration Manager (`CM_*`) device properties and allocated resources, `QueryDisplayConfig` / `DisplayConfigGetDeviceInfo`, `GetFirmwareType`, `IsProcessorFeaturePresent`, WMI.
- A value Windows doesn't report is `None` / empty and shows **Not reported**; nothing is guessed. Drive and volume sizes use 0 for unknown, like `ValorantInstall`, and the UI shows 0 as Not reported.
- A failing probe adds a `ProbeError` (names below), never fails the scan.
- The snapshot is schema version 2. Every new field is `Option`, an empty `Vec` or a `Default` enum, so a schema-1 JSON still loads.
- UI copy is English and lives in `app/src/strings.ts`; labels follow the spec and the brief.
- Stage numbers: tweak engine 5, Competitive tier 6, Lock In 7, fixes and troubleshooter 8 — one constant per language (`TWEAKS_STAGE` / `COMPETITIVE_STAGE` in `health.rs`; `STAGE` in `strings.ts`).
- The Health Check has 31 checks. Only a Problem lowers the score: High −15, Medium −8, Low −3.
- Version 0.3.0 (workspace `Cargo.toml`, every path dependency's `version`, `app/src-tauri/tauri.conf.json`, `app/package.json`).
- `windows-sys` stays 0.61.2 (new features only, no new crates). The PCI device property keys are declared locally: fmtid `3ab22e31-8264-4b4e-9af5-a8d2d8e33e62`, pids 1 (DeviceType), 3 (BaseClass), 4 (SubClass), 9 (CurrentLinkSpeed), 10 (CurrentLinkWidth), 11 (MaxLinkSpeed), 12 (MaxLinkWidth).
- Windows-only code sits behind `#[cfg(windows)]` (`CPUID` also behind `#[cfg(target_arch = "x86_64")]`). On the Mac it is checked with `cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc -- -D warnings`; CI builds and runs it on Windows.
- Every commit message ends with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A graphics card whose PCI link properties are missing or zero (a laptop card asleep, a virtual machine, an old Windows) — expect Not reported and an Unknown check, never "x0" or a false Problem. Pinned in Task 4 (`link_from_codes(Some(4), Some(0)) == None`, `resolve` with a node lacking links) and Task 6 (`pcie-lanes` Unknown).
2. A hybrid CPU (Intel 12th gen and later) — P and E counts right, L2 listed per size, clocks one cell per physical core with P/E kinds. Pinned in Task 3 (`p_e_split`, `group_caches`, `core_clocks` on an 8P+16E topology) and Task 9 (the `GetLogicalProcessorInformationEx` buffer parser on a hybrid buffer).
3. Windows in another language and more than 64 logical processors — PDH paths stay English through `PdhAddEnglishCounterW`, instance names `"g,n"` map to processor groups. Pinned in Task 3 (`parse_instance("1,3") == Some((1, 3))`, `"_Total"` / `"0,_Total"` → None).
4. Several graphics adapters (integrated + discrete + Parsec virtual) — checks use `main_gpu`; a link measured on another card is never applied. Pinned in Task 4 (`apply_links` ignores another `pnp_device_id`, picks the newest per card) and Task 6.
5. Data saved by v0.2.0 (schema-1 snapshot JSON, benchmarks without `gpuLink`) — loads with defaults; `pcie-speed` asks for a benchmark. Pinned in Task 1 (a schema-1 JSON parses) and Task 13 (a benchmark JSON without the new fields parses).

## File Structure

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/kestrel-core/src/snapshot.rs`, `fixtures/owner-pc.json` | Schema 2 model; owner fixture with the new fields | 1 |
| `crates/kestrel-core/src/cpu.rs` (new) | Decode `CPUID` registers; codename and socket table | 2 |
| `crates/kestrel-core/src/topology.rs` (new) | Physical cores, P/E split, cache grouping, PDH instance names, per-core clocks | 3 |
| `crates/kestrel-core/src/pcie.rs` (new) | Link codes, card-to-slot walk, Resizable BAR rule, which cards use it, link measurements | 4 |
| `crates/kestrel-core/src/hardware.rs` (new) | Placeholder names, chipset token, display connection names, drive grouping | 5 |
| `crates/kestrel-core/src/health.rs` | Four new checks, `BiosTopic::{PcieSlot, Virtualization}`, 31 checks | 6 |
| `crates/kestrel-core/src/bios_guide.rs` | `pcie-slot` and `virtualization` topics, topic order, wording fixes | 7 |
| `crates/kestrel-core/src/{health,ram,gpu}.rs`, `crates/kestrel-probe/src/win/display.rs` | Stage-number constants; `ram::normalize_configured`; stage-1 RAM and virtual-display fixes | 8 |
| `crates/kestrel-probe/src/win/cpu.rs` (new), `src/parse.rs`, `Cargo.toml` | `CPUID`, topology buffer parser, firmware type, virtualization | 9 |
| `crates/kestrel-probe/src/win/cfgmgr.rs` (new), `src/lib.rs` | PCI link and resources, LPC/eSPI description, `gpu_link()` | 10 |
| `crates/kestrel-probe/src/win/{wmi_probe,system_info}.rs`, `win/displayconfig.rs` (new), `src/parse.rs` | BIOS vendor, slots, rank, placeholders, every drive, monitor names | 11 |
| `crates/kestrel-probe/src/clocks.rs` (new), `src/win/mod.rs` | `ClockSampler` (PDH per core) | 12 |
| `crates/kestrel-bench/src/{model,text}.rs`, `app/src-tauri/src/bench/commands.rs` | `Context.gpu_pnp_device_id`, `Context.gpu_link`, Copy results line | 13 |
| `app/src-tauri/src/{lib.rs,hardware.rs (new)}`, `bench/{controller,commands}.rs` | `get_cpu_clocks`, link sampling, reports with the latest link | 14 |
| `app/src/api/{types,client,hardware (new),specs (new)}.ts` + tests, `api/mock-report.json` | Sample regenerated as schema 2, types, clocks call, formatting helpers, Copy specs text | 15 |
| `app/src/pages/Hardware.tsx`, `app/src/components/hardware/*` (new), `TitleBar.tsx`, `App.tsx`, `pages/Home.tsx`, `HardwarePanel.tsx`, `strings.ts`, CSS | The Hardware page, tab, See all, silent report refresh | 16 |
| `app/src/components/bench/*`, `strings.ts`, `api/sample-report.test.ts`, `README.md`, specs, `bench/controller.rs` docs | Benchmark PCIe line, stage-number sweep, a test of the sample, docs | 17 |
| `.github/workflows/ci.yml`, versions | macOS app-test job, version 0.3.0 | 18 |
| tracker doc, GitHub | v0.3.0 pre-release, PR, owner checklist | 19 |

Each task's own **Files** list is complete; this table names only the main files.

## Shared interfaces (every task uses these names exactly)

### `kestrel-core` snapshot, schema 2 (Task 1)

```rust
pub const SCHEMA_VERSION: u32 = 2;

// Inside `model! { … }` (Debug, Clone, Serialize, Deserialize, PartialEq, Default; camelCase; serde(default)).
pub struct CpuInfo {
    pub name: String,
    pub vendor: CpuVendor,
    pub cores: u32,
    pub threads: u32,
    /// CPUID display family and model (extended fields already added) and stepping.
    pub family: Option<u32>,
    pub model: Option<u32>,
    pub stepping: Option<u32>,
    pub codename: Option<String>,
    pub socket: Option<String>,
    pub base_mhz: Option<u32>,
    pub max_mhz: Option<u32>,
    /// Some only on hybrid CPUs (cores of more than one efficiency class).
    pub p_cores: Option<u32>,
    pub e_cores: Option<u32>,
    pub caches: Vec<CacheInfo>,
    /// Sets the CPU has and Windows enables, in this order: "SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI", "AMX".
    pub instructions: Vec<String>,
    /// Intel VT-x / AMD SVM on in the firmware, or a hypervisor running (which hides the firmware flag).
    pub virtualization: Option<bool>,
}
pub struct CacheInfo { pub level: u8, pub kind: CacheKind, pub size_kb: u32, pub count: u32 }
pub struct BoardInfo {
    pub manufacturer: String, pub product: String, pub bios_version: String, pub bios_date: Option<String>,
    pub bios_vendor: String, pub chipset: Option<String>, pub boot_mode: BootMode,
}
pub struct MemoryInfo {
    pub total_mb: u64, pub ddr_generation: Option<u8>, pub modules: Vec<MemoryModule>, pub slots_total: Option<u32>,
    /// The kit's rating from the part numbers (`kestrel_core::ram::rated_mts(&modules)`); the probe sets it (Task 11).
    pub rated_mts: Option<u32>,
}
// MemoryModule gains: pub rank: Option<u32>,
pub struct GpuInfo {
    pub name: String, pub vendor: GpuVendor, pub driver_version: String, pub driver_date: Option<String>,
    pub vram_mb: Option<u64>, pub is_integrated: bool,
    /// Win32_VideoController.PNPDeviceID, e.g. "PCI\\VEN_10DE&DEV_2489&SUBSYS_…\\4&…". Empty when unknown.
    pub pnp_device_id: String,
    pub pcie: Option<PcieInfo>,
    /// Filled by the app from the newest benchmark measured on this card, never by the probe.
    pub link_under_load: Option<LinkUnderLoad>,
    pub largest_bar_mb: Option<u64>,
    pub resizable_bar: Option<bool>,
}
pub struct PcieLink { pub gen: u8, pub width: u8 }
pub struct PcieInfo { pub card_max: Option<PcieLink>, pub slot_max: Option<PcieLink>, pub now: Option<PcieLink> }
pub struct LinkUnderLoad { pub link: PcieLink, pub measured_at: String, pub benchmark_id: String }
// DisplayInfo gains: pub monitor_name: Option<String>, pub connection: Option<String>,
pub struct StorageInfo { pub valorant: Option<ValorantInstall>, pub drives: Vec<DriveInfo> }
pub struct DriveInfo { pub model: String, pub media: DriveMedia, pub bus: DriveBus, pub size_gb: f64, pub volumes: Vec<VolumeInfo> }
pub struct VolumeInfo { pub letter: String, pub free_gb: f64, pub total_gb: f64 }

// Inside `choice! { … }` (Copy, Eq, kebab-case, a #[default] variant).
pub enum CacheKind { Data, Instruction, Unified, #[default] Unknown }
pub enum BootMode { Uefi, Legacy, #[default] Unknown }
pub enum DriveBus { Nvme, Sata, Usb, Sas, Raid, Virtual, #[default] Other }
```

Sizes: `size_gb`, `free_gb`, `total_gb` are GiB (bytes / 1024³), like the existing `ValorantInstall`.

### `kestrel-core::cpu` (Task 2)

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuidRaw {
    pub vendor: String,          // leaf 0 vendor string, e.g. "GenuineIntel", "AuthenticAMD"
    pub max_leaf: u32,           // leaf 0 EAX
    pub leaf1: [u32; 4],         // EAX, EBX, ECX, EDX
    pub leaf7: [u32; 4],         // leaf 7 sub-leaf 0 EAX, EBX, ECX, EDX; zeros when max_leaf < 7
    pub leaf7_1_eax: u32,        // leaf 7 sub-leaf 1 EAX; 0 when max_leaf < 7 or leaf7[0] < 1
    pub leaf16: Option<[u32; 3]>, // leaf 0x16 EAX, EBX, ECX when max_leaf >= 0x16
    pub brand: String,           // leaves 0x80000002–4, NULs removed, trimmed; empty when unavailable
    pub xcr0: Option<u64>,       // XGETBV(0) when leaf 1 ECX bit 27 (OSXSAVE) is set
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuIdentity {
    pub vendor: CpuVendor,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub instructions: Vec<String>,
    pub hypervisor: bool,
    pub base_mhz: Option<u32>,   // Intel leaf 0x16 EAX[15:0] when non-zero
    pub max_mhz: Option<u32>,    // Intel leaf 0x16 EBX[15:0] when non-zero
}
pub fn decode(raw: &CpuidRaw) -> CpuIdentity;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codename { pub name: &'static str, pub socket: Option<&'static str> }
pub fn codename(vendor: CpuVendor, family: u32, model: u32, stepping: u32) -> Option<Codename>;
```

### `kestrel-core::topology` (Task 3)

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhysicalCore {
    pub efficiency_class: u8,
    /// (processor group, number within the group) of each logical processor on the core.
    pub logical: Vec<(u16, u8)>,
}
/// Some((P-cores, E-cores)) only when the cores have more than one efficiency class; P = the highest class.
pub fn p_e_split(cores: &[PhysicalCore]) -> Option<(u32, u32)>;
/// One entry per distinct (level, kind, size) with its count; sorted by level, then Data, Instruction, Unified,
/// Unknown, then larger size first.
pub fn group_caches(caches: &[(u8, CacheKind, u32)]) -> Vec<CacheInfo>;
/// PDH `Processor Information` instance "g,n" → (g, n); "_Total", "0,_Total" and anything else → None.
pub fn parse_instance(name: &str) -> Option<(u16, u8)>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoreKind { Performance, Efficiency }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreClock { pub core: u32, pub kind: Option<CoreKind>, pub mhz: Option<u32> }
/// One clock per physical core, in `cores` order (`core` = its index). MHz = `Processor Frequency` ×
/// `% Processor Performance` / 100 of the core's first logical processor that has both values, rounded; None when
/// none has. `kind` is Some only on hybrid CPUs.
pub fn core_clocks(cores: &[PhysicalCore], frequency: &[(String, f64)], performance: &[(String, f64)]) -> Vec<CoreClock>;
```

### `kestrel-core::pcie` (Task 4)

```rust
pub const ROOT_PORT: u32 = 8;
pub const UPSTREAM_SWITCH_PORT: u32 = 9;
pub const DOWNSTREAM_SWITCH_PORT: u32 = 10;
pub const REBAR_THRESHOLD_MB: u64 = 256;
/// Speed codes 1–6 are PCIe 1.0–6.0; widths 1–32 lanes. Anything else (0 = asleep) → None.
pub fn link_from_codes(speed: Option<u32>, width: Option<u32>) -> Option<PcieLink>;
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PciNode {
    pub instance_id: String,
    pub parent: Option<String>,
    pub device_type: Option<u32>,
    pub current: Option<PcieLink>,
    pub max: Option<PcieLink>,
}
/// The device whose link is the slot's: the card itself, or — through a card's own internal switch — the upstream
/// switch port above it (walk up while the parent is a downstream switch port whose parent is an upstream switch
/// port, both with the card's PCI vendor id, so a chipset or Thunderbolt switch never hides the card's own link).
/// Instance ids compare case-insensitively.
pub fn slot_device<'a>(nodes: &'a [PciNode], gpu_instance_id: &str) -> Option<&'a PciNode>;
/// card_max and now from the slot device; slot_max from its parent when that is a root port. None when the card
/// isn't in `nodes`.
pub fn resolve(nodes: &[PciNode], gpu_instance_id: &str) -> Option<PcieInfo>;
/// (largest range in MB, Resizable BAR on). Empty → (None, None); a range > 256 MB → Some(true); else Some(false).
pub fn bar_summary(range_bytes: &[u64]) -> (Option<u64>, Option<bool>);
/// NVIDIA RTX 30 and newer, AMD RX 6000 and newer, discrete Intel Arc.
pub fn uses_resizable_bar(vendor: GpuVendor, name: &str) -> bool;
/// 4 → "4.0".
pub fn gen_label(gen: u8) -> String;
#[derive(Debug, Clone, PartialEq)]
pub struct LinkMeasurement { pub pnp_device_id: String, pub link: PcieLink, pub measured_at: String, pub benchmark_id: String }
/// For each GPU, the newest measurement (largest `measured_at`, RFC 3339) whose `pnp_device_id` matches it
/// case-insensitively becomes its `link_under_load`; GPUs without one keep None.
pub fn apply_links(snapshot: &mut SystemSnapshot, measurements: &[LinkMeasurement]);
```

### `kestrel-core::hardware` (Task 5)

```rust
/// True for empty text and BIOS filler: "To be filled by O.E.M.", "Default string", "System Product Name",
/// "System manufacturer", "System Version", "Not Applicable", "Not Specified", "Base Board Product Name",
/// "Type2 - Board Product Name1", "O.E.M.", "None", "x.x" (case-insensitive, trimmed).
pub fn is_placeholder(text: &str) -> bool;
/// A chipset token (e.g. "Z590", "B550", "X670E", "A620", "H610") from the board name, else from the Intel
/// LPC/eSPI controller's description ("Intel(R) Z590 LPC/eSPI Controller - 4385" → "Intel Z590"). Board-name
/// tokens get the maker prefix from the token (Intel for Z/H/B/Q/W-series 1xx–8xx, AMD for A/B/X 3xx–8xx).
pub fn chipset(board_product: &str, lpc_description: Option<&str>) -> Option<String>;
/// DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY → "VGA", "DVI", "HDMI", "DisplayPort", "Internal", "USB-C", "Wireless".
pub fn connection_name(output_technology: u32) -> Option<&'static str>;
#[derive(Debug, Clone, PartialEq)]
pub struct DiskRecord { pub number: u32, pub model: String, pub media: DriveMedia, pub bus: DriveBus, pub size_gb: f64 }
/// Drives by disk number; each with its lettered volumes (letter order) and their space; disks without a
/// lettered volume are still listed.
pub fn drives(disks: &[DiskRecord], partitions: &[(u32, char)], space: &[(char, f64, f64)]) -> Vec<DriveInfo>;
```

### `kestrel-core::health` (Task 6, 8)

`BiosTopic` gains `PcieSlot` (`"pcie-slot"`) and `Virtualization` (`"virtualization"`). Each new check's rows are evaluated top-down; the first that applies wins. `pub const TWEAKS_STAGE: u8 = 5;` and `pub const COMPETITIVE_STAGE: u8 = 6;`. `run_health_check` returns 31 findings in this order: ram-speed, ram-channels, bios-age, **pcie-lanes, pcie-speed, resizable-bar**, display-refresh, virtual-display, gpu-driver-age, valorant-gpu, on-battery, power-plan, game-mode, background-recording, pending-restart, windows-build, valorant-drive, drive-space, background-cpu, startup-apps, tpm, secure-boot, memory-integrity, **virtualization**, iommu, vanguard, on-demand, wired, link-speed, adapter-power, delivery-optimization.

The new findings, exactly (`{…}` is filled in; generations through `gen_label`; the details carry no date — the Hardware tab shows it in local time):

| id · category | state | status · severity | title | detail | fix |
| --- | --- | --- | --- | --- | --- |
| `pcie-lanes` · Hardware | main GPU missing, integrated or virtual | Info · Info | No separate graphics card to check | (empty) | Nothing |
| | `pcie` None, or `now` / `card_max` None | Unknown · Medium | Couldn't read the graphics card's PCIe link | Windows didn't report it for this card. | Nothing |
| | now.width ≥ card_max.width | Pass · Medium | Graphics card uses all its PCIe lanes | x{w}, as the card supports. | Nothing |
| | now.width < card_max.width | Problem · Medium (High when now.width ≤ 4) | Graphics card runs at x{now} instead of x{max} | It loses some FPS, more at x4. Usual causes: the card sits in a lower slot, an M.2 drive or second card shares its lanes, or it isn't pushed fully in. In an external (Thunderbolt) enclosure x4 is normal. | BiosGuide `pcie-slot` |
| `pcie-speed` · Hardware | no separate card (as above) | Info · Info | No separate graphics card to check | (empty) | Nothing |
| | `link_under_load` None | Info · Info | Run a benchmark to check the PCIe link under load | Graphics cards slow their link when idle, so Kestrel checks it during a benchmark. | Nothing |
| | card_max or slot_max None | Unknown · Low | Couldn't read the slot's PCIe speed | Windows didn't report what the slot supports. | Nothing |
| | got.gen ≥ min(card_max.gen, slot_max.gen) | Pass · Low | PCIe link runs at full speed under load | PCIe {gen} during your latest benchmark. | Nothing |
| | got.gen < expected | Problem · Low (High when got.gen ≤ 2) | PCIe link runs at {got} under load instead of {expected} | Measured during your latest benchmark. Set the slot's PCIe speed to Auto in the BIOS, and check the card sits in the top slot. | BiosGuide `pcie-slot` |
| `resizable-bar` · Hardware | no separate card | Info · Info | No separate graphics card to check | (empty) | Nothing |
| | `resizable_bar` None | Unknown · Low | Windows doesn't report Resizable BAR for this card | (empty) | Nothing |
| | Some(true) | Pass · Low | Resizable BAR is on | (empty) | Nothing |
| | Some(false), Intel and `uses_resizable_bar` | Problem · High | Resizable BAR is off | Intel Arc cards lose a lot of FPS without it. Turn on Above 4G Decoding and Re-Size BAR in the BIOS; Windows must start in UEFI mode (CSM off). | BiosGuide `resizable-bar` |
| | Some(false), AMD and `uses_resizable_bar` | Problem · Low | Resizable BAR is off | AMD calls it Smart Access Memory. It adds a few FPS in some games. Windows must start in UEFI mode (CSM off). | BiosGuide `resizable-bar` |
| | Some(false), NVIDIA and `uses_resizable_bar` | Info · Info | Resizable BAR is off | NVIDIA only uses it in games on its own list, so most games, VALORANT included, gain little. Turning it on is optional. | BiosGuide `resizable-bar` |
| | Some(false), card that doesn't use it | Info · Info | Resizable BAR is off | Only newer cards use it: NVIDIA RTX 30 series and newer, AMD RX 6000 and newer, and Intel Arc. | Nothing |
| `virtualization` · Vanguard | Some(true) | Pass · Info | Virtualization is on | (empty) | Nothing |
| | Some(false) | Info · Info | Virtualization is off | VALORANT works fine like this. Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It's called Intel Virtualization Technology (VT-x) or SVM on AMD. | BiosGuide `virtualization` |
| | None | Unknown · Info | Couldn't tell whether virtualization is on | (empty) | Nothing |

"No separate graphics card": `gpu::main_gpu(&s.gpus)` is None, or it `is_integrated`, or its vendor is `Virtual`.

### `kestrel-core::ram` (Task 8)

```rust
/// Some boards report the RAM clock in MHz (half the MT/s). Doubles a module's `configured_mts` only when its DDR
/// generation's JEDEC floor shows the value can't be MT/s — DDR4 below 2133, DDR5 below 4800 — and twice the value
/// is within 50 of the kit's rating (`rated_mts` of the unchanged modules). A DDR4-4266 kit really running at 2133
/// (XMP off) stays 2133. Unknown generations are never changed. The probe calls this after the WMI memory read.
pub fn normalize_configured(modules: &mut [MemoryModule], ddr_generation: Option<u8>);
```

Beside that call the probe sets `memory.rated_mts = ram::rated_mts(&modules)` (the existing function, unchanged), so the UI reads the kit's rating from the snapshot and never from a finding's wording.

### `kestrel-core::bios_guide` (Task 7)

Topic order: xmp, memory-gear (11th-gen Intel only), update-bios, **pcie-slot**, resizable-bar, **virtualization**, iommu.
- `pcie-slot`: title "Run the graphics card at full PCIe speed"; why "The card should use all its lanes (usually x16) at the PCIe generation it and the slot support. It drops when it sits in a lower slot, shares lanes with an M.2 drive, or the slot is set to an older generation."; caution "Menu names vary between boards and BIOS versions. Your board's manual lists which slots and M.2 drives share lanes."
- `virtualization`: title "Turn on virtualization (only if you need it)"; why "Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It doesn't change FPS."; caution "Menu names vary between boards and BIOS versions. If you can't find it, search the BIOS for SVM or Virtualization."

### `kestrel-probe` (Tasks 9–12)

```rust
// src/lib.rs
pub mod clocks;
/// The current PCIe link of the card's slot device (see kestrel_core::pcie::slot_device). Windows: Configuration
/// Manager; elsewhere None.
pub fn gpu_link(pnp_device_id: &str) -> Option<kestrel_core::PcieLink>;
// src/clocks.rs
pub struct ClockSampler { /* … */ }
impl ClockSampler {
    pub fn new() -> Self;                                  // collects once, so the next sample has a baseline
    pub fn sample(&mut self) -> Vec<kestrel_core::topology::CoreClock>; // off Windows: 8 Performance-less cores, 4800 + 50 × core MHz
}
```

On a laptop (`machine.form_factor == Laptop`) the probe leaves `cpu.socket` None: the table gives desktop sockets, and laptop chips can share a desktop signature.

`ProbeError.probe` names and the Hardware panel each one belongs to: Processor — `cpu`, `cpu-id`, `topology`; Graphics — `gpus`, `pcie`; Memory — `memory`; Motherboard and BIOS — `board`, `chipset`, `firmware`, `tpm`, `device-guard`; Displays — `displays`; Storage — `storage`, `drives`; Network — `network`; every panel — `wmi`, `probe`. (`mock` shows only the existing banner.)

### `kestrel-bench` (Task 13)

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GpuLink { pub gen: u8, pub width: u8 }
// Context gains: pub gpu_pnp_device_id: Option<String>, pub gpu_link: Option<GpuLink>,
```

Copy results gains the line `PCIe link under load: 4.0 x16` (or `PCIe link under load: not reported`).

### App (Task 14)

- `Platform` gains `fn gpu_link(&self, pnp_device_id: &str) -> Option<GpuLink>;`. While a capture runs, `pace` samples it every 2 s (every 10th 200 ms step, starting with the first) when `context.gpu_pnp_device_id` is Some, and keeps the best sample in the live session's `context.gpu_link`: the highest `gen`, then the wider link, then the first.
- `context_from` sets `gpu_pnp_device_id` from the main GPU (None when its id is empty).
- `src/hardware.rs`: `pub fn link_measurements(store: &Store) -> Vec<LinkMeasurement>` (every saved benchmark with both `gpu_pnp_device_id` and `gpu_link`; `measured_at` = `created_at`) and `pub fn with_links(report: Report, store: &Store, today: NaiveDate) -> Report` (applies them and rebuilds findings, score and guide with `kestrel_core::build_report`).
- `get_report`, `rescan` and `get_diagnostics` return `with_links(…)` of the cached report; the cache keeps the probe's report.
- Command `get_cpu_clocks` → `Vec<CoreClock>` from a `ClockSampler` in `AppState` (created in `setup`).

### UI (Tasks 15–17)

- `client.ts`: `getCpuClocks(): Promise<CoreClock[]>` (browser preview: 8 cores around 4.8 GHz, changing each call).
- `api/hardware.ts` (helpers used by the page and Copy specs) and `api/specs.ts`: `specsText(report: Report): string` — the spec's seven lines, a line left out when Kestrel can't fill it:
  - `CPU: {name} ({cores} cores, {threads} threads)`; hybrid `CPU: {name} ({p} P-cores + {e} E-cores, {threads} threads)`; `{name}` is the cleaned name `cpuLabel` already makes, without the `· 8C/16T` part.
  - `GPU: {main GPU name} {VRAM GB} GB · driver {driverLabel} · PCIe {gen} x{width}` — the link under load, else `PCIe x{now.width}`, else no PCIe part.
  - `RAM: {GB} GB DDR{n}-{lowest configured MT/s} · {dual|single} channel · {count} × {GB} GB {maker} {part}` — channel from the `ram-channels` finding (pass → dual, problem → single, else left out); modules that differ are joined with ` + ` as `{GB} GB {maker} {part}`.
  - `Board: {ASUS|MSI|Gigabyte|ASRock|manufacturer} {product} · BIOS {version} ({date})`.
  - `Displays: {w}×{h} at {currentHz} Hz` per real display, joined with ` · `.
  - `Storage: {model} ({NVMe|SSD|HDD})` per drive, joined with ` · ` (no parentheses when the media is unknown).
  - `OS: {caption without "Microsoft "} {displayVersion} (build {build})`.
- The kit's rating (the Memory panel's "Kit rating", the XMP row's "RAM runs at its rated 3600 MT/s") is `snapshot.memory.ratedMts`; null shows "Rating unknown" (and no XMP note). No helper parses a finding's wording.
- Tabs: `home, health, hardware, tweaks, lockIn, benchmark, toolbox`; label "Hardware".
- Live clocks poll `getCpuClocks` once a second only while the Hardware tab is shown and `document.visibilityState === 'visible' && document.hasFocus()`; otherwise the cells dim and say "Paused".
- On every tab change after the first run, `App` refreshes the report from `getReport()` without the loading screen.

---

### Task 1: Snapshot schema 2

**Files:**
- Modify: `crates/kestrel-core/src/snapshot.rs` (`SCHEMA_VERSION` 2; new fields on `CpuInfo`, `GpuInfo`, `MemoryInfo`, `MemoryModule`, `DisplayInfo`, `BoardInfo`, `StorageInfo`; new `CacheInfo`, `PcieLink`, `PcieInfo`, `LinkUnderLoad`, `DriveInfo`, `VolumeInfo`, `CacheKind`, `BootMode`, `DriveBus`)
- Modify: `crates/kestrel-core/fixtures/owner-pc.json` (whole file: schema 2 with the owner's new parts)
- Modify: `crates/kestrel-core/src/gpu.rs`, `crates/kestrel-core/src/ram.rs`, `crates/kestrel-core/src/health.rs` (test helpers that list every field gain `..Default::default()`)
- Modify: `crates/kestrel-probe/src/win/wmi_probe.rs` (`GpuInfo` keeps the `PNPDeviceID` the query already reads; `..Default::default()` on `GpuInfo` and `MemoryModule`), `crates/kestrel-probe/src/win/display.rs` (`..Default::default()` on `DisplayInfo`)
- Test: `crates/kestrel-core/src/snapshot.rs` (`mod tests`)

**Interfaces:**
- Consumes: nothing new.
- Produces (re-exported from `kestrel_core` by the existing `pub use snapshot::*`): `SCHEMA_VERSION = 2`; `CpuInfo` gains `family`, `model`, `stepping: Option<u32>`, `codename`, `socket: Option<String>`, `base_mhz`, `max_mhz`, `p_cores`, `e_cores: Option<u32>`, `caches: Vec<CacheInfo>`, `instructions: Vec<String>`, `virtualization: Option<bool>`; `CacheInfo { level: u8, kind: CacheKind, size_kb: u32, count: u32 }`; `GpuInfo` gains `pnp_device_id: String`, `pcie: Option<PcieInfo>`, `link_under_load: Option<LinkUnderLoad>`, `largest_bar_mb: Option<u64>`, `resizable_bar: Option<bool>`; `PcieLink { gen: u8, width: u8 }`; `PcieInfo { card_max, slot_max, now: Option<PcieLink> }`; `LinkUnderLoad { link: PcieLink, measured_at: String, benchmark_id: String }`; `MemoryInfo.slots_total: Option<u32>`; `MemoryInfo.rated_mts: Option<u32>` (the kit's rating from the part numbers, `ram::rated_mts`; Task 11 fills it); `MemoryModule.rank: Option<u32>`; `DisplayInfo` gains `monitor_name`, `connection: Option<String>`; `BoardInfo` gains `bios_vendor: String`, `chipset: Option<String>`, `boot_mode: BootMode`; `StorageInfo.drives: Vec<DriveInfo>`; `DriveInfo { model: String, media: DriveMedia, bus: DriveBus, size_gb: f64, volumes: Vec<VolumeInfo> }`; `VolumeInfo { letter: String, free_gb: f64, total_gb: f64 }`; `CacheKind { Data, Instruction, Unified, Unknown (default) }`, `BootMode { Uefi, Legacy, Unknown (default) }`, `DriveBus { Nvme, Sata, Usb, Sas, Raid, Virtual, Other (default) }`. All structs are `Clone`, not `Copy` (they live in `model!`). `owner_pc_fixture()` carries every new field: RTX 3060 Ti `pnp_device_id` `PCI\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\4&2B0D3A0E&0&0008` with PCIe card 4.0 x16, slot 4.0 x16, now 1.0 x16, `largest_bar_mb` 8192, `resizable_bar` true, `link_under_load` None; Parsec `ROOT\DISPLAY\0000` without PCIe; chipset "Intel Z590", UEFI; `rated_mts` 3600; two displays; two drives.

- [ ] **Step 1: Write the failing tests**

Replace the whole `#[cfg(test)] mod tests { … }` block at the end of `crates/kestrel-core/src/snapshot.rs` with the block below. `owner_fixture_parses`, `snapshot_round_trips_through_json` and `enums_use_kebab_case_and_fields_camel_case` are today's tests unchanged; `default_snapshot_means_unknown_everywhere` gains five asserts. New: `fixture_loses_nothing_when_parsed` (the stage-1 backlog's "Tests to add" item: every key in the file is a field and every field is in the file), `owner_fixture_has_the_schema_2_parts` (the owner's new values) and `a_schema_1_snapshot_still_loads_with_the_new_fields_empty` (Review Focus 5: a trimmed copy of v0.2.0's fixture loads with every new field at its default).

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
    fn fixture_loses_nothing_when_parsed() {
        // Every key in the file is a field of the model (serde would skip a misspelt one), and every field is in the
        // file, so the sample PC shows every part of the Hardware tab. Sizes in GiB are f64: write 1863.0, not 1863.
        let written: serde_json::Value = serde_json::from_str(OWNER_PC_JSON).unwrap();
        assert_eq!(serde_json::to_value(owner_pc_fixture()).unwrap(), written);
    }

    #[test]
    fn owner_fixture_has_the_schema_2_parts() {
        let s = owner_pc_fixture();
        let cache = |level, kind, size_kb, count| CacheInfo {
            level,
            kind,
            size_kb,
            count,
        };
        assert_eq!(
            s.cpu,
            CpuInfo {
                name: "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz".into(),
                vendor: CpuVendor::Intel,
                cores: 8,
                threads: 16,
                family: Some(6),
                model: Some(0xA7),
                stepping: Some(1),
                codename: Some("Rocket Lake-S".into()),
                socket: Some("LGA1200".into()),
                base_mhz: Some(3500),
                max_mhz: Some(5300),
                p_cores: None,
                e_cores: None,
                caches: vec![
                    cache(1, CacheKind::Data, 48, 8),
                    cache(1, CacheKind::Instruction, 32, 8),
                    cache(2, CacheKind::Unified, 512, 8),
                    cache(3, CacheKind::Unified, 16384, 1),
                ],
                instructions: vec![
                    "SSE4.2".into(),
                    "AVX".into(),
                    "AVX2".into(),
                    "AVX-512".into()
                ],
                virtualization: Some(true),
            }
        );
        let board = &s.board;
        assert_eq!(board.bios_vendor, "American Megatrends Inc.");
        assert_eq!(board.chipset.as_deref(), Some("Intel Z590"));
        assert_eq!(board.boot_mode, BootMode::Uefi);
        assert_eq!(s.memory.slots_total, Some(4));
        assert_eq!(s.memory.rated_mts, Some(3600));
        assert!(s.memory.modules.iter().all(|m| m.rank == Some(2)));

        let parsec = &s.gpus[0];
        assert_eq!(parsec.pnp_device_id, "ROOT\\DISPLAY\\0000");
        assert_eq!(parsec.pcie, None);
        let rtx = &s.gpus[1];
        assert_eq!(
            rtx.pnp_device_id,
            "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&2B0D3A0E&0&0008"
        );
        let link = |gen, width| Some(PcieLink { gen, width });
        assert_eq!(
            rtx.pcie,
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: link(4, 16),
                now: link(1, 16),
            })
        );
        assert_eq!(rtx.link_under_load, None);
        assert_eq!(rtx.largest_bar_mb, Some(8192));
        assert_eq!(rtx.resizable_bar, Some(true));

        assert_eq!(s.displays[0].monitor_name.as_deref(), Some("XL2566K"));
        assert_eq!(s.displays[0].connection.as_deref(), Some("DisplayPort"));
        assert_eq!(
            s.displays[1],
            DisplayInfo {
                name: "\\\\.\\DISPLAY2".into(),
                adapter: "NVIDIA GeForce RTX 3060 Ti".into(),
                current_hz: 180,
                max_hz: 180,
                width: 1920,
                height: 1080,
                is_primary: false,
                is_virtual: false,
                monitor_name: Some("24G4".into()),
                connection: Some("DisplayPort".into()),
            }
        );

        let volume = |letter: &str, free_gb, total_gb| VolumeInfo {
            letter: letter.into(),
            free_gb,
            total_gb,
        };
        assert_eq!(
            s.storage.drives,
            [
                DriveInfo {
                    model: "Samsung SSD 980 PRO 1TB".into(),
                    media: DriveMedia::Nvme,
                    bus: DriveBus::Nvme,
                    size_gb: 931.5,
                    volumes: vec![volume("C", 210.5, 931.5)],
                },
                DriveInfo {
                    model: "WDC WD20EZBX-00AYRA0".into(),
                    media: DriveMedia::Hdd,
                    bus: DriveBus::Sata,
                    size_gb: 1863.0,
                    volumes: vec![volume("D", 1228.8, 1863.0)],
                },
            ]
        );
    }

    /// The parts of a snapshot v0.2.0 saved (schema 1) that gain fields in schema 2, trimmed from its fixture.
    const SCHEMA_1_JSON: &str = r#"{
      "schemaVersion": 1,
      "collectedAt": "2026-10-09T16:30:00Z",
      "cpu": { "name": "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz", "vendor": "intel", "cores": 8, "threads": 16 },
      "gpus": [
        { "name": "Parsec Virtual Display Adapter", "vendor": "virtual", "driverVersion": "0.45.0.0", "driverDate": null, "vramMb": null, "isIntegrated": false },
        { "name": "NVIDIA GeForce RTX 3060 Ti", "vendor": "nvidia", "driverVersion": "32.0.16.1074", "driverDate": "2026-09-15", "vramMb": 8192, "isIntegrated": false }
      ],
      "memory": { "totalMb": 32768, "ddrGeneration": 4, "modules": [
        { "capacityMb": 16384, "configuredMts": 3600, "maxMts": 2133, "partNumber": "CMG32GX4M2D3600C18", "manufacturer": "Corsair", "bankLabel": "BANK 1", "deviceLocator": "ChannelA-DIMM2" }
      ] },
      "displays": [
        { "name": "\\\\.\\DISPLAY1", "adapter": "NVIDIA GeForce RTX 3060 Ti", "currentHz": 360, "maxHz": 360, "width": 1920, "height": 1080, "isPrimary": true, "isVirtual": false }
      ],
      "board": { "manufacturer": "ASUSTeK COMPUTER INC.", "product": "ROG MAXIMUS XIII HERO", "biosVersion": "2103", "biosDate": "2024-05-20" },
      "storage": { "valorant": { "path": "C:/Riot Games/VALORANT/live", "driveLetter": "C", "media": "nvme", "freeGb": 210.5, "totalGb": 931.5 } }
    }"#;

    #[test]
    fn a_schema_1_snapshot_still_loads_with_the_new_fields_empty() {
        let s: SystemSnapshot = serde_json::from_str(SCHEMA_1_JSON).expect("schema 1 still loads");
        assert_eq!(s.schema_version, 1);
        assert_eq!(
            s.cpu,
            CpuInfo {
                name: "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz".into(),
                vendor: CpuVendor::Intel,
                cores: 8,
                threads: 16,
                ..Default::default()
            }
        );
        assert_eq!(s.gpus[0].pnp_device_id, "");
        assert_eq!(
            s.gpus[1],
            GpuInfo {
                name: "NVIDIA GeForce RTX 3060 Ti".into(),
                vendor: GpuVendor::Nvidia,
                driver_version: "32.0.16.1074".into(),
                driver_date: Some("2026-09-15".into()),
                vram_mb: Some(8192),
                is_integrated: false,
                ..Default::default()
            }
        );
        assert_eq!(s.memory.slots_total, None);
        assert_eq!(s.memory.rated_mts, None);
        assert_eq!(
            s.memory.modules[0],
            MemoryModule {
                capacity_mb: 16384,
                configured_mts: Some(3600),
                max_mts: Some(2133),
                part_number: "CMG32GX4M2D3600C18".into(),
                manufacturer: "Corsair".into(),
                bank_label: "BANK 1".into(),
                device_locator: "ChannelA-DIMM2".into(),
                rank: None,
            }
        );
        let display = &s.displays[0];
        assert_eq!(display.current_hz, 360);
        assert_eq!(display.monitor_name, None);
        assert_eq!(display.connection, None);
        assert_eq!(
            s.board,
            BoardInfo {
                manufacturer: "ASUSTeK COMPUTER INC.".into(),
                product: "ROG MAXIMUS XIII HERO".into(),
                bios_version: "2103".into(),
                bios_date: Some("2024-05-20".into()),
                ..Default::default()
            }
        );
        assert_eq!(s.board.boot_mode, BootMode::Unknown);
        assert!(s.storage.drives.is_empty());
        assert_eq!(
            s.storage.valorant.map(|v| v.drive_letter).as_deref(),
            Some("C")
        );
    }

    #[test]
    fn enums_use_kebab_case_and_fields_camel_case() {
        assert_eq!(
            serde_json::to_string(&DriveMedia::Nvme).unwrap(),
            "\"nvme\""
        );
        assert_eq!(
            serde_json::to_string(&ServiceState::NotInstalled).unwrap(),
            "\"not-installed\""
        );
        let json = serde_json::to_string(&OsInfo {
            caption: "x".into(),
            build: 1,
            display_version: None,
        })
        .unwrap();
        assert!(json.contains("displayVersion"));
    }

    #[test]
    fn default_snapshot_means_unknown_everywhere() {
        let s = SystemSnapshot::default();
        assert_eq!(s.machine.form_factor, FormFactor::Unknown);
        assert_eq!(s.security.vanguard, ServiceState::Unknown);
        assert!(s.storage.valorant.is_none());
        assert_eq!(s.board.boot_mode, BootMode::Unknown);
        assert_eq!(s.cpu.virtualization, None);
        assert!(s.storage.drives.is_empty());
        assert_eq!(CacheKind::default(), CacheKind::Unknown);
        assert_eq!(DriveBus::default(), DriveBus::Other);
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core snapshot`
Expected: FAIL to compile — `cannot find type 'CacheKind' in this scope`, `cannot find struct, variant or union type 'DriveInfo' in this scope`, `no field 'pnp_device_id' on type '&GpuInfo'`, `no field 'drives' on type 'StorageInfo'` and more of the same kind.

- [ ] **Step 3: Implement the schema 2 model**

In `crates/kestrel-core/src/snapshot.rs`, replace everything from `pub const SCHEMA_VERSION: u32 = 1;` down to (not including) `pub const OWNER_PC_JSON` with the block below. The two macros and the structs that don't change are repeated as they are, so the block replaces cleanly. `model!` gives every struct `Default` and `#[serde(default)]`, so a field that older JSON lacks loads as `None`, empty or the `#[default]` variant. The Windows probe stamps `SCHEMA_VERSION`, so it writes schema 2 from here on; Tasks 9–11 fill the new fields.

```rust
pub const SCHEMA_VERSION: u32 = 2;

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
    pub struct CpuInfo {
        pub name: String,
        pub vendor: CpuVendor,
        pub cores: u32,
        pub threads: u32,
        /// CPUID display family and model (extended fields already added) and stepping.
        pub family: Option<u32>,
        pub model: Option<u32>,
        pub stepping: Option<u32>,
        pub codename: Option<String>,
        pub socket: Option<String>,
        pub base_mhz: Option<u32>,
        pub max_mhz: Option<u32>,
        /// Some only on hybrid CPUs (cores of more than one efficiency class).
        pub p_cores: Option<u32>,
        pub e_cores: Option<u32>,
        pub caches: Vec<CacheInfo>,
        /// Sets the CPU has and Windows enables, in this order: "SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI", "AMX".
        pub instructions: Vec<String>,
        /// Intel VT-x / AMD SVM on in the firmware, or a hypervisor running (which hides the firmware flag).
        pub virtualization: Option<bool>,
    }
    /// `count` caches of one level, kind and size (e.g. 8 × 48 KB L1 data).
    pub struct CacheInfo { pub level: u8, pub kind: CacheKind, pub size_kb: u32, pub count: u32 }
    pub struct GpuInfo {
        pub name: String,
        pub vendor: GpuVendor,
        pub driver_version: String,
        pub driver_date: Option<String>,
        pub vram_mb: Option<u64>,
        pub is_integrated: bool,
        /// Win32_VideoController.PNPDeviceID, e.g. "PCI\\VEN_10DE&DEV_2489&SUBSYS_…\\4&…". Empty when unknown.
        pub pnp_device_id: String,
        pub pcie: Option<PcieInfo>,
        /// Filled by the app from the newest benchmark measured on this card, never by the probe.
        pub link_under_load: Option<LinkUnderLoad>,
        pub largest_bar_mb: Option<u64>,
        pub resizable_bar: Option<bool>,
    }
    /// A PCIe link: generation (4 = PCIe 4.0) and lanes (16 = x16).
    pub struct PcieLink { pub gen: u8, pub width: u8 }
    /// What the card and its slot support, and the link right now (cards slow it down when idle).
    pub struct PcieInfo { pub card_max: Option<PcieLink>, pub slot_max: Option<PcieLink>, pub now: Option<PcieLink> }
    /// The fastest link seen during a benchmark on this card. `measured_at` is RFC 3339.
    pub struct LinkUnderLoad { pub link: PcieLink, pub measured_at: String, pub benchmark_id: String }
    pub struct MemoryInfo {
        pub total_mb: u64,
        pub ddr_generation: Option<u8>,
        pub modules: Vec<MemoryModule>,
        pub slots_total: Option<u32>,
        /// The kit's rated speed in MT/s from the modules' part numbers (`ram::rated_mts`); None when Kestrel doesn't
        /// recognise them. The probe fills it, so the UI never reads it out of a finding's wording.
        pub rated_mts: Option<u32>,
    }
    pub struct MemoryModule {
        pub capacity_mb: u64,
        /// The speed the module runs at now, in MT/s (`Win32_PhysicalMemory.ConfiguredClockSpeed`).
        pub configured_mts: Option<u32>,
        /// The module's own maximum from SMBIOS, in MT/s (`Win32_PhysicalMemory.Speed`). This is its JEDEC
        /// maximum, never an XMP or EXPO rating, so it must not be read as the kit's rated speed.
        pub max_mts: Option<u32>,
        pub part_number: String,
        pub manufacturer: String,
        pub bank_label: String,
        pub device_locator: String,
        /// Ranks on the module (`Win32_PhysicalMemory.Attributes`): 1 single, 2 dual.
        pub rank: Option<u32>,
    }
    pub struct DisplayInfo {
        pub name: String,
        pub adapter: String,
        pub current_hz: u32,
        pub max_hz: u32,
        pub width: u32,
        pub height: u32,
        pub is_primary: bool,
        pub is_virtual: bool,
        /// The monitor's own name from its EDID, e.g. "XL2566K".
        pub monitor_name: Option<String>,
        /// How it is plugged in: "DisplayPort", "HDMI", "DVI", "VGA", "USB-C", "Wireless" or "Internal".
        pub connection: Option<String>,
    }
    pub struct BoardInfo {
        pub manufacturer: String,
        pub product: String,
        pub bios_version: String,
        pub bios_date: Option<String>,
        pub bios_vendor: String,
        /// Maker and chipset, e.g. "Intel Z590".
        pub chipset: Option<String>,
        pub boot_mode: BootMode,
    }
    pub struct StorageInfo { pub valorant: Option<ValorantInstall>, pub drives: Vec<DriveInfo> }
    pub struct ValorantInstall { pub path: String, pub drive_letter: String, pub media: DriveMedia, pub free_gb: f64, pub total_gb: f64 }
    /// A physical drive. Sizes are GiB (bytes / 1024³), like `ValorantInstall`'s.
    pub struct DriveInfo { pub model: String, pub media: DriveMedia, pub bus: DriveBus, pub size_gb: f64, pub volumes: Vec<VolumeInfo> }
    /// A lettered volume on a drive: "C", with its free and total GiB.
    pub struct VolumeInfo { pub letter: String, pub free_gb: f64, pub total_gb: f64 }
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
    pub enum CacheKind { Data, Instruction, Unified, #[default] Unknown }
    pub enum BootMode { Uefi, Legacy, #[default] Unknown }
    pub enum DriveBus { Nvme, Sata, Usb, Sas, Raid, Virtual, #[default] Other }
}
```

- [ ] **Step 4: Update the owner fixture**

Replace `crates/kestrel-core/fixtures/owner-pc.json` with the owner's PC in schema 2: i9-11900K (family 6, model 0xA7, stepping 1, Rocket Lake-S, LGA1200, 3.5/5.3 GHz, its four cache lines, AVX-512, virtualization on); the Z590 board's AMI BIOS in UEFI mode; four slots holding two dual-rank modules of a kit rated 3600 MT/s; the RTX 3060 Ti in a PCIe 4.0 x16 slot, idling at 1.0, with Resizable BAR on (an 8 GB BAR); the XL2566K at 360 Hz plus a 24G4 at 180 Hz, both on DisplayPort; the 980 PRO (NVMe, C:) and the WD Blue HDD (SATA, D:). Sizes in GiB keep a decimal point (`1863.0`): `fixture_loses_nothing_when_parsed` compares the file with what serde writes, and serde writes an f64 that way.

```json
{
  "schemaVersion": 2,
  "collectedAt": "2026-10-09T16:30:00Z",
  "os": { "caption": "Microsoft Windows 11 Pro", "build": 26200, "displayVersion": "25H2" },
  "machine": { "formFactor": "desktop", "manufacturer": "ASUS", "model": "", "onBattery": null },
  "cpu": {
    "name": "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz", "vendor": "intel", "cores": 8, "threads": 16,
    "family": 6, "model": 167, "stepping": 1, "codename": "Rocket Lake-S", "socket": "LGA1200",
    "baseMhz": 3500, "maxMhz": 5300, "pCores": null, "eCores": null,
    "caches": [
      { "level": 1, "kind": "data", "sizeKb": 48, "count": 8 },
      { "level": 1, "kind": "instruction", "sizeKb": 32, "count": 8 },
      { "level": 2, "kind": "unified", "sizeKb": 512, "count": 8 },
      { "level": 3, "kind": "unified", "sizeKb": 16384, "count": 1 }
    ],
    "instructions": ["SSE4.2", "AVX", "AVX2", "AVX-512"],
    "virtualization": true
  },
  "gpus": [
    { "name": "Parsec Virtual Display Adapter", "vendor": "virtual", "driverVersion": "0.45.0.0", "driverDate": null, "vramMb": null, "isIntegrated": false,
      "pnpDeviceId": "ROOT\\DISPLAY\\0000", "pcie": null, "linkUnderLoad": null, "largestBarMb": null, "resizableBar": null },
    { "name": "NVIDIA GeForce RTX 3060 Ti", "vendor": "nvidia", "driverVersion": "32.0.16.1074", "driverDate": "2026-09-15", "vramMb": 8192, "isIntegrated": false,
      "pnpDeviceId": "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&2B0D3A0E&0&0008",
      "pcie": { "cardMax": { "gen": 4, "width": 16 }, "slotMax": { "gen": 4, "width": 16 }, "now": { "gen": 1, "width": 16 } },
      "linkUnderLoad": null, "largestBarMb": 8192, "resizableBar": true }
  ],
  "memory": { "totalMb": 32768, "ddrGeneration": 4, "slotsTotal": 4, "ratedMts": 3600, "modules": [
    { "capacityMb": 16384, "configuredMts": 3600, "maxMts": 2133, "partNumber": "CMG32GX4M2D3600C18", "manufacturer": "Corsair", "bankLabel": "BANK 1", "deviceLocator": "ChannelA-DIMM2", "rank": 2 },
    { "capacityMb": 16384, "configuredMts": 3600, "maxMts": 2133, "partNumber": "CMG32GX4M2D3600C18", "manufacturer": "Corsair", "bankLabel": "BANK 3", "deviceLocator": "ChannelB-DIMM2", "rank": 2 }
  ] },
  "displays": [
    { "name": "\\\\.\\DISPLAY1", "adapter": "NVIDIA GeForce RTX 3060 Ti", "currentHz": 360, "maxHz": 360, "width": 1920, "height": 1080, "isPrimary": true, "isVirtual": false, "monitorName": "XL2566K", "connection": "DisplayPort" },
    { "name": "\\\\.\\DISPLAY2", "adapter": "NVIDIA GeForce RTX 3060 Ti", "currentHz": 180, "maxHz": 180, "width": 1920, "height": 1080, "isPrimary": false, "isVirtual": false, "monitorName": "24G4", "connection": "DisplayPort" }
  ],
  "board": { "manufacturer": "ASUSTeK COMPUTER INC.", "product": "ROG MAXIMUS XIII HERO", "biosVersion": "2103", "biosDate": "2024-05-20", "biosVendor": "American Megatrends Inc.", "chipset": "Intel Z590", "bootMode": "uefi" },
  "storage": {
    "valorant": { "path": "C:/Riot Games/VALORANT/live", "driveLetter": "C", "media": "nvme", "freeGb": 210.5, "totalGb": 931.5 },
    "drives": [
      { "model": "Samsung SSD 980 PRO 1TB", "media": "nvme", "bus": "nvme", "sizeGb": 931.5, "volumes": [ { "letter": "C", "freeGb": 210.5, "totalGb": 931.5 } ] },
      { "model": "WDC WD20EZBX-00AYRA0", "media": "hdd", "bus": "sata", "sizeGb": 1863.0, "volumes": [ { "letter": "D", "freeGb": 1228.8, "totalGb": 1863.0 } ] }
    ]
  },
  "windows": { "powerPlan": { "guid": "381b4222-f694-41f0-9685-ff5bb260df2e", "name": "Balanced" }, "gameMode": true, "backgroundRecording": false, "hags": true, "valorantGpuPreference": null, "pendingRestart": false, "startupAppCount": 7 },
  "security": { "tpm2": true, "secureBoot": true, "vbsRunning": false, "hvciRunning": false, "iommu": false, "vanguard": "running" },
  "network": { "adapters": [ { "name": "Intel(R) Ethernet Controller (3) I225-V", "kind": "ethernet", "linkMbps": 2500, "connected": true, "powerSaving": true } ], "deliveryOptimizationInternet": false },
  "background": { "busyProcesses": [] },
  "probeErrors": []
}
```

- [ ] **Step 5: Fill in the struct literals the new fields break**

Run: `grep -rnE "\b(GpuInfo|MemoryModule|DisplayInfo|CpuInfo|BoardInfo|MemoryInfo|StorageInfo) \{" crates app/src-tauri/src --include='*.rs' | grep -v "pub struct"`
Expected: besides Step 1's own tests in `snapshot.rs`, hits only in `crates/kestrel-core/src/{gpu,health,ram}.rs` (test helpers, with their `-> GpuInfo {` style signature lines) and `crates/kestrel-probe/src/win/{display,wmi_probe}.rs`. `health.rs`'s `sodimm` and `gpu.rs`'s `GpuInfo { driver_version: …, ..g }` already end in `..` and need nothing; kestrel-bench and the app build none of these structs.

Add `..Default::default()` as the last field of each literal that lists every field:

`crates/kestrel-core/src/gpu.rs`, test helper `gpu`:
```rust
            vram_mb: vram,
            is_integrated: integrated,
            ..Default::default()
        }
```

`crates/kestrel-core/src/ram.rs`, test helper `module`:
```rust
            bank_label: bank.into(),
            device_locator: locator.into(),
            ..Default::default()
        }
```

`crates/kestrel-core/src/health.rs`, test helpers `display` and `gpu`:
```rust
            is_primary: primary,
            is_virtual,
            ..Default::default()
        }
```
```rust
            vram_mb: Some(if integrated { 128 } else { 8192 }),
            is_integrated: integrated,
            ..Default::default()
        }
```

`crates/kestrel-probe/src/win/display.rs`, in `fill` (Task 11 adds the monitor name and connection):
```rust
            is_primary: dev.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0,
            ..Default::default()
        });
```

`crates/kestrel-probe/src/win/wmi_probe.rs`, in `gpus`, replace the `.map(|row| { … })` closure so the card keeps the PNP device id the query already reads (Tasks 10, 11 and 14 need it):
```rust
            .map(|row| {
                let name = text(row, "Name");
                let pnp_device_id = text(row, "PNPDeviceID");
                let vendor = gpu::vendor_from(&pnp_device_id, &name);
                let vram_mb = registry_probe::gpu_memory_mb(&name);
                GpuInfo {
                    is_integrated: gpu::looks_integrated(vendor, &name, vram_mb),
                    driver_version: text(row, "DriverVersion"),
                    driver_date: parse::cim_date(&text(row, "DriverDate")),
                    vram_mb,
                    vendor,
                    name,
                    pnp_device_id,
                    ..Default::default()
                }
            })
```
and in `memory` (Task 11 reads the rank):
```rust
                bank_label: text(row, "BankLabel"),
                device_locator: text(row, "DeviceLocator"),
                ..Default::default()
            })
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core`
Expected: PASS — `test result: ok. 94 passed` (the 91 tests from before plus the three new ones). No existing test changes. The second monitor has one visible effect: the owner's `display-refresh` title becomes "All monitors run at their top refresh rate". No Rust test pins the one-monitor title, and `app/src/api/mock-report.json` keeps it until Task 15 regenerates the sample.

Run: `cargo test -p kestrel-probe -p kestrel-bench`
Expected: PASS — kestrel-probe 22 unit tests and 2 smoke tests (still 27 checks; the mock reads the new fixture), kestrel-bench 42.

Run: `cd app && npm run build && cd .. && cargo test -p kestrel`
Expected: PASS — 17 tests; `the_context_comes_from_the_scan` still reads 360 Hz from the primary display. (`npm run build` first: the Tauri crate needs `app/dist` to compile.)

Run: `cargo fmt --all --check && cargo clippy -p kestrel-core -p kestrel-probe --all-targets -- -D warnings && cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc -- -D warnings`
Expected: no fmt diff; both clippy runs finish with no warnings. Only the Windows run compiles `win/wmi_probe.rs` and `win/display.rs`.

- [ ] **Step 7: Commit**

```bash
git add crates/kestrel-core/src/snapshot.rs crates/kestrel-core/fixtures/owner-pc.json crates/kestrel-core/src/gpu.rs crates/kestrel-core/src/ram.rs crates/kestrel-core/src/health.rs crates/kestrel-probe/src/win/wmi_probe.rs crates/kestrel-probe/src/win/display.rs
git commit -m "feat(core): snapshot schema 2 with the Hardware tab's fields and the owner's new parts" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: CPUID decoding and the codename table

**Files:**
- Create: `crates/kestrel-core/src/cpu.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (add `pub mod cpu;` right after `pub mod bios_guide;`)
- Test: `crates/kestrel-core/src/cpu.rs` (its `#[cfg(test)] mod tests`, 24 tests)

**Interfaces:**
- Consumes: `crate::snapshot::CpuVendor` — `pub enum CpuVendor { Intel, Amd, #[default] Other }` (stage 1, unchanged by Task 1). Nothing else from earlier tasks.
- Produces (exactly the contract's `kestrel-core::cpu`; Task 9's reader fills a `CpuidRaw`, calls `decode`, then `codename(id.vendor, id.family, id.model, id.stepping)`):
  - `#[derive(Debug, Clone, Default, PartialEq)] pub struct CpuidRaw { pub vendor: String, pub max_leaf: u32, pub leaf1: [u32; 4], pub leaf7: [u32; 4], pub leaf7_1_eax: u32, pub leaf16: Option<[u32; 3]>, pub brand: String, pub xcr0: Option<u64> }`
  - `#[derive(Debug, Clone, Default, PartialEq)] pub struct CpuIdentity { pub vendor: CpuVendor, pub family: u32, pub model: u32, pub stepping: u32, pub instructions: Vec<String>, pub hypervisor: bool, pub base_mhz: Option<u32>, pub max_mhz: Option<u32> }`
  - `pub fn decode(raw: &CpuidRaw) -> CpuIdentity` — never fails; `decode(&CpuidRaw::default()) == CpuIdentity::default()`. It ignores `brand` (the probe cleans and uses it).
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Codename { pub name: &'static str, pub socket: Option<&'static str> }`
  - `pub fn codename(vendor: CpuVendor, family: u32, model: u32, stepping: u32) -> Option<Codename>` — `None` for `CpuVendor::Other` and for anything the table doesn't list. The owner's `(Intel, 6, 0xA7, 1)` gives `Codename { name: "Rocket Lake-S", socket: Some("LGA1200") }`.
  - Strings that reach `CpuInfo`: instruction names `"SSE4.2"`, `"AVX"`, `"AVX2"`, `"AVX-512"`, `"AVX-VNNI"`, `"AMX"` (that order); sockets `"LGA1151"`, `"LGA1200"`, `"LGA1700"`, `"LGA1851"`, `"AM4"`, `"AM5"`, `"sTRX4"`, `"sWRX8"`, `"sTR5"`.

- [ ] **Step 1: Write the failing tests** — create `crates/kestrel-core/src/cpu.rs` containing only this test module, and add `pub mod cpu;` to `crates/kestrel-core/src/lib.rs` right after `pub mod bios_guide;`. The register values of the i9-11900K, Ryzen 9 9950X and Xeon w7-2475X are copied from InstLatx64's CPUID dumps (named in the comments). `INTEL_CHIPS` and `AMD_CHIPS` pair a real chip's leaf 1 EAX with the codename and socket it must get, so every table row is checked against signatures written independently of the table, through `decode` as the probe will call it; `every_row_is_reachable_and_has_a_chip` fails when a row has no such chip or an earlier row hides one of its steppings.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::CpuVendor::{Amd, Intel, Other};

    const INTEL_ID: &str = "GenuineIntel";
    const AMD_ID: &str = "AuthenticAMD";

    // Feature bits, written out here independently of the constants under test.
    const ECX_SSE4_2: u32 = 1 << 20;
    const ECX_OSXSAVE: u32 = 1 << 27;
    const ECX_AVX: u32 = 1 << 28;
    const ECX_HYPERVISOR: u32 = 1 << 31;
    const EBX7_AVX2: u32 = 1 << 5;
    const EBX7_AVX512F: u32 = 1 << 16;
    const EDX7_AMX_TILE: u32 = 1 << 24;
    const EAX7_1_AVX_VNNI: u32 = 1 << 4;
    /// XCR0 with x87 (bit 0), SSE (1) and AVX (2) state.
    const STATE_AVX: u64 = 0b111;
    /// Plus the AVX-512 state: opmask (5), upper halves of ZMM0–15 (6), ZMM16–31 (7).
    const STATE_AVX512: u64 = 0b1110_0111;
    /// The AMX state: tile configuration (17) and tile data (18).
    const STATE_AMX: u64 = 0b11 << 17;

    /// The owner's CPU: leaves 0, 1, 7 and 0x16 of the i9-11900K in InstLatx64's dump
    /// GenuineIntel00A0671_RocketLake_01, with the XCR0 Windows sets on an AVX-512 CPU.
    fn i9_11900k() -> CpuidRaw {
        CpuidRaw {
            vendor: INTEL_ID.into(),
            max_leaf: 0x1B,
            leaf1: [0x000A0671, 0x00100800, 0x7FFAFBFF, 0xBFEBFBFF],
            leaf7: [0x00000002, 0xF2BF67EB, 0x00405F4E, 0xBC000410],
            leaf7_1_eax: 0,
            leaf16: Some([0x0DAC, 0x14B4, 0x0064]),
            brand: "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz".into(),
            xcr0: Some(0xE7),
        }
    }

    /// A Ryzen 9 9950X (Zen 5) from InstLatx64's AuthenticAMD0B40F40_K20_GraniteRidge_02: its highest standard
    /// leaf is 0x10, so there is no leaf 0x16.
    fn ryzen_9_9950x() -> CpuidRaw {
        CpuidRaw {
            vendor: AMD_ID.into(),
            max_leaf: 0x10,
            leaf1: [0x00B40F40, 0x00200800, 0x7ED8320B, 0x178BFBFF],
            leaf7: [0x00000001, 0xF1BF97AB, 0x19405FCE, 0x10000110],
            leaf7_1_eax: 0x00000030,
            leaf16: None,
            brand: "AMD Ryzen 9 9950X 16-Core Processor".into(),
            xcr0: Some(0xE7),
        }
    }

    /// A CPU with this leaf 1 EAX and no feature bits.
    fn signature(vendor: &str, eax: u32) -> CpuidRaw {
        CpuidRaw {
            vendor: vendor.into(),
            max_leaf: 0x10,
            leaf1: [eax, 0, 0, 0],
            ..CpuidRaw::default()
        }
    }

    /// The instruction sets of an Intel CPU with these feature registers: leaf 1 ECX, leaf 7.0 EBX and EDX, leaf
    /// 7.1 EAX, and XCR0.
    fn sets(ecx: u32, ebx7: u32, edx7: u32, eax7_1: u32, xcr0: Option<u64>) -> Vec<String> {
        decode(&CpuidRaw {
            leaf1: [0x000A0671, 0, ecx, 0],
            leaf7: [1, ebx7, 0, edx7],
            leaf7_1_eax: eax7_1,
            xcr0,
            ..signature(INTEL_ID, 0x000A0671)
        })
        .instructions
    }

    #[test]
    fn decodes_the_owners_i9_11900k() {
        assert_eq!(
            decode(&i9_11900k()),
            CpuIdentity {
                vendor: Intel,
                family: 6,
                model: 0xA7,
                stepping: 1,
                instructions: vec![
                    "SSE4.2".into(),
                    "AVX".into(),
                    "AVX2".into(),
                    "AVX-512".into()
                ],
                hypervisor: false,
                base_mhz: Some(3500),
                max_mhz: Some(5300),
            }
        );
    }

    #[test]
    fn the_owners_cpu_is_rocket_lake_s_on_lga1200() {
        let id = decode(&i9_11900k());
        assert_eq!(
            codename(id.vendor, id.family, id.model, id.stepping),
            Some(Codename {
                name: "Rocket Lake-S",
                socket: Some("LGA1200")
            })
        );
    }

    #[test]
    fn decodes_a_zen_5_ryzen() {
        let id = decode(&ryzen_9_9950x());
        assert_eq!(
            (id.vendor, id.family, id.model, id.stepping),
            (Amd, 0x1A, 0x44, 0)
        );
        assert_eq!(
            id.instructions,
            ["SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI"]
        );
        assert!(!id.hypervisor);
    }

    #[test]
    fn adds_amd_extended_family_and_model() {
        // Ryzen 9 5950X (Zen 3 Vermeer, B0) and Ryzen 5 7600X (Zen 4 Raphael, B2).
        let vermeer = decode(&signature(AMD_ID, 0x00A20F10));
        assert_eq!(
            (vermeer.family, vermeer.model, vermeer.stepping),
            (0x19, 0x21, 0)
        );
        let raphael = decode(&signature(AMD_ID, 0x00A60F12));
        assert_eq!(
            (raphael.family, raphael.model, raphael.stepping),
            (0x19, 0x61, 2)
        );
    }

    #[test]
    fn extended_fields_count_only_for_the_families_that_use_them() {
        // The extended model joins base families 6 and 0xF only: family 5 keeps model 2.
        let family_5 = decode(&signature(INTEL_ID, 0x00010521));
        assert_eq!(
            (family_5.family, family_5.model, family_5.stepping),
            (5, 2, 1)
        );
        // The extended family is added to base family 0xF only.
        let family_6 = decode(&signature(INTEL_ID, 0x0FF00671));
        assert_eq!((family_6.family, family_6.model), (6, 0x07));
        // A Pentium 4 (Prescott, 0F-03-04): base family 0xF with extended family 0.
        let pentium_4 = decode(&signature(INTEL_ID, 0x00000F34));
        assert_eq!(
            (pentium_4.family, pentium_4.model, pentium_4.stepping),
            (0xF, 3, 4)
        );
    }

    #[test]
    fn sse4_2_needs_only_its_cpuid_bit() {
        assert_eq!(sets(ECX_SSE4_2, 0, 0, 0, None), ["SSE4.2"]);
        assert!(sets(0, 0, 0, 0, None).is_empty());
    }

    #[test]
    fn avx_counts_only_when_windows_saves_its_registers() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        let wide = EBX7_AVX2 | EBX7_AVX512F;
        assert_eq!(sets(avx, 0, 0, 0, Some(STATE_AVX)), ["AVX"]);
        // XCR0 without bit 2: the CPU has AVX, AVX2 and AVX-512, but Windows doesn't save the YMM registers.
        assert!(sets(avx, wide, 0, 0, Some(STATE_AVX512 & !(1 << 2))).is_empty());
        assert!(sets(avx, wide, 0, 0, Some(STATE_AVX512 & !(1 << 1))).is_empty());
        // Without OSXSAVE there is no XCR0 to read, and one that is passed anyway isn't trusted.
        assert!(sets(ECX_AVX, wide, 0, 0, None).is_empty());
        assert!(sets(ECX_AVX, wide, 0, 0, Some(STATE_AVX512)).is_empty());
        // XCR0 alone doesn't make a CPU without the AVX bit support AVX.
        assert!(sets(ECX_OSXSAVE, wide, 0, 0, Some(STATE_AVX512)).is_empty());
    }

    #[test]
    fn avx2_needs_usable_avx() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        assert_eq!(sets(avx, EBX7_AVX2, 0, 0, Some(STATE_AVX)), ["AVX", "AVX2"]);
        assert!(sets(ECX_OSXSAVE, EBX7_AVX2, 0, 0, Some(STATE_AVX)).is_empty());
    }

    #[test]
    fn avx_512_needs_all_three_of_its_register_states() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        assert_eq!(
            sets(avx, EBX7_AVX512F, 0, 0, Some(STATE_AVX512)),
            ["AVX", "AVX-512"]
        );
        for bit in [5, 6, 7] {
            let without = STATE_AVX512 & !(1 << bit);
            assert_eq!(
                sets(avx, EBX7_AVX512F, 0, 0, Some(without)),
                ["AVX"],
                "XCR0 bit {bit}"
            );
        }
        assert_eq!(sets(avx, 0, 0, 0, Some(STATE_AVX512)), ["AVX"]);
    }

    #[test]
    fn avx_vnni_reads_leaf_7_sub_leaf_1_and_needs_avx() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        assert_eq!(
            sets(avx, 0, 0, EAX7_1_AVX_VNNI, Some(STATE_AVX)),
            ["AVX", "AVX-VNNI"]
        );
        assert!(sets(ECX_OSXSAVE, 0, 0, EAX7_1_AVX_VNNI, Some(STATE_AVX)).is_empty());
    }

    #[test]
    fn amx_needs_the_tile_config_and_tile_data_state() {
        assert_eq!(
            sets(ECX_OSXSAVE, 0, EDX7_AMX_TILE, 0, Some(STATE_AMX)),
            ["AMX"]
        );
        for xcr0 in [Some(1 << 17), Some(1 << 18), Some(STATE_AVX512), None] {
            assert!(
                sets(ECX_OSXSAVE, 0, EDX7_AMX_TILE, 0, xcr0).is_empty(),
                "{xcr0:?}"
            );
        }
        assert!(sets(ECX_OSXSAVE, 0, 0, 0, Some(STATE_AMX)).is_empty());
    }

    #[test]
    fn every_set_is_listed_in_order() {
        let ecx = ECX_SSE4_2 | ECX_OSXSAVE | ECX_AVX;
        let ebx7 = EBX7_AVX2 | EBX7_AVX512F;
        assert_eq!(
            sets(
                ecx,
                ebx7,
                EDX7_AMX_TILE,
                EAX7_1_AVX_VNNI,
                Some(STATE_AVX512 | STATE_AMX)
            ),
            ["SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI", "AMX"]
        );
    }

    #[test]
    fn the_hypervisor_bit_is_leaf_1_ecx_bit_31() {
        let mut raw = i9_11900k();
        assert!(!decode(&raw).hypervisor);
        raw.leaf1[2] |= ECX_HYPERVISOR;
        assert!(decode(&raw).hypervisor);
    }

    #[test]
    fn leaf_0x16_clocks_are_intel_only_and_never_zero() {
        let clocks = |raw: &CpuidRaw| {
            let id = decode(raw);
            (id.base_mhz, id.max_mhz)
        };
        let mut intel = signature(INTEL_ID, 0x000A0671);
        // Only bits 15:0 hold the clock.
        intel.leaf16 = Some([0xFFFF_0000 | 3500, 0x0001_0000 | 5300, 100]);
        assert_eq!(clocks(&intel), (Some(3500), Some(5300)));
        // Zeros, as some virtual machines report.
        intel.leaf16 = Some([0, 0, 0]);
        assert_eq!(clocks(&intel), (None, None));
        intel.leaf16 = None;
        assert_eq!(clocks(&intel), (None, None));
        // AMD keeps leaf 0x16 reserved: whatever it holds is ignored.
        let mut amd = signature(AMD_ID, 0x00A60F12);
        amd.leaf16 = Some([3500, 5300, 100]);
        assert_eq!(clocks(&amd), (None, None));
        assert_eq!(clocks(&ryzen_9_9950x()), (None, None));
    }

    #[test]
    fn other_vendors_are_other_but_still_decoded() {
        // Hygon, VIA, Zhaoxin and nothing at all.
        for vendor in ["HygonGenuine", "CentaurHauls", "  Shanghai  ", ""] {
            let id = decode(&signature(vendor, 0x00A20F10));
            assert_eq!(id.vendor, Other, "{vendor:?}");
            assert_eq!((id.family, id.model), (0x19, 0x21));
        }
        assert_eq!(decode(&CpuidRaw::default()), CpuIdentity::default());
    }

    /// A real chip for every Intel row: leaf 1 EAX, codename, socket. Signatures from InstLatx64's dump names and
    /// Intel's microcode release notes.
    const INTEL_CHIPS: &[(u32, &str, Option<&str>)] = &[
        (0x000506E3, "Skylake-S/H", Some("LGA1151")), // Core i7-6700K
        (0x000406E3, "Skylake-U/Y", None),            // Core i7-6500U
        (0x000906E9, "Kaby Lake-S/H", Some("LGA1151")), // Core i3-7100, i7-8705G
        (0x000806E9, "Kaby Lake-U/Y / Amber Lake-Y", None), // Core i5-7200U, m3-8100Y
        (0x000806EA, "Kaby Lake-R / Coffee Lake-U", None), // Core i5-8250U
        (0x000806EB, "Whiskey Lake-U", None),         // Core i7-8565U
        (
            0x000806EC,
            "Whiskey Lake-U / Amber Lake-Y / Comet Lake-U",
            None,
        ), // Core i5-10210U
        (0x000906EA, "Coffee Lake-S/H", Some("LGA1151")), // Core i7-8700K, i7-9750H
        (0x000906EC, "Coffee Lake-S/H", Some("LGA1151")), // Core i5-9600K
        (0x000A0655, "Comet Lake-S", Some("LGA1200")), // Core i9-10900K
        (0x000A0654, "Comet Lake-S", Some("LGA1200")), // Xeon W-1290P
        (0x000A0652, "Comet Lake-H", None),           // Core i7-10750H
        (0x000A0660, "Comet Lake-U", None),           // Core i7-10710U
        (0x00060663, "Cannon Lake-U", None),          // Core i3-8121U
        (0x000706E5, "Ice Lake-U/Y", None),           // Core i7-1065G7
        (0x000806A1, "Lakefield", None),              // Core i5-L16G7
        (0x000806C1, "Tiger Lake-UP3/UP4", None),     // Core i5-1135G7
        (0x000806D1, "Tiger Lake-H", None),           // Core i7-11800H
        (0x000A0671, "Rocket Lake-S", Some("LGA1200")), // Core i9-11900K
        (0x00090672, "Alder Lake-S/HX", Some("LGA1700")), // Core i9-12900K, i5-12400
        (0x000906A3, "Alder Lake-H/P/U", None),       // Core i9-12900H
        (0x000B06E0, "Alder Lake-N / Twin Lake", None), // N95, Core 3 N355
        (0x000B0671, "Raptor Lake-S/HX", Some("LGA1700")), // Core i9-13900K
        (0x000B0674, "Bartlett Lake-S", Some("LGA1700")), // Core 7 251TE
        (0x000B06A2, "Raptor Lake-H/P/U", None),      // Core i5-1350P
        (0x000B06F2, "Raptor Lake-S/HX", Some("LGA1700")), // Core i5-13500
        (0x000D0670, "Bartlett Lake-S", Some("LGA1700")), // Core 9 273PQE
        (0x000A06A4, "Meteor Lake-H/U", None),        // Core Ultra 5 125H
        (0x000C0662, "Arrow Lake-S/HX", Some("LGA1851")), // Core Ultra 5 245K, 9 275HX
        (0x000C0652, "Arrow Lake-H", None),           // Core Ultra 9 285H
        (0x000B0650, "Arrow Lake-U", None),           // Core Ultra 7 265U
        (0x000B06D1, "Lunar Lake", None),             // Core Ultra 9 288V
        (0x000C06C3, "Panther Lake", None),           // Core Ultra 7 355
        (0x000D0651, "Wildcat Lake", None),           // Core 7 350
    ];

    /// A real chip for every AMD row. Signatures from InstLatx64's dump names; Chagall's from Linux's amd.c, the
    /// 8500G's and 9955HX's from CPU-Z validations.
    const AMD_CHIPS: &[(u32, &str, Option<&str>)] = &[
        (0x00800F11, "Summit Ridge", Some("AM4")), // Ryzen 7 1800X
        (0x00800F82, "Pinnacle Ridge", Some("AM4")), // Ryzen 5 2600X
        (0x00810F10, "Raven Ridge", Some("AM4")),  // Ryzen 5 2500U
        (0x00810F81, "Picasso", Some("AM4")),      // Athlon PRO 300GE, Ryzen 5 3550H
        (0x00820F01, "Dali", None),                // AMD 3020e
        (0x00830F10, "Castle Peak", Some("sTRX4")), // Threadripper 3970X
        (0x00860F01, "Renoir", Some("AM4")),       // Ryzen 7 4800H
        (0x00860F81, "Lucienne", None),            // Ryzen 5 5500U
        (0x00870F10, "Matisse", Some("AM4")),      // Ryzen 5 3600X
        (0x00890F02, "Van Gogh", None),            // Steam Deck LCD (Custom APU 0405)
        (0x00890F10, "Van Gogh", None),            // Steam Deck OLED (Custom APU 0932)
        (0x008A0F00, "Mendocino", None),           // Ryzen 5 7520U
        (0x00A00F82, "Chagall", Some("sWRX8")),    // Threadripper PRO 5000WX
        (0x00A10F81, "Storm Peak", Some("sTR5")),  // Threadripper PRO 7995WX
        (0x00A20F12, "Vermeer", Some("AM4")),      // Ryzen 7 5700X3D
        (0x00A40F41, "Rembrandt", None),           // Ryzen 7 6800HS
        (0x00A50F00, "Cezanne / Barcelo", Some("AM4")), // Ryzen 7 5800H, 5825U
        (0x00A60F12, "Raphael / Dragon Range", Some("AM5")), // Ryzen 5 7600X
        (0x00A70F41, "Phoenix", None),             // Ryzen 9 7940H
        (0x00A70F52, "Phoenix / Hawk Point", Some("AM5")), // Ryzen 7 8700G, 8845HS
        (0x00A70F80, "Phoenix 2", Some("AM5")),    // Ryzen 5 8500G, Ryzen Z1
        (0x00A70FC0, "Hawk Point 2", None),        // Ryzen 5 PRO 215
        (0x00B00F81, "Shimada Peak", Some("sTR5")), // Threadripper PRO 9985WX
        (0x00B20F40, "Strix Point / Gorgon Point", None), // Ryzen AI 9 HX 370, AI 9 465
        (0x00B40F40, "Granite Ridge / Fire Range", Some("AM5")), // Ryzen 9 9950X, 9955HX
        (0x00B60F00, "Krackan Point / Gorgon Point", None), // Ryzen AI 7 PRO 350, AI 7 450
        (0x00B70F00, "Strix Halo", None),          // Ryzen AI MAX+ 395
    ];

    #[test]
    fn every_chip_resolves_to_its_codename_and_socket() {
        for (vendor, chips) in [(INTEL_ID, INTEL_CHIPS), (AMD_ID, AMD_CHIPS)] {
            for &(eax, name, socket) in chips {
                let id = decode(&signature(vendor, eax));
                assert_eq!(
                    codename(id.vendor, id.family, id.model, id.stepping),
                    Some(Codename { name, socket }),
                    "{eax:#010X}"
                );
            }
        }
    }

    #[test]
    fn every_row_is_reachable_and_has_a_chip() {
        let tables = [
            (Intel, INTEL, INTEL_ID, INTEL_CHIPS),
            (Amd, AMD, AMD_ID, AMD_CHIPS),
        ];
        for (vendor, rows, id, chips) in tables {
            for row in rows {
                // No earlier row hides any of this row's steppings.
                let expected = Some(Codename {
                    name: row.name,
                    socket: row.socket,
                });
                for stepping in row.steppings.clone() {
                    let found = codename(vendor, row.family, row.model, stepping);
                    assert_eq!(found, expected, "{} stepping {stepping}", row.name);
                }
                let has_chip = chips.iter().any(|&(eax, _, _)| {
                    let chip = decode(&signature(id, eax));
                    chip.family == row.family
                        && chip.model == row.model
                        && row.steppings.contains(&chip.stepping)
                });
                assert!(has_chip, "no chip for {}", row.name);
            }
        }
    }

    #[test]
    fn kaby_lake_and_coffee_lake_share_model_0x9e() {
        let at = |stepping| codename(Intel, 6, 0x9E, stepping).map(|c| c.name);
        assert_eq!(at(9), Some("Kaby Lake-S/H"));
        for stepping in 10..=13 {
            assert_eq!(at(stepping), Some("Coffee Lake-S/H"));
        }
        assert_eq!(at(8), None);
        assert_eq!(at(14), None);
    }

    #[test]
    fn model_0x8e_tells_four_laptop_generations_apart_by_stepping() {
        let at = |stepping| codename(Intel, 6, 0x8E, stepping);
        assert_eq!(at(9).map(|c| c.name), Some("Kaby Lake-U/Y / Amber Lake-Y"));
        assert_eq!(at(10).map(|c| c.name), Some("Kaby Lake-R / Coffee Lake-U"));
        assert_eq!(at(11).map(|c| c.name), Some("Whiskey Lake-U"));
        assert_eq!(
            at(12).map(|c| c.name),
            Some("Whiskey Lake-U / Amber Lake-Y / Comet Lake-U")
        );
        assert!((9..=12).all(|stepping| at(stepping).is_some_and(|c| c.socket.is_none())));
        assert_eq!(at(8), None);
        assert_eq!(at(13), None);
    }

    #[test]
    fn comet_lake_h_and_s_differ_by_stepping() {
        let h = Codename {
            name: "Comet Lake-H",
            socket: None,
        };
        let s = Codename {
            name: "Comet Lake-S",
            socket: Some("LGA1200"),
        };
        assert_eq!(codename(Intel, 6, 0xA5, 2), Some(h));
        for stepping in 3..=5 {
            assert_eq!(codename(Intel, 6, 0xA5, stepping), Some(s));
        }
        assert_eq!(codename(Intel, 6, 0xA5, 1), None);
    }

    #[test]
    fn bartlett_lake_shares_raptor_lakes_model() {
        let at = |stepping| codename(Intel, 6, 0xB7, stepping).map(|c| c.name);
        assert_eq!(at(1), Some("Raptor Lake-S/HX"));
        assert_eq!(at(4), Some("Bartlett Lake-S"));
        assert_eq!(at(2), None);
    }

    #[test]
    fn unknown_models_are_not_guessed() {
        for (vendor, family, model, stepping) in [
            (Intel, 6, 0x55, 4),    // Skylake-SP Xeon
            (Intel, 6, 0x8F, 8),    // Sapphire Rapids Xeon w7-2475X
            (Intel, 6, 0x3C, 3),    // Haswell, older than the table
            (Intel, 0x12, 0x01, 0), // Nova Lake: no shipping chip to confirm yet
            (Amd, 0x19, 0x01, 1),   // EPYC Milan
            (Amd, 0x1A, 0x68, 0),   // Ryzen AI 5 430: one dump, not confirmed
            (Amd, 0x15, 0x02, 0),   // FX (Piledriver), older than the table
        ] {
            assert_eq!(
                codename(vendor, family, model, stepping),
                None,
                "{vendor:?} {family:#X}/{model:#X}"
            );
        }
    }

    #[test]
    fn the_vendor_must_match() {
        assert_eq!(codename(Amd, 6, 0xA7, 1), None);
        assert_eq!(codename(Intel, 0x19, 0x21, 0), None);
        assert_eq!(codename(Other, 6, 0xA7, 1), None);
        assert_eq!(codename(Other, 0x19, 0x21, 0), None);
    }

    #[test]
    fn decodes_amx_on_a_xeon_w_that_the_table_does_not_name() {
        // Xeon w7-2475X (Sapphire Rapids) from InstLatx64's GenuineIntel00806F8_SapphireRapids_05, with Windows'
        // AMX state on.
        let raw = CpuidRaw {
            vendor: INTEL_ID.into(),
            max_leaf: 0x20,
            leaf1: [0x000806F8, 0x00800800, 0x7FFEFBFF, 0xBFEBFBFF],
            leaf7: [0x00000002, 0xF3BFBFFB, 0xBB417FEE, 0xFFDD4430],
            leaf7_1_eax: 0x00001C30,
            leaf16: None,
            brand: "Intel(R) Xeon(R) w7-2475X".into(),
            xcr0: Some(STATE_AVX512 | STATE_AMX),
        };
        let id = decode(&raw);
        assert_eq!(
            id.instructions,
            ["SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI", "AMX"]
        );
        assert_eq!(codename(id.vendor, id.family, id.model, id.stepping), None);
    }
}
```

`crates/kestrel-core/src/lib.rs` then starts:

```rust
//! Kestrel core: the data model and every decision Kestrel makes. Pure Rust, no OS calls.

pub mod bios_guide;
pub mod cpu;
pub mod gpu;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core cpu::`
Expected: FAIL to compile with ``could not compile `kestrel-core` (lib test) due to 52 previous errors``. The errors are ``error[E0425]: cannot find function `decode` in this scope`` (and `codename`), ``error[E0422]: cannot find struct, variant or union type `CpuidRaw` in this scope`` (and `Codename`, `CpuIdentity`) and ``error[E0425]: cannot find value `INTEL` in this scope`` (and `AMD`), with the warning ``unused import: `super::*` ``.

- [ ] **Step 3: Implement** — put this above `#[cfg(test)]` in `crates/kestrel-core/src/cpu.rs` (the `//!` lines must be the first lines of the file). How the table works: a row is a family, a model, the steppings it covers and the codename with its socket; the first matching row wins. Rows use `ANY` unless Intel sells different CPUs under one model (0x8E, 0x9E, 0xA5, 0xB7), where each row lists only the steppings the sources confirm, so an unlisted stepping of those models is `None`. Where desktop and laptop chips share a signature (which CPUID can't tell apart), the name lists both (`"Coffee Lake-S/H"`, `"Raphael / Dragon Range"`) and the socket is the desktop one, as the contract defines it.

```rust
//! CPUID decoding: the CPU's maker, family, model and stepping, the instruction sets Windows lets programs use,
//! and the codename table. The probe runs the instruction; everything here works on the register values.
use crate::snapshot::CpuVendor;
use std::ops::RangeInclusive;

/// The registers the probe reads with `CPUID` and `XGETBV`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuidRaw {
    /// Leaf 0 vendor string, e.g. "GenuineIntel", "AuthenticAMD".
    pub vendor: String,
    /// Leaf 0 EAX: the highest standard leaf.
    pub max_leaf: u32,
    /// Leaf 1 EAX, EBX, ECX, EDX.
    pub leaf1: [u32; 4],
    /// Leaf 7 sub-leaf 0 EAX, EBX, ECX, EDX; zeros when `max_leaf` < 7.
    pub leaf7: [u32; 4],
    /// Leaf 7 sub-leaf 1 EAX; 0 when `max_leaf` < 7 or `leaf7[0]` < 1.
    pub leaf7_1_eax: u32,
    /// Leaf 0x16 EAX, EBX, ECX when `max_leaf` >= 0x16.
    pub leaf16: Option<[u32; 3]>,
    /// Leaves 0x80000002–4, NULs removed, trimmed; empty when unavailable.
    pub brand: String,
    /// XGETBV(0) when leaf 1 ECX bit 27 (OSXSAVE) is set.
    pub xcr0: Option<u64>,
}

/// What the registers say about the CPU.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuIdentity {
    pub vendor: CpuVendor,
    /// Display family and model: the extended fields already added.
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    /// The sets the CPU has and Windows enables, in this order: "SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI",
    /// "AMX".
    pub instructions: Vec<String>,
    /// Leaf 1 ECX bit 31: Windows runs on a hypervisor (Hyper-V, VBS) or inside a virtual machine.
    pub hypervisor: bool,
    /// Intel leaf 0x16 EAX[15:0] when non-zero.
    pub base_mhz: Option<u32>,
    /// Intel leaf 0x16 EBX[15:0] when non-zero.
    pub max_mhz: Option<u32>,
}

// Feature bits, as in Intel's SDM (Vol. 2A, CPUID) and Linux's arch/x86/include/asm/cpufeatures.h, where word 4 is
// leaf 1 ECX, word 9 leaf 7.0 EBX, word 12 leaf 7.1 EAX and word 18 leaf 7.0 EDX.
const SSE4_2: u32 = 1 << 20; // leaf 1 ECX: X86_FEATURE_XMM4_2 (4*32+20)
const OSXSAVE: u32 = 1 << 27; // leaf 1 ECX: X86_FEATURE_OSXSAVE (4*32+27)
const AVX: u32 = 1 << 28; // leaf 1 ECX: X86_FEATURE_AVX (4*32+28)
const HYPERVISOR: u32 = 1 << 31; // leaf 1 ECX: X86_FEATURE_HYPERVISOR (4*32+31)
const AVX2: u32 = 1 << 5; // leaf 7.0 EBX: X86_FEATURE_AVX2 (9*32+5)
const AVX512F: u32 = 1 << 16; // leaf 7.0 EBX: X86_FEATURE_AVX512F (9*32+16)
const AMX_TILE: u32 = 1 << 24; // leaf 7.0 EDX: X86_FEATURE_AMX_TILE (18*32+24)
const AVX_VNNI: u32 = 1 << 4; // leaf 7.1 EAX: X86_FEATURE_AVX_VNNI (12*32+4)

// XCR0: the register state the OS saves on a task switch (Linux's arch/x86/include/asm/fpu/types.h, enum
// xfeature). A set counts only when its state is saved, as the SDM's AVX and AVX-512 detection steps require.
const STATE_AVX: u64 = (1 << 1) | (1 << 2); // XFEATURE_SSE, XFEATURE_YMM
const STATE_AVX512: u64 = (1 << 5) | (1 << 6) | (1 << 7); // XFEATURE_OPMASK, _ZMM_Hi256, _Hi16_ZMM
const STATE_AMX: u64 = (1 << 17) | (1 << 18); // XFEATURE_XTILE_CFG, XFEATURE_XTILE_DATA

/// Decodes the registers. As Intel's and AMD's manuals define the display values, the extended family is added
/// when the base family is 0xF, and the extended model becomes the model's high nibble when the base family is 6
/// or 0xF.
pub fn decode(raw: &CpuidRaw) -> CpuIdentity {
    let vendor = match raw.vendor.as_str() {
        "GenuineIntel" => CpuVendor::Intel,
        "AuthenticAMD" => CpuVendor::Amd,
        _ => CpuVendor::Other,
    };
    let [eax, _, ecx, _] = raw.leaf1;
    let base_family = (eax >> 8) & 0xF;
    let base_model = (eax >> 4) & 0xF;
    let family = match base_family {
        0xF => base_family + ((eax >> 20) & 0xFF),
        _ => base_family,
    };
    let model = match base_family {
        6 | 0xF => (((eax >> 16) & 0xF) << 4) | base_model,
        _ => base_model,
    };

    let has = |register: u32, bit: u32| (register & bit) != 0;
    let os_saves =
        |state: u64| has(ecx, OSXSAVE) && raw.xcr0.is_some_and(|xcr0| (xcr0 & state) == state);
    let [_, ebx7, _, edx7] = raw.leaf7;
    let avx = has(ecx, AVX) && os_saves(STATE_AVX);
    let avx512 = avx && has(ebx7, AVX512F) && os_saves(STATE_AVX512);
    let sets = [
        ("SSE4.2", has(ecx, SSE4_2)),
        ("AVX", avx),
        ("AVX2", avx && has(ebx7, AVX2)),
        ("AVX-512", avx512),
        ("AVX-VNNI", avx && has(raw.leaf7_1_eax, AVX_VNNI)),
        ("AMX", has(edx7, AMX_TILE) && os_saves(STATE_AMX)),
    ];

    // Leaf 0x16 is Intel's; AMD keeps it reserved.
    let mhz = |value: u32| Some(value & 0xFFFF).filter(|&mhz| mhz > 0);
    let (base_mhz, max_mhz) = match (vendor, raw.leaf16) {
        (CpuVendor::Intel, Some([base, max, _])) => (mhz(base), mhz(max)),
        _ => (None, None),
    };

    CpuIdentity {
        vendor,
        family,
        model,
        stepping: eax & 0xF,
        instructions: sets
            .into_iter()
            .filter(|&(_, on)| on)
            .map(|(name, _)| name.to_string())
            .collect(),
        hypervisor: has(ecx, HYPERVISOR),
        base_mhz,
        max_mhz,
    }
}

/// A CPU's codename, and the socket of its desktop parts (None for chips sold only soldered in laptops and
/// handhelds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codename {
    pub name: &'static str,
    pub socket: Option<&'static str>,
}

/// The codename of a display family, model and stepping from `decode`, or None when the table doesn't list it.
pub fn codename(vendor: CpuVendor, family: u32, model: u32, stepping: u32) -> Option<Codename> {
    let rows = match vendor {
        CpuVendor::Intel => INTEL,
        CpuVendor::Amd => AMD,
        CpuVendor::Other => return None,
    };
    rows.iter()
        .find(|row| row.family == family && row.model == model && row.steppings.contains(&stepping))
        .map(|row| Codename {
            name: row.name,
            socket: row.socket,
        })
}

/// One table row: a family and model, the steppings it covers, and what that CPU is.
struct Row {
    family: u32,
    model: u32,
    steppings: RangeInclusive<u32>,
    name: &'static str,
    socket: Option<&'static str>,
}

const fn row(
    family: u32,
    model: u32,
    steppings: RangeInclusive<u32>,
    name: &'static str,
    socket: Option<&'static str>,
) -> Row {
    Row {
        family,
        model,
        steppings,
        name,
        socket,
    }
}

/// Every stepping: the model alone says which CPU it is.
const ANY: RangeInclusive<u32> = 0..=0xF;

const LGA1151: Option<&str> = Some("LGA1151");
const LGA1200: Option<&str> = Some("LGA1200");
const LGA1700: Option<&str> = Some("LGA1700");
const LGA1851: Option<&str> = Some("LGA1851");
const AM4: Option<&str> = Some("AM4");
const AM5: Option<&str> = Some("AM5");
const STRX4: Option<&str> = Some("sTRX4");
const SWRX8: Option<&str> = Some("sWRX8");
const STR5: Option<&str> = Some("sTR5");
/// Sold only soldered: laptops, handhelds, mini PCs.
const MOBILE: Option<&str> = None;

/// Intel Core client CPUs, 6th gen (Skylake) to Core Ultra series 3 (Panther Lake) and Core series 3 (Wildcat
/// Lake). Sources:
/// - Linux, arch/x86/include/asm/intel-family.h: which microarchitecture each model is, and the stepping split of
///   models 0x8E and 0x9E.
/// - Intel's microcode release notes (github.com/intel/Intel-Linux-Processor-Microcode-Data-Files, releasenote.md):
///   the products and segments behind each family-model-stepping, e.g. "RKL-S B0 06-a7-01 Core Gen11".
/// - InstLatx64's CPUID dumps (github.com/InstLatx64/InstLatx64): a real chip's signature for every row, and the
///   newest names (Bartlett Lake, Twin Lake, Wildcat Lake).
/// - libcpuid's src/recog_intel.c (github.com/anrieff/libcpuid): the segment names, e.g. Tiger Lake-UP3/UP4.
///
/// A model that desktop and laptop chips share names both segments (S/H, S/HX) and gives the desktop socket.
/// Kaby Lake-X (Core i7-7740X, LGA2066) shares 06-9E-09 with Kaby Lake-S and shows LGA1151.
const INTEL: &[Row] = &[
    // Skylake, Kaby Lake, Coffee Lake, Whiskey Lake, Amber Lake, Comet Lake (6th to 10th gen).
    row(6, 0x4E, ANY, "Skylake-U/Y", MOBILE),
    row(6, 0x5E, ANY, "Skylake-S/H", LGA1151),
    row(6, 0x8E, 9..=9, "Kaby Lake-U/Y / Amber Lake-Y", MOBILE),
    row(6, 0x8E, 10..=10, "Kaby Lake-R / Coffee Lake-U", MOBILE),
    row(6, 0x8E, 11..=11, "Whiskey Lake-U", MOBILE),
    row(
        6,
        0x8E,
        12..=12,
        "Whiskey Lake-U / Amber Lake-Y / Comet Lake-U",
        MOBILE,
    ),
    row(6, 0x9E, 9..=9, "Kaby Lake-S/H", LGA1151),
    row(6, 0x9E, 10..=13, "Coffee Lake-S/H", LGA1151),
    row(6, 0xA5, 2..=2, "Comet Lake-H", MOBILE),
    row(6, 0xA5, 3..=5, "Comet Lake-S", LGA1200),
    row(6, 0xA6, ANY, "Comet Lake-U", MOBILE),
    // Cannon Lake, Ice Lake, Lakefield, Tiger Lake, Rocket Lake (8th to 11th gen on new cores).
    row(6, 0x66, ANY, "Cannon Lake-U", MOBILE),
    row(6, 0x7E, ANY, "Ice Lake-U/Y", MOBILE),
    row(6, 0x8A, ANY, "Lakefield", MOBILE),
    row(6, 0x8C, ANY, "Tiger Lake-UP3/UP4", MOBILE),
    row(6, 0x8D, ANY, "Tiger Lake-H", MOBILE),
    row(6, 0xA7, ANY, "Rocket Lake-S", LGA1200),
    // Alder Lake, Raptor Lake, Bartlett Lake (12th to 14th gen, Core Series 2 with P-cores).
    row(6, 0x97, ANY, "Alder Lake-S/HX", LGA1700),
    row(6, 0x9A, ANY, "Alder Lake-H/P/U", MOBILE),
    row(6, 0xBE, ANY, "Alder Lake-N / Twin Lake", MOBILE),
    row(6, 0xB7, 1..=1, "Raptor Lake-S/HX", LGA1700),
    row(6, 0xB7, 4..=4, "Bartlett Lake-S", LGA1700),
    row(6, 0xBA, ANY, "Raptor Lake-H/P/U", MOBILE),
    row(6, 0xBF, ANY, "Raptor Lake-S/HX", LGA1700),
    row(6, 0xD7, ANY, "Bartlett Lake-S", LGA1700),
    // Core Ultra series 1 to 3, Core series 3.
    row(6, 0xAA, ANY, "Meteor Lake-H/U", MOBILE),
    row(6, 0xC6, ANY, "Arrow Lake-S/HX", LGA1851),
    row(6, 0xC5, ANY, "Arrow Lake-H", MOBILE),
    row(6, 0xB5, ANY, "Arrow Lake-U", MOBILE),
    row(6, 0xBD, ANY, "Lunar Lake", MOBILE),
    row(6, 0xCC, ANY, "Panther Lake", MOBILE),
    row(6, 0xD5, ANY, "Wildcat Lake", MOBILE),
];

/// AMD Ryzen and Threadripper, Zen (family 0x17) to Zen 5 (family 0x1A), desktop and mobile. Sources:
/// - Linux, arch/x86/kernel/cpu/amd.c (Zen generation by model range) and drivers/hwmon/k10temp.c (model comments,
///   e.g. "0x8: Zen3 TR Chagall").
/// - libcpuid's src/recog_amd.c (github.com/anrieff/libcpuid): the codename of each model.
/// - InstLatx64's CPUID dumps (github.com/InstLatx64/InstLatx64): a real chip's signature for every row but
///   Chagall (whose model and stepping are in amd.c's microcode table), and the names libcpuid lacks (Phoenix 2,
///   Hawk Point 2, Gorgon Point).
/// - CPU-Z validations (valid.x86.fr): Ryzen 9 9955HX as Fire Range (1A/44), Ryzen 5 8500G as Phoenix 2 (19/78).
///
/// A die sold as desktop and laptop chips names both (Raphael / Dragon Range) and gives the desktop socket.
/// Threadripper 1000 and 2000 (TR4) share 17/01 and 17/08 with Ryzen, and Threadripper PRO 3000WX (sWRX8) shares
/// 17/31 with Threadripper 3000, so those show the Ryzen or Threadripper 3000 name and socket.
const AMD: &[Row] = &[
    // Zen, Zen+, Zen 2.
    row(0x17, 0x01, ANY, "Summit Ridge", AM4),
    row(0x17, 0x08, ANY, "Pinnacle Ridge", AM4),
    row(0x17, 0x11, ANY, "Raven Ridge", AM4),
    row(0x17, 0x18, ANY, "Picasso", AM4),
    row(0x17, 0x20, ANY, "Dali", MOBILE),
    row(0x17, 0x31, ANY, "Castle Peak", STRX4),
    row(0x17, 0x60, ANY, "Renoir", AM4),
    row(0x17, 0x68, ANY, "Lucienne", MOBILE),
    row(0x17, 0x71, ANY, "Matisse", AM4),
    row(0x17, 0x90, ANY, "Van Gogh", MOBILE),
    row(0x17, 0x91, ANY, "Van Gogh", MOBILE),
    row(0x17, 0xA0, ANY, "Mendocino", MOBILE),
    // Zen 3, Zen 3+, Zen 4.
    row(0x19, 0x08, ANY, "Chagall", SWRX8),
    row(0x19, 0x18, ANY, "Storm Peak", STR5),
    row(0x19, 0x21, ANY, "Vermeer", AM4),
    row(0x19, 0x44, ANY, "Rembrandt", MOBILE),
    row(0x19, 0x50, ANY, "Cezanne / Barcelo", AM4),
    row(0x19, 0x61, ANY, "Raphael / Dragon Range", AM5),
    row(0x19, 0x74, ANY, "Phoenix", MOBILE),
    row(0x19, 0x75, ANY, "Phoenix / Hawk Point", AM5),
    row(0x19, 0x78, ANY, "Phoenix 2", AM5),
    row(0x19, 0x7C, ANY, "Hawk Point 2", MOBILE),
    // Zen 5.
    row(0x1A, 0x08, ANY, "Shimada Peak", STR5),
    row(0x1A, 0x24, ANY, "Strix Point / Gorgon Point", MOBILE),
    row(0x1A, 0x44, ANY, "Granite Ridge / Fire Range", AM5),
    row(0x1A, 0x60, ANY, "Krackan Point / Gorgon Point", MOBILE),
    row(0x1A, 0x70, ANY, "Strix Halo", MOBILE),
];
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core cpu::`
Expected: `test result: ok. 24 passed; 0 failed` (the other kestrel-core tests are filtered out).

Run: `cargo test -p kestrel-core`
Expected: every test passes — 24 more than after Task 1 (on the stage-3 branch before Task 1 the crate had 91, so 115 without Task 1's additions).

Run: `cargo fmt --all --check`
Expected: no output, exit code 0.

Run: `cargo clippy -p kestrel-core --all-targets -- -D warnings`
Expected: `Finished` with no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core/src/cpu.rs crates/kestrel-core/src/lib.rs
git commit -m "feat(core): decode CPUID registers and name the CPU's codename and socket" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Processor topology and per-core clocks

**Files:**
- Create: `crates/kestrel-core/src/topology.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (`pub mod topology;`)
- Test: `crates/kestrel-core/src/topology.rs` (`mod tests`)

**Interfaces:**
- Consumes: `kestrel_core::{CacheInfo, CacheKind}` (Task 1).
- Produces: `kestrel_core::topology::{PhysicalCore { efficiency_class: u8, logical: Vec<(u16, u8)> }, p_e_split(&[PhysicalCore]) -> Option<(u32, u32)>, group_caches(&[(u8, CacheKind, u32)]) -> Vec<CacheInfo>, parse_instance(&str) -> Option<(u16, u8)>, CoreKind { Performance, Efficiency }, CoreClock { core: u32, kind: Option<CoreKind>, mhz: Option<u32> }, core_clocks(&[PhysicalCore], &[(String, f64)], &[(String, f64)]) -> Vec<CoreClock>}`, exactly as in "Shared interfaces". `CoreClock` reaches the UI as `{"core": 9, "kind": "efficiency", "mhz": 4300}` (`kind` and `mhz` are `null` when unknown). Task 9 builds the `PhysicalCore`s and the `(level, kind, size_kb)` list, one entry per cache Windows reports; Task 12 passes `core_clocks` the PDH `Processor Frequency` and `% Processor Performance` values by instance name, `_Total` rows included (they are skipped).

- [ ] **Step 1: Write the failing tests**

Create `crates/kestrel-core/src/topology.rs` with the module comment and the tests; the code goes between them in Step 3. Review Focus 2 (hybrid CPUs) is pinned by `hybrid_cores_split_into_p_and_e`, `hybrid_caches_list_each_size_with_the_larger_first` and `hybrid_clocks_say_which_cores_are_p_and_e` on an 8P + 16E topology; Review Focus 3 (more than 64 logical processors) by `pdh_instance_names` and `clocks_map_cores_across_processor_groups`.

```rust
//! Processor topology: physical cores and their P/E kind, caches grouped by size, and each core's clock from the
//! per-processor PDH counters.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The owner's i9-11900K: 8 cores of one class, two threads each.
    fn rocket_lake() -> Vec<PhysicalCore> {
        (0..8u8)
            .map(|i| PhysicalCore {
                efficiency_class: 0,
                logical: vec![(0, 2 * i), (0, 2 * i + 1)],
            })
            .collect()
    }

    /// An i9-13900K as Windows lists it: 8 P-cores (class 1, two threads each), then 16 E-cores (class 0, one each).
    fn raptor_lake() -> Vec<PhysicalCore> {
        let p = (0..8u8).map(|i| PhysicalCore {
            efficiency_class: 1,
            logical: vec![(0, 2 * i), (0, 2 * i + 1)],
        });
        let e = (16..32u8).map(|n| PhysicalCore {
            efficiency_class: 0,
            logical: vec![(0, n)],
        });
        p.chain(e).collect()
    }

    fn values(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs
            .iter()
            .map(|&(name, value)| (name.to_string(), value))
            .collect()
    }

    fn cache(level: u8, kind: CacheKind, size_kb: u32, count: u32) -> CacheInfo {
        CacheInfo {
            level,
            kind,
            size_kb,
            count,
        }
    }

    #[test]
    fn one_efficiency_class_is_not_hybrid() {
        assert_eq!(p_e_split(&rocket_lake()), None);
        assert_eq!(p_e_split(&[]), None);
    }

    #[test]
    fn hybrid_cores_split_into_p_and_e() {
        assert_eq!(p_e_split(&raptor_lake()), Some((8, 16)));
    }

    #[test]
    fn the_highest_class_is_p_and_every_other_class_is_e() {
        // Three classes, such as P-cores, E-cores and low-power E-cores: only the top one is P.
        let class = |efficiency_class, n| {
            (0..n).map(move |i| PhysicalCore {
                efficiency_class,
                logical: vec![(0, i)],
            })
        };
        let cores: Vec<PhysicalCore> = class(2, 6).chain(class(1, 8)).chain(class(0, 2)).collect();
        assert_eq!(p_e_split(&cores), Some((6, 10)));
    }

    #[test]
    fn the_owners_caches_group_into_four_lines() {
        // Windows lists every cache: each core's L1 data, L1 instruction and L2, then the shared L3.
        let mut list = Vec::new();
        for _ in 0..8 {
            list.extend([
                (1, CacheKind::Data, 48),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 512),
            ]);
        }
        list.push((3, CacheKind::Unified, 16384));
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 48, 8),
                cache(1, CacheKind::Instruction, 32, 8),
                cache(2, CacheKind::Unified, 512, 8),
                cache(3, CacheKind::Unified, 16384, 1),
            ]
        );
    }

    #[test]
    fn hybrid_caches_list_each_size_with_the_larger_first() {
        // i9-13900K: each P-core has 48 KB + 32 KB of L1 and 2 MB of L2; each E-core 32 KB + 64 KB of L1, and each
        // cluster of four E-cores shares 4 MB of L2.
        let mut list = Vec::new();
        for _ in 0..8 {
            list.extend([
                (1, CacheKind::Data, 48),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 2048),
            ]);
        }
        for _ in 0..16 {
            list.extend([(1, CacheKind::Data, 32), (1, CacheKind::Instruction, 64)]);
        }
        for _ in 0..4 {
            list.push((2, CacheKind::Unified, 4096));
        }
        list.push((3, CacheKind::Unified, 36864));
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 48, 8),
                cache(1, CacheKind::Data, 32, 16),
                cache(1, CacheKind::Instruction, 64, 16),
                cache(1, CacheKind::Instruction, 32, 8),
                cache(2, CacheKind::Unified, 4096, 4),
                cache(2, CacheKind::Unified, 2048, 8),
                cache(3, CacheKind::Unified, 36864, 1),
            ]
        );
    }

    #[test]
    fn two_l3_caches_are_counted_not_merged() {
        // Ryzen 9 7950X: two dies, each with its own 32 MB L3.
        let mut list = Vec::new();
        for _ in 0..16 {
            list.extend([
                (1, CacheKind::Data, 32),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 1024),
            ]);
        }
        list.extend([
            (3, CacheKind::Unified, 32768),
            (3, CacheKind::Unified, 32768),
        ]);
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 32, 16),
                cache(1, CacheKind::Instruction, 32, 16),
                cache(2, CacheKind::Unified, 1024, 16),
                cache(3, CacheKind::Unified, 32768, 2),
            ]
        );
    }

    #[test]
    fn caches_sort_by_level_then_kind_whatever_order_windows_gives() {
        let list = [
            (3, CacheKind::Unified, 8192),
            (2, CacheKind::Unknown, 256),
            (2, CacheKind::Unified, 1024),
            (1, CacheKind::Instruction, 32),
            (1, CacheKind::Data, 32),
        ];
        assert_eq!(
            group_caches(&list),
            [
                cache(1, CacheKind::Data, 32, 1),
                cache(1, CacheKind::Instruction, 32, 1),
                cache(2, CacheKind::Unified, 1024, 1),
                cache(2, CacheKind::Unknown, 256, 1),
                cache(3, CacheKind::Unified, 8192, 1),
            ]
        );
        assert!(group_caches(&[]).is_empty());
    }

    #[test]
    fn pdh_instance_names() {
        assert_eq!(parse_instance("0,15"), Some((0, 15)));
        assert_eq!(parse_instance("1,3"), Some((1, 3)));
        assert_eq!(parse_instance("2,63"), Some((2, 63)));
        for other in [
            "_Total", "0,_Total", "", "0", "0,", ",3", "0,1,2", "a,b", "-1,3", "+0,+1", " 0,1",
            "0,256", "70000,1",
        ] {
            assert_eq!(parse_instance(other), None, "{other:?}");
        }
    }

    #[test]
    fn clocks_are_frequency_times_performance_one_per_core() {
        let mut frequency = Vec::new();
        let mut performance = Vec::new();
        for n in 0..16u8 {
            frequency.push((format!("0,{n}"), 3500.0));
            // Above 100%: the core runs over its base clock.
            performance.push((format!("0,{n}"), 137.14 + f64::from(n / 2) * 2.0));
        }
        frequency.push(("0,_Total".into(), 3500.0));
        frequency.push(("_Total".into(), 3500.0));
        performance.push(("_Total".into(), 140.0));
        let clocks = core_clocks(&rocket_lake(), &frequency, &performance);
        assert_eq!(clocks.len(), 8);
        assert_eq!(
            clocks[0],
            CoreClock {
                core: 0,
                kind: None,
                mhz: Some(4800)
            }
        );
        // 3500 × 151.14% = 5289.9 MHz, rounded.
        assert_eq!(
            clocks[7],
            CoreClock {
                core: 7,
                kind: None,
                mhz: Some(5290)
            }
        );
        assert!(clocks.iter().all(|c| c.kind.is_none()));
    }

    #[test]
    fn hybrid_clocks_say_which_cores_are_p_and_e() {
        let cores = raptor_lake();
        let (mut frequency, mut performance) = (Vec::new(), Vec::new());
        for n in 0..32u8 {
            let (base, percent) = if n < 16 {
                (3000.0, 183.4)
            } else {
                (2200.0, 195.5)
            };
            frequency.push((format!("0,{n}"), base));
            performance.push((format!("0,{n}"), percent));
        }
        let clocks = core_clocks(&cores, &frequency, &performance);
        assert_eq!(clocks.len(), 24);
        assert!(clocks[..8]
            .iter()
            .all(|c| c.kind == Some(CoreKind::Performance) && c.mhz == Some(5502)));
        assert!(clocks[8..]
            .iter()
            .all(|c| c.kind == Some(CoreKind::Efficiency) && c.mhz == Some(4301)));
        assert_eq!(
            clocks.iter().map(|c| c.core).collect::<Vec<_>>(),
            (0..24).collect::<Vec<u32>>()
        );
    }

    #[test]
    fn clocks_map_cores_across_processor_groups() {
        // More than 64 logical processors: Windows splits them into groups, and PDH names each one "group,number".
        let core = |logical: Vec<(u16, u8)>| PhysicalCore {
            efficiency_class: 0,
            logical,
        };
        let cores = [
            core(vec![(0, 62), (0, 63)]),
            core(vec![(1, 0), (1, 1)]),
            core(vec![(1, 2), (1, 3)]),
        ];
        let frequency = values(&[
            ("0,1", 9999.0),
            ("0,62", 2500.0),
            ("0,63", 2500.0),
            ("1,0", 2600.0),
            ("1,1", 2600.0),
            ("1,2", 2700.0),
            ("1,3", 2700.0),
            ("0,_Total", 2500.0),
            ("1,_Total", 2650.0),
            ("_Total", 2600.0),
        ]);
        let performance = values(&[
            ("0,1", 100.0),
            ("0,62", 100.0),
            ("0,63", 100.0),
            ("1,0", 150.0),
            ("1,1", 150.0),
            ("1,2", 120.0),
            ("1,3", 120.0),
        ]);
        let mhz: Vec<Option<u32>> = core_clocks(&cores, &frequency, &performance)
            .iter()
            .map(|c| c.mhz)
            .collect();
        assert_eq!(mhz, [Some(2500), Some(3900), Some(3240)]);
    }

    #[test]
    fn a_core_uses_its_first_logical_processor_with_both_values() {
        let core = |logical: Vec<(u16, u8)>| PhysicalCore {
            efficiency_class: 0,
            logical,
        };
        let cores = [
            core(vec![(0, 0), (0, 1)]),
            core(vec![(0, 2), (0, 3)]),
            core(vec![(0, 4), (0, 5)]),
            core(vec![(0, 6), (0, 7)]),
        ];
        let frequency = values(&[
            ("0,0", 3500.0),
            ("0,1", 3500.0),
            ("0,2", 3500.0),
            ("0,3", 3500.0),
            ("0,6", 0.0),
            ("0,7", f64::NAN),
        ]);
        let performance = values(&[
            ("0,0", 100.0),
            ("0,1", 140.0),
            ("0,3", 120.0),
            ("0,6", 100.0),
            ("0,7", 100.0),
        ]);
        let mhz: Vec<Option<u32>> = core_clocks(&cores, &frequency, &performance)
            .iter()
            .map(|c| c.mhz)
            .collect();
        // Core 0 takes its first thread; core 1's first has no performance value, so its second counts; core 2 has
        // no data; core 3's values aren't real clocks (0 and not a number).
        assert_eq!(mhz, [Some(3500), Some(4200), None, None]);
    }

    #[test]
    fn no_cores_or_no_counters() {
        assert!(core_clocks(&[], &[], &[]).is_empty());
        let clocks = core_clocks(&rocket_lake(), &[], &[]);
        assert_eq!(clocks.len(), 8);
        assert!(clocks.iter().all(|c| c.mhz.is_none()));
    }

    #[test]
    fn clocks_reach_the_ui_as_camel_case_json() {
        let e = CoreClock {
            core: 9,
            kind: Some(CoreKind::Efficiency),
            mhz: Some(4300),
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"core": 9, "kind": "efficiency", "mhz": 4300})
        );
        let unknown = CoreClock {
            core: 0,
            kind: None,
            mhz: None,
        };
        assert_eq!(
            serde_json::to_value(&unknown).unwrap(),
            json!({"core": 0, "kind": null, "mhz": null})
        );
        assert_eq!(
            serde_json::to_value(CoreKind::Performance).unwrap(),
            json!("performance")
        );
    }
}
```

In `crates/kestrel-core/src/lib.rs`, add the module after `pub mod snapshot;`:

```rust
pub mod snapshot;
pub mod topology;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core topology`
Expected: FAIL to compile — `cannot find function 'p_e_split' in this scope`, `cannot find struct, variant or union type 'PhysicalCore' in this scope`, `cannot find type 'CacheKind' in this scope` and the like.

- [ ] **Step 3: Implement**

Insert this in `crates/kestrel-core/src/topology.rs` between the module comment and `#[cfg(test)]`. P-cores are the cores of the highest efficiency class and every other class counts as E (low-power E-cores included). A counter value that isn't a finite number above zero counts as missing, so a core whose first logical processor has no usable pair falls through to the next one, and a core with none reads Not reported rather than 0 GHz.

```rust
use crate::snapshot::{CacheInfo, CacheKind};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashMap;

/// One physical core as `GetLogicalProcessorInformationEx` lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhysicalCore {
    pub efficiency_class: u8,
    /// (processor group, number within the group) of each logical processor on the core.
    pub logical: Vec<(u16, u8)>,
}

/// The efficiency class of the P-cores: the highest one, but only when the cores don't all share it.
fn performance_class(cores: &[PhysicalCore]) -> Option<u8> {
    let classes = cores.iter().map(|c| c.efficiency_class);
    let (low, high) = (classes.clone().min()?, classes.max()?);
    (low != high).then_some(high)
}

/// Some((P-cores, E-cores)) only when the cores have more than one efficiency class; P = the highest class.
pub fn p_e_split(cores: &[PhysicalCore]) -> Option<(u32, u32)> {
    let top = performance_class(cores)?;
    let p = cores.iter().filter(|c| c.efficiency_class == top).count() as u32;
    Some((p, cores.len() as u32 - p))
}

fn kind_order(kind: CacheKind) -> u8 {
    match kind {
        CacheKind::Data => 0,
        CacheKind::Instruction => 1,
        CacheKind::Unified => 2,
        CacheKind::Unknown => 3,
    }
}

/// One entry per distinct (level, kind, size) with its count; sorted by level, then Data, Instruction, Unified,
/// Unknown, then larger size first.
pub fn group_caches(caches: &[(u8, CacheKind, u32)]) -> Vec<CacheInfo> {
    let mut groups: Vec<CacheInfo> = Vec::new();
    for &(level, kind, size_kb) in caches {
        match groups
            .iter_mut()
            .find(|g| (g.level, g.kind, g.size_kb) == (level, kind, size_kb))
        {
            Some(group) => group.count += 1,
            None => groups.push(CacheInfo {
                level,
                kind,
                size_kb,
                count: 1,
            }),
        }
    }
    groups.sort_by_key(|g| (g.level, kind_order(g.kind), Reverse(g.size_kb)));
    groups
}

/// Plain decimal digits only: `str::parse` would also take a leading "+".
fn digits<T: std::str::FromStr>(text: &str) -> Option<T> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// PDH `Processor Information` instance "g,n" → (g, n); "_Total", "0,_Total" and anything else → None.
pub fn parse_instance(name: &str) -> Option<(u16, u8)> {
    let (group, number) = name.split_once(',')?;
    Some((digits(group)?, digits(number)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoreKind {
    Performance,
    Efficiency,
}

/// A physical core's clock right now, for the Hardware tab's live clocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreClock {
    pub core: u32,
    pub kind: Option<CoreKind>,
    pub mhz: Option<u32>,
}

/// Counter values by logical processor. A value that isn't a number above zero counts as not reported.
fn by_processor(values: &[(String, f64)]) -> HashMap<(u16, u8), f64> {
    values
        .iter()
        .filter(|(_, value)| value.is_finite() && *value > 0.0)
        .filter_map(|(name, value)| Some((parse_instance(name)?, *value)))
        .collect()
}

/// One clock per physical core, in `cores` order (`core` = its index). MHz = `Processor Frequency` ×
/// `% Processor Performance` / 100 of the core's first logical processor that has both values, rounded; None when
/// none has. `kind` is Some only on hybrid CPUs.
pub fn core_clocks(
    cores: &[PhysicalCore],
    frequency: &[(String, f64)],
    performance: &[(String, f64)],
) -> Vec<CoreClock> {
    let frequency = by_processor(frequency);
    let performance = by_processor(performance);
    let top = performance_class(cores);
    cores
        .iter()
        .enumerate()
        .map(|(index, core)| CoreClock {
            core: index as u32,
            kind: top.map(|top| {
                if core.efficiency_class == top {
                    CoreKind::Performance
                } else {
                    CoreKind::Efficiency
                }
            }),
            mhz: core.logical.iter().find_map(|processor| {
                let mhz = frequency.get(processor)? * performance.get(processor)? / 100.0;
                Some(mhz.round() as u32)
            }),
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core topology`
Expected: PASS — `test result: ok. 14 passed`.

Run: `cargo test -p kestrel-core && cargo fmt --all --check && cargo clippy -p kestrel-core --all-targets -- -D warnings`
Expected: every kestrel-core test passes, no fmt diff, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core/src/topology.rs crates/kestrel-core/src/lib.rs
git commit -m "feat(core): physical cores, P/E split, cache groups and per-core clocks" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: PCIe link, Resizable BAR and links measured under load

**Files:**
- Create: `crates/kestrel-core/src/pcie.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (`pub mod pcie;`)
- Test: `crates/kestrel-core/src/pcie.rs` (`mod tests`)

**Interfaces:**
- Consumes: `kestrel_core::{GpuVendor, LinkUnderLoad, PcieInfo, PcieLink, SystemSnapshot, owner_pc_fixture}` (Task 1); `kestrel_core::gpu::looks_integrated` (stage 1).
- Produces: `kestrel_core::pcie::{ROOT_PORT = 8, UPSTREAM_SWITCH_PORT = 9, DOWNSTREAM_SWITCH_PORT = 10, REBAR_THRESHOLD_MB = 256, link_from_codes(Option<u32>, Option<u32>) -> Option<PcieLink>, PciNode { instance_id: String, parent: Option<String>, device_type: Option<u32>, current: Option<PcieLink>, max: Option<PcieLink> }, slot_device<'a>(&'a [PciNode], &str) -> Option<&'a PciNode>, resolve(&[PciNode], &str) -> Option<PcieInfo>, bar_summary(&[u64]) -> (Option<u64>, Option<bool>), uses_resizable_bar(GpuVendor, &str) -> bool, gen_label(u8) -> String, LinkMeasurement { pnp_device_id: String, link: PcieLink, measured_at: String, benchmark_id: String }, apply_links(&mut SystemSnapshot, &[LinkMeasurement])}`, as in "Shared interfaces". Task 10 builds `nodes` from the card up to and including its root port (each with its parent's instance id and `DeviceType`); Task 6 uses `uses_resizable_bar` and `gen_label`; Task 14 builds the `LinkMeasurement`s.

- [ ] **Step 1: Write the failing tests**

Create `crates/kestrel-core/src/pcie.rs` with the module comment and the tests; the code goes between them in Step 3. Review Focus 1 (missing or zero link properties) is pinned by `a_sleeping_or_unreadable_link_is_none_never_x0` (`link_from_codes(Some(4), Some(0)) == None`) and `a_card_without_link_data_resolves_to_not_reported`; Review Focus 4 (several adapters) by `each_card_gets_its_newest_measurement` and `a_measurement_on_another_card_is_never_applied`. The instance ids are written as Configuration Manager reports them; only their `VEN_` part and their parent links matter.

```rust
//! The graphics card's PCIe link: decoding Windows' link codes, finding the slot's link above the card, the
//! Resizable BAR rule, and applying links measured during benchmarks.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    const RTX: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&2B0D3A0E&0&0008";
    const INTEL_ROOT: &str = "PCI\\VEN_8086&DEV_4C01&SUBSYS_86941043&REV_01\\3&11583659&0&08";
    const RADEON: &str = "PCI\\VEN_1002&DEV_73BF&SUBSYS_0E3A1002&REV_C1\\6&1B3F2C1A&0&00000019";
    const RADEON_DOWN: &str = "PCI\\VEN_1002&DEV_1479&SUBSYS_00000000&REV_00\\5&22A3F1B&0&000019";
    const RADEON_UP: &str = "PCI\\VEN_1002&DEV_1478&SUBSYS_00000000&REV_C1\\4&2C9E7C26&0&0019";
    const AMD_ROOT: &str = "PCI\\VEN_1022&DEV_1483&SUBSYS_14531022&REV_00\\3&2411E6FE&0&09";
    const HOST_BRIDGE: &str = "ACPI\\PNP0A08\\0";
    /// Device types in the lists below: the graphics card (a PCI Express endpoint) and the two kinds of switch port.
    const ENDPOINT: u32 = 2;
    const UP: u32 = UPSTREAM_SWITCH_PORT;
    const DOWN: u32 = DOWNSTREAM_SWITCH_PORT;

    fn link(gen: u8, width: u8) -> Option<PcieLink> {
        Some(PcieLink { gen, width })
    }

    fn node(
        id: &str,
        parent: &str,
        device_type: u32,
        current: Option<PcieLink>,
        max: Option<PcieLink>,
    ) -> PciNode {
        PciNode {
            instance_id: id.into(),
            parent: Some(parent.into()),
            device_type: Some(device_type),
            current,
            max,
        }
    }

    /// The owner's RTX 3060 Ti in the CPU's x16 slot, idling at PCIe 1.0.
    fn owner_nodes() -> Vec<PciNode> {
        vec![
            node(INTEL_ROOT, HOST_BRIDGE, ROOT_PORT, link(1, 16), link(4, 16)),
            node(RTX, INTEL_ROOT, ENDPOINT, link(1, 16), link(4, 16)),
        ]
    }

    /// An RX 6800 XT: the GPU sits behind the card's own switch, whose upstream port is in a PCIe 3.0 slot.
    fn radeon_nodes() -> Vec<PciNode> {
        vec![
            node(AMD_ROOT, HOST_BRIDGE, ROOT_PORT, link(3, 8), link(3, 16)),
            node(RADEON_UP, AMD_ROOT, UP, link(3, 8), link(4, 16)),
            node(RADEON_DOWN, RADEON_UP, DOWN, link(4, 16), link(4, 16)),
            node(RADEON, RADEON_DOWN, ENDPOINT, link(4, 16), link(4, 16)),
        ]
    }

    fn slot_id<'a>(nodes: &'a [PciNode], gpu: &str) -> Option<&'a str> {
        slot_device(nodes, gpu).map(|n| n.instance_id.as_str())
    }

    #[test]
    fn link_codes_decode_to_generations_and_lanes() {
        for gen in 1..=6u8 {
            assert_eq!(
                link_from_codes(Some(u32::from(gen)), Some(16)),
                link(gen, 16)
            );
        }
        for width in [1u8, 2, 4, 8, 12, 16, 32] {
            assert_eq!(
                link_from_codes(Some(4), Some(u32::from(width))),
                link(4, width)
            );
        }
    }

    #[test]
    fn a_sleeping_or_unreadable_link_is_none_never_x0() {
        assert_eq!(link_from_codes(Some(4), Some(0)), None);
        assert_eq!(link_from_codes(Some(0), Some(16)), None);
        assert_eq!(link_from_codes(Some(7), Some(16)), None);
        assert_eq!(link_from_codes(Some(4), Some(33)), None);
        // 272 would wrap to 16 if it were cast to a byte.
        assert_eq!(link_from_codes(Some(4), Some(272)), None);
        assert_eq!(link_from_codes(Some(u32::MAX), Some(16)), None);
        assert_eq!(link_from_codes(None, Some(16)), None);
        assert_eq!(link_from_codes(Some(4), None), None);
        assert_eq!(link_from_codes(None, None), None);
    }

    #[test]
    fn a_card_straight_under_the_root_port_is_its_own_slot_device() {
        let nodes = owner_nodes();
        assert_eq!(slot_id(&nodes, RTX), Some(RTX));
        assert_eq!(
            resolve(&nodes, RTX),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: link(4, 16),
                now: link(1, 16),
            })
        );
        assert_eq!(resolve(&nodes, RTX), owner_pc_fixture().gpus[1].pcie);
    }

    #[test]
    fn an_amd_card_is_read_above_its_own_switch() {
        // The GPU's own link to its switch is internal (here 4.0 x16); the slot's link is the upstream port's.
        let nodes = radeon_nodes();
        assert_eq!(slot_id(&nodes, RADEON), Some(RADEON_UP));
        assert_eq!(
            resolve(&nodes, RADEON),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: link(3, 16),
                now: link(3, 8),
            })
        );
    }

    #[test]
    fn a_chipset_switch_is_not_the_cards_own_and_its_slot_maximum_is_unknown() {
        // AMD's X570 chipset is a PCIe switch (vendor 1022). Cards in its slot run at x4 there.
        const X570_UP: &str = "PCI\\VEN_1022&DEV_57AD&SUBSYS_00000000&REV_00\\4&1A2B3C4D&0&0209";
        const X570_DOWN: &str =
            "PCI\\VEN_1022&DEV_57A4&SUBSYS_00000000&REV_00\\5&3E4F5A6B&0&400209";
        let chipset = || {
            vec![
                node(AMD_ROOT, HOST_BRIDGE, ROOT_PORT, link(4, 4), link(4, 4)),
                node(X570_UP, AMD_ROOT, UP, link(4, 4), link(4, 4)),
                node(X570_DOWN, X570_UP, DOWN, link(4, 4), link(4, 4)),
            ]
        };
        let mut nvidia = chipset();
        nvidia.push(node(RTX, X570_DOWN, ENDPOINT, link(4, 4), link(4, 16)));
        assert_eq!(slot_id(&nvidia, RTX), Some(RTX));
        assert_eq!(
            resolve(&nvidia, RTX),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: None,
                now: link(4, 4),
            })
        );
        // A Radeon there climbs its own switch, then stops at the chipset's.
        let mut amd = chipset();
        amd.extend([
            node(RADEON_UP, X570_DOWN, UP, link(4, 4), link(4, 16)),
            node(RADEON_DOWN, RADEON_UP, DOWN, link(4, 16), link(4, 16)),
            node(RADEON, RADEON_DOWN, ENDPOINT, link(4, 16), link(4, 16)),
        ]);
        assert_eq!(slot_id(&amd, RADEON), Some(RADEON_UP));
        assert_eq!(
            resolve(&amd, RADEON),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: None,
                now: link(4, 4),
            })
        );
    }

    #[test]
    fn a_card_in_a_thunderbolt_enclosure_reads_its_own_link() {
        // The enclosure's and the PC's Thunderbolt controllers are Intel switches; the card's x4 link is real.
        const BOX_DOWN: &str = "PCI\\VEN_8086&DEV_15EF&SUBSYS_00000000&REV_06\\7&1F2E3D4C&0&20";
        const BOX_UP: &str = "PCI\\VEN_8086&DEV_15EF&SUBSYS_00000000&REV_06\\6&2A3B4C5D&0&00";
        const PC_DOWN: &str = "PCI\\VEN_8086&DEV_15EA&SUBSYS_00000000&REV_06\\5&3B4C5D6E&0&08";
        const PC_UP: &str = "PCI\\VEN_8086&DEV_15EA&SUBSYS_00000000&REV_06\\4&4C5D6E7F&0&00";
        let nodes = vec![
            node(INTEL_ROOT, HOST_BRIDGE, ROOT_PORT, link(3, 4), link(3, 4)),
            node(PC_UP, INTEL_ROOT, UP, link(3, 4), link(3, 4)),
            node(PC_DOWN, PC_UP, DOWN, link(1, 4), link(3, 4)),
            node(BOX_UP, PC_DOWN, UP, link(1, 4), link(3, 4)),
            node(BOX_DOWN, BOX_UP, DOWN, link(3, 4), link(3, 4)),
            node(RTX, BOX_DOWN, ENDPOINT, link(3, 4), link(4, 16)),
        ];
        assert_eq!(slot_id(&nodes, RTX), Some(RTX));
        assert_eq!(
            resolve(&nodes, RTX),
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: None,
                now: link(3, 4),
            })
        );
    }

    #[test]
    fn a_card_missing_from_the_list_has_no_link() {
        let nodes = owner_nodes();
        let other = "PCI\\VEN_10DE&DEV_2704&SUBSYS_00000000&REV_A1\\4&0&0&0008";
        assert_eq!(slot_device(&nodes, other), None);
        assert_eq!(resolve(&nodes, other), None);
        assert_eq!(resolve(&nodes, ""), None);
        assert_eq!(resolve(&[], RTX), None);
    }

    #[test]
    fn instance_ids_compare_case_insensitively() {
        let lower = RTX.to_ascii_lowercase();
        assert_eq!(
            resolve(&owner_nodes(), &lower),
            resolve(&owner_nodes(), RTX)
        );
        let mut nodes = radeon_nodes();
        for n in &mut nodes {
            n.parent = n.parent.as_ref().map(|p| p.to_ascii_lowercase());
        }
        assert_eq!(slot_id(&nodes, RADEON), Some(RADEON_UP));
    }

    #[test]
    fn a_card_without_link_data_resolves_to_not_reported() {
        // A laptop card asleep, a virtual machine or an old Windows: the properties are missing.
        let mut nodes = owner_nodes();
        nodes[1].current = None;
        nodes[1].max = None;
        assert_eq!(
            resolve(&nodes, RTX),
            Some(PcieInfo {
                card_max: None,
                slot_max: link(4, 16),
                now: None,
            })
        );
        nodes[0].max = None;
        assert_eq!(resolve(&nodes, RTX), Some(PcieInfo::default()));
    }

    #[test]
    fn ports_that_point_at_each_other_do_not_hang_the_walk() {
        let nodes = vec![
            node(RADEON_UP, RADEON_DOWN, UP, link(4, 16), link(4, 16)),
            node(RADEON_DOWN, RADEON_UP, DOWN, link(4, 16), link(4, 16)),
            node(RADEON, RADEON_DOWN, ENDPOINT, link(4, 16), link(4, 16)),
        ];
        assert_eq!(slot_id(&nodes, RADEON), Some(RADEON_UP));
    }

    #[test]
    fn resizable_bar_is_on_only_with_a_range_over_256_mb() {
        assert_eq!(bar_summary(&[]), (None, None));
        assert_eq!(bar_summary(&[256 * MIB]), (Some(256), Some(false)));
        assert_eq!(
            bar_summary(&[16 * MIB, 256 * MIB, 32 * MIB]),
            (Some(256), Some(false))
        );
        assert_eq!(
            bar_summary(&[16 * MIB, 8192 * MIB, 32 * MIB]),
            (Some(8192), Some(true))
        );
        assert_eq!(bar_summary(&[512 * MIB]), (Some(512), Some(true)));
    }

    #[test]
    fn bar_sizes_round_to_the_nearest_mb() {
        // A range one byte short of 256 MB is still the standard BAR.
        assert_eq!(bar_summary(&[256 * MIB - 1]), (Some(256), Some(false)));
        assert_eq!(
            bar_summary(&[256 * MIB + MIB / 2 - 1]),
            (Some(256), Some(false))
        );
        assert_eq!(bar_summary(&[256 * MIB + MIB / 2]), (Some(257), Some(true)));
        assert_eq!(bar_summary(&[MIB / 2 - 1]), (Some(0), Some(false)));
    }

    #[test]
    fn which_cards_use_resizable_bar() {
        use GpuVendor::{Amd, Intel, Nvidia, Other, Virtual};
        for (vendor, name, uses) in [
            (Nvidia, "NVIDIA GeForce RTX 3060 Ti", true),
            (Nvidia, "NVIDIA GeForce RTX 4090 Laptop GPU", true),
            (Nvidia, "NVIDIA GeForce RTX 5070", true),
            (Nvidia, "RTX 2080", false),
            (Nvidia, "GTX 1660", false),
            (Nvidia, "Quadro RTX 4000", false),
            (Amd, "AMD Radeon RX 6600 XT", true),
            (Amd, "AMD Radeon RX 7900 XTX", true),
            (Amd, "Radeon RX 9070 XT", true),
            (Amd, "RX 5700 XT", false),
            (Amd, "Radeon (TM) RX 480 Graphics", false),
            (Amd, "AMD Radeon(TM) RX Vega 10 Graphics", false),
            (Intel, "Intel(R) Arc(TM) A770 Graphics", true),
            (Intel, "Intel(R) Arc(TM) B580 Graphics", true),
            (Intel, "Intel(R) UHD Graphics 750", false),
            (Intel, "Intel(R) Arc(TM) Graphics", false),
            // The vendor decides which rule applies.
            (Amd, "NVIDIA GeForce RTX 3060 Ti", false),
            (Nvidia, "AMD Radeon RX 6600 XT", false),
            (Intel, "NVIDIA GeForce RTX 3060 Ti", false),
            (Virtual, "Parsec Virtual Display Adapter", false),
            (Other, "NVIDIA GeForce RTX 3060 Ti", false),
        ] {
            assert_eq!(uses_resizable_bar(vendor, name), uses, "{vendor:?} {name}");
        }
    }

    #[test]
    fn generation_labels() {
        assert_eq!(gen_label(1), "1.0");
        assert_eq!(gen_label(4), "4.0");
        assert_eq!(gen_label(6), "6.0");
    }

    fn measured(pnp_device_id: &str, gen: u8, at: &str) -> LinkMeasurement {
        LinkMeasurement {
            pnp_device_id: pnp_device_id.into(),
            link: PcieLink { gen, width: 16 },
            measured_at: at.into(),
            benchmark_id: format!("bench-{at}"),
        }
    }

    #[test]
    fn each_card_gets_its_newest_measurement() {
        let mut s = owner_pc_fixture();
        apply_links(
            &mut s,
            &[
                measured(RTX, 3, "2026-10-12T21:30:05Z"),
                measured(RTX, 4, "2026-10-13T08:15:00Z"),
                measured(RTX, 2, "2026-10-11T19:00:00Z"),
                // Newer, but measured on another card.
                measured(RADEON, 1, "2026-10-14T10:00:00Z"),
            ],
        );
        assert_eq!(
            s.gpus[1].link_under_load,
            Some(LinkUnderLoad {
                link: PcieLink { gen: 4, width: 16 },
                measured_at: "2026-10-13T08:15:00Z".into(),
                benchmark_id: "bench-2026-10-13T08:15:00Z".into(),
            })
        );
        // Parsec's virtual adapter has no measurement of its own.
        assert_eq!(s.gpus[0].link_under_load, None);
    }

    #[test]
    fn a_measurement_on_another_card_is_never_applied() {
        let mut s = owner_pc_fixture();
        apply_links(&mut s, &[measured(RADEON, 4, "2026-10-13T08:15:00Z")]);
        assert!(s.gpus.iter().all(|g| g.link_under_load.is_none()));
        apply_links(&mut s, &[]);
        assert_eq!(s, owner_pc_fixture());
    }

    #[test]
    fn card_ids_match_whatever_their_case() {
        let mut s = owner_pc_fixture();
        let lower = RTX.to_ascii_lowercase();
        apply_links(&mut s, &[measured(&lower, 4, "2026-10-13T08:15:00Z")]);
        assert_eq!(
            s.gpus[1].link_under_load.as_ref().map(|l| l.link.gen),
            Some(4)
        );
    }

    #[test]
    fn an_empty_card_id_never_matches() {
        let mut s = owner_pc_fixture();
        s.gpus[1].pnp_device_id.clear();
        apply_links(&mut s, &[measured("", 4, "2026-10-13T08:15:00Z")]);
        assert!(s.gpus.iter().all(|g| g.link_under_load.is_none()));
    }
}
```

In `crates/kestrel-core/src/lib.rs`, add the module after `pub mod health;`:

```rust
pub mod health;
pub mod pcie;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core pcie`
Expected: FAIL to compile — `cannot find function 'link_from_codes' in this scope`, `cannot find function 'resolve' in this scope`, `cannot find struct, variant or union type 'PciNode' in this scope` and the like.

- [ ] **Step 3: Implement**

Insert this in `crates/kestrel-core/src/pcie.rs` between the module comment and `#[cfg(test)]`. The walk climbs a switch only when both its ports carry the card's own PCI vendor id: an AMD Radeon's or Intel Arc's internal switch is climbed, while an AMD chipset (X570, B550, X670, B650 are PCIe switches) or a Thunderbolt controller is not, so a card in a chipset slot or an eGPU enclosure reads its own x4 link instead of the switch's uplink (the doc comment on `slot_device` below says the same). The loop is bounded by the list's length, so a malformed list can't hang the scan. The Resizable BAR rule looks at the rounded MB it returns, so the size shown and On/Off always agree. `measured_at` compares as text: Kestrel writes it in UTC with whole seconds, the same order the benchmark store uses.

```rust
use crate::gpu;
use crate::snapshot::{GpuVendor, LinkUnderLoad, PcieInfo, PcieLink, SystemSnapshot};
use regex::Regex;
use std::sync::OnceLock;

/// `DEVPKEY_PciDevice_DeviceType` values (pciprop.h) of the bridges on the way from a card to the CPU.
pub const ROOT_PORT: u32 = 8;
pub const UPSTREAM_SWITCH_PORT: u32 = 9;
pub const DOWNSTREAM_SWITCH_PORT: u32 = 10;
/// The BAR every card gets without Resizable BAR; a larger memory range means it is on.
pub const REBAR_THRESHOLD_MB: u64 = 256;

const MIB: u64 = 1024 * 1024;

/// Speed codes 1–6 are PCIe 1.0–6.0; widths 1–32 lanes. Anything else (0 = asleep) → None.
pub fn link_from_codes(speed: Option<u32>, width: Option<u32>) -> Option<PcieLink> {
    let gen = u8::try_from(speed?).ok().filter(|g| (1..=6).contains(g))?;
    let width = u8::try_from(width?).ok().filter(|w| (1..=32).contains(w))?;
    Some(PcieLink { gen, width })
}

/// A PCI device and its link, as Configuration Manager reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PciNode {
    pub instance_id: String,
    pub parent: Option<String>,
    pub device_type: Option<u32>,
    pub current: Option<PcieLink>,
    pub max: Option<PcieLink>,
}

fn find<'a>(nodes: &'a [PciNode], instance_id: &str) -> Option<&'a PciNode> {
    if instance_id.is_empty() {
        return None;
    }
    nodes
        .iter()
        .find(|n| n.instance_id.eq_ignore_ascii_case(instance_id))
}

fn parent<'a>(nodes: &'a [PciNode], child: &PciNode) -> Option<&'a PciNode> {
    find(nodes, child.parent.as_deref()?)
}

/// The PCI vendor in an instance id, upper case: "PCI\\VEN_1002&DEV_1478&…" → "1002".
fn pci_vendor(instance_id: &str) -> Option<String> {
    let upper = instance_id.to_ascii_uppercase();
    let start = upper.find("VEN_")? + 4;
    let id = upper.get(start..start + 4)?;
    id.bytes()
        .all(|b| b.is_ascii_hexdigit())
        .then(|| id.to_string())
}

/// The device whose link is the slot's: the card itself, or — through a card's internal switch — the upstream
/// switch port above it (walk up while the parent is a downstream switch port whose parent is an upstream switch
/// port). Instance ids compare case-insensitively.
///
/// A switch counts as the card's own only when both its ports carry the card's PCI vendor id (AMD Radeon and
/// Intel Arc cards have one). A chipset that is a switch (AMD's X570, B650…) or a Thunderbolt enclosure has
/// another vendor's id, so the walk stops at the card and its own link is read.
pub fn slot_device<'a>(nodes: &'a [PciNode], gpu_instance_id: &str) -> Option<&'a PciNode> {
    let mut device = find(nodes, gpu_instance_id)?;
    let Some(vendor) = pci_vendor(&device.instance_id) else {
        return Some(device);
    };
    let card_port = |node: &PciNode, kind: u32| {
        node.device_type == Some(kind) && pci_vendor(&node.instance_id).as_ref() == Some(&vendor)
    };
    // Bounded, so a malformed list whose ports point at each other can't hang the scan.
    for _ in 0..nodes.len() {
        let Some(down) = parent(nodes, device).filter(|p| card_port(p, DOWNSTREAM_SWITCH_PORT))
        else {
            break;
        };
        let Some(up) = parent(nodes, down).filter(|p| card_port(p, UPSTREAM_SWITCH_PORT)) else {
            break;
        };
        device = up;
    }
    Some(device)
}

/// card_max and now from the slot device; slot_max from its parent when that is a root port. None when the card
/// isn't in `nodes`.
pub fn resolve(nodes: &[PciNode], gpu_instance_id: &str) -> Option<PcieInfo> {
    let device = slot_device(nodes, gpu_instance_id)?;
    let slot_max = parent(nodes, device)
        .filter(|p| p.device_type == Some(ROOT_PORT))
        .and_then(|p| p.max.clone());
    Some(PcieInfo {
        card_max: device.max.clone(),
        slot_max,
        now: device.current.clone(),
    })
}

/// (largest range in MB, Resizable BAR on). Empty → (None, None); a range > 256 MB → Some(true); else Some(false).
/// MB are rounded to the nearest whole one, and the rule looks at that rounded number.
pub fn bar_summary(range_bytes: &[u64]) -> (Option<u64>, Option<bool>) {
    let Some(&largest) = range_bytes.iter().max() else {
        return (None, None);
    };
    let mb = largest / MIB + u64::from(largest % MIB >= MIB / 2);
    (Some(mb), Some(mb > REBAR_THRESHOLD_MB))
}

/// The series digit `pattern` captures from a card's name: "RTX 3060" → 3.
fn series(cell: &OnceLock<Regex>, pattern: &str, name: &str) -> Option<u32> {
    let re = cell.get_or_init(|| Regex::new(pattern).expect("valid pattern"));
    re.captures(name)?.get(1)?.as_str().parse().ok()
}

/// NVIDIA RTX 30 and newer, AMD RX 6000 and newer, discrete Intel Arc.
pub fn uses_resizable_bar(vendor: GpuVendor, name: &str) -> bool {
    static RTX: OnceLock<Regex> = OnceLock::new();
    static RX: OnceLock<Regex> = OnceLock::new();
    match vendor {
        // Turing's Quadro RTX 4000–8000 share the numbers of later GeForce cards.
        GpuVendor::Nvidia => {
            !name.to_ascii_lowercase().contains("quadro")
                && series(&RTX, r"(?i)\brtx\s*(\d)\d{3}", name).is_some_and(|s| s >= 3)
        }
        GpuVendor::Amd => series(&RX, r"(?i)\brx\s*(\d)\d{3}", name).is_some_and(|s| s >= 6),
        // Discrete Arc (A- and B-series); the integrated "Arc Graphics" doesn't count.
        GpuVendor::Intel => !gpu::looks_integrated(GpuVendor::Intel, name, None),
        GpuVendor::Virtual | GpuVendor::Other => false,
    }
}

/// 4 → "4.0".
pub fn gen_label(gen: u8) -> String {
    format!("{gen}.0")
}

/// A link measured during a saved benchmark, on the card with this PNP device id.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkMeasurement {
    pub pnp_device_id: String,
    pub link: PcieLink,
    pub measured_at: String,
    pub benchmark_id: String,
}

/// For each GPU, the newest measurement (largest `measured_at`, RFC 3339) whose `pnp_device_id` matches it
/// case-insensitively becomes its `link_under_load`; GPUs without one keep None.
pub fn apply_links(snapshot: &mut SystemSnapshot, measurements: &[LinkMeasurement]) {
    for gpu in &mut snapshot.gpus {
        if gpu.pnp_device_id.is_empty() {
            continue;
        }
        // Kestrel writes every timestamp in UTC with whole seconds, so text order is time order.
        let newest = measurements
            .iter()
            .filter(|m| m.pnp_device_id.eq_ignore_ascii_case(&gpu.pnp_device_id))
            .max_by(|a, b| a.measured_at.cmp(&b.measured_at));
        if let Some(m) = newest {
            gpu.link_under_load = Some(LinkUnderLoad {
                link: m.link.clone(),
                measured_at: m.measured_at.clone(),
                benchmark_id: m.benchmark_id.clone(),
            });
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core pcie`
Expected: PASS — `test result: ok. 18 passed`.

Run: `cargo test -p kestrel-core && cargo fmt --all --check && cargo clippy -p kestrel-core --all-targets -- -D warnings`
Expected: every kestrel-core test passes, no fmt diff, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core/src/pcie.rs crates/kestrel-core/src/lib.rs
git commit -m "feat(core): PCIe link codes, the card-to-slot walk, the Resizable BAR rule and links under load" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: BIOS placeholders, chipset, display connections and drives

**Files:**
- Create: `crates/kestrel-core/src/hardware.rs`
- Modify: `crates/kestrel-core/src/lib.rs` (`pub mod hardware;`)
- Test: `crates/kestrel-core/src/hardware.rs` (`mod tests`)

**Interfaces:**
- Consumes: `kestrel_core::{DriveBus, DriveInfo, DriveMedia, VolumeInfo, owner_pc_fixture}` (Task 1).
- Produces: `kestrel_core::hardware::{is_placeholder(&str) -> bool, chipset(&str, Option<&str>) -> Option<String>, connection_name(u32) -> Option<&'static str>, DiskRecord { number: u32, model: String, media: DriveMedia, bus: DriveBus, size_gb: f64 }, drives(&[DiskRecord], &[(u32, char)], &[(char, f64, f64)]) -> Vec<DriveInfo>}`, as in "Shared interfaces". Task 11 calls them with what WMI, the LPC/eSPI controller's description, `QueryDisplayConfig` (`outputTechnology as u32`) and `MSFT_PhysicalDisk` / `MSFT_Partition` return; `drives` takes partitions as they come, letterless ones (`'\0'`) included.

- [ ] **Step 1: Write the failing tests**

Create `crates/kestrel-core/src/hardware.rs` with the module comment and the tests; the code goes between them in Step 3. `the_owners_drives_group_with_their_volumes` checks `drives` rebuilds exactly the fixture's `storage.drives` from the owner's disks, partitions and free space.

```rust
//! Hardware tab helpers: BIOS filler names, the chipset, how a monitor is plugged in, and drives with their volumes.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    #[test]
    fn bios_filler_is_a_placeholder() {
        for filler in [
            "To be filled by O.E.M.",
            "To Be Filled By O.E.M.",
            "Default string",
            "System Product Name",
            "System manufacturer",
            "System Version",
            "Not Applicable",
            "Not Specified",
            "Base Board Product Name",
            "Type2 - Board Product Name1",
            "O.E.M.",
            "None",
            "x.x",
            "  default STRING  ",
            "",
            "   ",
        ] {
            assert!(is_placeholder(filler), "{filler:?}");
        }
    }

    #[test]
    fn real_names_are_not_placeholders() {
        for name in [
            "ROG MAXIMUS XIII HERO",
            "ASUSTeK COMPUTER INC.",
            "American Megatrends Inc.",
            "2103",
            "Nonexistent",
            "System Product Name 2",
            "1.x",
        ] {
            assert!(!is_placeholder(name), "{name:?}");
        }
    }

    #[test]
    fn the_owners_chipset_comes_from_the_lpc_controller() {
        assert_eq!(
            chipset(
                "ROG MAXIMUS XIII HERO",
                Some("Intel(R) Z590 LPC/eSPI Controller - 4385")
            )
            .as_deref(),
            Some("Intel Z590")
        );
    }

    #[test]
    fn board_names_carry_the_chipset() {
        for (board, expected) in [
            ("PRIME B550-PLUS", "AMD B550"),
            ("MAG B760 TOMAHAWK WIFI", "Intel B760"),
            ("ROG STRIX X670E-E GAMING WIFI", "AMD X670E"),
            ("B650M PG Riptide", "AMD B650"),
            ("Z790 AORUS ELITE AX", "Intel Z790"),
            ("PRO B760M-A WIFI", "Intel B760"),
            ("PRO H610M-E DDR4", "Intel H610"),
            ("A620M-HDV/M.2+", "AMD A620"),
            ("MPG X570S EDGE MAX WIFI", "AMD X570"),
            ("X870E AORUS MASTER", "AMD X870E"),
            ("MAG B850 TOMAHAWK MAX WIFI", "AMD B850"),
            ("B840M-HDV/M.2", "AMD B840"),
            ("prime b550-plus", "AMD B550"),
            ("W480 VISION D", "Intel W480"),
            ("Q370M-C", "Intel Q370"),
        ] {
            assert_eq!(chipset(board, None).as_deref(), Some(expected), "{board}");
        }
    }

    #[test]
    fn b_series_numbers_tell_intel_from_amd() {
        for (token, maker) in [
            ("B150", "Intel"),
            ("B250", "Intel"),
            ("B350", "AMD"),
            ("B360", "Intel"),
            ("B365", "Intel"),
            ("B450", "AMD"),
            ("B460", "Intel"),
            ("B550", "AMD"),
            ("B560", "Intel"),
            ("B650", "AMD"),
            ("B650E", "AMD"),
            ("B660", "Intel"),
            ("B760", "Intel"),
            ("B840", "AMD"),
            ("B850", "AMD"),
            ("B860", "Intel"),
        ] {
            let board = format!("{token} GAMING");
            assert_eq!(
                chipset(&board, None),
                Some(format!("{maker} {token}")),
                "{board}"
            );
        }
    }

    #[test]
    fn every_chipset_series_is_named() {
        for (token, maker) in [
            ("A320", "AMD"),
            ("X370", "AMD"),
            ("X470", "AMD"),
            ("A520", "AMD"),
            ("X570", "AMD"),
            ("X670", "AMD"),
            ("X870", "AMD"),
            ("H110", "Intel"),
            ("Z170", "Intel"),
            ("Z270", "Intel"),
            ("H310", "Intel"),
            ("Z390", "Intel"),
            ("Z490", "Intel"),
            ("H510", "Intel"),
            ("Z590", "Intel"),
            ("Z690", "Intel"),
            ("H770", "Intel"),
            ("Z890", "Intel"),
        ] {
            assert_eq!(
                chipset(&format!("PRIME {token}-P"), None),
                Some(format!("{maker} {token}")),
                "{token}"
            );
        }
    }

    #[test]
    fn tokens_that_are_not_chipsets_are_skipped() {
        for board in [
            "TRX40 AORUS XTREME",
            "X299 AORUS Gaming 7",
            "X99-A II",
            "B5500 GAMING",
            "Z10PE-D16 WS",
            "ROG CROSSHAIR VIII HERO",
            "A305",
        ] {
            assert_eq!(chipset(board, None), None, "{board}");
        }
    }

    #[test]
    fn the_lpc_description_is_used_only_when_the_board_name_has_no_token() {
        let z590 = Some("Intel(R) Z590 LPC/eSPI Controller - 4385");
        assert_eq!(chipset("PRIME Z790-P", z590).as_deref(), Some("Intel Z790"));
        assert_eq!(
            chipset("To be filled by O.E.M.", z590).as_deref(),
            Some("Intel Z590")
        );
        assert_eq!(
            chipset(
                "",
                Some("Intel(R) 300 Series Chipset Family LPC Controller (Z390) - A305")
            )
            .as_deref(),
            Some("Intel Z390")
        );
        assert_eq!(
            chipset(
                "",
                Some("Intel(R) 100 Series/C230 Series Chipset Family LPC Controller (Z170) - A145")
            )
            .as_deref(),
            Some("Intel Z170")
        );
    }

    #[test]
    fn no_token_anywhere_is_none() {
        assert_eq!(chipset("ROG MAXIMUS XIII HERO", None), None);
        assert_eq!(chipset("", None), None);
        assert_eq!(chipset("To be filled by O.E.M.", None), None);
        assert_eq!(
            chipset("ROG CROSSHAIR VIII HERO", Some("PCI standard ISA bridge")),
            None
        );
        // Only an Intel controller's description names the chipset.
        assert_eq!(chipset("", Some("Z590 LPC Controller")), None);
    }

    #[test]
    fn connection_names_follow_windows_output_technology() {
        for (code, name) in [
            (0, "VGA"),
            (4, "DVI"),
            (5, "HDMI"),
            (10, "DisplayPort"),
            (11, "DisplayPort"),
            (6, "Internal"),
            (0x8000_0000, "Internal"),
            (18, "USB-C"),
            (15, "Wireless"),
        ] {
            assert_eq!(connection_name(code), Some(name), "{code}");
        }
        // S-Video, composite, component, D-JPN, SDI, UDI, SDTV dongle, indirect, OTHER (-1) and unknown codes.
        for code in [1, 2, 3, 7, 8, 9, 12, 13, 14, 16, 17, 19, u32::MAX] {
            assert_eq!(connection_name(code), None, "{code}");
        }
    }

    fn disk(
        number: u32,
        model: &str,
        media: DriveMedia,
        bus: DriveBus,
        size_gb: f64,
    ) -> DiskRecord {
        DiskRecord {
            number,
            model: model.into(),
            media,
            bus,
            size_gb,
        }
    }

    /// The owner's two disks, in the order Windows happened to list them.
    fn owner_disks() -> Vec<DiskRecord> {
        vec![
            disk(
                1,
                "WDC WD20EZBX-00AYRA0",
                DriveMedia::Hdd,
                DriveBus::Sata,
                1863.0,
            ),
            disk(
                0,
                "Samsung SSD 980 PRO 1TB",
                DriveMedia::Nvme,
                DriveBus::Nvme,
                931.5,
            ),
        ]
    }

    #[test]
    fn the_owners_drives_group_with_their_volumes() {
        // The EFI and recovery partitions have no letter.
        let partitions = [(0, '\0'), (0, 'C'), (0, '\0'), (1, 'D')];
        let space = [('D', 1228.8, 1863.0), ('C', 210.5, 931.5)];
        assert_eq!(
            drives(&owner_disks(), &partitions, &space),
            owner_pc_fixture().storage.drives
        );
    }

    #[test]
    fn volumes_list_in_letter_order_with_their_space() {
        let partitions = [(0, 'C'), (1, 'G'), (1, 'e'), (1, 'g')];
        let space = [
            ('C', 210.5, 931.5),
            ('E', 400.0, 931.5),
            ('G', 828.8, 931.5),
        ];
        let found = drives(&owner_disks(), &partitions, &space);
        let letters: Vec<&str> = found[1].volumes.iter().map(|v| v.letter.as_str()).collect();
        assert_eq!(letters, ["E", "G"]);
        assert_eq!(
            found[1].volumes[1],
            VolumeInfo {
                letter: "G".into(),
                free_gb: 828.8,
                total_gb: 931.5
            }
        );
    }

    #[test]
    fn a_volume_without_space_shows_zero() {
        let found = drives(&owner_disks(), &[(0, 'C')], &[]);
        assert_eq!(
            found[0].volumes,
            [VolumeInfo {
                letter: "C".into(),
                free_gb: 0.0,
                total_gb: 0.0
            }]
        );
    }

    #[test]
    fn a_disk_without_a_lettered_volume_is_still_listed() {
        let mut disks = owner_disks();
        disks.push(disk(
            2,
            "Msft Virtual Disk",
            DriveMedia::Unknown,
            DriveBus::Virtual,
            127.0,
        ));
        let found = drives(&disks, &[(0, 'C'), (1, '\0')], &[('C', 210.5, 931.5)]);
        let models: Vec<&str> = found.iter().map(|d| d.model.as_str()).collect();
        assert_eq!(
            models,
            [
                "Samsung SSD 980 PRO 1TB",
                "WDC WD20EZBX-00AYRA0",
                "Msft Virtual Disk"
            ]
        );
        assert!(found[1].volumes.is_empty());
        assert!(found[2].volumes.is_empty());
        assert_eq!(found[2].bus, DriveBus::Virtual);
    }

    #[test]
    fn partitions_on_unknown_disks_are_ignored() {
        let partitions = [(0, 'C'), (5, 'E')];
        let space = [('C', 210.5, 931.5), ('E', 10.0, 20.0)];
        let found = drives(&owner_disks(), &partitions, &space);
        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .flat_map(|d| &d.volumes)
            .all(|v| v.letter != "E"));
        assert!(drives(&[], &partitions, &space).is_empty());
    }
}
```

In `crates/kestrel-core/src/lib.rs`, add the module after `pub mod gpu;`:

```rust
pub mod gpu;
pub mod hardware;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core hardware`
Expected: FAIL to compile — `cannot find function 'chipset' in this scope`, `cannot find function 'drives' in this scope`, `cannot find struct, variant or union type 'DiskRecord' in this scope` and the like.

- [ ] **Step 3: Implement**

Insert this in `crates/kestrel-core/src/hardware.rs` between the module comment and `#[cfg(test)]`. A chipset token is one letter and three digits at the start of a word (`B760M` and `X570S` count as B760 and X570; an `E` right after an AMD number is kept, as in X670E). The maker comes from the letter and number: A and X with a 300–800 number ending in 0 are AMD; B is AMD only for x40 and x50 from 300 to 800 (B350 … B850), so B150, B250, B360, B365, B460, B560, B660, B760 and B860 are Intel; H, Q, W and Z with 100–800 are Intel. Anything else (X299, TRX40, X99, C230, device ids such as A305) is skipped. The LPC fallback is read only when the description mentions Intel. The output-technology values were checked against windows-sys 0.61.2's `DISPLAYCONFIG_OUTPUT_TECHNOLOGY_*` constants.

```rust
use crate::snapshot::{DriveBus, DriveInfo, DriveMedia, VolumeInfo};
use regex::Regex;
use std::sync::OnceLock;

/// Text board makers leave in SMBIOS when they don't fill a field in.
const PLACEHOLDERS: &[&str] = &[
    "To be filled by O.E.M.",
    "Default string",
    "System Product Name",
    "System manufacturer",
    "System Version",
    "Not Applicable",
    "Not Specified",
    "Base Board Product Name",
    "Type2 - Board Product Name1",
    "O.E.M.",
    "None",
    "x.x",
];

/// True for empty text and BIOS filler: "To be filled by O.E.M.", "Default string", "System Product Name",
/// "System manufacturer", "System Version", "Not Applicable", "Not Specified", "Base Board Product Name",
/// "Type2 - Board Product Name1", "O.E.M.", "None", "x.x" (case-insensitive, trimmed).
pub fn is_placeholder(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || PLACEHOLDERS.iter().any(|p| p.eq_ignore_ascii_case(text))
}

/// Who makes a chipset, from its letter and number. AMD's are A, B and X with a 300–800 number ending in 0 (its
/// B-series ends in 40 or 50: B350 … B850). Intel's are B, H, Q, W and Z with a 100–800 number, so B360, B460,
/// B560, B660, B760 and B860 are Intel's.
fn maker(letter: char, number: u32) -> Option<&'static str> {
    let series = number / 100;
    match letter {
        'A' | 'X' if (3..=8).contains(&series) && number.is_multiple_of(10) => Some("AMD"),
        'B' if (3..=8).contains(&series) && matches!(number % 100, 40 | 50) => Some("AMD"),
        'B' | 'H' | 'Q' | 'W' | 'Z' if (1..=8).contains(&series) => Some("Intel"),
        _ => None,
    }
}

/// Chipset tokens in `text`, in order, as (maker, token): "PRO B760M-A WIFI" → ("Intel", "B760"). AMD tokens keep
/// an "E" written right after the number (X670E); other letters after it (B760M, X570S) aren't part of the chipset.
fn tokens(text: &str) -> impl Iterator<Item = (&'static str, String)> + '_ {
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let re = TOKEN.get_or_init(|| Regex::new(r"(?i)\b([a-z])(\d+)(e?)").expect("valid pattern"));
    re.captures_iter(text).filter_map(|caps| {
        let digits = caps.get(2)?.as_str();
        if digits.len() != 3 {
            return None;
        }
        let letter = caps.get(1)?.as_str().chars().next()?.to_ascii_uppercase();
        let maker = maker(letter, digits.parse().ok()?)?;
        let e = if maker == "AMD" && !caps.get(3)?.as_str().is_empty() {
            "E"
        } else {
            ""
        };
        Some((maker, format!("{letter}{digits}{e}")))
    })
}

/// A chipset token (e.g. "Z590", "B550", "X670E", "A620", "H610") from the board name, else from the Intel
/// LPC/eSPI controller's description ("Intel(R) Z590 LPC/eSPI Controller - 4385" → "Intel Z590"). Board-name
/// tokens get the maker prefix from the token (Intel for Z/H/B/Q/W-series 1xx–8xx, AMD for A/B/X 3xx–8xx).
pub fn chipset(board_product: &str, lpc_description: Option<&str>) -> Option<String> {
    if let Some((maker, token)) = tokens(board_product).next() {
        return Some(format!("{maker} {token}"));
    }
    let description = lpc_description?;
    if !description.to_ascii_lowercase().contains("intel") {
        return None;
    }
    tokens(description)
        .find(|(maker, _)| *maker == "Intel")
        .map(|(_, token)| format!("Intel {token}"))
}

/// DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY → "VGA", "DVI", "HDMI", "DisplayPort", "Internal", "USB-C", "Wireless".
/// windows-sys declares the type as i32; INTERNAL is 0x8000_0000 once read as u32.
pub fn connection_name(output_technology: u32) -> Option<&'static str> {
    match output_technology {
        0 => Some("VGA"),                    // HD15
        4 => Some("DVI"),                    // DVI
        5 => Some("HDMI"),                   // HDMI
        10 | 11 => Some("DisplayPort"),      // DISPLAYPORT_EXTERNAL, DISPLAYPORT_EMBEDDED
        6 | 0x8000_0000 => Some("Internal"), // LVDS, INTERNAL
        18 => Some("USB-C"),                 // DISPLAYPORT_USB_TUNNEL
        15 => Some("Wireless"),              // MIRACAST
        _ => None,
    }
}

/// One `MSFT_PhysicalDisk`, with its size already in GiB.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskRecord {
    pub number: u32,
    pub model: String,
    pub media: DriveMedia,
    pub bus: DriveBus,
    pub size_gb: f64,
}

/// Drives by disk number; each with its lettered volumes (letter order) and their space; disks without a
/// lettered volume are still listed.
///
/// `partitions` are (disk number, drive letter); a partition without a letter (Windows gives "\0") or on a disk
/// that isn't listed is left out. `space` is (letter, free GiB, total GiB); a volume without one shows 0.
pub fn drives(
    disks: &[DiskRecord],
    partitions: &[(u32, char)],
    space: &[(char, f64, f64)],
) -> Vec<DriveInfo> {
    let mut sorted: Vec<&DiskRecord> = disks.iter().collect();
    sorted.sort_by_key(|d| d.number);
    sorted
        .into_iter()
        .map(|disk| {
            let mut letters: Vec<char> = partitions
                .iter()
                .filter(|(number, letter)| *number == disk.number && letter.is_ascii_alphabetic())
                .map(|(_, letter)| letter.to_ascii_uppercase())
                .collect();
            letters.sort_unstable();
            letters.dedup();
            let volumes = letters
                .into_iter()
                .map(|letter| {
                    let (free_gb, total_gb) = space
                        .iter()
                        .find(|(l, _, _)| l.eq_ignore_ascii_case(&letter))
                        .map_or((0.0, 0.0), |&(_, free, total)| (free, total));
                    VolumeInfo {
                        letter: letter.to_string(),
                        free_gb,
                        total_gb,
                    }
                })
                .collect();
            DriveInfo {
                model: disk.model.clone(),
                media: disk.media,
                bus: disk.bus,
                size_gb: disk.size_gb,
                volumes,
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core hardware`
Expected: PASS — `test result: ok. 15 passed`.

Run: `cargo test -p kestrel-core && cargo fmt --all --check && cargo clippy -p kestrel-core --all-targets -- -D warnings`
Expected: every kestrel-core test passes, no fmt diff, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core/src/hardware.rs crates/kestrel-core/src/lib.rs
git commit -m "feat(core): BIOS placeholders, chipset names, display connections and drive grouping" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Four BIOS-related Health Check items (31 checks)

**Files:**
- Modify: `crates/kestrel-core/src/health.rs` — `use crate::pcie;`, `BiosTopic::{PcieSlot, Virtualization}`, the checks `pcie-lanes`, `pcie-speed`, `resizable-bar` and `virtualization`, their places in `run_health_check`
- Test: `crates/kestrel-core/src/health.rs` (`mod tests`: 16 new tests; `owner_pc_full_results` updated)
- Test: `crates/kestrel-core/src/report.rs` (`owner_report_has_everything`: 31 findings)
- Test: `crates/kestrel-probe/tests/smoke.rs` (`report_has_every_check`: 31 findings)

**Interfaces:**
- Consumes:
  - Task 1: `GpuInfo { pcie: Option<PcieInfo>, link_under_load: Option<LinkUnderLoad>, resizable_bar: Option<bool>, .. }`; `PcieInfo { card_max: Option<PcieLink>, slot_max: Option<PcieLink>, now: Option<PcieLink> }`; `PcieLink { gen: u8, width: u8 }`; `LinkUnderLoad { link: PcieLink, measured_at: String, benchmark_id: String }`; `CpuInfo { virtualization: Option<bool>, .. }`. The schema-2 owner fixture: its RTX 3060 Ti (the only `GpuVendor::Nvidia` entry) has `pcie.cardMax` and `pcie.slotMax` 4.0 x16, `pcie.now` x16, `resizableBar: true`, `linkUnderLoad: null`; `cpu.virtualization` is `true`. The test helpers `gpu(name, vendor, integrated, date) -> GpuInfo` and `display(…)` in `health.rs` keep their signatures (Task 1 defaults the new fields).
  - Task 4: `kestrel_core::pcie::gen_label(gen: u8) -> String` (4 → `"4.0"`); `kestrel_core::pcie::uses_resizable_bar(vendor: GpuVendor, name: &str) -> bool`.
  - Existing: `gpu::main_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo>`; `score::score(findings: &[Finding]) -> Score`.
- Produces:
  - `health::BiosTopic::PcieSlot` (serialized `"pcie-slot"`) and `health::BiosTopic::Virtualization` (`"virtualization"`), for Task 7's guide sections and Task 15's TypeScript `BiosTopic`.
  - `run_health_check(s: &SystemSnapshot, today: NaiveDate) -> Vec<Finding>` returns 31 findings in the contract order; `pcie-lanes`, `pcie-speed` and `resizable-bar` (`Category::Hardware`) and `virtualization` (`Category::Vanguard`) read exactly as in the contract table. The owner fixture still scores 86 with 3 problems.

- [ ] **Step 1: Write the failing tests**

In `crates/kestrel-core/src/health.rs`, `mod tests`, test `owner_pc_full_results`: the Health Check grows to 31 checks, and the owner's PC gets one more Info (no benchmark yet, so `pcie-speed` asks for one). Replace

```rust
        assert_eq!(f.len(), 27);
```

with

```rust
        assert_eq!(f.len(), 31);
```

and

```rust
            vec!["memory-integrity", "iommu", "on-demand"]
```

with

```rust
            vec!["pcie-speed", "memory-integrity", "iommu", "on-demand"]
```

In `crates/kestrel-core/src/report.rs`, test `owner_report_has_everything`, replace `assert_eq!(r.findings.len(), 27);` with `assert_eq!(r.findings.len(), 31);`.

In `crates/kestrel-probe/tests/smoke.rs`, test `report_has_every_check`, replace `assert_eq!(report.findings.len(), 27);` with `assert_eq!(report.findings.len(), 31);`.

Then add these tests at the end of `mod tests` in `crates/kestrel-core/src/health.rs`, just above the module's closing `}`. Each takes the owner's PC, changes one thing, and compares the whole finding (id, category, status, severity, title, detail and fix) with the contract table:

```rust
    // The four BIOS-related checks: pcie-lanes, pcie-speed, resizable-bar and virtualization.

    /// A finding as the contract table spells it out.
    fn expected(
        id: &str,
        category: Category,
        status: Status,
        severity: Severity,
        title: &str,
        detail: &str,
        fix: Fix,
    ) -> Finding {
        Finding {
            id: id.into(),
            category,
            severity,
            status,
            title: title.into(),
            detail: detail.into(),
            fix,
        }
    }

    fn finding(s: &SystemSnapshot, id: &str) -> Finding {
        get(&run_health_check(s, today()), id).clone()
    }

    /// The owner's PC with its graphics card (the RTX 3060 Ti: 4.0 x16 card in a 4.0 x16 slot, running x16,
    /// Resizable BAR on, no benchmark yet) changed by `change`.
    fn with_card(change: impl FnOnce(&mut GpuInfo)) -> SystemSnapshot {
        let mut s = owner_pc_fixture();
        let card = s
            .gpus
            .iter_mut()
            .find(|g| g.vendor == GpuVendor::Nvidia)
            .expect("the owner's RTX 3060 Ti");
        change(card);
        s
    }

    fn link(gen: u8, width: u8) -> Option<PcieLink> {
        Some(PcieLink { gen, width })
    }

    /// A link measured during a benchmark on 13 October 2026.
    fn under_load(gen: u8, width: u8) -> Option<LinkUnderLoad> {
        Some(LinkUnderLoad {
            link: PcieLink { gen, width },
            measured_at: "2026-10-13T18:42:07+01:00".into(),
            benchmark_id: "2026-10-13-184207".into(),
        })
    }

    const LANES_DETAIL: &str = "It loses some FPS, more at x4. Usual causes: the card sits in a lower slot, an M.2 drive or second card shares its lanes, or it isn't pushed fully in. In an external (Thunderbolt) enclosure x4 is normal.";
    const SPEED_PROBLEM_DETAIL: &str = "Measured during your benchmark on 2026-10-13. Set the slot's PCIe speed to Auto in the BIOS, and check the card sits in the top slot.";
    const RUN_A_BENCHMARK: &str =
        "Graphics cards slow their link when idle, so Kestrel checks it during a benchmark.";

    #[test]
    fn the_31_checks_run_in_a_fixed_order() {
        let ids: Vec<String> = run_health_check(&owner_pc_fixture(), today())
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(
            ids,
            [
                "ram-speed",
                "ram-channels",
                "bios-age",
                "pcie-lanes",
                "pcie-speed",
                "resizable-bar",
                "display-refresh",
                "virtual-display",
                "gpu-driver-age",
                "valorant-gpu",
                "on-battery",
                "power-plan",
                "game-mode",
                "background-recording",
                "pending-restart",
                "windows-build",
                "valorant-drive",
                "drive-space",
                "background-cpu",
                "startup-apps",
                "tpm",
                "secure-boot",
                "memory-integrity",
                "virtualization",
                "iommu",
                "vanguard",
                "on-demand",
                "wired",
                "link-speed",
                "adapter-power",
                "delivery-optimization",
            ]
        );
    }

    #[test]
    fn the_owners_pc_passes_the_new_checks_asks_for_a_benchmark_and_still_scores_86() {
        let s = owner_pc_fixture();
        assert_eq!(
            finding(&s, "pcie-lanes"),
            expected(
                "pcie-lanes",
                Category::Hardware,
                Status::Pass,
                Severity::Medium,
                "Graphics card uses all its PCIe lanes",
                "x16, as the card supports.",
                Fix::Nothing
            )
        );
        assert_eq!(
            finding(&s, "pcie-speed"),
            expected(
                "pcie-speed",
                Category::Hardware,
                Status::Info,
                Severity::Info,
                "Run a benchmark to check the PCIe link under load",
                RUN_A_BENCHMARK,
                Fix::Nothing
            )
        );
        assert_eq!(
            finding(&s, "resizable-bar"),
            expected(
                "resizable-bar",
                Category::Hardware,
                Status::Pass,
                Severity::Low,
                "Resizable BAR is on",
                "",
                Fix::Nothing
            )
        );
        assert_eq!(
            finding(&s, "virtualization"),
            expected(
                "virtualization",
                Category::Vanguard,
                Status::Pass,
                Severity::Info,
                "Virtualization is on",
                "",
                Fix::Nothing
            )
        );
        let score = crate::score::score(&run_health_check(&s, today()));
        assert_eq!((score.value, score.problems), (86, 3));
    }

    #[test]
    fn without_a_separate_card_the_graphics_checks_have_nothing_to_check() {
        // The integrated GPU's own values would be problems if they counted.
        let integrated = GpuInfo {
            pcie: Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: link(4, 16),
                now: link(1, 1),
            }),
            link_under_load: under_load(1, 1),
            resizable_bar: Some(false),
            ..gpu(
                "Intel(R) UHD Graphics 750",
                GpuVendor::Intel,
                true,
                Some("2026-09-01"),
            )
        };
        let parsec = gpu(
            "Parsec Virtual Display Adapter",
            GpuVendor::Virtual,
            false,
            None,
        );
        for (case, gpus) in [
            ("no GPUs", vec![]),
            ("integrated only", vec![integrated]),
            ("virtual only", vec![parsec]),
        ] {
            let mut s = owner_pc_fixture();
            s.gpus = gpus;
            for id in ["pcie-lanes", "pcie-speed", "resizable-bar"] {
                assert_eq!(
                    finding(&s, id),
                    expected(
                        id,
                        Category::Hardware,
                        Status::Info,
                        Severity::Info,
                        "No separate graphics card to check",
                        "",
                        Fix::Nothing
                    ),
                    "{case}"
                );
            }
        }
    }

    #[test]
    fn the_graphics_checks_look_at_the_main_card_only() {
        // Integrated + Parsec + the RTX 3060 Ti at x8: only the RTX counts.
        let mut s = with_card(|card| card.pcie.as_mut().unwrap().now = link(4, 8));
        s.gpus.insert(
            0,
            GpuInfo {
                pcie: Some(PcieInfo {
                    card_max: link(4, 16),
                    slot_max: link(4, 16),
                    now: link(4, 16),
                }),
                resizable_bar: Some(false),
                ..gpu(
                    "Intel(R) UHD Graphics 750",
                    GpuVendor::Intel,
                    true,
                    Some("2026-09-01"),
                )
            },
        );
        let f = run_health_check(&s, today());
        assert_eq!(
            get(&f, "pcie-lanes").title,
            "Graphics card runs at x8 instead of x16"
        );
        assert_eq!(get(&f, "resizable-bar").status, Status::Pass);
    }

    #[test]
    fn an_unreadable_link_is_unknown_never_x0() {
        let unknown = expected(
            "pcie-lanes",
            Category::Hardware,
            Status::Unknown,
            Severity::Medium,
            "Couldn't read the graphics card's PCIe link",
            "Windows didn't report it for this card.",
            Fix::Nothing,
        );
        for (case, s) in [
            ("no PCIe info", with_card(|card| card.pcie = None)),
            (
                "no link now",
                with_card(|card| card.pcie.as_mut().unwrap().now = None),
            ),
            (
                "no card maximum",
                with_card(|card| card.pcie.as_mut().unwrap().card_max = None),
            ),
        ] {
            assert_eq!(finding(&s, "pcie-lanes"), unknown, "{case}");
        }
    }

    #[test]
    fn a_card_using_all_its_lanes_passes_without_the_slot_maximum() {
        // An x8 card (RX 6600 class) running x8 uses all its lanes, whatever the slot reports.
        let s = with_card(|card| {
            let pcie = card.pcie.as_mut().unwrap();
            pcie.card_max = link(4, 8);
            pcie.now = link(4, 8);
            pcie.slot_max = None;
        });
        assert_eq!(
            finding(&s, "pcie-lanes"),
            expected(
                "pcie-lanes",
                Category::Hardware,
                Status::Pass,
                Severity::Medium,
                "Graphics card uses all its PCIe lanes",
                "x8, as the card supports.",
                Fix::Nothing
            )
        );
    }

    #[test]
    fn fewer_lanes_than_the_card_supports_is_a_problem_high_at_x4_or_fewer() {
        for (width, severity) in [
            (8, Severity::Medium),
            (4, Severity::High),
            (2, Severity::High),
            (1, Severity::High),
        ] {
            let s = with_card(|card| card.pcie.as_mut().unwrap().now = link(4, width));
            assert_eq!(
                finding(&s, "pcie-lanes"),
                expected(
                    "pcie-lanes",
                    Category::Hardware,
                    Status::Problem,
                    severity,
                    &format!("Graphics card runs at x{width} instead of x16"),
                    LANES_DETAIL,
                    Fix::BiosGuide {
                        topic: BiosTopic::PcieSlot
                    }
                ),
                "x{width}"
            );
        }
    }

    #[test]
    fn pcie_speed_asks_for_a_benchmark_until_one_measured_the_link() {
        let info = expected(
            "pcie-speed",
            Category::Hardware,
            Status::Info,
            Severity::Info,
            "Run a benchmark to check the PCIe link under load",
            RUN_A_BENCHMARK,
            Fix::Nothing,
        );
        assert_eq!(finding(&owner_pc_fixture(), "pcie-speed"), info);
        // No benchmark comes before an unreadable slot.
        assert_eq!(
            finding(&with_card(|card| card.pcie = None), "pcie-speed"),
            info
        );
    }

    #[test]
    fn pcie_speed_is_unknown_without_the_card_and_slot_maximum() {
        let unknown = expected(
            "pcie-speed",
            Category::Hardware,
            Status::Unknown,
            Severity::Low,
            "Couldn't read the slot's PCIe speed",
            "Windows didn't report what the slot supports.",
            Fix::Nothing,
        );
        let measured = |change: fn(&mut GpuInfo)| {
            with_card(|card| {
                card.link_under_load = under_load(3, 16);
                change(card);
            })
        };
        for (case, s) in [
            ("no PCIe info", measured(|card| card.pcie = None)),
            (
                "no card maximum",
                measured(|card| card.pcie.as_mut().unwrap().card_max = None),
            ),
            (
                "no slot maximum",
                measured(|card| card.pcie.as_mut().unwrap().slot_max = None),
            ),
        ] {
            assert_eq!(finding(&s, "pcie-speed"), unknown, "{case}");
        }
    }

    /// pcie-speed with the card and slot maxima at `card_gen` and `slot_gen` (x16) and `got` measured under load.
    fn speed(got: u8, card_gen: u8, slot_gen: u8) -> Finding {
        let s = with_card(|card| {
            let pcie = card.pcie.as_mut().unwrap();
            pcie.card_max = link(card_gen, 16);
            pcie.slot_max = link(slot_gen, 16);
            card.link_under_load = under_load(got, 16);
        });
        finding(&s, "pcie-speed")
    }

    #[test]
    fn a_link_at_the_lower_of_card_and_slot_passes() {
        assert_eq!(
            speed(4, 4, 4),
            expected(
                "pcie-speed",
                Category::Hardware,
                Status::Pass,
                Severity::Low,
                "PCIe link runs at full speed under load",
                "PCIe 4.0 during your benchmark on 2026-10-13.",
                Fix::Nothing
            )
        );
        // A 4.0 card on a 3.0 board, and a 3.0 card on a 4.0 board, are at full speed at 3.0.
        for (card_gen, slot_gen) in [(4, 3), (3, 4)] {
            let f = speed(3, card_gen, slot_gen);
            assert_eq!(
                (f.status, f.detail.as_str()),
                (
                    Status::Pass,
                    "PCIe 3.0 during your benchmark on 2026-10-13."
                ),
                "card {card_gen}, slot {slot_gen}"
            );
        }
    }

    #[test]
    fn a_slower_link_under_load_is_a_problem_high_at_2_0_or_below() {
        for (got, severity) in [(3, Severity::Low), (2, Severity::High), (1, Severity::High)] {
            assert_eq!(
                speed(got, 4, 4),
                expected(
                    "pcie-speed",
                    Category::Hardware,
                    Status::Problem,
                    severity,
                    &format!("PCIe link runs at {got}.0 under load instead of 4.0"),
                    SPEED_PROBLEM_DETAIL,
                    Fix::BiosGuide {
                        topic: BiosTopic::PcieSlot
                    }
                ),
                "PCIe {got}.0"
            );
        }
        let five = speed(4, 5, 5);
        assert_eq!(
            (five.status, five.severity, five.title.as_str()),
            (
                Status::Problem,
                Severity::Low,
                "PCIe link runs at 4.0 under load instead of 5.0"
            )
        );
    }

    #[test]
    fn unreported_resizable_bar_is_unknown() {
        assert_eq!(
            finding(
                &with_card(|card| card.resizable_bar = None),
                "resizable-bar"
            ),
            expected(
                "resizable-bar",
                Category::Hardware,
                Status::Unknown,
                Severity::Low,
                "Windows doesn't report Resizable BAR for this card",
                "",
                Fix::Nothing
            )
        );
    }

    /// resizable-bar with the owner's card replaced by `name` from `vendor`, Resizable BAR off.
    fn bar_off(vendor: GpuVendor, name: &str) -> Finding {
        let s = with_card(|card| {
            card.vendor = vendor;
            card.name = name.into();
            card.resizable_bar = Some(false);
        });
        finding(&s, "resizable-bar")
    }

    #[test]
    fn resizable_bar_off_weighs_by_who_made_the_card() {
        let guide = || Fix::BiosGuide {
            topic: BiosTopic::ResizableBar,
        };
        assert_eq!(
            bar_off(GpuVendor::Intel, "Intel(R) Arc(TM) A770 Graphics"),
            expected(
                "resizable-bar",
                Category::Hardware,
                Status::Problem,
                Severity::High,
                "Resizable BAR is off",
                "Intel Arc cards lose a lot of FPS without it. Turn on Above 4G Decoding and Re-Size BAR in the BIOS; Windows must start in UEFI mode (CSM off).",
                guide()
            )
        );
        assert_eq!(
            bar_off(GpuVendor::Amd, "AMD Radeon RX 6800 XT"),
            expected(
                "resizable-bar",
                Category::Hardware,
                Status::Problem,
                Severity::Low,
                "Resizable BAR is off",
                "AMD calls it Smart Access Memory. It adds a few FPS in some games. Windows must start in UEFI mode (CSM off).",
                guide()
            )
        );
        assert_eq!(
            bar_off(GpuVendor::Nvidia, "NVIDIA GeForce RTX 3060 Ti"),
            expected(
                "resizable-bar",
                Category::Hardware,
                Status::Info,
                Severity::Info,
                "Resizable BAR is off",
                "NVIDIA only uses it in games on its own list, so most games, VALORANT included, gain little. Turning it on is optional.",
                guide()
            )
        );
    }

    #[test]
    fn resizable_bar_off_on_a_card_that_doesnt_use_it_is_info_without_a_fix() {
        for (vendor, name) in [
            (GpuVendor::Nvidia, "NVIDIA GeForce GTX 1660 SUPER"),
            (GpuVendor::Amd, "AMD Radeon RX 580"),
        ] {
            assert_eq!(
                bar_off(vendor, name),
                expected(
                    "resizable-bar",
                    Category::Hardware,
                    Status::Info,
                    Severity::Info,
                    "Resizable BAR is off",
                    "Only newer cards use it: NVIDIA RTX 30 series and newer, AMD RX 6000 and newer, and Intel Arc.",
                    Fix::Nothing
                ),
                "{name}"
            );
        }
    }

    #[test]
    fn virtualization_is_never_a_problem() {
        let with = |on: Option<bool>| {
            let mut s = owner_pc_fixture();
            s.cpu.virtualization = on;
            finding(&s, "virtualization")
        };
        assert_eq!(
            with(Some(true)),
            expected(
                "virtualization",
                Category::Vanguard,
                Status::Pass,
                Severity::Info,
                "Virtualization is on",
                "",
                Fix::Nothing
            )
        );
        assert_eq!(
            with(Some(false)),
            expected(
                "virtualization",
                Category::Vanguard,
                Status::Info,
                Severity::Info,
                "Virtualization is off",
                "VALORANT works fine like this. Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It's called Intel Virtualization Technology (VT-x) or SVM on AMD.",
                Fix::BiosGuide {
                    topic: BiosTopic::Virtualization
                }
            )
        );
        assert_eq!(
            with(None),
            expected(
                "virtualization",
                Category::Vanguard,
                Status::Unknown,
                Severity::Info,
                "Couldn't tell whether virtualization is on",
                "",
                Fix::Nothing
            )
        );
    }

    #[test]
    fn the_new_bios_topics_serialize_in_kebab_case() {
        assert_eq!(
            serde_json::to_string(&BiosTopic::PcieSlot).unwrap(),
            r#""pcie-slot""#
        );
        assert_eq!(
            serde_json::to_string(&BiosTopic::Virtualization).unwrap(),
            r#""virtualization""#
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core`

Expected: FAIL to compile with error E0599, "no variant, associated function, or constant named `PcieSlot` found for enum `health::BiosTopic`" (3 times), and the same error for `Virtualization` (2 times). After edit 2 below adds the variants, 17 tests fail on their assertions until the checks exist: the 15 new check tests, `owner_pc_full_results` and `report::tests::owner_report_has_everything`.

- [ ] **Step 3: Implement**

Make all of these edits in `crates/kestrel-core/src/health.rs`.

1. Imports. Replace

```rust
use crate::gpu;
use crate::ram::{self, ChannelLayout};
```

with

```rust
use crate::gpu;
use crate::pcie;
use crate::ram::{self, ChannelLayout};
```

2. `BiosTopic`. Replace

```rust
    ResizableBar,
    UpdateBios,
}
```

with

```rust
    ResizableBar,
    UpdateBios,
    PcieSlot,
    Virtualization,
}
```

The enum's `#[serde(rename_all = "kebab-case")]` serializes them as `"pcie-slot"` and `"virtualization"`.

3. `run_health_check`, in the contract order. Replace

```rust
        bios_age(s),
        display_refresh(s),
```

with

```rust
        bios_age(s),
        pcie_lanes(s),
        pcie_speed(s),
        resizable_bar(s),
        display_refresh(s),
```

and

```rust
        memory_integrity(s),
        iommu(s),
```

with

```rust
        memory_integrity(s),
        virtualization(s),
        iommu(s),
```

4. The three graphics checks. Insert this block directly above `fn display_label(d: &DisplayInfo) -> String {` (that is, after `bios_age`). A row whose severity differs from the check's own builds its `Check` with that severity (`Check { severity, ..c }`), as `windows_build` already does. "No separate graphics card" means `gpu::main_gpu` finds none, or the one it finds is integrated or virtual:

```rust
/// The main graphics card when it is a separate one: None when there is no GPU, or the main one is built into the
/// processor or virtual.
fn separate_card(s: &SystemSnapshot) -> Option<&GpuInfo> {
    gpu::main_gpu(&s.gpus).filter(|g| !g.is_integrated && g.vendor != GpuVendor::Virtual)
}

/// What a graphics check says without a separate card: Info, so it never counts.
fn no_separate_card(c: Check) -> Finding {
    Check {
        severity: Severity::Info,
        ..c
    }
    .info("No separate graphics card to check", "")
}

/// The `YYYY-MM-DD` part of an RFC 3339 time.
fn day(measured_at: &str) -> String {
    measured_at.chars().take(10).collect()
}

fn pcie_lanes(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "pcie-lanes",
        category: Category::Hardware,
        severity: Severity::Medium,
    };
    let Some(card) = separate_card(s) else {
        return no_separate_card(c);
    };
    let pcie_info = card.pcie.as_ref();
    match (
        pcie_info.and_then(|p| p.now.as_ref()),
        pcie_info.and_then(|p| p.card_max.as_ref()),
    ) {
        (Some(now), Some(max)) if now.width >= max.width => c.pass(
            "Graphics card uses all its PCIe lanes",
            format!("x{}, as the card supports.", now.width),
        ),
        (Some(now), Some(max)) => {
            let severity = if now.width <= 4 {
                Severity::High
            } else {
                Severity::Medium
            };
            Check { severity, ..c }.problem(
                format!("Graphics card runs at x{} instead of x{}", now.width, max.width),
                "It loses some FPS, more at x4. Usual causes: the card sits in a lower slot, an M.2 drive or second card shares its lanes, or it isn't pushed fully in. In an external (Thunderbolt) enclosure x4 is normal.",
                Fix::BiosGuide { topic: BiosTopic::PcieSlot },
            )
        }
        _ => c.unknown(
            "Couldn't read the graphics card's PCIe link",
            "Windows didn't report it for this card.",
        ),
    }
}

/// Cards slow their link when idle, so the generation is judged from the link measured during a benchmark on this
/// card, against the lower of what the card and the slot support.
fn pcie_speed(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "pcie-speed",
        category: Category::Hardware,
        severity: Severity::Low,
    };
    let Some(card) = separate_card(s) else {
        return no_separate_card(c);
    };
    let Some(load) = &card.link_under_load else {
        return Check {
            severity: Severity::Info,
            ..c
        }
        .info(
            "Run a benchmark to check the PCIe link under load",
            "Graphics cards slow their link when idle, so Kestrel checks it during a benchmark.",
        );
    };
    let pcie_info = card.pcie.as_ref();
    let (Some(card_max), Some(slot_max)) = (
        pcie_info.and_then(|p| p.card_max.as_ref()),
        pcie_info.and_then(|p| p.slot_max.as_ref()),
    ) else {
        return c.unknown(
            "Couldn't read the slot's PCIe speed",
            "Windows didn't report what the slot supports.",
        );
    };
    let expected = card_max.gen.min(slot_max.gen);
    let got = load.link.gen;
    let date = day(&load.measured_at);
    if got >= expected {
        return c.pass(
            "PCIe link runs at full speed under load",
            format!(
                "PCIe {} during your benchmark on {date}.",
                pcie::gen_label(got)
            ),
        );
    }
    let severity = if got <= 2 {
        Severity::High
    } else {
        Severity::Low
    };
    Check { severity, ..c }.problem(
        format!(
            "PCIe link runs at {} under load instead of {}",
            pcie::gen_label(got),
            pcie::gen_label(expected)
        ),
        format!("Measured during your benchmark on {date}. Set the slot's PCIe speed to Auto in the BIOS, and check the card sits in the top slot."),
        Fix::BiosGuide { topic: BiosTopic::PcieSlot },
    )
}

/// How much Resizable BAR matters depends on the card: a lot on Intel Arc, a little on AMD (Smart Access Memory),
/// next to nothing on NVIDIA, and not at all on cards that don't use it.
fn resizable_bar(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "resizable-bar",
        category: Category::Hardware,
        severity: Severity::Low,
    };
    let Some(card) = separate_card(s) else {
        return no_separate_card(c);
    };
    let off = "Resizable BAR is off";
    let guide = Fix::BiosGuide {
        topic: BiosTopic::ResizableBar,
    };
    let info = Check {
        severity: Severity::Info,
        ..c
    };
    let uses_it = pcie::uses_resizable_bar(card.vendor, &card.name);
    match (card.resizable_bar, card.vendor, uses_it) {
        (None, ..) => c.unknown("Windows doesn't report Resizable BAR for this card", ""),
        (Some(true), ..) => c.pass("Resizable BAR is on", ""),
        (Some(false), GpuVendor::Intel, true) => Check {
            severity: Severity::High,
            ..c
        }
        .problem(
            off,
            "Intel Arc cards lose a lot of FPS without it. Turn on Above 4G Decoding and Re-Size BAR in the BIOS; Windows must start in UEFI mode (CSM off).",
            guide,
        ),
        (Some(false), GpuVendor::Amd, true) => c.problem(
            off,
            "AMD calls it Smart Access Memory. It adds a few FPS in some games. Windows must start in UEFI mode (CSM off).",
            guide,
        ),
        (Some(false), GpuVendor::Nvidia, true) => info.result(
            Status::Info,
            off,
            "NVIDIA only uses it in games on its own list, so most games, VALORANT included, gain little. Turning it on is optional.",
            guide,
        ),
        (Some(false), ..) => info.info(
            off,
            "Only newer cards use it: NVIDIA RTX 30 series and newer, AMD RX 6000 and newer, and Intel Arc.",
        ),
    }
}
```

5. The virtualization check. Insert this function directly above `fn iommu(s: &SystemSnapshot) -> Finding {`:

```rust
fn virtualization(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "virtualization",
        category: Category::Vanguard,
        severity: Severity::Info,
    };
    match s.cpu.virtualization {
        Some(true) => c.pass("Virtualization is on", ""),
        Some(false) => c.result(
            Status::Info,
            "Virtualization is off",
            "VALORANT works fine like this. Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It's called Intel Virtualization Technology (VT-x) or SVM on AMD.",
            Fix::BiosGuide { topic: BiosTopic::Virtualization },
        ),
        None => c.unknown("Couldn't tell whether virtualization is on", ""),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core -p kestrel-probe`

Expected: PASS, 0 failed. kestrel-core has 16 more tests than before this task; kestrel-probe still has 22 unit tests and 2 smoke tests (`report_has_every_check` now counts 31).

Run: `cargo fmt --all --check`

Expected: no output.

Run: `cargo clippy -p kestrel-core -p kestrel-probe --all-targets -- -D warnings`

Expected: no warnings.

Tests changed on purpose: `health::tests::owner_pc_full_results` (31 findings, and `pcie-speed` joins the Info list), `report::tests::owner_report_has_everything` and the probe's `report_has_every_check` (27 → 31). Every other test stays as it is, including `score::tests::owner_pc_scores_86`: the owner's PC still scores 86 with 3 problems.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core/src/health.rs crates/kestrel-core/src/report.rs crates/kestrel-probe/tests/smoke.rs
git commit -m "feat(core): PCIe lanes, PCIe speed under load, Resizable BAR and virtualization checks" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: BIOS guide — PCIe slot and virtualization topics, topic order, stage-1 wording fixes

**Files:**
- Modify: `crates/kestrel-core/src/bios_guide.rs` (code above `#[cfg(test)]`)
- Test: `crates/kestrel-core/src/bios_guide.rs` (`mod tests` at the bottom of the same file)

**Interfaces:**
- Consumes:
  - `crate::health::BiosTopic::{PcieSlot, Virtualization}` (Task 6; the enum's `#[serde(rename_all = "kebab-case")]` sends them as `"pcie-slot"` and `"virtualization"`).
  - `crate::hardware::is_placeholder(text: &str) -> bool` (Task 5: true for empty text and BIOS filler, case-insensitive, trimmed).
  - Existing snapshot fields only: `board.manufacturer`, `board.product`, `cpu.vendor`, `cpu.name`, `machine.form_factor`, `memory.ddr_generation`.
- Produces (no signature changes; `BoardVendor`, `GuideLink`, `GuideSection`, `BiosGuide` stay as they are):
  - `pub fn build_guide(s: &SystemSnapshot) -> BiosGuide` — sections in the order `Xmp`, `MemoryGear` (only when `cpu.vendor == Intel` and `cpu.name` contains `"11th Gen"`, the unchanged rule), `UpdateBios`, `PcieSlot`, `ResizableBar`, `Virtualization`, `Iommu`. `BiosGuide.board` is `"your motherboard"` whenever `is_placeholder(product)`.
  - `pub fn board_vendor(manufacturer: &str) -> BoardVendor` — `BoardVendor::Other` for any placeholder manufacturer.
  - The `pcie-slot` and `virtualization` `GuideSection`s with exactly the contract's title, why and caution, and `link: None`. Task 6's `Fix::BiosGuide { topic: PcieSlot | Virtualization }` and the Hardware page's BIOS-settings links (Task 16) scroll the Toolbox page to them; Toolbox renders any section, so no UI change is needed here.

Background for the implementer: the guide is rendered only by `app/src/pages/Toolbox.tsx`. Its header already says "To open the BIOS, restart and press {enterKey}." and shows `tip`, then lists every section's steps, using each step's text as the React list key (so steps within a section must stay unique). `Ctx` carries what the step builders need; `v(&[...])` turns string slices into `Vec<String>`. Every menu path below was checked against the board makers' own BIOS manuals and FAQs (sources are in the comment above each new function and in "Sources" at the end of this task). Where a maker names the setting differently across chipset generations, the step names the common forms and says "or similar"; the contract's cautions cover the rest. In `pcie-slot` the physical checks (top slot, M.2 drive) come first, while the PC is off, then the BIOS steps end with saving.

- [ ] **Step 1: Write the failing tests**

All in `crates/kestrel-core/src/bios_guide.rs`, inside `mod tests`.

1a. Replace the test module's imports and `section` helper:

```rust
    use super::*;
    use crate::snapshot::{owner_pc_fixture, CpuVendor};

    fn section(g: &BiosGuide, topic: BiosTopic) -> &GuideSection {
        g.sections.iter().find(|s| s.topic == topic).unwrap()
    }
```

with:

```rust
    use super::*;
    use crate::snapshot::{owner_pc_fixture, CpuVendor, FormFactor};

    fn section(g: &BiosGuide, topic: BiosTopic) -> &GuideSection {
        g.sections.iter().find(|s| s.topic == topic).unwrap()
    }

    const MAKERS: [&str; 5] = [
        "ASUSTeK COMPUTER INC.",
        "Micro-Star International Co., Ltd.",
        "Gigabyte Technology Co., Ltd.",
        "ASRock",
        "Dell Inc.",
    ];

    /// The owner's PC on a board made by `maker`, with an Intel CPU (the owner's 11th-gen) or an AM5 Ryzen.
    fn pc(maker: &str, cpu: CpuVendor) -> SystemSnapshot {
        let mut s = owner_pc_fixture();
        s.board.manufacturer = maker.into();
        s.cpu.vendor = cpu;
        if cpu == CpuVendor::Amd {
            s.cpu.name = "AMD Ryzen 7 7800X3D 8-Core Processor".into();
            s.memory.ddr_generation = Some(5);
        }
        s
    }

    fn topics(g: &BiosGuide) -> Vec<BiosTopic> {
        g.sections.iter().map(|s| s.topic).collect()
    }
```

1b. Update the existing test `owner_pc_gets_asus_extreme_tweaker_steps` — its expectation changes because the owner's guide now has the two new topics and the new order. Replace:

```rust
        let topics: Vec<BiosTopic> = g.sections.iter().map(|s| s.topic).collect();
        assert_eq!(
            topics,
            vec![
                BiosTopic::Xmp,
                BiosTopic::MemoryGear,
                BiosTopic::UpdateBios,
                BiosTopic::Iommu,
                BiosTopic::ResizableBar
            ]
        );
```

with:

```rust
        assert_eq!(
            topics(&g),
            vec![
                BiosTopic::Xmp,
                BiosTopic::MemoryGear,
                BiosTopic::UpdateBios,
                BiosTopic::PcieSlot,
                BiosTopic::ResizableBar,
                BiosTopic::Virtualization,
                BiosTopic::Iommu
            ]
        );
```

No other existing test changes: `the_owners_intel_pc_still_reads_xmp` and `am4_asus_board_calls_it_docp_not_expo` still find "turn XMP back on" / "turn D.O.C.P. back on" in the update caution, `unknown_board_name_reads_naturally` still gets "your motherboard" (empty text is a placeholder), and `every_vendor_and_cpu_gets_complete_sections` now also covers the two new sections.

1c. Add these tests after `unknown_board_name_reads_naturally`, before the module's closing `}`:

```rust
    #[test]
    fn topics_run_memory_update_pcie_bar_virtualization_iommu() {
        use BiosTopic::*;
        // 11th-gen Intel (the owner's i9-11900K) gets the Gear 1 topic right after XMP.
        assert_eq!(
            topics(&build_guide(&owner_pc_fixture())),
            vec![
                Xmp,
                MemoryGear,
                UpdateBios,
                PcieSlot,
                ResizableBar,
                Virtualization,
                Iommu
            ]
        );
        assert_eq!(
            topics(&build_guide(&pc(MAKERS[0], CpuVendor::Amd))),
            vec![
                Xmp,
                UpdateBios,
                PcieSlot,
                ResizableBar,
                Virtualization,
                Iommu
            ]
        );
    }

    #[test]
    fn pcie_slot_section_says_why_and_where_lanes_are_shared() {
        for maker in MAKERS {
            for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
                let g = build_guide(&pc(maker, cpu));
                let pcie = section(&g, BiosTopic::PcieSlot);
                assert_eq!(pcie.title, "Run the graphics card at full PCIe speed");
                assert_eq!(pcie.why, "The card should use all its lanes (usually x16) at the PCIe generation it and the slot support. It drops when it sits in a lower slot, shares lanes with an M.2 drive, or the slot is set to an older generation.");
                assert_eq!(
                    pcie.caution.as_deref(),
                    Some("Menu names vary between boards and BIOS versions. Your board's manual lists which slots and M.2 drives share lanes.")
                );
                assert_eq!(pcie.link, None);
            }
        }
    }

    #[test]
    fn pcie_slot_steps_check_the_slot_then_set_its_speed_to_auto() {
        let cases: [(&str, CpuVendor, &[&str]); 10] = [
            (
                MAKERS[0],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "Advanced → System Agent (SA) Configuration → PCI Express Configuration",
                    "PCIEX16_1 Link Speed",
                ],
            ),
            (
                MAKERS[0],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "Advanced → Onboard Devices Configuration",
                    "PCIEX16_1 Link Mode",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "Settings → Advanced → PCIe/PCI Sub-system Settings",
                    "PCIE_E1 - Max Link Speed",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "Settings → Advanced → PCIe/PCI Sub-system Settings",
                    "PCI_E1 Gen Switch",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Intel,
                &[
                    "Press F2 for Advanced Mode.",
                    "Settings → Miscellaneous",
                    "CPU PCIe Link Speed",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Amd,
                &[
                    "Press F2 for Advanced Mode.",
                    "Settings → Miscellaneous",
                    "PCIEX16 Slot Link Speed",
                ],
            ),
            (
                MAKERS[3],
                CpuVendor::Intel,
                &["Advanced → Chipset Configuration", "PCIE1 Link Speed"],
            ),
            (
                MAKERS[3],
                CpuVendor::Amd,
                &["Advanced → AMD PBS", "PCIe x16 Link Speed"],
            ),
            (
                MAKERS[4],
                CpuVendor::Intel,
                &["link speed setting for the top x16 slot"],
            ),
            (
                MAKERS[4],
                CpuVendor::Amd,
                &["link speed setting for the top x16 slot"],
            ),
        ];
        for (maker, cpu, expected) in cases {
            let steps = section(&build_guide(&pc(maker, cpu)), BiosTopic::PcieSlot)
                .steps
                .clone();
            let all = steps.join(" ");
            // The card goes in the top long slot first, and an M.2 drive that shares its lanes moves elsewhere.
            assert!(steps[0].contains("top long slot"), "{maker}: {all}");
            assert!(
                steps[1].contains("M.2") && steps[1].contains("shares lanes"),
                "{maker}: {all}"
            );
            for text in expected {
                assert!(all.contains(text), "{maker} {cpu:?} lacks {text:?}: {all}");
            }
            assert!(all.contains("to Auto"), "{maker} {cpu:?}: {all}");
            let last = steps.last().unwrap();
            assert!(
                last == "Press F10 and choose Yes." || last.ends_with("then save and exit."),
                "{maker}: {last}"
            );
        }
    }

    #[test]
    fn virtualization_section_says_why_and_how_to_find_it() {
        for maker in MAKERS {
            for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
                let g = build_guide(&pc(maker, cpu));
                let virt = section(&g, BiosTopic::Virtualization);
                assert_eq!(virt.title, "Turn on virtualization (only if you need it)");
                assert_eq!(virt.why, "Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It doesn't change FPS.");
                assert_eq!(
                    virt.caution.as_deref(),
                    Some("Menu names vary between boards and BIOS versions. If you can't find it, search the BIOS for SVM or Virtualization.")
                );
                assert_eq!(virt.link, None);
            }
        }
    }

    #[test]
    fn virtualization_steps_name_each_boards_setting() {
        let cases: [(&str, CpuVendor, &[&str]); 10] = [
            (
                MAKERS[0],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "Open Advanced → CPU Configuration.",
                    "Set Intel (VMX) Virtualization Technology to Enabled.",
                ],
            ),
            (
                MAKERS[0],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "Open Advanced → CPU Configuration.",
                    "Set SVM Mode to Enabled.",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Intel,
                &[
                    "Press F7 for Advanced Mode.",
                    "OC → CPU Features",
                    "Set Intel Virtualization Tech to Enabled.",
                ],
            ),
            (
                MAKERS[1],
                CpuVendor::Amd,
                &[
                    "Press F7 for Advanced Mode.",
                    "OC → Advanced CPU Configuration",
                    "SVM Mode",
                    "SVM Enable",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Intel,
                &[
                    "Press F2 for Advanced Mode.",
                    "Open Tweaker → Advanced CPU Settings.",
                    "Set Intel (VMX) Virtualization Technology to Enabled.",
                ],
            ),
            (
                MAKERS[2],
                CpuVendor::Amd,
                &[
                    "Press F2 for Advanced Mode.",
                    "Open Tweaker → Advanced CPU Settings.",
                    "Set SVM Mode to Enabled.",
                ],
            ),
            (
                MAKERS[3],
                CpuVendor::Intel,
                &[
                    "Open Advanced → CPU Configuration.",
                    "Set Intel Virtualization Technology to Enabled.",
                ],
            ),
            (
                MAKERS[3],
                CpuVendor::Amd,
                &[
                    "Open Advanced → CPU Configuration.",
                    "Set SVM Mode to Enabled.",
                ],
            ),
            (
                MAKERS[4],
                CpuVendor::Intel,
                &["Intel Virtualization Technology (VT-x)"],
            ),
            (MAKERS[4], CpuVendor::Amd, &["SVM Mode (AMD-V)"]),
        ];
        for (maker, cpu, expected) in cases {
            let steps = section(&build_guide(&pc(maker, cpu)), BiosTopic::Virtualization)
                .steps
                .join(" ");
            for text in expected {
                assert!(
                    steps.contains(text),
                    "{maker} {cpu:?} lacks {text:?}: {steps}"
                );
            }
            // An Intel PC is never sent to the AMD setting, nor the other way round.
            let wrong = if cpu == CpuVendor::Amd {
                "Intel"
            } else {
                "SVM"
            };
            assert!(!steps.contains(wrong), "{maker} {cpu:?}: {steps}");
        }
    }

    #[test]
    fn an_asus_laptop_is_told_f2_and_never_del() {
        for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
            let mut s = pc(MAKERS[0], cpu);
            s.machine.form_factor = FormFactor::Laptop;
            s.board.product = "G614JV".into();
            let g = build_guide(&s);
            assert_eq!(g.enter_key, "F2");
            assert_eq!(
                section(&g, BiosTopic::Xmp).steps[0],
                "Restart and press F2 to open the BIOS."
            );
            for sec in &g.sections {
                for step in &sec.steps {
                    assert!(!step.contains("Del"), "{:?}: {step}", sec.topic);
                }
            }
        }
        // ASUS desktops keep Del.
        let desktop = build_guide(&owner_pc_fixture());
        assert_eq!(
            section(&desktop, BiosTopic::Xmp).steps[0],
            "Restart and press Del to open the BIOS."
        );
    }

    #[test]
    fn advanced_mode_is_spelled_one_way_everywhere() {
        for maker in MAKERS {
            for cpu in [CpuVendor::Intel, CpuVendor::Amd] {
                let json = serde_json::to_string(&build_guide(&pc(maker, cpu))).unwrap();
                assert!(!json.contains("Advanced mode"), "{maker} {cpu:?}: {json}");
                // MSI's own manuals head the page "Advanced Mode" too.
                if maker == MAKERS[1] {
                    assert!(json.contains("EZ Mode and Advanced Mode"), "{json}");
                }
            }
        }
    }

    #[test]
    fn the_update_caution_asks_for_gear_1_again_only_on_11th_gen_intel() {
        let caution = |s: &SystemSnapshot| {
            section(&build_guide(s), BiosTopic::UpdateBios)
                .caution
                .clone()
                .unwrap()
        };
        let owner = caution(&owner_pc_fixture());
        assert_eq!(owner, "Don't turn off the PC while it updates. Updating resets BIOS settings, so turn XMP back on afterwards. On 11th-gen Intel the memory gear goes back to its default too, so set Gear 1 again.");
        let mut alder_lake = owner_pc_fixture();
        alder_lake.cpu.name = "12th Gen Intel(R) Core(TM) i7-12700K".into();
        for s in [alder_lake, pc(MAKERS[0], CpuVendor::Amd)] {
            let text = caution(&s);
            assert!(!text.contains("Gear"), "{}: {text}", s.cpu.name);
            assert!(text.ends_with("back on afterwards."), "{text}");
        }
    }

    #[test]
    fn placeholder_board_names_are_never_shown_or_matched() {
        for product in [
            "To Be Filled By O.E.M.",
            "Default string",
            " System Product Name ",
        ] {
            let mut s = owner_pc_fixture();
            s.board.product = product.into();
            let g = build_guide(&s);
            assert_eq!(g.board, "your motherboard", "{product}");
            let update = section(&g, BiosTopic::UpdateBios).steps[0].clone();
            assert!(
                update.contains("the latest BIOS for your motherboard from ASUS support"),
                "{update}"
            );
        }
        for maker in [
            "To be filled by O.E.M.",
            "Default string",
            "System manufacturer",
        ] {
            assert_eq!(board_vendor(maker), BoardVendor::Other, "{maker}");
            let mut s = owner_pc_fixture();
            s.board.manufacturer = maker.into();
            assert_eq!(build_guide(&s).vendor, BoardVendor::Other, "{maker}");
        }
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core bios_guide::`

Expected: it compiles (Tasks 5 and 6 are in) and reports `test result: FAILED. 12 passed; 10 failed`. The failures:
- `pcie_slot_section_says_why_and_where_lanes_are_shared`, `pcie_slot_steps_check_the_slot_then_set_its_speed_to_auto`, `virtualization_section_says_why_and_how_to_find_it`, `virtualization_steps_name_each_boards_setting` — panic with ``called `Option::unwrap()` on a `None` value`` in `section` (the topics don't exist yet).
- `owner_pc_gets_asus_extreme_tweaker_steps`, `topics_run_memory_update_pcie_bar_virtualization_iommu` — `assert_eq!` on the topic list (old order, no new topics).
- `an_asus_laptop_is_told_f2_and_never_del` — the XMP step still says "Restart and press Del to open the BIOS.".
- `advanced_mode_is_spelled_one_way_everywhere` — MSI's tip and steps say "Advanced mode".
- `the_update_caution_asks_for_gear_1_again_only_on_11th_gen_intel` — no Gear 1 sentence.
- `placeholder_board_names_are_never_shown_or_matched` — `board` is "To Be Filled By O.E.M.". (Its `board_vendor` assertions already hold today, because no filler string contains a maker's name; they pin the rule.)

- [ ] **Step 3: Implement**

All in `crates/kestrel-core/src/bios_guide.rs`, above `#[cfg(test)]`.

3a. Imports. Replace:

```rust
//! The BIOS guide: step-by-step help for settings Windows can't change, in this board's words.
use crate::health::BiosTopic;
```

with:

```rust
//! The BIOS guide: step-by-step help for settings Windows can't change, in this board's words.
use crate::hardware::is_placeholder;
use crate::health::BiosTopic;
```

3b. Vendor detection ignores placeholders. Replace:

```rust
pub fn board_vendor(manufacturer: &str) -> BoardVendor {
    let m = manufacturer.to_ascii_lowercase();
```

with:

```rust
/// The board's maker. BIOS filler such as "To be filled by O.E.M." never names one.
pub fn board_vendor(manufacturer: &str) -> BoardVendor {
    if is_placeholder(manufacturer) {
        return BoardVendor::Other;
    }
    let m = manufacturer.to_ascii_lowercase();
```

3c. `Ctx` gains the enter key and the Gear 1 flag; `build_guide` hides placeholder board names and uses the new topic order. Replace:

```rust
    tweaker: &'static str,
    profile: &'static str,
}

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn build_guide(s: &SystemSnapshot) -> BiosGuide {
    let vendor = board_vendor(&s.board.manufacturer);
    let product = s.board.product.trim();
    let board = if product.is_empty() {
        "your motherboard"
    } else {
        product
    };
    let upper = product.to_ascii_uppercase();
    let rog = ["MAXIMUS", "CROSSHAIR", "RAMPAGE", "ZENITH"]
        .iter()
        .any(|n| upper.contains(n));
    let amd = s.cpu.vendor == CpuVendor::Amd;
    let ctx = Ctx {
        vendor,
        board,
        amd,
        tweaker: if rog { "Extreme Tweaker" } else { "Ai Tweaker" },
        profile: memory_profile_name(s),
    };

    let mut sections = vec![xmp(&ctx)];
    if s.cpu.vendor == CpuVendor::Intel && s.cpu.name.contains("11th Gen") {
        sections.push(memory_gear(&ctx));
    }
    sections.extend([update_bios(&ctx), iommu(&ctx), resizable_bar(&ctx)]);

    BiosGuide {
        vendor,
        board: board.into(),
        enter_key: enter_key(vendor, s.machine.form_factor).into(),
        tip: tip(vendor).into(),
        sections,
    }
}
```

with:

```rust
    tweaker: &'static str,
    profile: &'static str,
    /// The key the page header names for opening the BIOS.
    enter_key: &'static str,
    /// 11th-gen Intel: the guide has the Gear 1 topic, and a BIOS update resets the gear.
    gear1: bool,
}

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn build_guide(s: &SystemSnapshot) -> BiosGuide {
    let vendor = board_vendor(&s.board.manufacturer);
    let product = s.board.product.trim();
    let board = if is_placeholder(product) {
        "your motherboard"
    } else {
        product
    };
    let upper = product.to_ascii_uppercase();
    let rog = ["MAXIMUS", "CROSSHAIR", "RAMPAGE", "ZENITH"]
        .iter()
        .any(|n| upper.contains(n));
    let amd = s.cpu.vendor == CpuVendor::Amd;
    let key = enter_key(vendor, s.machine.form_factor);
    let ctx = Ctx {
        vendor,
        board,
        amd,
        tweaker: if rog { "Extreme Tweaker" } else { "Ai Tweaker" },
        profile: memory_profile_name(s),
        enter_key: key,
        gear1: s.cpu.vendor == CpuVendor::Intel && s.cpu.name.contains("11th Gen"),
    };

    let mut sections = vec![xmp(&ctx)];
    if ctx.gear1 {
        sections.push(memory_gear(&ctx));
    }
    sections.extend([
        update_bios(&ctx),
        pcie_slot(&ctx),
        resizable_bar(&ctx),
        virtualization(&ctx),
        iommu(&ctx),
    ]);

    BiosGuide {
        vendor,
        board: board.into(),
        enter_key: key.into(),
        tip: tip(vendor).into(),
        sections,
    }
}
```

3d. MSI's tip. Replace:

```rust
        BoardVendor::Msi => "Press F7 to switch between EZ Mode and Advanced mode.",
```

with:

```rust
        BoardVendor::Msi => "Press F7 to switch between EZ Mode and Advanced Mode.",
```

3e. The XMP step that opens the BIOS follows the header's key. Insert directly above `fn xmp(c: &Ctx) -> GuideSection {`:

```rust
/// The step that opens the BIOS names the page header's key, so an ASUS laptop says F2, not Del.
fn open_bios(c: &Ctx) -> String {
    match c.vendor {
        BoardVendor::Other => "Restart and open the BIOS (usually Del or F2 at start-up).".into(),
        _ => format!("Restart and press {} to open the BIOS.", c.enter_key),
    }
}

```

Then, inside `fn xmp`, replace the first step of every arm with `open_bios(c),` — these five lines, which appear nowhere else in the file:
- `            "Restart and press Del to open the BIOS.".into(),` (three times: the ASUS, MSI and Gigabyte arms) → `            open_bios(c),`
- `            "Restart and press F2 or Del to open the BIOS.".into(),` (ASRock arm) → `            open_bios(c),`
- `            "Restart and open the BIOS (usually Del or F2 at start-up).".into(),` (Other arm) → `            open_bios(c),`

(Desktops, ASRock and other makers get the same text as before; only an ASUS laptop changes, to F2.)

3f. The update caution mentions Gear 1 on 11th-gen Intel. In `fn update_bios`, replace:

```rust
    };
    GuideSection {
        topic: BiosTopic::UpdateBios,
```

with:

```rust
    };
    let mut caution = format!("Don't turn off the PC while it updates. Updating resets BIOS settings, so turn {} back on afterwards.", c.profile);
    if c.gear1 {
        caution.push_str(
            " On 11th-gen Intel the memory gear goes back to its default too, so set Gear 1 again.",
        );
    }
    GuideSection {
        topic: BiosTopic::UpdateBios,
```

and replace:

```rust
        caution: Some(format!("Don't turn off the PC while it updates. Updating resets BIOS settings, so turn {} back on afterwards.", c.profile)),
```

with:

```rust
        caution: Some(caution),
```

3g. "Advanced Mode" on MSI. Replace all three occurrences (the two MSI arms of `iommu`, the MSI arm of `resizable_bar`) of:

```rust
            "Press F7 for Advanced mode.",
```

with:

```rust
            "Press F7 for Advanced Mode.",
```

3h. The `pcie-slot` topic. Insert directly above `fn iommu(c: &Ctx) -> GuideSection {`:

```rust
// Menu paths checked in October 2026 against: ASUS BIOS manuals E18567 (ROG MAXIMUS XIII HERO), E19474 (Intel 600),
// E21631 (ROG STRIX B760), E25827 (Intel 800), E17678 (ROG STRIX B550-XE), E21968 (AMD 600), E25269 (AMD 800) and
// FAQ 1055579; MSI BIOS user guides Intel 500/600/700/800, AMD X570-B550, AM5 and AM5 800; GIGABYTE BIOS Setup
// Intel 600/700/800 and AMD X670-B650; ASRock Z590 Pro4 manual, Software/BIOS Setup Guides Intel Z790, Intel 800,
// AMD X670 and AMD 800. Older boards name the slot setting differently, hence "or similar".
/// The card's slot is checked first, while the PC is off; then the slot's speed setting goes back to Auto.
fn pcie_slot(c: &Ctx) -> GuideSection {
    let mut steps = v(&[
        "Shut down the PC and check that the graphics card sits in the top long slot (the one closest to the CPU), pushed fully in with the slot's latch closed.",
        "If your board's manual says an M.2 slot shares lanes with the graphics card's slot, move that drive to another M.2 slot.",
        "Start the PC and open the BIOS.",
    ]);
    steps.extend(match (c.vendor, c.amd) {
        (BoardVendor::Asus, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → System Agent (SA) Configuration → PCI Express Configuration.",
            "Set the top x16 slot's link speed (PCIEX16_1 Link Speed or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asus, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → Onboard Devices Configuration, then PCIE Link Speed if your board lists it.",
            "Set the top x16 slot's link mode (PCIEX16_1 Link Mode or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Settings → Advanced → PCIe/PCI Sub-system Settings (on newer boards, Advanced → PCIe Sub-system Settings).",
            "Set the top x16 slot's speed (PCIE_E1 - Max Link Speed, PEG0 - Max Link Speed or PCI_E1 Gen Mode, depending on the board) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Settings → Advanced → PCIe/PCI Sub-system Settings (on newer boards, Advanced → PCIe Sub-system Settings).",
            "Set the top x16 slot's speed (PCI_E1 Gen Switch or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, false) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Settings → Miscellaneous (on newer boards, Settings → IO Ports → PCIe Link Speed Configuration).",
            "Set CPU PCIe Link Speed to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, true) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Settings → Miscellaneous (on newer boards, Settings → IO Ports → PCIe Link Speed Configuration).",
            "Set the top x16 slot's link speed (PCIEX16 Slot Link Speed or similar) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, false) => v(&[
            "Open Advanced → Chipset Configuration.",
            "Set the top x16 slot's link speed (PCIE1 Link Speed or PCIE2 Link Speed, by the slot's number in your manual) to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, true) => v(&[
            "Open Advanced → AMD PBS.",
            "Set PCIe x16 Link Speed to Auto.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, _) => v(&[
            "Find the link speed setting for the top x16 slot, usually with the PCIe, chipset or onboard device settings.",
            "Set it to Auto, then save and exit.",
        ]),
    });
    GuideSection {
        topic: BiosTopic::PcieSlot,
        title: "Run the graphics card at full PCIe speed".into(),
        why: "The card should use all its lanes (usually x16) at the PCIe generation it and the slot support. It drops when it sits in a lower slot, shares lanes with an M.2 drive, or the slot is set to an older generation.".into(),
        steps,
        caution: Some("Menu names vary between boards and BIOS versions. Your board's manual lists which slots and M.2 drives share lanes.".into()),
        link: None,
    }
}

```

3i. The `virtualization` topic. Insert directly above `#[cfg(test)]`:

```rust
// Menu paths checked in October 2026 against: ASUS FAQ 1045141 and the ASUS BIOS manuals above (Advanced → CPU
// Configuration); MSI BIOS user guides Intel 500/600/700 (OC → CPU Features), Intel 800 (Overclocking → CPU
// Features), AMD X570-B550 and AM5 (OC → Advanced CPU Configuration → SVM Mode), AM5 800 (… → AMD CBS → SVM Enable);
// GIGABYTE BIOS Setup Intel 700 for X boards and AMD X670-B650 (Tweaker → Advanced CPU Settings); ASRock FAQ 405,
// Z590 Pro4 and B550 Pro4 manuals, Software/BIOS Setup Guides Intel Z790, Intel 800 and AMD X670 (CPU Configuration).
/// VT-x on Intel, SVM on AMD. Windows needs it for Memory Integrity; it doesn't change FPS.
fn virtualization(c: &Ctx) -> GuideSection {
    let steps = match (c.vendor, c.amd) {
        (BoardVendor::Asus, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → CPU Configuration.",
            "Set Intel (VMX) Virtualization Technology to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, false) => v(&[
            "Press F7 for Advanced Mode.",
            "Open OC → CPU Features (on newer boards, Overclocking → CPU Features).",
            "Set Intel Virtualization Tech to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, false) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Tweaker → Advanced CPU Settings.",
            "Set Intel (VMX) Virtualization Technology to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, false) => v(&[
            "Open Advanced → CPU Configuration.",
            "Set Intel Virtualization Technology to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, false) => v(&[
            "Find Intel Virtualization Technology (VT-x) in the CPU settings.",
            "Set it to Enabled, then save and exit.",
        ]),
        (BoardVendor::Asus, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open Advanced → CPU Configuration.",
            "Set SVM Mode to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Msi, true) => v(&[
            "Press F7 for Advanced Mode.",
            "Open OC → Advanced CPU Configuration (on newer boards, Overclocking → Advanced CPU Configuration → AMD CBS).",
            "Set SVM Mode (SVM Enable on newer boards) to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Gigabyte, true) => v(&[
            "Press F2 for Advanced Mode.",
            "Open Tweaker → Advanced CPU Settings.",
            "Set SVM Mode to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Asrock, true) => v(&[
            "Open Advanced → CPU Configuration.",
            "Set SVM Mode to Enabled.",
            "Press F10 and choose Yes.",
        ]),
        (BoardVendor::Other, true) => v(&[
            "Find SVM Mode (AMD-V) in the CPU settings.",
            "Set it to Enabled, then save and exit.",
        ]),
    };
    GuideSection {
        topic: BiosTopic::Virtualization,
        title: "Turn on virtualization (only if you need it)".into(),
        why: "Windows needs it for Memory Integrity, which Riot can require on flagged accounts. It doesn't change FPS.".into(),
        steps,
        caution: Some("Menu names vary between boards and BIOS versions. If you can't find it, search the BIOS for SVM or Virtualization.".into()),
        link: None,
    }
}

```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core bios_guide::`
Expected: `test result: ok. 22 passed; 0 failed` (13 existing guide tests, one of them updated, plus 9 new).

Run: `cargo test -p kestrel-core`
Expected: every test passes.

Run: `cargo fmt --all --check`
Expected: no output, exit code 0.

Run: `cargo clippy -p kestrel-core --all-targets -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core/src/bios_guide.rs
git commit -m "feat(core): PCIe slot and virtualization guide topics, new topic order, stage-1 guide wording fixes" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Stage numbers and the stage-1 RAM and virtual-display fixes

**Files:**
- Modify: `crates/kestrel-core/src/health.rs` — `TWEAKS_STAGE` and `COMPETITIVE_STAGE`; the six later-stage fixes and the texts that name a stage
- Modify: `crates/kestrel-core/src/ram.rs` — `normalize_configured` puts a clock a board reported in MHz back into MT/s; `CPU<N>` and underscore-joined locators
- Modify: `crates/kestrel-core/src/gpu.rs` — the name hints split into two lists; `is_virtual_display`
- Modify: `crates/kestrel-probe/src/win/display.rs` — a monitor is virtual only when `gpu::is_virtual_display` says so
- Test: `crates/kestrel-core/src/health.rs` (2 new tests; `only_the_built_in_balanced_and_power_saver_plans_are_problems` updated), `crates/kestrel-core/src/ram.rs` (4 new), `crates/kestrel-core/src/gpu.rs` (1 new)

**Interfaces:**
- Consumes: Task 6's `run_health_check` (31 findings) and the `health.rs` test helpers `get` and `today`; `ram::rated_mts(modules: &[MemoryModule]) -> Option<u32>` (unchanged); the `ram.rs` test helper `module(part, configured, max, locator, bank) -> MemoryModule` (Task 1 only defaults its new `rank`); `gpu::vendor_from(pnp_device_id: &str, name: &str) -> GpuVendor`.
- Produces:
  - `pub const TWEAKS_STAGE: u8 = 5;` and `pub const COMPETITIVE_STAGE: u8 = 6;` in `kestrel_core::health`. power-plan, game-mode and background-recording return `Fix::LaterStage { stage: TWEAKS_STAGE }`; startup-apps, adapter-power and delivery-optimization return `Fix::LaterStage { stage: COMPETITIVE_STAGE }`; every text that names a stage builds it from the constant with `format!`.
  - `pub fn kestrel_core::gpu::is_virtual_display(adapter: &str) -> bool`, which sets the probe's `DisplayInfo.is_virtual`.
  - `pub fn kestrel_core::ram::normalize_configured(modules: &mut [MemoryModule], ddr_generation: Option<u8>)`. Task 11 calls it in the probe right after the WMI memory read, so the snapshot carries MT/s for every reader: the Health Check, the benchmark context, the TypeScript UI and Copy specs. `ram::configured_mts` and the ram-speed check don't change.
  - `ram::channel_layout` reads `CPU<N>` sockets and underscore-joined labels.

What the stage-1 backlog items turned out to be (checked against the stage-2 code):
- **RAM channels.** `CPU1 DIMM A1` + `CPU1 DIMM B1` and `P0_ChannelA` + `P0_ChannelB` already read Dual. The bugs were elsewhere. The socket was lost: `CPU<N>` was never read, and `\bp(\d+)\b` misses `P0_` because `_` is a word character. So channel A on two sockets read as one channel (Single). Underscore-joined words (`CPU1_DIMM_A1`, `P0_ChannelA_DIMM0`, `P0_CHANNEL_A`) matched nothing at all (Unknown). The fix reads `_` as a space and adds `CPU<N>` as a socket token; `Controller<N>`, `P<N>`, `ChannelA-DIMM2` and `DIMM_A2` read as before.
- **XMP alarm.** Some boards report `ConfiguredClockSpeed` in MHz (1800 for DDR4-3600). `ram::normalize_configured` doubles a module's speed only when two things hold. It is below its generation's JEDEC floor, so it can't be MT/s (DDR4 under 2133, DDR5 under 4800). And, doubled, it is within 50 MT/s of the kit's rating. A DDR4-4266 kit really running at 2133 (XMP off), a DDR5-9600 kit at 4800 and a 3600 kit at 2133 keep their speeds and stay flagged. Because the probe normalises the snapshot (Task 11), the ram-speed check itself doesn't change.
- **Virtual displays.** The probe marked a monitor virtual when its adapter's name matched `vendor_from`'s virtual hints, and those included "displaylink" and "basic display". Monitors on a DisplayLink dock (real monitors over USB) or on the Microsoft Basic Display Adapter (a card without its driver) were therefore "a virtual display is active", and the refresh check skipped them. Displays now use `is_virtual_display`, which leaves those two out. In the GPU list both adapters stay `GpuVendor::Virtual`, so neither becomes the card `main_gpu`, `is_hybrid` and the PCIe checks look at (`vendors_from_pci_ids_and_names` and the new test pin both sides).

- [ ] **Step 1: Write the failing tests**

In `crates/kestrel-core/src/health.rs`, test `only_the_built_in_balanced_and_power_saver_plans_are_problems`, replace

```rust
            assert_eq!(plan.fix, Fix::LaterStage { stage: 3 });
```

with

```rust
            assert_eq!(plan.fix, Fix::LaterStage { stage: 5 });
```

Add these tests at the end of `mod tests` in `crates/kestrel-core/src/health.rs`, just above the module's closing `}` (after Task 6's tests):

```rust
    #[test]
    fn later_stage_fixes_follow_the_current_stage_order() {
        assert_eq!((TWEAKS_STAGE, COMPETITIVE_STAGE), (5, 6));
        let mut s = owner_pc_fixture();
        // The owner's PC already has Balanced and adapter power saving; make the other four problems too.
        s.windows.game_mode = Some(false);
        s.windows.background_recording = Some(true);
        s.windows.startup_app_count = Some(11);
        s.network.delivery_optimization_internet = Some(true);
        let f = run_health_check(&s, today());
        for (id, stage) in [
            ("power-plan", 5),
            ("game-mode", 5),
            ("background-recording", 5),
            ("startup-apps", 6),
            ("adapter-power", 6),
            ("delivery-optimization", 6),
        ] {
            let finding = get(&f, id);
            assert_eq!(
                (finding.status, finding.fix.clone()),
                (Status::Problem, Fix::LaterStage { stage }),
                "{id}"
            );
        }
        assert_eq!(get(&f, "power-plan").detail, "This plan lets the CPU slow down between frames, which hurts 1% lows. Kestrel's own plan arrives in stage 5. Until then, pick High performance in Control Panel → Power Options. If it only lists Balanced (laptops with Modern Standby do that), set Settings → System → Power → Power mode → Best performance instead.");
        assert_eq!(get(&f, "startup-apps").detail, "Each one uses memory and CPU in the background. Kestrel's startup manager arrives in stage 6; until then, turn off what you don't need in Task Manager → Startup apps.");
        assert_eq!(get(&f, "adapter-power").detail, "Windows can let the adapter doze, which causes small latency spikes. Kestrel's Competitive tier turns this off in stage 6.");
        for finding in &f {
            for old in ["stage 3", "stage 4"] {
                assert!(
                    !finding.title.contains(old) && !finding.detail.contains(old),
                    "{} still says {old}",
                    finding.id
                );
            }
        }
    }

    #[test]
    fn ram_speed_passes_normalised_mhz_clocks_and_flags_slow_kits() {
        // The owner's PC with both sticks of `part` reporting `configured`, normalised as the probe does.
        let ram_at = |part: &str, configured: u32| {
            let mut s = owner_pc_fixture();
            for m in &mut s.memory.modules {
                m.part_number = part.into();
                m.configured_mts = Some(configured);
            }
            ram::normalize_configured(&mut s.memory.modules, s.memory.ddr_generation);
            get(&run_health_check(&s, today()), "ram-speed").clone()
        };
        // The owner's DDR4-3600 kit on a board that reports 1800 MHz.
        let mhz = ram_at("CMG32GX4M2D3600C18", 1800);
        assert_eq!(
            (mhz.status, mhz.title.as_str()),
            (Status::Pass, "RAM runs at 3600 MT/s, its rated speed")
        );
        // A DDR4-4266 kit really running at 2133 (XMP off), and a 3600 kit at 2133, stay problems.
        for (part, rated) in [("F4-4266C17D-16GTZR", 4266), ("CMG32GX4M2D3600C18", 3600)] {
            let slow = ram_at(part, 2133);
            assert_eq!(
                (slow.status, slow.severity, slow.title),
                (
                    Status::Problem,
                    Severity::High,
                    format!("RAM runs at 2133 MT/s, but the kit is rated {rated}")
                ),
                "{part}"
            );
        }
    }
```

Add these tests at the end of `mod tests` in `crates/kestrel-core/src/ram.rs`, just above the module's closing `}`:

```rust
    #[test]
    fn dual_socket_labels_name_the_cpu_and_the_channel() {
        let layout = |a: &str, b: &str| {
            channel_layout(&[module("", None, None, a, ""), module("", None, None, b, "")])
        };
        assert_eq!(layout("CPU1 DIMM A1", "CPU1 DIMM B1"), ChannelLayout::Dual);
        assert_eq!(
            layout("CPU1 DIMM A1", "CPU1 DIMM A2"),
            ChannelLayout::Single
        );
        // Channel A of each processor is two channels, as with P0/P1 and Controller0/1.
        assert_eq!(layout("CPU1 DIMM A1", "CPU2 DIMM A1"), ChannelLayout::Dual);
        // Some boards join the words with underscores.
        assert_eq!(layout("CPU1_DIMM_A1", "CPU1_DIMM_B1"), ChannelLayout::Dual);
        assert_eq!(
            layout("CPU1_DIMM_A1", "CPU1_DIMM_A2"),
            ChannelLayout::Single
        );
        assert_eq!(layout("CPU1_DIMM_A1", "CPU2_DIMM_A1"), ChannelLayout::Dual);
    }

    #[test]
    fn underscore_labels_name_the_socket_and_the_channel() {
        let layout = |a: (&str, &str), b: (&str, &str)| {
            channel_layout(&[
                module("", None, None, a.0, a.1),
                module("", None, None, b.0, b.1),
            ])
        };
        assert_eq!(
            layout(("P0_ChannelA", ""), ("P0_ChannelB", "")),
            ChannelLayout::Dual
        );
        assert_eq!(
            layout(("P0_ChannelA_DIMM0", ""), ("P0_ChannelA_DIMM1", "")),
            ChannelLayout::Single
        );
        assert_eq!(
            layout(("P0_ChannelA", ""), ("P1_ChannelA", "")),
            ChannelLayout::Dual
        );
        // In the bank label, with underscores throughout.
        assert_eq!(
            layout(("DIMM 0", "P0_CHANNEL_A"), ("DIMM 1", "P0_CHANNEL_B")),
            ChannelLayout::Dual
        );
        assert_eq!(
            layout(("DIMM 0", "P0_CHANNEL_A"), ("DIMM 1", "P0_CHANNEL_A")),
            ChannelLayout::Single
        );
    }

    /// Two sticks of `part` reporting `configured`, normalised for DDR `generation`: their speeds afterwards.
    fn normalized(part: &str, configured: u32, generation: Option<u8>) -> Vec<Option<u32>> {
        let mut kit = [
            module(part, Some(configured), None, "ChannelA-DIMM2", ""),
            module(part, Some(configured), None, "ChannelB-DIMM2", ""),
        ];
        normalize_configured(&mut kit, generation);
        kit.iter().map(|m| m.configured_mts).collect()
    }

    #[test]
    fn a_clock_reported_in_mhz_is_doubled_only_below_the_jedec_floor() {
        // DDR4-3600 reported as 1800: under DDR4's 2133 floor, and 3600 once doubled.
        assert_eq!(
            normalized("CMG32GX4M2D3600C18", 1800, Some(4)),
            [Some(3600); 2]
        );
        // DDR5-8000 reported as 4000: under DDR5's 4800 floor.
        assert_eq!(
            normalized("F5-8000J3848H16GX2-TZ5RK", 4000, Some(5)),
            [Some(8000); 2]
        );
        // At or above the floor it is MT/s: a DDR4-4266 kit really at 2133 (XMP off), a DDR5-9600 kit at 4800, and
        // a 3600 kit at 2133.
        assert_eq!(
            normalized("F4-4266C17D-16GTZR", 2133, Some(4)),
            [Some(2133); 2]
        );
        assert_eq!(
            normalized("F5-9600J4656F24GX2-TZ5CK", 4800, Some(5)),
            [Some(4800); 2]
        );
        assert_eq!(
            normalized("CMG32GX4M2D3600C18", 2133, Some(4)),
            [Some(2133); 2]
        );
        // Doubled, it must be within 50 MT/s of the kit's rating.
        assert_eq!(
            normalized("CMG32GX4M2D3600C18", 1775, Some(4)),
            [Some(3550); 2]
        );
        assert_eq!(
            normalized("CMG32GX4M2D3600C18", 1774, Some(4)),
            [Some(1774); 2]
        );
        // A mixed kit goes by its rating, the slowest module's: 3600 and 3200 sticks reporting 1600.
        let mut mixed = [
            module("CMG32GX4M2D3600C18", Some(1600), None, "", ""),
            module("F4-3200C16-8GVKB", Some(1600), None, "", ""),
        ];
        normalize_configured(&mut mixed, Some(4));
        assert_eq!(configured_mts(&mixed), Some(3200));
    }

    #[test]
    fn without_a_known_generation_or_rating_nothing_is_doubled() {
        assert_eq!(
            normalized("CMG32GX4M2D3600C18", 1800, None),
            [Some(1800); 2]
        );
        assert_eq!(
            normalized("CMG32GX4M2D3600C18", 1800, Some(3)),
            [Some(1800); 2]
        );
        assert_eq!(normalized("CT16G4DFRA32A", 1600, Some(4)), [Some(1600); 2]);
        // Missing and zero speeds stay as they are.
        let mut unreported = [
            module("CMG32GX4M2D3600C18", None, None, "", ""),
            module("CMG32GX4M2D3600C18", Some(0), None, "", ""),
        ];
        normalize_configured(&mut unreported, Some(4));
        assert_eq!(
            [unreported[0].configured_mts, unreported[1].configured_mts],
            [None, Some(0)]
        );
    }
```

Add this test at the end of `mod tests` in `crates/kestrel-core/src/gpu.rs`, just above the module's closing `}`:

```rust
    #[test]
    fn monitors_on_displaylink_and_basic_display_adapters_are_real() {
        // A DisplayLink dock drives real monitors over USB; Windows uses the Basic Display Adapter for a graphics card
        // without its driver. Neither monitor is virtual.
        for adapter in [
            "DisplayLink USB Device",
            "Microsoft Basic Display Adapter",
            "NVIDIA GeForce RTX 3060 Ti",
            "Intel(R) UHD Graphics 750",
        ] {
            assert!(!is_virtual_display(adapter), "{adapter}");
        }
        for adapter in [
            "Parsec Virtual Display Adapter",
            "Microsoft Remote Display Adapter",
            "Virtual Display Driver",
        ] {
            assert!(is_virtual_display(adapter), "{adapter}");
        }
        // As graphics cards they still don't count, so neither becomes the card the checks look at.
        assert_eq!(
            vendor_from("USB\\VID_17E9&PID_4301", "DisplayLink USB Device"),
            GpuVendor::Virtual
        );
        assert_eq!(
            vendor_from("PCI\\VEN_10DE&DEV_2489", "Microsoft Basic Display Adapter"),
            GpuVendor::Virtual
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core`

Expected: FAIL to compile with error E0425 for each of the four new names:
- "cannot find value `TWEAKS_STAGE` in this scope", and the same for `COMPETITIVE_STAGE`;
- "cannot find function `is_virtual_display` in this scope" (2 times);
- `normalize_configured`: "cannot find function `normalize_configured` in this scope" (3 times, in `ram.rs`) and "cannot find function `normalize_configured` in module `ram`" (once, in `health.rs`).

Stand-ins for the new names were then added: the two constants, an `is_virtual_display` that keeps today's rule, and a `normalize_configured` that does nothing. With those, 7 tests fail on their assertions:
- `later_stage_fixes_follow_the_current_stage_order` and `only_the_built_in_balanced_and_power_saver_plans_are_problems`: stage 3 where 5 is expected.
- `ram_speed_passes_normalised_mhz_clocks_and_flags_slow_kits`: "RAM runs at 1800 MT/s, but the kit is rated 3600".
- `a_clock_reported_in_mhz_is_doubled_only_below_the_jedec_floor`: `[Some(1800), Some(1800)]` where `[Some(3600), Some(3600)]` is expected.
- `dual_socket_labels_name_the_cpu_and_the_channel`: Single where Dual is expected.
- `underscore_labels_name_the_socket_and_the_channel`: Unknown where Single is expected.
- `monitors_on_displaylink_and_basic_display_adapters_are_real`: DisplayLink is classed virtual.

`without_a_known_generation_or_rating_nothing_is_doubled` passes even against the do-nothing stand-in. That's by design: it pins the cases that must stay untouched.

- [ ] **Step 3: Implement**

`crates/kestrel-core/src/health.rs`:

1. The stage constants, below the `Fix` enum. Replace

```rust
    Link { label: String, url: String },
    Nothing,
}
```

with

```rust
    Link { label: String, url: String },
    Nothing,
}

/// The stage that brings the tweak engine: Kestrel's own power plan, Game Mode and background recording.
pub const TWEAKS_STAGE: u8 = 5;
/// The stage that brings the Competitive tier: startup apps, adapter power saving and Delivery Optimization.
pub const COMPETITIVE_STAGE: u8 = 6;
```

2. `power_plan`. In its Balanced / Power saver branch, replace

```rust
            "This plan lets the CPU slow down between frames, which hurts 1% lows. Kestrel's own plan arrives in stage 3. Until then, pick High performance in Control Panel → Power Options. If it only lists Balanced (laptops with Modern Standby do that), set Settings → System → Power → Power mode → Best performance instead.",
            Fix::LaterStage { stage: 3 },
```

with

```rust
            format!("This plan lets the CPU slow down between frames, which hurts 1% lows. Kestrel's own plan arrives in stage {TWEAKS_STAGE}. Until then, pick High performance in Control Panel → Power Options. If it only lists Balanced (laptops with Modern Standby do that), set Settings → System → Power → Power mode → Best performance instead."),
            Fix::LaterStage { stage: TWEAKS_STAGE },
```

3. `game_mode`. Replace the `Some(false)` arm

```rust
        Some(false) => c.problem("Game Mode is off", "Game Mode tells Windows to favour the game you're playing. Ryzen X3D chips also need it to pick the right cores.", Fix::LaterStage { stage: 3 }),
```

with

```rust
        Some(false) => c.problem("Game Mode is off", "Game Mode tells Windows to favour the game you're playing. Ryzen X3D chips also need it to pick the right cores.", Fix::LaterStage { stage: TWEAKS_STAGE }),
```

4. `background_recording`. Replace the `Some(true)` arm

```rust
        Some(true) => c.problem("Background recording is on", "Xbox Game Bar keeps recording gameplay in the background (\"Record what happened\"), which costs FPS.", Fix::LaterStage { stage: 3 }),
```

with

```rust
        Some(true) => c.problem("Background recording is on", "Xbox Game Bar keeps recording gameplay in the background (\"Record what happened\"), which costs FPS.", Fix::LaterStage { stage: TWEAKS_STAGE }),
```

5. `startup_apps`. Replace

```rust
            "Each one uses memory and CPU in the background. Kestrel's startup manager arrives in stage 4; until then, turn off what you don't need in Task Manager → Startup apps.",
            Fix::LaterStage { stage: 4 },
```

with

```rust
            format!("Each one uses memory and CPU in the background. Kestrel's startup manager arrives in stage {COMPETITIVE_STAGE}; until then, turn off what you don't need in Task Manager → Startup apps."),
            Fix::LaterStage { stage: COMPETITIVE_STAGE },
```

6. `adapter_power`. Replace

```rust
        c.problem("Network adapter power saving is on", "Windows can let the adapter doze, which causes small latency spikes. Kestrel's Competitive tier turns this off in stage 4.", Fix::LaterStage { stage: 4 })
```

with

```rust
        c.problem("Network adapter power saving is on", format!("Windows can let the adapter doze, which causes small latency spikes. Kestrel's Competitive tier turns this off in stage {COMPETITIVE_STAGE}."), Fix::LaterStage { stage: COMPETITIVE_STAGE })
```

7. `delivery_optimization`. Replace

```rust
            "Your PC shares updates with strangers, which uses upload bandwidth while you play.",
            Fix::LaterStage { stage: 4 },
```

with (rustfmt's layout)

```rust
            "Your PC shares updates with strangers, which uses upload bandwidth while you play.",
            Fix::LaterStage {
                stage: COMPETITIVE_STAGE,
            },
```

`crates/kestrel-core/src/ram.rs`:

8. Insert `normalize_configured` directly below `configured_mts`, that is, above `fn channel_letter(module: &MemoryModule) -> Option<char> {`. `configured_mts` itself doesn't change.

```rust
/// Puts speeds a board reported in MHz back into MT/s. Some boards report `ConfiguredClockSpeed` in MHz, half the
/// MT/s (1800 for a DDR4-3600 kit). A module's speed is doubled only when it is below its generation's JEDEC floor,
/// so it can't be MT/s (DDR4 under 2133, DDR5 under 4800), and when, doubled, it is within 50 MT/s of the kit's
/// rating (`rated_mts`, read before anything changes). So a DDR4-4266 kit really running at 2133 (XMP off) and a
/// DDR5-9600 kit at 4800 keep those speeds and stay flagged. With an unknown or other generation, or no rating,
/// nothing changes. The probe calls this right after reading the modules, so every reader of the snapshot sees MT/s.
pub fn normalize_configured(modules: &mut [MemoryModule], ddr_generation: Option<u8>) {
    let floor = match ddr_generation {
        Some(4) => 2133,
        Some(5) => 4800,
        _ => return,
    };
    let Some(rated) = rated_mts(modules) else {
        return;
    };
    for m in modules.iter_mut() {
        m.configured_mts = m.configured_mts.map(|c| {
            if c < floor && (c * 2).abs_diff(rated) <= 50 {
                c * 2
            } else {
                c
            }
        });
    }
}
```

9. Replace `channel_letter` and `socket_qualifier` (everything from `fn channel_letter` down to the doc comment of `channel_key`):

```rust
fn channel_letter(module: &MemoryModule) -> Option<char> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)channel\s*([a-h])\b|\bdimm[_\s]?([a-h])\d").expect("valid pattern")
    });
    [&module.device_locator, &module.bank_label]
        .into_iter()
        .find_map(|text| {
            let caps = re.captures(text)?;
            let letter = caps.get(1).or_else(|| caps.get(2))?;
            letter
                .as_str()
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
        })
}

/// The socket or controller a module sits behind: the N in `Controller<N>` or a `P<N>` socket token.
fn socket_qualifier(module: &MemoryModule) -> Option<u32> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"(?i)controller\s*(\d+)|\bp(\d+)\b").expect("valid pattern"));
    [&module.device_locator, &module.bank_label]
        .into_iter()
        .find_map(|text| {
            let caps = re.captures(text)?;
            caps.get(1)
                .or_else(|| caps.get(2))?
                .as_str()
                .parse::<u32>()
                .ok()
        })
}
```

with

```rust
/// A module's locator and bank label, with underscores read as spaces ("P0_ChannelA" → "P0 ChannelA").
fn labels(module: &MemoryModule) -> [String; 2] {
    [&module.device_locator, &module.bank_label].map(|text| text.replace('_', " "))
}

fn channel_letter(module: &MemoryModule) -> Option<char> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)channel\s*([a-h])\b|\bdimm[_\s]?([a-h])\d").expect("valid pattern")
    });
    labels(module).iter().find_map(|text| {
        let caps = re.captures(text)?;
        let letter = caps.get(1).or_else(|| caps.get(2))?;
        letter
            .as_str()
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
    })
}

/// The socket or controller a module sits behind: the N in `Controller<N>`, `CPU<N>` or a `P<N>` socket token.
fn socket_qualifier(module: &MemoryModule) -> Option<u32> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)controller\s*(\d+)|\bcpu\s*(\d+)|\bp(\d+)\b").expect("valid pattern")
    });
    labels(module).iter().find_map(|text| {
        let caps = re.captures(text)?;
        caps.get(1)
            .or_else(|| caps.get(2))
            .or_else(|| caps.get(3))?
            .as_str()
            .parse::<u32>()
            .ok()
    })
}
```

`crates/kestrel-core/src/gpu.rs`:

10. Replace everything from `const VIRTUAL_HINTS` to the end of `vendor_from`:

```rust
const VIRTUAL_HINTS: &[&str] = &[
    "virtual",
    "parsec",
    "basic display",
    "remote display",
    "indirect display",
    "displaylink",
];

/// PCI vendor ids (uppercase) of virtual adapters: Microsoft, VMware, VirtIO and Parallels.
const VIRTUAL_VENDOR_IDS: &[&str] = &["VEN_1414", "VEN_15AD", "VEN_1AF4", "VEN_1AB8"];

/// Vendor from the PCI id inside a PNP device id ("PCI\\VEN_10DE&…"), falling back to the name.
/// Virtual name hints and virtual vendor ids are checked first and override the PCI vendor.
pub fn vendor_from(pnp_device_id: &str, name: &str) -> GpuVendor {
    let id = pnp_device_id.to_ascii_uppercase();
    let lower = name.to_ascii_lowercase();
    if VIRTUAL_HINTS.iter().any(|h| lower.contains(h))
        || VIRTUAL_VENDOR_IDS.iter().any(|v| id.contains(v))
    {
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
```

with

```rust
/// Name hints of adapters whose monitors exist only in software: remote play, remote desktop and virtual displays.
const VIRTUAL_DISPLAY_HINTS: &[&str] = &["virtual", "parsec", "remote display", "indirect display"];

/// Adapters that drive real monitors but aren't a graphics card a game runs on: DisplayLink USB docks, and the
/// Microsoft Basic Display Adapter Windows uses for a card without its driver.
const NOT_A_GAMING_GPU_HINTS: &[&str] = &["basic display", "displaylink"];

/// PCI vendor ids (uppercase) of virtual adapters: Microsoft, VMware, VirtIO and Parallels.
const VIRTUAL_VENDOR_IDS: &[&str] = &["VEN_1414", "VEN_15AD", "VEN_1AF4", "VEN_1AB8"];

/// Vendor from the PCI id inside a PNP device id ("PCI\\VEN_10DE&…"), falling back to the name.
/// Virtual name hints and virtual vendor ids are checked first and override the PCI vendor, so virtual adapters and
/// adapters no game runs on (DisplayLink, Microsoft Basic Display Adapter) never become the main GPU.
pub fn vendor_from(pnp_device_id: &str, name: &str) -> GpuVendor {
    let id = pnp_device_id.to_ascii_uppercase();
    let lower = name.to_ascii_lowercase();
    if VIRTUAL_DISPLAY_HINTS
        .iter()
        .chain(NOT_A_GAMING_GPU_HINTS)
        .any(|h| lower.contains(h))
        || VIRTUAL_VENDOR_IDS.iter().any(|v| id.contains(v))
    {
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

/// Whether a monitor on this adapter (the name `EnumDisplayDevices` gives) exists only in software, like Parsec's or
/// a remote desktop's. Monitors on a DisplayLink dock or on the Microsoft Basic Display Adapter are real.
pub fn is_virtual_display(adapter: &str) -> bool {
    let lower = adapter.to_ascii_lowercase();
    VIRTUAL_DISPLAY_HINTS.iter().any(|h| lower.contains(h))
}
```

`crates/kestrel-probe/src/win/display.rs` (Windows only):

11. Replace the import line

```rust
use kestrel_core::{gpu, DisplayInfo, GpuVendor, SystemSnapshot};
```

with

```rust
use kestrel_core::{gpu, DisplayInfo, SystemSnapshot};
```

and, in `fill`,

```rust
            is_virtual: gpu::vendor_from("", &adapter) == GpuVendor::Virtual,
```

with

```rust
            is_virtual: gpu::is_virtual_display(&adapter),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kestrel-core -p kestrel-probe`

Expected: PASS, 0 failed. kestrel-core has 7 more tests than before this task; kestrel-probe still has 22 unit tests and 2 smoke tests.

Run: `cargo fmt --all --check`

Expected: no output.

Run: `cargo clippy -p kestrel-core -p kestrel-probe --all-targets -- -D warnings`

Expected: no warnings.

Run: `cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc -- -D warnings`

This run is needed because `display.rs` only builds on Windows. Run `rustup target add x86_64-pc-windows-msvc` once beforehand if needed.

Expected: no warnings. This also confirms `GpuVendor` is no longer imported in `display.rs`.

Tests changed on purpose: `health::tests::only_the_built_in_balanced_and_power_saver_plans_are_problems` (stage 3 → 5: decision 66 moved the tweak engine to stage 5). Every other existing test passes unchanged, including:
- `gpu.rs`'s `vendors_from_pci_ids_and_names`, which still expects DisplayLink and the Basic Display Adapter to be `GpuVendor::Virtual` as graphics cards.
- The `ram.rs` channel tests (`ChannelA-DIMM2`, `DIMM_A2`, `P0 CHANNEL A`, `Controller<N>`, `P<N>`), and the `configured_speed_*` tests (`configured_mts` is unchanged).
- The ram-speed tests: the check is unchanged, so 2133 on the owner's 3600 kit stays a High problem.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-core/src/health.rs crates/kestrel-core/src/ram.rs crates/kestrel-core/src/gpu.rs crates/kestrel-probe/src/win/display.rs
git commit -m "fix(core): stage numbers 5 and 6, dual-socket and underscore RAM labels, RAM clock in MHz, real monitors on DisplayLink and Basic Display" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Probe — CPUID identity, processor topology, boot mode and virtualization

**Files:**
- Modify: `crates/kestrel-probe/Cargo.toml` (the windows-sys features Tasks 9–11 need)
- Modify: `crates/kestrel-probe/src/parse.rs` (CPUID text, the topology buffer parser, boot mode, virtualization, laptop socket)
- Create: `crates/kestrel-probe/src/win/cpu.rs`
- Modify: `crates/kestrel-probe/src/win/mod.rs`
- Test: `crates/kestrel-probe/src/parse.rs` (tests module); `crates/kestrel-probe/src/win/cpu.rs` (two Windows-only tests, run on CI)

**Interfaces:**
- Consumes:
  - Task 1: `CpuInfo.{family, model, stepping, codename, socket, base_mhz, max_mhz, p_cores, e_cores, caches, instructions, virtualization}`, `BoardInfo.boot_mode`, `kestrel_core::{CacheKind, BootMode}`.
  - Task 2: `kestrel_core::cpu::CpuidRaw { vendor, max_leaf, leaf1, leaf7, leaf7_1_eax, leaf16, brand, xcr0 }`, `cpu::decode(raw: &CpuidRaw) -> CpuIdentity` (uses `vendor`, `family`, `model`, `stepping`, `instructions`, `hypervisor`, `base_mhz`, `max_mhz`), `cpu::codename(vendor: CpuVendor, family: u32, model: u32, stepping: u32) -> Option<Codename>` with `Codename { name: &'static str, socket: Option<&'static str> }`.
  - Task 3: `kestrel_core::topology::PhysicalCore { efficiency_class: u8, logical: Vec<(u16, u8)> }`, `topology::p_e_split(cores: &[PhysicalCore]) -> Option<(u32, u32)>`, `topology::group_caches(caches: &[(u8, CacheKind, u32)]) -> Vec<CacheInfo>`.
  - Existing: `SecurityInfo.vbs_running` (WMI `device-guard`), `MachineInfo.form_factor` (WMI `machine`), `win::pdh::Query`, `win::problem`.
- Produces:
  - `parse::cpuid_text(registers: &[u32]) -> String`
  - `parse::logical_processors(buffer: &[u8]) -> (Vec<kestrel_core::topology::PhysicalCore>, Vec<(u8, kestrel_core::CacheKind, u32)>)` — cache sizes in KB (Windows reports bytes; the parser divides by 1024)
  - `parse::boot_mode(firmware_type: i32) -> BootMode`
  - `parse::virtualization(hypervisor: bool, vbs_running: Option<bool>, firmware_enabled: Option<bool>) -> Option<bool>`
  - `parse::cpu_socket(form_factor: FormFactor, table_socket: Option<&str>) -> Option<String>`
  - `win::cpu::fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>)` and `pub(crate) fn win::cpu::topology() -> Option<(Vec<PhysicalCore>, Vec<(u8, CacheKind, u32)>)>` (Task 12's clocks use it)
  - Probe errors `cpu-id`, `topology`, `firmware`
  - windows-sys features `Win32_System_Threading` (here), `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Devices_Properties` (Task 10) and `Win32_Devices_Display` (Task 11)

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module at the end of `crates/kestrel-probe/src/parse.rs` (inside its closing `}`):
```rust
    /// CPUID registers holding `text` the way the processor stores it: four bytes per register, little-endian,
    /// padded with NULs to `count` registers.
    fn registers(text: &str, count: usize) -> Vec<u32> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.resize(count * 4, 0);
        let (words, _) = bytes.as_chunks::<4>();
        words.iter().map(|word| u32::from_le_bytes(*word)).collect()
    }

    #[test]
    fn cpuid_registers_spell_the_vendor() {
        // Leaf 0 spells it across EBX, EDX and ECX.
        assert_eq!(
            cpuid_text(&[0x756e_6547, 0x4965_6e69, 0x6c65_746e]),
            "GenuineIntel"
        );
        assert_eq!(
            cpuid_text(&[0x6874_7541, 0x6974_6e65, 0x444d_4163]),
            "AuthenticAMD"
        );
    }

    #[test]
    fn cpuid_brands_lose_their_nuls_and_padding() {
        let brand = registers("11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz", 12);
        assert_eq!(
            cpuid_text(&brand),
            "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz"
        );
        // Older Intel processors right-align the brand with spaces.
        let padded = registers("       Intel(R) Core(TM) i7-4790K CPU @ 4.00GHz", 12);
        assert_eq!(
            cpuid_text(&padded),
            "Intel(R) Core(TM) i7-4790K CPU @ 4.00GHz"
        );
        assert_eq!(cpuid_text(&[0; 12]), "");
    }

    const KB: u32 = 1024;

    /// A RelationProcessorCore record as 64-bit Windows writes it: the header (relationship, size), Flags,
    /// EfficiencyClass, 20 reserved bytes, GroupCount at 30, then a 16-byte GROUP_AFFINITY (Mask, Group) per group.
    fn core_record(efficiency_class: u8, groups: &[(u16, u64)]) -> Vec<u8> {
        let size = 32 + 16 * groups.len();
        let mut record = vec![0u8; size];
        record[4..8].copy_from_slice(&(size as u32).to_le_bytes());
        let threads: u32 = groups.iter().map(|(_, mask)| mask.count_ones()).sum();
        record[8] = u8::from(threads > 1); // LTP_PC_SMT
        record[9] = efficiency_class;
        record[30..32].copy_from_slice(&(groups.len() as u16).to_le_bytes());
        for (index, (group, mask)) in groups.iter().enumerate() {
            let at = 32 + 16 * index;
            record[at..at + 8].copy_from_slice(&mask.to_le_bytes());
            record[at + 8..at + 10].copy_from_slice(&group.to_le_bytes());
        }
        record
    }

    /// A RelationCache record (56 bytes, as Windows 11 writes it): Level at 8, CacheSize in bytes at 12, Type at 16.
    fn cache_record(level: u8, size_bytes: u32, cache_type: u32) -> Vec<u8> {
        let mut record = vec![0u8; 56];
        record[0..4].copy_from_slice(&2u32.to_le_bytes());
        record[4..8].copy_from_slice(&56u32.to_le_bytes());
        record[8] = level;
        record[12..16].copy_from_slice(&size_bytes.to_le_bytes());
        record[16..20].copy_from_slice(&cache_type.to_le_bytes());
        record
    }

    /// A record of a relationship Kestrel skips: 3 is a processor package, 4 a processor group.
    fn other_record(relationship: u32, size: u32) -> Vec<u8> {
        let mut record = vec![0u8; size as usize];
        record[0..4].copy_from_slice(&relationship.to_le_bytes());
        record[4..8].copy_from_slice(&size.to_le_bytes());
        record
    }

    #[test]
    fn an_eight_core_cpu_with_two_threads_per_core() {
        // The i9-11900K: per core 48 KB L1 data, 32 KB L1 instruction and 512 KB L2; one 16 MB L3.
        let mut buffer = other_record(3, 48);
        for core in 0..8 {
            buffer.extend(core_record(0, &[(0, 0b11 << (2 * core))]));
            buffer.extend(cache_record(1, 48 * KB, 2));
            buffer.extend(cache_record(1, 32 * KB, 1));
            buffer.extend(cache_record(2, 512 * KB, 0));
        }
        buffer.extend(cache_record(3, 16 * 1024 * KB, 0));
        buffer.extend(other_record(4, 80));
        let (cores, caches) = logical_processors(&buffer);
        assert_eq!(cores.len(), 8);
        assert!(cores
            .iter()
            .all(|c| c.efficiency_class == 0 && c.logical.len() == 2));
        assert_eq!(cores[3].logical, vec![(0, 6), (0, 7)]);
        assert_eq!(caches.len(), 25);
        assert_eq!(
            caches[..3],
            [
                (1, CacheKind::Data, 48),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 512)
            ]
        );
        assert_eq!(caches[24], (3, CacheKind::Unified, 16384));
    }

    #[test]
    fn a_hybrid_cpu_keeps_each_cores_efficiency_class() {
        // 2 P-cores (class 1) with two threads each, then 4 E-cores (class 0) with one, sharing a 2 MB L2.
        let mut buffer = core_record(1, &[(0, 0b0011)]);
        buffer.extend(core_record(1, &[(0, 0b1100)]));
        for thread in 4..8 {
            buffer.extend(core_record(0, &[(0, 1 << thread)]));
        }
        buffer.extend(cache_record(2, 1280 * KB, 0));
        buffer.extend(cache_record(2, 1280 * KB, 0));
        buffer.extend(cache_record(2, 2048 * KB, 0));
        let (cores, caches) = logical_processors(&buffer);
        let classes: Vec<u8> = cores.iter().map(|c| c.efficiency_class).collect();
        assert_eq!(classes, vec![1, 1, 0, 0, 0, 0]);
        assert_eq!(cores[1].logical, vec![(0, 2), (0, 3)]);
        assert_eq!(cores[5].logical, vec![(0, 7)]);
        assert_eq!(
            caches,
            vec![
                (2, CacheKind::Unified, 1280),
                (2, CacheKind::Unified, 1280),
                (2, CacheKind::Unified, 2048)
            ]
        );
    }

    #[test]
    fn a_core_in_the_second_processor_group_keeps_its_group() {
        // Past 64 logical processors Windows makes more groups; this core's threads are 62 and 63 of group 1.
        let (cores, _) = logical_processors(&core_record(0, &[(1, 0b11 << 62)]));
        assert_eq!(
            cores,
            vec![PhysicalCore {
                efficiency_class: 0,
                logical: vec![(1, 62), (1, 63)],
            }]
        );
    }

    #[test]
    fn a_record_cut_short_ends_the_list() {
        let mut buffer = core_record(0, &[(0, 0b11)]);
        buffer.extend(&cache_record(1, 48 * KB, 2)[..30]);
        let (cores, caches) = logical_processors(&buffer);
        assert_eq!(cores.len(), 1);
        assert!(caches.is_empty());
        // A record that claims no size would never move the walk on, so it ends it.
        let mut stuck = other_record(3, 8);
        stuck[4..8].copy_from_slice(&0u32.to_le_bytes());
        stuck.extend(core_record(0, &[(0, 0b1)]));
        assert_eq!(logical_processors(&stuck), (Vec::new(), Vec::new()));
        // A group count promising more masks than the record holds: only the masks inside it are read.
        let mut short = core_record(0, &[(0, 0b11)]);
        short[30..32].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(
            logical_processors(&short).0[0].logical,
            vec![(0, 0), (0, 1)]
        );
        assert_eq!(logical_processors(&[]), (Vec::new(), Vec::new()));
    }

    #[test]
    fn cache_types_become_kinds() {
        let mut buffer = Vec::new();
        for cache_type in 0..4 {
            buffer.extend(cache_record(1, 64 * KB, cache_type));
        }
        let kinds: Vec<CacheKind> = logical_processors(&buffer).1.iter().map(|c| c.1).collect();
        // 3 is a trace cache (Pentium 4), which the model has no name for.
        assert_eq!(
            kinds,
            vec![
                CacheKind::Unified,
                CacheKind::Instruction,
                CacheKind::Data,
                CacheKind::Unknown
            ]
        );
    }

    #[test]
    fn firmware_types_are_boot_modes() {
        assert_eq!(boot_mode(2), BootMode::Uefi);
        assert_eq!(boot_mode(1), BootMode::Legacy);
        assert_eq!(boot_mode(0), BootMode::Unknown);
        assert_eq!(boot_mode(3), BootMode::Unknown);
    }

    #[test]
    fn a_running_hypervisor_means_virtualization_is_on() {
        // A hypervisor (seen by CPUID, or VBS) takes VT-x / SVM for itself and hides the firmware flag.
        assert_eq!(virtualization(true, None, Some(false)), Some(true));
        assert_eq!(virtualization(false, Some(true), Some(false)), Some(true));
        // Otherwise the firmware flag says, and without it nothing is known.
        assert_eq!(virtualization(false, Some(false), Some(true)), Some(true));
        assert_eq!(virtualization(false, None, Some(false)), Some(false));
        assert_eq!(virtualization(false, None, None), None);
    }

    #[test]
    fn a_laptop_has_no_cpu_socket() {
        // Dragon Range (laptop) shares Raphael's signature, so the table says AM5; its chip is soldered on.
        assert_eq!(cpu_socket(FormFactor::Laptop, Some("AM5")), None);
        assert_eq!(
            cpu_socket(FormFactor::Desktop, Some("AM5")).as_deref(),
            Some("AM5")
        );
        // An unknown form factor (WMI couldn't tell) keeps the table's socket.
        assert_eq!(
            cpu_socket(FormFactor::Unknown, Some("LGA1200")).as_deref(),
            Some("LGA1200")
        );
        assert_eq!(cpu_socket(FormFactor::Desktop, None), None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-probe`
Expected: FAIL to compile — cannot find function `cpuid_text`, `logical_processors`, `boot_mode`, `virtualization`, `cpu_socket`; cannot find type `CacheKind`, `BootMode`; cannot find struct `PhysicalCore`.

- [ ] **Step 3: Implement**

`crates/kestrel-probe/Cargo.toml` — this file belongs to Task 9 in the File Structure, so all four new features go in now (features don't change `Cargo.lock`). Replace the windows-sys line with:
```toml
windows-sys = { version = "0.61.2", features = ["Win32_Foundation", "Win32_Graphics_Gdi", "Win32_System_SystemInformation", "Win32_System_Performance", "Win32_System_Diagnostics_ToolHelp", "Win32_System_Threading", "Win32_Devices_DeviceAndDriverInstallation", "Win32_Devices_Properties", "Win32_Devices_Display"] }
```

`crates/kestrel-probe/src/parse.rs` — replace the import line
```rust
use kestrel_core::{BusyProcess, CpuVendor, DriveMedia, FormFactor, NetKind};
```
with
```rust
use kestrel_core::topology::PhysicalCore;
use kestrel_core::{BootMode, BusyProcess, CacheKind, CpuVendor, DriveMedia, FormFactor, NetKind};
```
and add these after `exe_name`, above `#[cfg(test)]`:
```rust
/// CPUID registers that spell text → the text. Each register holds four characters, lowest byte first (the vendor
/// is leaf 0's EBX, EDX, ECX; the brand is EAX to EDX of leaves 0x80000002–4). NULs are dropped and the ends trimmed:
/// older Intel processors pad the brand with spaces.
pub fn cpuid_text(registers: &[u32]) -> String {
    let bytes: Vec<u8> = registers
        .iter()
        .flat_map(|register| register.to_le_bytes())
        .filter(|&byte| byte != 0)
        .collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// The two LOGICAL_PROCESSOR_RELATIONSHIP values Kestrel reads (winnt.h).
const RELATION_PROCESSOR_CORE: u32 = 0;
const RELATION_CACHE: u32 = 2;

/// What `GetLogicalProcessorInformationEx(RelationAll)` wrote: SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX records one
/// after another, each starting with its relationship (u32) and its size in bytes (u32) → the physical cores and one
/// (level, kind, size in KB) per cache, in Windows' order. Other relationships (packages, NUMA nodes, groups, dies)
/// are skipped. Offsets are those of 64-bit Windows (8-byte affinity masks). Nothing is read past the buffer: a
/// record that claims more room than is left, or no room at all, ends the list.
pub fn logical_processors(buffer: &[u8]) -> (Vec<PhysicalCore>, Vec<(u8, CacheKind, u32)>) {
    let (mut cores, mut caches) = (Vec::new(), Vec::new());
    let mut at = 0;
    while let (Some(relationship), Some(size)) = (le_u32(buffer, at), le_u32(buffer, at + 4)) {
        let size = size as usize;
        let Some(record) = at
            .checked_add(size)
            .and_then(|end| buffer.get(at..end))
            .filter(|_| size >= 8)
        else {
            break;
        };
        match relationship {
            RELATION_PROCESSOR_CORE => cores.extend(processor_core(record)),
            RELATION_CACHE => caches.extend(cache(record)),
            _ => {}
        }
        at += size;
    }
    (cores, caches)
}

/// A RelationProcessorCore record: EfficiencyClass (u8) at 9, GroupCount (u16) at 30, then GroupCount
/// GROUP_AFFINITY of 16 bytes from 32, each a Mask (u64) and its Group (u16). Every mask bit is a logical processor.
fn processor_core(record: &[u8]) -> Option<PhysicalCore> {
    let efficiency_class = *record.get(9)?;
    let mut logical = Vec::new();
    for index in 0..usize::from(le_u16(record, 30)?) {
        let affinity = 32 + 16 * index;
        let (Some(mask), Some(group)) = (le_u64(record, affinity), le_u16(record, affinity + 8))
        else {
            break;
        };
        logical.extend(
            (0..64u8)
                .filter(|bit| mask & (1u64 << bit) != 0)
                .map(|bit| (group, bit)),
        );
    }
    (!logical.is_empty()).then_some(PhysicalCore {
        efficiency_class,
        logical,
    })
}

/// A RelationCache record: Level (u8) at 8, CacheSize (u32, bytes) at 12, Type (u32) at 16: 0 unified,
/// 1 instruction, 2 data. 3 is a Pentium 4 trace cache, which the model has no kind for.
fn cache(record: &[u8]) -> Option<(u8, CacheKind, u32)> {
    let kind = match le_u32(record, 16)? {
        0 => CacheKind::Unified,
        1 => CacheKind::Instruction,
        2 => CacheKind::Data,
        _ => CacheKind::Unknown,
    };
    Some((*record.get(8)?, kind, le_u32(record, 12)? / 1024))
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// `GetFirmwareType`'s FIRMWARE_TYPE: 1 is BIOS (legacy boot, CSM), 2 is UEFI; 0 (unknown) and anything else unknown.
pub fn boot_mode(firmware_type: i32) -> BootMode {
    match firmware_type {
        1 => BootMode::Legacy,
        2 => BootMode::Uefi,
        _ => BootMode::Unknown,
    }
}

/// Whether VT-x / SVM is on. A running hypervisor (CPUID's hypervisor bit, or VBS) uses it and hides the firmware
/// flag, so either one means on; otherwise the firmware flag (`PF_VIRT_FIRMWARE_ENABLED`) says.
pub fn virtualization(
    hypervisor: bool,
    vbs_running: Option<bool>,
    firmware_enabled: Option<bool>,
) -> Option<bool> {
    if hypervisor || vbs_running == Some(true) {
        Some(true)
    } else {
        firmware_enabled
    }
}

/// The socket to show: the codename table names the desktop part's, and a laptop chip that shares the desktop's
/// CPUID signature (Dragon Range shares Raphael's) is soldered to the board, so a laptop has none.
pub fn cpu_socket(form_factor: FormFactor, table_socket: Option<&str>) -> Option<String> {
    if form_factor == FormFactor::Laptop {
        None
    } else {
        table_socket.map(str::to_string)
    }
}
```

The layout `logical_processors` reads (64-bit Windows): every record starts with `Relationship` (u32) and `Size` (u32), and its data follows from byte 8; affinity masks are 8 bytes. A core record has `Flags` at 8, `EfficiencyClass` at 9, `GroupCount` at 30 and `GroupCount` × 16-byte `GROUP_AFFINITY` (`Mask` u64, `Group` u16, three reserved u16) from 32; a cache record has `Level` at 8, `CacheSize` in bytes at 12 and `Type` at 16. `win/cpu.rs` asserts these offsets against windows-sys's own structs at compile time, so the Windows build breaks if they ever disagree.

Create `crates/kestrel-probe/src/win/cpu.rs`:
```rust
//! The processor and two firmware facts read with it: CPUID (identity, instruction sets, a running hypervisor), the
//! cores and caches (`GetLogicalProcessorInformationEx`), the base clock when CPUID has none (PDH), the boot mode
//! (`GetFirmwareType`) and whether virtualization is on. All user mode: CPUID and XGETBV are ordinary instructions
//! that only read the processor's identification, and the rest only ask Windows.
use super::{pdh, problem};
use crate::parse;
use kestrel_core::cpu::{self, CpuidRaw};
use kestrel_core::topology::{self, PhysicalCore};
use kestrel_core::{CacheKind, ProbeError, SystemSnapshot};
use windows_sys::Win32::System::SystemInformation::{
    CacheData, CacheInstruction, CacheUnified, FirmwareTypeUnknown, GetFirmwareType,
    GetLogicalProcessorInformationEx, RelationAll, RelationCache, RelationProcessorCore,
    CACHE_RELATIONSHIP, FIRMWARE_TYPE, GROUP_AFFINITY, PROCESSOR_RELATIONSHIP,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::{IsProcessorFeaturePresent, PF_VIRT_FIRMWARE_ENABLED};

// `parse::logical_processors` reads the records byte by byte at these offsets (64-bit Windows); this keeps them true.
const _: () = {
    use std::mem::offset_of;
    assert!(offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Anonymous) == 8);
    assert!(offset_of!(PROCESSOR_RELATIONSHIP, EfficiencyClass) == 1);
    assert!(offset_of!(PROCESSOR_RELATIONSHIP, GroupCount) == 22);
    assert!(offset_of!(PROCESSOR_RELATIONSHIP, GroupMask) == 24);
    assert!(size_of::<GROUP_AFFINITY>() == 16 && offset_of!(GROUP_AFFINITY, Group) == 8);
    assert!(offset_of!(CACHE_RELATIONSHIP, CacheSize) == 4);
    assert!(offset_of!(CACHE_RELATIONSHIP, Type) == 8);
    assert!(RelationProcessorCore == 0 && RelationCache == 2);
    assert!(CacheUnified == 0 && CacheInstruction == 1 && CacheData == 2);
};

/// Fills the processor's identity, cores, caches, clocks and virtualization, and the board's boot mode. Runs after
/// WMI: a laptop (the machine query) gets no socket, and virtualization counts VBS (the device-guard query).
pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    let mut hypervisor = false;
    match read_cpuid() {
        Some(raw) => {
            let id = cpu::decode(&raw);
            hypervisor = id.hypervisor;
            if let Some(codename) = cpu::codename(id.vendor, id.family, id.model, id.stepping) {
                s.cpu.codename = Some(codename.name.to_string());
                s.cpu.socket = parse::cpu_socket(s.machine.form_factor, codename.socket);
            }
            s.cpu.family = Some(id.family);
            s.cpu.model = Some(id.model);
            s.cpu.stepping = Some(id.stepping);
            s.cpu.base_mhz = id.base_mhz;
            s.cpu.max_mhz = id.max_mhz;
            s.cpu.instructions = id.instructions;
        }
        None => errors.push(problem("cpu-id", "CPUID is read only on x86-64 processors")),
    }
    match topology() {
        Some((cores, caches)) => {
            if let Some((p_cores, e_cores)) = topology::p_e_split(&cores) {
                s.cpu.p_cores = Some(p_cores);
                s.cpu.e_cores = Some(e_cores);
            }
            s.cpu.caches = topology::group_caches(&caches);
        }
        None => errors.push(problem(
            "topology",
            "Windows didn't list the processor's cores and caches (GetLogicalProcessorInformationEx)",
        )),
    }
    // CPUID gives the base clock only on Intel (leaf 0x16); otherwise Windows' rated frequency stands in.
    if s.cpu.base_mhz.is_none() {
        s.cpu.base_mhz = rated_mhz();
    }
    match firmware_type() {
        Some(firmware) => s.board.boot_mode = parse::boot_mode(firmware),
        None => errors.push(problem(
            "firmware",
            "couldn't read the firmware type (GetFirmwareType)",
        )),
    }
    // SAFETY: only asks whether a processor feature is on; nothing is passed or written.
    let firmware_flag = unsafe { IsProcessorFeaturePresent(PF_VIRT_FIRMWARE_ENABLED) } != 0;
    s.cpu.virtualization =
        parse::virtualization(hypervisor, s.security.vbs_running, Some(firmware_flag));
}

/// The CPUID leaves `kestrel_core::cpu::decode` reads.
#[cfg(target_arch = "x86_64")]
fn read_cpuid() -> Option<CpuidRaw> {
    use std::arch::x86_64::{__cpuid, __cpuid_count, CpuidResult};
    fn registers(r: CpuidResult) -> [u32; 4] {
        [r.eax, r.ebx, r.ecx, r.edx]
    }
    let leaf0 = __cpuid(0);
    let max_leaf = leaf0.eax;
    let leaf1 = if max_leaf >= 1 {
        registers(__cpuid(1))
    } else {
        [0; 4]
    };
    let leaf7 = if max_leaf >= 7 {
        registers(__cpuid_count(7, 0))
    } else {
        [0; 4]
    };
    let leaf7_1_eax = if max_leaf >= 7 && leaf7[0] >= 1 {
        __cpuid_count(7, 1).eax
    } else {
        0
    };
    let leaf16 = (max_leaf >= 0x16).then(|| {
        let r = __cpuid(0x16);
        [r.eax, r.ebx, r.ecx]
    });
    let brand = if __cpuid(0x8000_0000).eax >= 0x8000_0004 {
        let parts: Vec<u32> = (0x8000_0002..=0x8000_0004u32)
            .flat_map(|leaf| registers(__cpuid(leaf)))
            .collect();
        parse::cpuid_text(&parts)
    } else {
        String::new()
    };
    // XGETBV exists only once the OS has turned XSAVE on, which leaf 1 reports as OSXSAVE (ECX bit 27).
    let osxsave = leaf1[2] & (1 << 27) != 0;
    // SAFETY: `xcr0` needs XSAVE, and OSXSAVE set means the processor has it and the OS turned it on.
    let xcr0 = osxsave.then(|| unsafe { xcr0() });
    Some(CpuidRaw {
        vendor: parse::cpuid_text(&[leaf0.ebx, leaf0.edx, leaf0.ecx]),
        max_leaf,
        leaf1,
        leaf7,
        leaf7_1_eax,
        leaf16,
        brand,
        xcr0,
    })
}

/// Off x86-64 there is no CPUID to read.
#[cfg(not(target_arch = "x86_64"))]
fn read_cpuid() -> Option<CpuidRaw> {
    None
}

/// XCR0: which register sets (SSE, AVX, AVX-512, AMX) the OS saves when it switches threads. Compiled with XSAVE
/// enabled, so it may only be called where the processor has XSAVE and the OS turned it on (OSXSAVE).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "xsave")]
fn xcr0() -> u64 {
    // SAFETY: XGETBV with ECX = 0 only reads XCR0, and this function runs only where XGETBV exists (see above).
    unsafe { std::arch::x86_64::_xgetbv(0) }
}

/// The physical cores, and every cache as (level, kind, size in KB).
type Topology = (Vec<PhysicalCore>, Vec<(u8, CacheKind, u32)>);

/// The physical cores and every cache, from `GetLogicalProcessorInformationEx(RelationAll)`. None when Windows doesn't
/// answer or lists no core. The live clocks use the cores too.
pub(crate) fn topology() -> Option<Topology> {
    let mut needed = 0u32;
    // SAFETY: a null buffer with a length of 0 only asks how many bytes the answer needs; nothing is written.
    unsafe { GetLogicalProcessorInformationEx(RelationAll, std::ptr::null_mut(), &mut needed) };
    // A processor added between two calls makes the answer outgrow the buffer; Windows then says the new size.
    for _ in 0..3 {
        if needed == 0 {
            return None;
        }
        // u64s, so the records (which hold 8-byte affinity masks) are aligned as Windows expects.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        let room = u32::try_from(size_of_val(buffer.as_slice())).ok()?;
        let mut length = room;
        // SAFETY: `buffer` is 8-byte aligned and `room` bytes long, which is what `length` tells Windows.
        let ok = unsafe {
            GetLogicalProcessorInformationEx(RelationAll, buffer.as_mut_ptr().cast(), &mut length)
        } != 0;
        if ok {
            let bytes: Vec<u8> = buffer
                .iter()
                .flat_map(|word| word.to_ne_bytes())
                .take(length as usize)
                .collect();
            let (cores, caches) = parse::logical_processors(&bytes);
            return (!cores.is_empty()).then_some((cores, caches));
        }
        if length <= room {
            return None; // it failed for another reason than room
        }
        needed = length;
    }
    None
}

/// The processor's rated clock in MHz from PDH's `Processor Frequency`, which Windows keeps at the rated (base)
/// clock; the live clock is that times `% Processor Performance`. It isn't a rate, so one sample is enough.
fn rated_mhz() -> Option<u32> {
    let query = pdh::Query::open(r"\Processor Information(_Total)\Processor Frequency")?;
    if !query.collect() {
        return None;
    }
    let (_, mhz) = query.formatted_array(0)?.into_iter().next()?;
    (mhz >= 1.0).then(|| mhz.round() as u32)
}

fn firmware_type() -> Option<FIRMWARE_TYPE> {
    let mut firmware: FIRMWARE_TYPE = FirmwareTypeUnknown;
    // SAFETY: `firmware` is a valid place for the answer.
    (unsafe { GetFirmwareType(&mut firmware) } != 0).then_some(firmware)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_lists_each_logical_processor_once() {
        let (cores, caches) = topology().expect("Windows lists the processor's cores");
        let mut logical: Vec<(u16, u8)> = cores.iter().flat_map(|c| c.logical.clone()).collect();
        let count = logical.len();
        logical.sort_unstable();
        logical.dedup();
        assert_eq!(logical.len(), count, "{cores:?}");
        assert!(!caches.is_empty());
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn cpuid_names_the_vendor() {
        let raw = read_cpuid().expect("x86-64 processors have CPUID");
        assert!(raw.max_leaf >= 1, "{raw:?}");
        assert!(!raw.vendor.is_empty(), "{raw:?}");
    }
}
```

Why it is written this way:
- `__cpuid` and `__cpuid_count` are safe functions in current Rust; wrapping them in `unsafe` fails clippy (`unused_unsafe`). `_xgetbv` is still `unsafe` and only exists with the `xsave` target feature, so it sits in `xcr0`, a safe `#[target_feature(enable = "xsave")]` function. Calling such a function from code compiled without the feature is `unsafe`, and the one call is behind the OSXSAVE check (leaf 1 ECX bit 27: the processor has XSAVE and the OS turned it on), with its SAFETY comment. std's own feature detection makes the same OSXSAVE check before it calls `_xgetbv`.
- The answer buffer is a `Vec<u64>` so the records (8-byte masks) are aligned as Windows expects; it is copied to bytes for the pure parser, which uses bounds-checked little-endian reads and never reads past the end. A processor added between the two calls makes Windows report a bigger size, so the call is retried up to three times.
- `Processor Frequency` is an instantaneous counter (the rated clock), so one `collect()` is enough; it is read only when CPUID gave no base clock (AMD, or Intel without leaf 0x16).
- `CpuInfo.name`, `vendor`, `cores` and `threads` still come from `Win32_Processor`; the codename lookup uses CPUID's own vendor.
- `cpu::fill` runs after WMI: a laptop (`machine.form_factor`) gets no socket, because the codename table gives the desktop socket even for a laptop chip with the same CPUID signature (Dragon Range under Raphael's AM5); the codename is kept. Virtualization counts `security.vbs_running` from the device-guard query.

`crates/kestrel-probe/src/win/mod.rs` — replace
```rust
//! Real probes for Windows. Each one is independent: a failure becomes `None` plus a ProbeError.
mod display;
```
with
```rust
//! Real probes for Windows. Each one is independent: a failure becomes `None` plus a ProbeError.
mod cpu;
mod display;
```
and in `collect_here` replace
```rust
        Err(e) => errors.push(problem("wmi", e)),
    }
    registry_probe::fill(&mut s, &mut errors);
```
with
```rust
        Err(e) => errors.push(problem("wmi", e)),
    }
    // After WMI: the socket depends on the form factor (machine query), virtualization on VBS (device-guard query).
    cpu::fill(&mut s, &mut errors);
    registry_probe::fill(&mut s, &mut errors);
```

- [ ] **Step 4: Verify**

Run:
```bash
cargo test -p kestrel-probe
cargo fmt --all --check
cargo clippy -p kestrel-probe --all-targets -- -D warnings
cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```
Expected: the 10 new tests pass (32 unit tests in the lib target if Tasks 1–8 added none to kestrel-probe; `tests/smoke.rs` 2 passed on the Mac); fmt prints nothing; both clippy runs finish with no warning. The Windows run compiles `win/cpu.rs` with its layout assertions and its two Windows-only tests (`windows_lists_each_logical_processor_once`, `cpuid_names_the_vendor`), which CI's Windows job runs.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-probe/Cargo.toml crates/kestrel-probe/src/parse.rs crates/kestrel-probe/src/win/cpu.rs crates/kestrel-probe/src/win/mod.rs
git commit -m "feat(probe): CPUID identity, processor topology, boot mode and virtualization" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Probe — PCIe link, Resizable BAR and chipset from Configuration Manager; `gpu_link()`

**Files:**
- Create: `crates/kestrel-probe/src/win/cfgmgr.rs`
- Modify: `crates/kestrel-probe/src/parse.rs`, `crates/kestrel-probe/src/lib.rs`, `crates/kestrel-probe/src/win/mod.rs`
- Test: `crates/kestrel-probe/src/parse.rs` (tests module); `crates/kestrel-probe/src/lib.rs` (new tests module, one test per platform)

**Interfaces:**
- Consumes:
  - Task 1: `GpuInfo.pnp_device_id` (Task 1 already stores `Win32_VideoController.PNPDeviceID` there in `win/wmi_probe.rs` `gpus()`; this task doesn't touch `gpus()`), `GpuInfo.{pcie, largest_bar_mb, resizable_bar}`, `BoardInfo.chipset`, `kestrel_core::{PcieLink, PcieInfo}`.
  - Task 4: `kestrel_core::pcie::ROOT_PORT`, `pcie::PciNode { instance_id, parent, device_type, current, max }`, `pcie::link_from_codes(speed: Option<u32>, width: Option<u32>) -> Option<PcieLink>`, `pcie::slot_device<'a>(nodes: &'a [PciNode], gpu_instance_id: &str) -> Option<&'a PciNode>`, `pcie::resolve(nodes: &[PciNode], gpu_instance_id: &str) -> Option<PcieInfo>`, `pcie::bar_summary(range_bytes: &[u64]) -> (Option<u64>, Option<bool>)`.
  - Task 5: `kestrel_core::hardware::chipset(board_product: &str, lpc_description: Option<&str>) -> Option<String>`.
  - Task 9: the windows-sys features; `parse.rs`'s private `le_u64`.
- Produces:
  - `kestrel_probe::gpu_link(pnp_device_id: &str) -> Option<kestrel_core::PcieLink>` (Task 14's `Platform::gpu_link`)
  - `parse::{DEVPROP_BYTE, DEVPROP_UINT32, DEVPROP_STRING}: u32`, `parse::device_property_u32(property_type: u32, data: &[u8]) -> Option<u32>`, `parse::device_property_byte(property_type: u32, data: &[u8]) -> Option<u8>`, `parse::device_property_text(property_type: u32, data: &[u8]) -> Option<String>`, `parse::allocated_range_bytes(data: &[u8]) -> Option<u64>`, `parse::multi_sz(units: &[u16]) -> Vec<String>`, `parse::is_pci_instance(instance_id: &str) -> bool`
  - `win::cfgmgr::fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>)` and `win::cfgmgr::gpu_link(pnp_device_id: &str) -> Option<PcieLink>`
  - Probe errors `pcie` (a PCI card Configuration Manager can't find, or whose memory ranges can't be read) and `chipset` (Windows can't list the PCI devices)

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module at the end of `crates/kestrel-probe/src/parse.rs`:
```rust
    #[test]
    fn device_properties_are_read_only_with_their_expected_type() {
        assert_eq!(device_property_u32(DEVPROP_UINT32, &[4, 0, 0, 0]), Some(4));
        assert_eq!(
            device_property_u32(DEVPROP_UINT32, &[16, 0, 0, 0]),
            Some(16)
        );
        // Another type or size is not a u32, whatever its bytes say.
        assert_eq!(device_property_u32(DEVPROP_BYTE, &[4, 0, 0, 0]), None);
        assert_eq!(device_property_u32(DEVPROP_UINT32, &[4, 0]), None);
        assert_eq!(device_property_byte(DEVPROP_BYTE, &[0x06]), Some(0x06));
        assert_eq!(device_property_byte(DEVPROP_UINT32, &[0x06, 0, 0, 0]), None);
        assert_eq!(device_property_byte(DEVPROP_BYTE, &[]), None);
    }

    #[test]
    fn device_property_text_is_utf16_up_to_its_nul() {
        let bytes: Vec<u8> = "Intel(R) Z590 LPC/eSPI Controller - 4385\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            device_property_text(DEVPROP_STRING, &bytes).as_deref(),
            Some("Intel(R) Z590 LPC/eSPI Controller - 4385")
        );
        assert_eq!(device_property_text(DEVPROP_UINT32, &bytes), None);
        assert_eq!(device_property_text(DEVPROP_STRING, &[0, 0]), None);
    }

    /// The 32-byte header of a memory resource descriptor (MEM_DES or MEM_LARGE_DES): Count, Type, Alloc_Base,
    /// Alloc_End, Flags, Reserved.
    fn memory_header(base: u64, end: u64) -> Vec<u8> {
        let mut header = vec![0u8; 32];
        header[8..16].copy_from_slice(&base.to_le_bytes());
        header[16..24].copy_from_slice(&end.to_le_bytes());
        header
    }

    #[test]
    fn an_allocated_range_runs_from_its_base_to_its_end_inclusive() {
        // A card's 16 MB of registers, the 256 MB window it gets without Resizable BAR, and 8 GB with it.
        assert_eq!(
            allocated_range_bytes(&memory_header(0xF600_0000, 0xF6FF_FFFF)),
            Some(16 << 20)
        );
        assert_eq!(
            allocated_range_bytes(&memory_header(0xE000_0000, 0xEFFF_FFFF)),
            Some(256 << 20)
        );
        assert_eq!(
            allocated_range_bytes(&memory_header(0x40_0000_0000, 0x41_FFFF_FFFF)),
            Some(8 << 30)
        );
        // Data cut before the end address, a range that ends before it starts, and one with no size that fits.
        assert_eq!(
            allocated_range_bytes(&memory_header(0xE000_0000, 0xEFFF_FFFF)[..20]),
            None
        );
        assert_eq!(allocated_range_bytes(&memory_header(0x2000, 0x1FFF)), None);
        assert_eq!(allocated_range_bytes(&memory_header(0, u64::MAX)), None);
    }

    #[test]
    fn a_device_id_list_splits_at_each_nul() {
        let list: Vec<u16> =
            "PCI\\VEN_8086&DEV_4385\\3&11583659&0&F8\0PCI\\VEN_10DE&DEV_2489\\4&2B5E7C3&0&0008\0\0"
                .encode_utf16()
                .collect();
        assert_eq!(
            multi_sz(&list),
            vec![
                "PCI\\VEN_8086&DEV_4385\\3&11583659&0&F8",
                "PCI\\VEN_10DE&DEV_2489\\4&2B5E7C3&0&0008"
            ]
        );
        assert!(multi_sz(&[0]).is_empty());
        assert!(multi_sz(&[]).is_empty());
    }

    #[test]
    fn only_pci_instance_ids_are_pci_devices() {
        assert!(is_pci_instance(
            "PCI\\VEN_10DE&DEV_2489&SUBSYS_88131043&REV_A1\\4&2B5E7C3&0&0008"
        ));
        assert!(is_pci_instance(
            "pci\\ven_1002&dev_73bf\\6&1a2b3c4d&0&00000019"
        ));
        // Virtual adapters, Hyper-V's video, the PCI root bus itself and an IDE channel are not.
        for id in [
            "ROOT\\DISPLAY\\0000",
            "ROOT\\BASICDISPLAY\\0000",
            "VMBUS\\{da0a7802-e377-4aac-8e77-0558eb1073f8}\\{5620e0c7-8062-4dce-aeb7-520c7ef76171}",
            "ACPI\\PNP0A08\\0",
            "PCIIDE\\IDECHANNEL\\4&1&0&0",
            "",
        ] {
            assert!(!is_pci_instance(id), "{id}");
        }
    }
```

Append to the end of `crates/kestrel-probe/src/lib.rs`:
```rust
#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn the_link_is_not_read_off_windows() {
        let id = "PCI\\VEN_10DE&DEV_2489&SUBSYS_88131043&REV_A1\\4&2B5E7C3&0&0008";
        assert_eq!(super::gpu_link(id), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_card_windows_does_not_list_has_no_link() {
        assert_eq!(
            super::gpu_link("PCI\\VEN_FFFF&DEV_FFFF\\0&KESTREL&0&0000"),
            None
        );
        assert_eq!(super::gpu_link("ROOT\\DISPLAY\\0000"), None);
        assert_eq!(super::gpu_link(""), None);
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-probe`
Expected: FAIL to compile — cannot find function `device_property_u32`, `device_property_byte`, `device_property_text`, `allocated_range_bytes`, `multi_sz`, `is_pci_instance`; cannot find value `DEVPROP_UINT32`, `DEVPROP_BYTE`, `DEVPROP_STRING`; cannot find function `gpu_link` in module `super`.

- [ ] **Step 3: Implement**

`crates/kestrel-probe/src/parse.rs` — add after `cpu_socket`, above `#[cfg(test)]`:
```rust
/// DEVPROPTYPE values (devpropdef.h) of the device properties Kestrel reads; `win::cfgmgr` checks them against
/// windows-sys.
pub const DEVPROP_BYTE: u32 = 3;
pub const DEVPROP_UINT32: u32 = 7;
pub const DEVPROP_STRING: u32 = 18;

/// A device property stored as DEVPROP_TYPE_UINT32 (four bytes) → its value; None for any other type or size.
pub fn device_property_u32(property_type: u32, data: &[u8]) -> Option<u32> {
    if property_type != DEVPROP_UINT32 {
        return None;
    }
    Some(u32::from_le_bytes(data.try_into().ok()?))
}

/// A device property stored as DEVPROP_TYPE_BYTE (one byte) → its value; None for any other type or size.
pub fn device_property_byte(property_type: u32, data: &[u8]) -> Option<u8> {
    match data {
        [byte] if property_type == DEVPROP_BYTE => Some(*byte),
        _ => None,
    }
}

/// A device property stored as DEVPROP_TYPE_STRING (UTF-16, little-endian, NUL-terminated) → the text; None for any
/// other type or an empty text.
pub fn device_property_text(property_type: u32, data: &[u8]) -> Option<String> {
    if property_type != DEVPROP_STRING {
        return None;
    }
    let (pairs, _) = data.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    Some(exe_name(&units).trim().to_string()).filter(|text| !text.is_empty())
}

/// The data of a memory resource descriptor (`CM_Get_Res_Des_Data` for ResType_Mem or ResType_MemLarge) starts with
/// a MEM_DES or MEM_LARGE_DES header, both laid out the same: Count (u32), Type (u32), Alloc_Base (u64) at 8 and
/// Alloc_End (u64, inclusive) at 16 → the allocated range's size in bytes, End − Base + 1. None when the data is too
/// short or the range impossible.
pub fn allocated_range_bytes(data: &[u8]) -> Option<u64> {
    let (base, end) = (le_u64(data, 8)?, le_u64(data, 16)?);
    end.checked_sub(base)?.checked_add(1)
}

/// NUL-terminated UTF-16 strings one after another, ended by an empty one (what `CM_Get_Device_ID_ListW` writes) →
/// the strings.
pub fn multi_sz(units: &[u16]) -> Vec<String> {
    units
        .split(|&unit| unit == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Whether a device instance id ("PCI\VEN_10DE&DEV_2489&…\4&2B5E7C3&0&0008") names a device on the PCI bus.
pub fn is_pci_instance(instance_id: &str) -> bool {
    instance_id
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("PCI\\"))
}
```

Create `crates/kestrel-probe/src/win/cfgmgr.rs`:
```rust
//! Configuration Manager reads: the graphics card's PCIe link (its PCI device and the bridges above it), the memory
//! ranges Windows gave the card (Resizable BAR), and the Intel LPC/eSPI controller's name (the chipset). Read-only:
//! every call reads a device property or a list Windows keeps; nothing is changed and no driver is involved.
use super::problem;
use crate::parse;
use kestrel_core::pcie::{self, PciNode};
use kestrel_core::{hardware, PcieLink, ProbeError, SystemSnapshot};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Free_Log_Conf_Handle, CM_Free_Res_Des_Handle, CM_Get_DevNode_PropertyW, CM_Get_Device_IDW,
    CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW, CM_Get_First_Log_Conf,
    CM_Get_Next_Res_Des, CM_Get_Parent, CM_Get_Res_Des_Data, CM_Get_Res_Des_Data_Size,
    CM_Locate_DevNodeW, ResType_Mem, ResType_MemLarge, ALLOC_LOG_CONF,
    CM_GETIDLIST_FILTER_ENUMERATOR, CM_GETIDLIST_FILTER_PRESENT, CM_LOCATE_DEVNODE_NORMAL,
    CM_RESTYPE, CR_BUFFER_SMALL, CR_NO_MORE_LOG_CONF, CR_SUCCESS, MAX_DEVICE_ID_LEN, MEM_DES,
    MEM_LARGE_DES,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_Device_DeviceDesc, DEVPROPTYPE, DEVPROP_TYPE_BYTE, DEVPROP_TYPE_STRING,
    DEVPROP_TYPE_UINT32,
};
use windows_sys::Win32::Foundation::DEVPROPKEY;

/// The PCI device property set (pciprop.h). windows-sys doesn't export its keys, so they are declared here.
const PCI_DEVICE: GUID = GUID::from_u128(0x3ab22e31_8264_4b4e_9af5_a8d2d8e33e62);

const fn pci_key(pid: u32) -> DEVPROPKEY {
    DEVPROPKEY {
        fmtid: PCI_DEVICE,
        pid,
    }
}

/// UINT32: what kind of PCI device or bridge this is (`pcie::ROOT_PORT` and the switch ports among them).
const DEVICE_TYPE: DEVPROPKEY = pci_key(1);
/// BYTE: the class code's base class and subclass (06/01 is an ISA bridge, which is how LPC/eSPI shows).
const BASE_CLASS: DEVPROPKEY = pci_key(3);
const SUB_CLASS: DEVPROPKEY = pci_key(4);
/// UINT32: link speed codes (1–6 = PCIe 1.0–6.0) and widths (lanes), now and at most.
const CURRENT_LINK_SPEED: DEVPROPKEY = pci_key(9);
const CURRENT_LINK_WIDTH: DEVPROPKEY = pci_key(10);
const MAX_LINK_SPEED: DEVPROPKEY = pci_key(11);
const MAX_LINK_WIDTH: DEVPROPKEY = pci_key(12);

/// The walk reads the card and at most this many PCI devices above it (a switch inside the card is two of them).
const MAX_ANCESTORS: usize = 8;

// The pure decoders in `parse` use their own copies of these numbers and offsets; this keeps them true.
const _: () = {
    use std::mem::offset_of;
    assert!(parse::DEVPROP_BYTE == DEVPROP_TYPE_BYTE);
    assert!(parse::DEVPROP_UINT32 == DEVPROP_TYPE_UINT32);
    assert!(parse::DEVPROP_STRING == DEVPROP_TYPE_STRING);
    assert!(offset_of!(MEM_DES, MD_Alloc_Base) == 8 && offset_of!(MEM_DES, MD_Alloc_End) == 16);
    assert!(
        offset_of!(MEM_LARGE_DES, MLD_Alloc_Base) == 8
            && offset_of!(MEM_LARGE_DES, MLD_Alloc_End) == 16
    );
};

/// Each graphics card's PCIe link and Resizable BAR, and the board's chipset. Runs after WMI, which lists the cards
/// with their PNPDeviceID and reads the board's name.
pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    for gpu in &mut s.gpus {
        // Virtual adapters (ROOT\…, SWD\…, VMBUS\…) and cards WMI gave no id for have no PCI link to read.
        if !parse::is_pci_instance(&gpu.pnp_device_id) {
            continue;
        }
        let Some(node) = locate(&gpu.pnp_device_id) else {
            errors.push(problem(
                "pcie",
                format!("couldn't find {} among Windows' devices", gpu.name),
            ));
            continue;
        };
        gpu.pcie = pcie::resolve(&chain(node, &gpu.pnp_device_id), &gpu.pnp_device_id);
        match memory_ranges(node) {
            Some(ranges) => (gpu.largest_bar_mb, gpu.resizable_bar) = pcie::bar_summary(&ranges),
            None => errors.push(problem(
                "pcie",
                format!("couldn't read the memory ranges Windows gave {}", gpu.name),
            )),
        }
    }
    let description = lpc_description().unwrap_or_else(|e| {
        errors.push(problem("chipset", e));
        None
    });
    // The board's name is enough on its own, so the chipset is worked out even without the controller's name.
    s.board.chipset = hardware::chipset(&s.board.product, description.as_deref());
}

/// The current link of the card's slot device. None when the card isn't a PCI device Windows lists, or its link
/// isn't reported (a card asleep, a virtual machine).
pub fn gpu_link(pnp_device_id: &str) -> Option<PcieLink> {
    if !parse::is_pci_instance(pnp_device_id) {
        return None;
    }
    let nodes = chain(locate(pnp_device_id)?, pnp_device_id);
    pcie::slot_device(&nodes, pnp_device_id)?.current.clone()
}

/// The card's node and its PCI ancestors up to the first root port (at most MAX_ANCESTORS), each with its type and
/// links.
fn chain(card: u32, card_id: &str) -> Vec<PciNode> {
    let mut nodes = Vec::new();
    let mut next = Some((card, card_id.to_string()));
    while let Some((node, instance_id)) = next.take() {
        let parent = parent_of(node);
        let device_type = property_u32(node, &DEVICE_TYPE);
        nodes.push(PciNode {
            instance_id,
            parent: parent.as_ref().map(|(_, id)| id.clone()),
            device_type,
            current: pcie::link_from_codes(
                property_u32(node, &CURRENT_LINK_SPEED),
                property_u32(node, &CURRENT_LINK_WIDTH),
            ),
            max: pcie::link_from_codes(
                property_u32(node, &MAX_LINK_SPEED),
                property_u32(node, &MAX_LINK_WIDTH),
            ),
        });
        if device_type == Some(pcie::ROOT_PORT) || nodes.len() > MAX_ANCESTORS {
            break;
        }
        next = parent.filter(|(_, id)| parse::is_pci_instance(id));
    }
    nodes
}

/// The device node of a device instance id, if that device is present.
fn locate(instance_id: &str) -> Option<u32> {
    let id = wide(instance_id);
    let mut node = 0u32;
    // SAFETY: `id` is NUL-terminated and outlives the call; `node` is a valid place for the handle.
    let status = unsafe { CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) };
    (status == CR_SUCCESS).then_some(node)
}

/// The parent device's node and instance id.
fn parent_of(node: u32) -> Option<(u32, String)> {
    let mut parent = 0u32;
    // SAFETY: `parent` is a valid place for the handle.
    if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
        return None;
    }
    Some((parent, device_id(parent)?))
}

fn device_id(node: u32) -> Option<String> {
    // An instance id is at most MAX_DEVICE_ID_LEN characters, plus its NUL.
    let mut buffer = [0u16; MAX_DEVICE_ID_LEN as usize + 1];
    let room = buffer.len() as u32;
    // SAFETY: `buffer` is writable for the `room` UTF-16 units passed with it.
    let status = unsafe { CM_Get_Device_IDW(node, buffer.as_mut_ptr(), room, 0) };
    if status != CR_SUCCESS {
        return None;
    }
    Some(parse::exe_name(&buffer)).filter(|id| !id.is_empty())
}

/// A device property's type and bytes, if the device has it and it fits in `room` bytes.
fn property(node: u32, key: &DEVPROPKEY, room: u32) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    let mut property_type: DEVPROPTYPE = 0;
    let mut data = vec![0u8; room as usize];
    let mut size = room;
    // SAFETY: `key` and `property_type` are valid, and `data` is writable for the `size` bytes passed with it.
    let status = unsafe {
        CM_Get_DevNode_PropertyW(
            node,
            key,
            &mut property_type,
            data.as_mut_ptr(),
            &mut size,
            0,
        )
    };
    if status != CR_SUCCESS {
        return None;
    }
    data.truncate(size as usize);
    Some((property_type, data))
}

fn property_u32(node: u32, key: &DEVPROPKEY) -> Option<u32> {
    let (property_type, data) = property(node, key, 4)?;
    parse::device_property_u32(property_type, &data)
}

fn property_byte(node: u32, key: &DEVPROPKEY) -> Option<u8> {
    let (property_type, data) = property(node, key, 1)?;
    parse::device_property_byte(property_type, &data)
}

fn property_text(node: u32, key: &DEVPROPKEY) -> Option<String> {
    let mut property_type: DEVPROPTYPE = 0;
    let mut size = 0u32;
    // SAFETY: a null buffer with a size of 0 only asks how many bytes the value needs; nothing is written.
    let status = unsafe {
        CM_Get_DevNode_PropertyW(
            node,
            key,
            &mut property_type,
            std::ptr::null_mut(),
            &mut size,
            0,
        )
    };
    if status != CR_BUFFER_SMALL {
        return None;
    }
    let (property_type, data) = property(node, key, size)?;
    parse::device_property_text(property_type, &data)
}

/// The size in bytes of every memory range Windows allocated to the device (its allocated configuration's memory and
/// large-memory descriptors). Empty when it has none; None when they can't be read.
fn memory_ranges(node: u32) -> Option<Vec<u64>> {
    let mut config = 0usize;
    // SAFETY: `config` is a valid place for the configuration's handle.
    match unsafe { CM_Get_First_Log_Conf(&mut config, node, ALLOC_LOG_CONF) } {
        CR_SUCCESS => {}
        CR_NO_MORE_LOG_CONF => return Some(Vec::new()), // nothing allocated
        _ => return None,
    }
    let mut ranges = descriptor_sizes(config, ResType_Mem);
    ranges.extend(descriptor_sizes(config, ResType_MemLarge));
    // SAFETY: `config` came from CM_Get_First_Log_Conf above and is freed once.
    unsafe { CM_Free_Log_Conf_Handle(config) };
    Some(ranges)
}

/// The allocated size of each descriptor of resource type `kind` in the configuration `config`.
fn descriptor_sizes(config: usize, kind: CM_RESTYPE) -> Vec<u64> {
    let mut sizes = Vec::new();
    let mut last: Option<usize> = None;
    loop {
        let mut next = 0usize;
        // SAFETY: the walk starts at the open configuration and goes on from the descriptor read last, which is still
        // open; `next` is a valid place for the next handle, and the resource type isn't asked for (null).
        let status = unsafe {
            CM_Get_Next_Res_Des(
                &mut next,
                last.unwrap_or(config),
                kind,
                std::ptr::null_mut(),
                0,
            )
        };
        if let Some(done) = last.take() {
            // SAFETY: `done` came from CM_Get_Next_Res_Des, the call above was its last use, and it is freed once.
            unsafe { CM_Free_Res_Des_Handle(done) };
        }
        if status != CR_SUCCESS {
            return sizes;
        }
        sizes.extend(
            descriptor_data(next)
                .as_deref()
                .and_then(parse::allocated_range_bytes),
        );
        last = Some(next);
    }
}

fn descriptor_data(descriptor: usize) -> Option<Vec<u8>> {
    let mut size = 0u32;
    // SAFETY: `size` is a valid place for the answer.
    if unsafe { CM_Get_Res_Des_Data_Size(&mut size, descriptor, 0) } != CR_SUCCESS {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` is writable for the `size` bytes passed with it.
    let status = unsafe { CM_Get_Res_Des_Data(descriptor, data.as_mut_ptr().cast(), size, 0) };
    (status == CR_SUCCESS).then_some(data)
}

/// The name of the Intel LPC/eSPI controller (vendor 8086, PCI class 06/01), which names the chipset on Intel boards.
/// Ok(None) on boards without one (AMD); Err when Windows can't list its PCI devices.
fn lpc_description() -> Result<Option<String>, &'static str> {
    let ids =
        present_pci_devices().ok_or("couldn't list the PCI devices (CM_Get_Device_ID_ListW)")?;
    Ok(ids
        .iter()
        .filter(|id| id.to_ascii_uppercase().contains("VEN_8086"))
        .filter_map(|id| locate(id))
        .find(|&node| {
            property_byte(node, &BASE_CLASS) == Some(0x06)
                && property_byte(node, &SUB_CLASS) == Some(0x01)
        })
        .and_then(|node| property_text(node, &DEVPKEY_Device_DeviceDesc)))
}

/// The instance ids of the PCI devices present now.
fn present_pci_devices() -> Option<Vec<String>> {
    let filter = wide("PCI");
    let flags = CM_GETIDLIST_FILTER_ENUMERATOR | CM_GETIDLIST_FILTER_PRESENT;
    // A device that arrives between the two calls makes the list outgrow the buffer, so then ask again.
    for _ in 0..3 {
        let mut length = 0u32;
        // SAFETY: `filter` is NUL-terminated and outlives the call; `length` is a valid place for the answer.
        if unsafe { CM_Get_Device_ID_List_SizeW(&mut length, filter.as_ptr(), flags) } != CR_SUCCESS
        {
            return None;
        }
        let mut list = vec![0u16; length as usize];
        // SAFETY: as above, and `list` is writable for the `length` UTF-16 units passed with it.
        match unsafe { CM_Get_Device_ID_ListW(filter.as_ptr(), list.as_mut_ptr(), length, flags) } {
            CR_SUCCESS => return Some(parse::multi_sz(&list)),
            CR_BUFFER_SMALL => continue,
            _ => return None,
        }
    }
    None
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
```

How it reads:
- The PCI property keys are declared locally (fmtid `3ab22e31-8264-4b4e-9af5-a8d2d8e33e62`, pids 1, 3, 4, 9–12, as in pciprop.h). Every read checks the stored DEVPROPTYPE and size through the pure decoders: UINT32 for the device type, speeds and widths, BYTE for the class codes, STRING for `DEVPKEY_Device_DeviceDesc`. A missing property (an old Windows, a virtual machine, a laptop card asleep) is `None`, never 0, so `link_from_codes` gives `None` and the page shows Not reported.
- `chain` reads the card, then its parents while they are PCI devices, stopping after the first root port (the slot's maximum is the root port's) or 8 ancestors. `pcie::resolve` and `pcie::slot_device` (Task 4) pick the slot device and the slot's maximum from that list.
- Resizable BAR uses `ALLOC_LOG_CONF` (what Windows actually assigned). `CM_Get_Next_Res_Des` is walked once for `ResType_Mem` and once for `ResType_MemLarge`; each descriptor handle is freed right after its successor is read (the last one when the walk ends) and the configuration handle after both walks. The `MEM_DES` / `MEM_LARGE_DES` offsets the parser uses (8 and 16; both structs are `packed(1)` in windows-sys) are asserted at compile time. No configuration at all (`CR_NO_MORE_LOG_CONF`) gives an empty list, which `bar_summary` turns into Not reported. These resource calls work only in a 64-bit process (a 32-bit one on 64-bit Windows gets `CR_CALL_NOT_IMPLEMENTED`); Kestrel ships for x86-64.
- `gpu_link` walks the same chain: during a benchmark (every 2 s) it reads device properties of the card and the PCI bridges above it, nothing else.
- Chipset: the present PCI devices of vendor 8086 are checked for BaseClass 06 / SubClass 01 (an ISA bridge: the LPC/eSPI controller), and the first one's description is passed to `hardware::chipset`, which runs even without one (AMD boards, or the list failed) so the board-name token still counts.

`crates/kestrel-probe/src/lib.rs` — add above the doc comment of `busy_now` (`/// Apps keeping the CPU busy right now (measured over about 1.5 s), for flagging a benchmark run. Off Windows:`):
```rust
/// The current PCIe link of the card's slot device (see kestrel_core::pcie::slot_device). Windows: Configuration
/// Manager; elsewhere None. The benchmark reads it every 2 s while a pass runs: it reads only device properties of
/// the card and the PCI bridges above it.
pub fn gpu_link(pnp_device_id: &str) -> Option<kestrel_core::PcieLink> {
    #[cfg(windows)]
    {
        win::cfgmgr::gpu_link(pnp_device_id)
    }
    #[cfg(not(windows))]
    {
        let _ = pnp_device_id;
        None
    }
}
```

`crates/kestrel-probe/src/win/mod.rs` — replace
```rust
//! Real probes for Windows. Each one is independent: a failure becomes `None` plus a ProbeError.
mod cpu;
```
with
```rust
//! Real probes for Windows. Each one is independent: a failure becomes `None` plus a ProbeError.
pub(crate) mod cfgmgr;
mod cpu;
```
and in `collect_here` replace
```rust
    cpu::fill(&mut s, &mut errors);
    registry_probe::fill(&mut s, &mut errors);
```
with
```rust
    cpu::fill(&mut s, &mut errors);
    // After WMI too: the PCIe reads start from each card's PNPDeviceID, and the chipset from the board's name.
    cfgmgr::fill(&mut s, &mut errors);
    registry_probe::fill(&mut s, &mut errors);
```

- [ ] **Step 4: Verify**

Run:
```bash
cargo test -p kestrel-probe
cargo fmt --all --check
cargo clippy -p kestrel-probe --all-targets -- -D warnings
cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```
Expected: the 6 new tests pass (5 in parse.rs, `the_link_is_not_read_off_windows` in lib.rs; 38 in the lib target if Tasks 1–8 added none); fmt prints nothing; both clippy runs are clean. The Windows run compiles `cfgmgr.rs`, its compile-time checks and `a_card_windows_does_not_list_has_no_link`, which CI runs.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-probe/src/parse.rs crates/kestrel-probe/src/lib.rs crates/kestrel-probe/src/win/cfgmgr.rs crates/kestrel-probe/src/win/mod.rs
git commit -m "feat(probe): PCIe link, Resizable BAR and chipset from Configuration Manager; gpu_link()" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Probe — BIOS vendor, memory slots and rank, placeholder names, every drive, monitor names

**Files:**
- Create: `crates/kestrel-probe/src/win/displayconfig.rs`
- Modify: `crates/kestrel-probe/src/parse.rs`, `crates/kestrel-probe/src/win/wmi_probe.rs`, `crates/kestrel-probe/src/win/system_info.rs`, `crates/kestrel-probe/src/win/mod.rs`
- Test: `crates/kestrel-probe/src/parse.rs` (tests module)

**Interfaces:**
- Consumes:
  - Task 1: `BoardInfo.bios_vendor`, `MemoryInfo.slots_total`, `MemoryInfo.rated_mts`, `MemoryModule.rank`, `DisplayInfo.{monitor_name, connection}`, `StorageInfo.drives`, `kestrel_core::{DriveBus, DisplayInfo, DriveInfo}`.
  - Task 5: `kestrel_core::hardware::is_placeholder(text: &str) -> bool`, `hardware::connection_name(output_technology: u32) -> Option<&'static str>`, `hardware::DiskRecord { number: u32, model: String, media: DriveMedia, bus: DriveBus, size_gb: f64 }`, `hardware::drives(disks: &[DiskRecord], partitions: &[(u32, char)], space: &[(char, f64, f64)]) -> Vec<DriveInfo>`.
  - Task 8: `kestrel_core::ram::normalize_configured(modules: &mut [MemoryModule], ddr_generation: Option<u8>)`; the existing `ram::rated_mts(modules: &[MemoryModule]) -> Option<u32>`.
  - Task 9: the windows-sys `Win32_Devices_Display` feature.
  - Existing: `parse::{drive_media, exe_name}`; `wmi_probe`'s `text`, `num`, `first`, `rows_in`, `drive_letter_of`.
- Produces:
  - `parse::drive_bus(bus_type: Option<u64>) -> DriveBus`, `parse::memory_rank(attributes: u64) -> Option<u32>`, `parse::memory_slots(arrays: &[(Option<u64>, Option<u64>)]) -> Option<u32>`, `parse::volume_letter(mount_point: &str) -> Option<char>`, `parse::MonitorPath { gdi_name: String, monitor_name: String, output_technology: u32 }`, `parse::name_monitors(displays: &mut [DisplayInfo], paths: &[MonitorPath])`
  - `pub(crate) win::system_info::GIB` and `pub(crate) fn win::system_info::volume_space() -> Vec<(char, f64, f64)>` (letter, free GiB, total GiB)
  - Filled: `board.bios_vendor`; `board.manufacturer`, `board.product`, `board.bios_vendor`, `machine.manufacturer`, `machine.model` blanked when they are BIOS filler; `memory.slots_total`; `memory.rated_mts`; each module's `rank`; `storage.drives`; each display's `monitor_name` and `connection`
  - Probe errors `drives` and `displays`; `memory` now also when `Win32_PhysicalMemoryArray` fails (the modules are kept); `storage` when VALORANT's letter is on none of the listed drives

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module at the end of `crates/kestrel-probe/src/parse.rs`:
```rust
    #[test]
    fn bus_types_name_the_drive_bus() {
        // MSFT_PhysicalDisk.BusType is a STORAGE_BUS_TYPE (winioctl.h).
        let cases = [
            (17, DriveBus::Nvme),
            (11, DriveBus::Sata),
            (3, DriveBus::Sata), // ATA
            (7, DriveBus::Usb),
            (10, DriveBus::Sas),
            (8, DriveBus::Raid),
            (14, DriveBus::Virtual),
            (15, DriveBus::Virtual), // file-backed: a mounted VHD
            (1, DriveBus::Other),    // SCSI
            (12, DriveBus::Other),   // SD card
            (16, DriveBus::Other),   // Storage Spaces
            (0, DriveBus::Other),
        ];
        for (code, bus) in cases {
            assert_eq!(drive_bus(Some(code)), bus, "BusType {code}");
        }
        assert_eq!(drive_bus(None), DriveBus::Other);
    }

    #[test]
    fn memory_rank_is_the_low_four_bits_of_the_attributes() {
        assert_eq!(memory_rank(1), Some(1));
        assert_eq!(memory_rank(2), Some(2));
        assert_eq!(memory_rank(4), Some(4));
        assert_eq!(memory_rank(0x12), Some(2), "bits 7–4 are reserved");
        assert_eq!(memory_rank(0), None, "0 is an unknown rank");
        assert_eq!(memory_rank(0x10), None);
    }

    #[test]
    fn memory_slots_add_up_the_system_memory_arrays() {
        // (Use, MemoryDevices): Use 3 is system memory; 4 is video memory, 5 flash, 7 cache.
        assert_eq!(memory_slots(&[(Some(3), Some(4))]), Some(4));
        assert_eq!(
            memory_slots(&[(Some(3), Some(2)), (Some(3), Some(2))]),
            Some(4)
        );
        assert_eq!(
            memory_slots(&[(Some(3), Some(4)), (Some(5), Some(1))]),
            Some(4)
        );
        assert_eq!(memory_slots(&[(Some(5), Some(1))]), None);
        assert_eq!(memory_slots(&[(Some(3), Some(0))]), None);
        assert_eq!(memory_slots(&[(None, Some(4))]), None);
        assert_eq!(memory_slots(&[]), None);
    }

    #[test]
    fn only_a_volume_at_the_root_of_a_drive_has_a_letter() {
        assert_eq!(volume_letter("C:\\"), Some('C'));
        assert_eq!(volume_letter("d:\\"), Some('D'));
        assert_eq!(volume_letter("E:"), Some('E'));
        // Mounted in a folder, or with no letter at all.
        assert_eq!(volume_letter("D:\\Games\\"), None);
        assert_eq!(
            volume_letter("\\\\?\\Volume{0b5ac4d2-0000-0000-0000-100000000000}\\"),
            None
        );
        assert_eq!(volume_letter(""), None);
    }

    fn display(name: &str) -> DisplayInfo {
        DisplayInfo {
            name: name.into(),
            ..Default::default()
        }
    }

    fn monitor(gdi_name: &str, monitor_name: &str, output_technology: u32) -> MonitorPath {
        MonitorPath {
            gdi_name: gdi_name.into(),
            monitor_name: monitor_name.into(),
            output_technology,
        }
    }

    #[test]
    fn displays_take_their_monitors_name_and_connection() {
        let mut displays = vec![
            display("\\\\.\\DISPLAY1"),
            display("\\\\.\\DISPLAY2"),
            display("\\\\.\\DISPLAY3"),
        ];
        // DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY 10 is DisplayPort, 5 HDMI. DISPLAY1 is cloned onto two monitors: the
        // first path names it, whatever the letter case.
        let paths = [
            monitor("\\\\.\\DISPLAY2", "", 5),
            monitor("\\\\.\\display1", "XL2566K", 10),
            monitor("\\\\.\\DISPLAY1", "LG ULTRAGEAR", 5),
        ];
        name_monitors(&mut displays, &paths);
        assert_eq!(displays[0].monitor_name.as_deref(), Some("XL2566K"));
        assert_eq!(displays[0].connection.as_deref(), Some("DisplayPort"));
        // No name in the monitor's EDID: the connection is still known.
        assert_eq!(displays[1].monitor_name, None);
        assert_eq!(displays[1].connection.as_deref(), Some("HDMI"));
        // A display with no active path keeps nothing.
        assert_eq!(displays[2].monitor_name, None);
        assert_eq!(displays[2].connection, None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-probe`
Expected: FAIL to compile — cannot find function `drive_bus`, `memory_rank`, `memory_slots`, `volume_letter`, `name_monitors`; cannot find struct `MonitorPath`, `DisplayInfo`; cannot find type `DriveBus` in this scope.

- [ ] **Step 3: Implement**

`crates/kestrel-probe/src/parse.rs` — replace the imports Task 9 left
```rust
use kestrel_core::topology::PhysicalCore;
use kestrel_core::{BootMode, BusyProcess, CacheKind, CpuVendor, DriveMedia, FormFactor, NetKind};
```
with
```rust
use kestrel_core::topology::PhysicalCore;
use kestrel_core::{
    hardware, BootMode, BusyProcess, CacheKind, CpuVendor, DisplayInfo, DriveBus, DriveMedia,
    FormFactor, NetKind,
};
```
and add after `is_pci_instance`, above `#[cfg(test)]`:
```rust
/// MSFT_PhysicalDisk.BusType (STORAGE_BUS_TYPE in winioctl.h): 17 NVMe; 11 SATA and 3 ATA; 7 USB; 10 SAS; 8 RAID;
/// 14 virtual and 15 file-backed virtual (a mounted VHD). Anything else (SCSI, iSCSI, SD, Storage Spaces…) is Other.
pub fn drive_bus(bus_type: Option<u64>) -> DriveBus {
    match bus_type {
        Some(17) => DriveBus::Nvme,
        Some(3 | 11) => DriveBus::Sata,
        Some(7) => DriveBus::Usb,
        Some(10) => DriveBus::Sas,
        Some(8) => DriveBus::Raid,
        Some(14 | 15) => DriveBus::Virtual,
        _ => DriveBus::Other,
    }
}

/// Win32_PhysicalMemory.Attributes is SMBIOS type 17's Attributes byte: bits 3–0 are the module's rank (1 single,
/// 2 dual, 4 quad), 0 when unknown.
pub fn memory_rank(attributes: u64) -> Option<u32> {
    Some((attributes & 0x0F) as u32).filter(|&rank| rank > 0)
}

/// Win32_PhysicalMemoryArray rows as (Use, MemoryDevices) → the slots of the arrays that hold system memory (Use 3),
/// added up. None when none lists a slot.
pub fn memory_slots(arrays: &[(Option<u64>, Option<u64>)]) -> Option<u32> {
    let slots: u64 = arrays
        .iter()
        .filter(|(usage, _)| *usage == Some(3))
        .filter_map(|(_, devices)| *devices)
        .sum();
    u32::try_from(slots).ok().filter(|&slots| slots > 0)
}

/// A volume's mount point → its drive letter: "C:\" → 'C'. A volume mounted in a folder ("D:\Games\") or without a
/// letter ("\\?\Volume{…}\") has none.
pub fn volume_letter(mount_point: &str) -> Option<char> {
    let mut chars = mount_point.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    (chars.next() == Some(':') && matches!(chars.as_str(), "" | "\\" | "/"))
        .then_some(letter.to_ascii_uppercase())
}

/// One active path of Windows' display configuration: which desktop shows on which monitor, and how it's connected.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorPath {
    /// The desktop's GDI name, like `\\.\DISPLAY1`: the same as `DisplayInfo::name`.
    pub gdi_name: String,
    /// The monitor's name from its EDID; empty when it has none.
    pub monitor_name: String,
    /// DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY as unsigned bits (INTERNAL is 0x8000_0000).
    pub output_technology: u32,
}

/// Gives each display its monitor's name (when the EDID has one) and its connection, from the first path with its GDI
/// name (a desktop cloned onto two monitors has two paths). A display without a path is left as it is.
pub fn name_monitors(displays: &mut [DisplayInfo], paths: &[MonitorPath]) {
    for display in displays {
        let Some(path) = paths
            .iter()
            .find(|p| p.gdi_name.eq_ignore_ascii_case(&display.name))
        else {
            continue;
        };
        let name = path.monitor_name.trim();
        display.monitor_name = (!name.is_empty()).then(|| name.to_string());
        display.connection = hardware::connection_name(path.output_technology).map(str::to_string);
    }
}
```

The bus codes are STORAGE_BUS_TYPE's, as windows-sys 0.61.2 defines them (`BusTypeAta` 3, `BusTypeUsb` 7, `BusTypeRAID` 8, `BusTypeSas` 10, `BusTypeSata` 11, `BusTypeVirtual` 14, `BusTypeFileBackedVirtual` 15, `BusTypeNvme` 17); `MSFT_PhysicalDisk.BusType` uses the same numbers. `Win32_PhysicalMemory.Attributes` is SMBIOS type 17's Attributes byte, whose bits 3–0 are the rank.

`crates/kestrel-probe/src/win/wmi_probe.rs`, seven edits:

1. Replace the imports
```rust
use super::{problem, registry_probe};
use crate::parse;
use kestrel_core::{
    gpu, GpuInfo, MemoryModule, NetAdapter, ProbeError, ServiceState, SystemSnapshot,
};
```
with
```rust
use super::{problem, registry_probe, system_info};
use crate::parse;
use kestrel_core::hardware::{self, DiskRecord};
use kestrel_core::{
    gpu, ram, GpuInfo, MemoryModule, NetAdapter, ProbeError, ServiceState, SystemSnapshot,
};
```

2. In `fill`, replace
```rust
        attempt("network", self.network(s));
        attempt("storage", self.storage(s));
```
with
```rust
        attempt("network", self.network(s));
        attempt("drives", self.drives(s));
        attempt("storage", valorant_media(s));
```

3. In `machine`, replace
```rust
        s.machine.manufacturer = text(&row, "Manufacturer");
        s.machine.model = text(&row, "Model");
```
with
```rust
        s.machine.manufacturer = real_name(&row, "Manufacturer");
        s.machine.model = real_name(&row, "Model");
```

4. Replace everything from `    fn memory(&self, s: &mut SystemSnapshot) -> Outcome {` up to (not including) `    fn tpm(&self, s: &mut SystemSnapshot) -> Outcome {` — that is, `memory` and `board` — with:
```rust
    fn memory(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows(
            "SELECT Capacity, ConfiguredClockSpeed, Speed, PartNumber, Manufacturer, BankLabel, DeviceLocator, SMBIOSMemoryType, Attributes FROM Win32_PhysicalMemory",
        )?;
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
                rank: num(row, "Attributes").and_then(parse::memory_rank),
            })
            .collect();
        s.memory.total_mb = s.memory.modules.iter().map(|m| m.capacity_mb).sum();
        s.memory.ddr_generation = rows
            .first()
            .and_then(|r| num(r, "SMBIOSMemoryType"))
            .and_then(parse::ddr_generation);
        // Some boards report the clock in MHz (half the MT/s); this puts every module in MT/s.
        ram::normalize_configured(&mut s.memory.modules, s.memory.ddr_generation);
        // The kit's rating from its part numbers, which the Hardware tab shows (the speeds don't change it).
        s.memory.rated_mts = ram::rated_mts(&s.memory.modules);
        // The slot count can fail on its own: the modules above are kept, and the error is reported after them.
        let arrays = self
            .rows("SELECT Use, MemoryDevices FROM Win32_PhysicalMemoryArray")
            .map_err(|e| {
                format!("couldn't read the memory slots (Win32_PhysicalMemoryArray): {e}")
            })?;
        let arrays: Vec<(Option<u64>, Option<u64>)> = arrays
            .iter()
            .map(|a| (num(a, "Use"), num(a, "MemoryDevices")))
            .collect();
        s.memory.slots_total = parse::memory_slots(&arrays);
        Ok(())
    }

    fn board(&self, s: &mut SystemSnapshot) -> Outcome {
        let board = first(self.rows("SELECT Manufacturer, Product FROM Win32_BaseBoard")?)?;
        s.board.manufacturer = real_name(&board, "Manufacturer");
        s.board.product = real_name(&board, "Product");
        let bios = first(
            self.rows("SELECT Manufacturer, SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS")?,
        )?;
        s.board.bios_vendor = real_name(&bios, "Manufacturer");
        s.board.bios_version = text(&bios, "SMBIOSBIOSVersion");
        s.board.bios_date = parse::cim_date(&text(&bios, "ReleaseDate"));
        Ok(())
    }
```

5. Replace everything from `    fn storage(&self, s: &mut SystemSnapshot) -> Outcome {` up to (not including) `fn first(mut rows: Vec<Row>)` — `storage` and the `}` that closes `impl Wmi` — with:
```rust
    /// Every physical drive with its lettered volumes and their space (`storage.drives`).
    fn drives(&self, s: &mut SystemSnapshot) -> Outcome {
        let namespace = "root\\Microsoft\\Windows\\Storage";
        let disks: Vec<DiskRecord> = self
            .rows_in(
                namespace,
                "SELECT DeviceId, FriendlyName, MediaType, BusType, Size FROM MSFT_PhysicalDisk",
            )?
            .iter()
            .filter_map(|d| {
                let bus_type = num(d, "BusType");
                Some(DiskRecord {
                    number: u32::try_from(num(d, "DeviceId")?).ok()?,
                    model: text(d, "FriendlyName"),
                    media: parse::drive_media(bus_type, num(d, "MediaType")),
                    bus: parse::drive_bus(bus_type),
                    size_gb: num(d, "Size").unwrap_or(0) as f64 / system_info::GIB,
                })
            })
            .collect();
        let partitions: Vec<(u32, char)> = self
            .rows_in(
                namespace,
                "SELECT DiskNumber, DriveLetter FROM MSFT_Partition",
            )?
            .iter()
            .filter_map(|p| {
                Some((
                    u32::try_from(num(p, "DiskNumber")?).ok()?,
                    drive_letter_of(p)?,
                ))
            })
            .collect();
        s.storage.drives = hardware::drives(&disks, &partitions, &system_info::volume_space());
        Ok(())
    }
}

/// VALORANT's drive type: the type of the drive that holds its letter, from the list `drives` just read. An empty
/// list has its own error already.
fn valorant_media(s: &mut SystemSnapshot) -> Outcome {
    let Some(install) = s.storage.valorant.as_mut() else {
        return Ok(());
    };
    if s.storage.drives.is_empty() {
        return Ok(());
    }
    let drive = s
        .storage
        .drives
        .iter()
        .find(|d| {
            d.volumes.iter().any(|v| {
                v.letter
                    .trim_end_matches(':')
                    .eq_ignore_ascii_case(&install.drive_letter)
            })
        })
        .ok_or("VALORANT's drive isn't among the drives Windows listed")?;
    install.media = drive.media;
    Ok(())
}
```

6. Add after `fn text` (above `fn variant_num`):
```rust
/// A maker's or model's name, or "" when the firmware left filler there ("To be filled by O.E.M.", "Default string"…).
fn real_name(row: &Row, key: &str) -> String {
    let value = text(row, key);
    if hardware::is_placeholder(&value) {
        String::new()
    } else {
        value
    }
}
```

7. Replace `drive_letter_of` with its doc comment (the end of the file) with:
```rust
/// MSFT_Partition.DriveLetter is a CIM char16. The wmi crate returns it as a one-letter string (a partition
/// with no letter comes back as "\0", which is None here); a plain number is handled too.
fn drive_letter_of(row: &Row) -> Option<char> {
    match row.get("DriveLetter")? {
        Variant::String(v) => v.chars().next(),
        other => variant_num(other).and_then(|n| char::from_u32(n as u32)),
    }
    .filter(char::is_ascii_alphabetic)
    .map(|c| c.to_ascii_uppercase())
}
```

What changed and why:
- `memory` lists every `MemoryModule` field (no `..Default::default()`: clippy's `needless_update` rejects it once nothing is left to default). `ram::normalize_configured` (Task 8) runs right after the modules, total and DDR generation are filled, with `memory.rated_mts` set beside it from the part numbers (`ram::rated_mts`), and both before the slot query, which can return early with its own error after the modules are kept (the same pattern as the battery query in `machine`).
- Placeholders: `real_name` blanks BIOS filler with `hardware::is_placeholder`, so "To be filled by O.E.M." is never shown or matched; `bios_version` and `bios_date` are not names and stay as read.
- Drives: both storage tables are read once. `MSFT_PhysicalDisk.DeviceId` is a string such as "0" and `Size` a uint64, which WMI hands over as a string; `num` parses both. `drive_letter_of` now drops partitions without a letter (WMI gives "\0"), which used to come back as `Some('\0')`.
- VALORANT's drive type now comes from the drive list instead of a second pair of queries. `valorant_media` reports `storage` only when drives were listed but none holds VALORANT's letter; a failed drive list already has its `drives` error.

`crates/kestrel-probe/src/win/system_info.rs` — replace everything above `/// Which apps keep the CPU busy, measured over about a second and a half.` (the module doc, the imports, `GIB` and `fill`) with:
```rust
//! Free space on each volume (VALORANT's drive, the drive list) and busy background apps. Busy apps come from Windows
//! performance counters (see `pdh`), so Kestrel itself opens no handle to any other process. The cross-platform
//! `sysinfo` crate is used only for disk sizes and the memory total, and neither touches a process.
use super::{pdh, problem};
use crate::parse;
use kestrel_core::{BusyProcess, ProbeError, SystemSnapshot};
use std::time::Duration;
use sysinfo::{DiskRefreshKind, Disks, System};

pub(crate) const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    if let Some(install) = s.storage.valorant.as_mut() {
        let letter = install
            .drive_letter
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase());
        if let Some(&(_, free, total)) = volume_space().iter().find(|(l, ..)| Some(*l) == letter) {
            install.free_gb = free;
            install.total_gb = total;
        }
    }
    s.background.busy_processes = busy_processes();
    if s.background.busy_processes.is_none() {
        errors.push(problem("background", "couldn't measure CPU use per app"));
    }
    // The WMI probe runs first and sets the total from Win32_PhysicalMemory, which can come back empty (a virtual
    // machine, say); then ask Windows for the total instead. `System::new()` loads nothing, so refresh memory first.
    if s.memory.total_mb == 0 {
        let mut sys = System::new();
        sys.refresh_memory();
        s.memory.total_mb = sys.total_memory() / 1024 / 1024;
    }
}

/// Free and total space in GiB of each volume with a drive letter: (letter, free, total).
pub(crate) fn volume_space() -> Vec<(char, f64, f64)> {
    // Sizes only: the other refresh kinds open each volume's device to ask about its hardware and I/O.
    let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_storage());
    disks
        .list()
        .iter()
        .filter_map(|d| {
            let letter = parse::volume_letter(&d.mount_point().to_string_lossy())?;
            Some((
                letter,
                d.available_space() as f64 / GIB,
                d.total_space() as f64 / GIB,
            ))
        })
        .collect()
}
```

`volume_space` keeps the storage-only refresh (`DiskRefreshKind::nothing().with_storage()`: sizes, no device opened) and keys space by the letter at a volume's root; a volume mounted in a folder or without a letter is skipped. VALORANT's free space uses the same list.

Create `crates/kestrel-probe/src/win/displayconfig.rs`:
```rust
//! Monitor names and connections from Windows' display configuration (the names Settings → Display shows), joined to
//! the displays `display` listed by their GDI name. Read-only: QueryDisplayConfig and DisplayConfigGetDeviceInfo only
//! describe the current setup.
use super::problem;
use crate::parse::{self, MonitorPath};
use kestrel_core::{ProbeError, SystemSnapshot};
use windows_sys::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    QDC_ONLY_ACTIVE_PATHS,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID};

pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    match active_paths() {
        Some(paths) => {
            let monitors: Vec<MonitorPath> = paths.iter().filter_map(monitor_path).collect();
            parse::name_monitors(&mut s.displays, &monitors);
        }
        None => errors.push(problem(
            "displays",
            "couldn't read the display configuration (QueryDisplayConfig)",
        )),
    }
}

/// The paths from each desktop to the monitor that shows it, for the displays in use now.
fn active_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // A monitor plugged in between the two calls makes the answer outgrow the arrays, so then ask again.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0u32, 0u32);
        // SAFETY: both counts are valid places for the answers.
        let status = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        if path_count == 0 {
            return Some(Vec::new()); // no display in use (a PC without a monitor)
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: each array holds as many elements as its count says, and the topology id may be null because
        // QDC_DATABASE_CURRENT isn't asked for.
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        match status {
            ERROR_SUCCESS => {
                paths.truncate(path_count as usize);
                return Some(paths);
            }
            ERROR_INSUFFICIENT_BUFFER => continue,
            _ => return None,
        }
    }
    None
}

fn header(
    request: DISPLAYCONFIG_DEVICE_INFO_TYPE,
    size: usize,
    adapter: LUID,
    id: u32,
) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: request,
        size: size as u32,
        adapterId: adapter,
        id,
    }
}

/// The path's desktop (GDI name) and its monitor's name and connection. None when Windows can't name the desktop; a
/// monitor it can't name keeps the connection the path itself gives.
fn monitor_path(path: &DISPLAYCONFIG_PATH_INFO) -> Option<MonitorPath> {
    let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
            path.sourceInfo.adapterId,
            path.sourceInfo.id,
        ),
        ..Default::default()
    };
    // SAFETY: the header names the request and the size of the whole packet, and the pointer covers the whole packet.
    if unsafe { DisplayConfigGetDeviceInfo((&raw mut source).cast()) } != 0 {
        return None;
    }
    let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>(),
            path.targetInfo.adapterId,
            path.targetInfo.id,
        ),
        ..Default::default()
    };
    // SAFETY: as above, for the target's packet.
    let named = unsafe { DisplayConfigGetDeviceInfo((&raw mut target).cast()) } == 0;
    let (monitor_name, technology) = if named {
        (
            parse::exe_name(&target.monitorFriendlyDeviceName),
            target.outputTechnology,
        )
    } else {
        (String::new(), path.targetInfo.outputTechnology)
    };
    Some(MonitorPath {
        gdi_name: parse::exe_name(&source.viewGdiDeviceName),
        monitor_name,
        output_technology: technology.cast_unsigned(),
    })
}
```

`DisplayConfigGetDeviceInfo` fills the whole packet its header describes, so the pointer is taken from the whole struct (`&raw mut source`) rather than from `&mut source.header`, whose provenance would cover the header alone. windows-sys types `outputTechnology` as `i32`; `cast_unsigned()` keeps the bits, so `DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL` (`i32::MIN`) reaches `hardware::connection_name` as `0x8000_0000`. A target Windows can't name still gives its connection from the path itself; an EDID without a name leaves `monitor_name` None (the page then says "Display 1", "Display 2"…).

`crates/kestrel-probe/src/win/mod.rs` — replace
```rust
mod display;
mod firmware;
```
with
```rust
mod display;
mod displayconfig;
mod firmware;
```
and in `collect_here` replace
```rust
    display::fill(&mut s);
    s.security.iommu = Some(firmware::iommu_enabled());
```
with
```rust
    display::fill(&mut s);
    // After `display`: monitor names join the displays it listed by their GDI name.
    displayconfig::fill(&mut s, &mut errors);
    s.security.iommu = Some(firmware::iommu_enabled());
```

- [ ] **Step 4: Verify**

Run:
```bash
cargo test -p kestrel-probe
cargo fmt --all --check
cargo clippy -p kestrel-probe --all-targets -- -D warnings
cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```
Expected: the 5 new tests pass (43 in the lib target if Tasks 1–8 added none); fmt prints nothing; both clippy runs are clean (the Windows run compiles the WMI, sysinfo and display-config changes).

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-probe/src/parse.rs crates/kestrel-probe/src/win/wmi_probe.rs crates/kestrel-probe/src/win/system_info.rs crates/kestrel-probe/src/win/displayconfig.rs crates/kestrel-probe/src/win/mod.rs
git commit -m "feat(probe): BIOS vendor, memory slots and rank, placeholder names, every drive, monitor names" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Probe — live per-core clocks (`ClockSampler`) and Windows smoke checks

**Files:**
- Create: `crates/kestrel-probe/src/clocks.rs`
- Modify: `crates/kestrel-probe/src/lib.rs`, `crates/kestrel-probe/src/win/mod.rs`
- Test: `crates/kestrel-probe/src/clocks.rs` (tests module); `crates/kestrel-probe/tests/smoke.rs` (one Windows-only test, run on CI)

**Interfaces:**
- Consumes:
  - Task 3: `kestrel_core::topology::PhysicalCore`, `topology::CoreClock { core: u32, kind: Option<CoreKind>, mhz: Option<u32> }`, `topology::core_clocks(cores: &[PhysicalCore], frequency: &[(String, f64)], performance: &[(String, f64)]) -> Vec<CoreClock>` (it maps the PDH instance names "g,n" to cores).
  - Task 9: `pub(crate) fn win::cpu::topology() -> Option<(Vec<PhysicalCore>, Vec<(u8, CacheKind, u32)>)>`.
  - Existing: `win::pdh::{Query::open, Query::collect, Query::formatted_array, FMT_NOCAP100}`; `pdh::Query` is `Send` (`unsafe impl Send` with its SAFETY note).
  - Tasks 1 and 9–11: the snapshot fields the smoke test checks (`BootMode` from Task 1).
- Produces:
  - `pub struct kestrel_probe::clocks::ClockSampler` with `pub fn new() -> Self` (collects once, so the next sample has a baseline), `pub fn sample(&mut self) -> Vec<kestrel_core::topology::CoreClock>` and `impl Default`; off Windows 8 cores, `kind` None, 4800 + 50 × core MHz.
  - `ClockSampler: Send`, checked at compile time on both targets (Task 14 keeps it in `AppState` behind a `Mutex`).

- [ ] **Step 1: Write the failing tests**

Create `crates/kestrel-probe/src/clocks.rs` with only its tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn off_windows_eight_sample_cores_tick_at_4800_mhz_and_up() {
        let clocks = ClockSampler::new().sample();
        assert_eq!(clocks.len(), 8);
        for (index, clock) in clocks.iter().enumerate() {
            let core = index as u32;
            assert_eq!(
                *clock,
                CoreClock {
                    core,
                    kind: None,
                    mhz: Some(4800 + 50 * core),
                }
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_gives_a_clock_per_physical_core() {
        let mut sampler = ClockSampler::default();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let clocks = sampler.sample();
        assert!(!clocks.is_empty());
        for (index, clock) in clocks.iter().enumerate() {
            assert_eq!(clock.core, index as u32, "{clocks:?}");
        }
    }

    /// The app keeps the sampler in a `Mutex` in Tauri's state, which needs `Send`. On Windows that includes the PDH
    /// queries, so this is only fully checked by compiling for Windows.
    #[test]
    fn the_sampler_can_move_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<ClockSampler>();
    }
}
```

In `crates/kestrel-probe/src/lib.rs`, replace `pub mod load;` with:
```rust
pub mod clocks;
pub mod load;
```

Append to `crates/kestrel-probe/tests/smoke.rs` (Windows only; CI runs it):
```rust
#[cfg(windows)]
#[test]
fn windows_probe_reads_the_hardware() {
    // A hosted CI runner is a Hyper-V virtual machine without a graphics card: the PCIe link, Resizable BAR, the
    // chipset and monitor names can all be missing there, so only what every Windows PC reports is checked.
    let s = kestrel_probe::collect_snapshot();
    let errors = &s.probe_errors;
    assert!(s.cpu.family.is_some(), "probe errors: {errors:?}");
    assert!(
        s.cpu.instructions.iter().any(|i| i == "SSE4.2"),
        "probe errors: {errors:?}"
    );
    assert!(!s.cpu.caches.is_empty(), "probe errors: {errors:?}");
    assert!(s.cpu.virtualization.is_some(), "probe errors: {errors:?}");
    assert_ne!(
        s.board.boot_mode,
        kestrel_core::BootMode::Unknown,
        "probe errors: {errors:?}"
    );
    assert!(!s.storage.drives.is_empty(), "probe errors: {errors:?}");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-probe`
Expected: FAIL to compile — cannot find type `ClockSampler` in this scope; cannot find struct `CoreClock` in this scope.

- [ ] **Step 3: Implement**

`crates/kestrel-probe/src/clocks.rs` — add above the tests module:
```rust
//! Live clock of each physical core for the Hardware tab: PDH's `Processor Frequency` (the rated MHz) times
//! `% Processor Performance` / 100 per logical processor, mapped to physical cores. Like the load sampler, the app
//! keeps one and asks it about once a second while the tab is shown.
#[cfg(windows)]
use crate::win::pdh;
use kestrel_core::topology::CoreClock;
#[cfg(windows)]
use kestrel_core::topology::{self, PhysicalCore};

pub struct ClockSampler {
    /// The physical cores from Windows' topology, in order: one clock each.
    #[cfg(windows)]
    cores: Vec<PhysicalCore>,
    /// Every logical processor's `Processor Frequency` and `% Processor Performance`; None when Windows can't give the
    /// counter. A PDH query can move between threads (see `pdh::Query`), so the sampler can sit behind a Mutex in the
    /// app's state.
    #[cfg(windows)]
    frequency: Option<pdh::Query>,
    #[cfg(windows)]
    performance: Option<pdh::Query>,
}

impl Default for ClockSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockSampler {
    /// Reads the topology and collects once, so the next sample has a baseline: `% Processor Performance` is a rate.
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            let sampler = Self {
                cores: crate::win::cpu::topology()
                    .map(|(cores, _)| cores)
                    .unwrap_or_default(),
                frequency: pdh::Query::open(r"\Processor Information(*)\Processor Frequency"),
                performance: pdh::Query::open(r"\Processor Information(*)\% Processor Performance"),
            };
            for query in [&sampler.frequency, &sampler.performance]
                .into_iter()
                .flatten()
            {
                query.collect();
            }
            sampler
        }
        #[cfg(not(windows))]
        {
            Self {}
        }
    }

    /// One clock per physical core, in topology order; call about once a second. On Windows a core whose counters
    /// have no data has no MHz (every core, when a counter can't be opened). Off Windows: eight sample cores at
    /// 4800 + 50 × core MHz.
    pub fn sample(&mut self) -> Vec<CoreClock> {
        #[cfg(windows)]
        {
            let frequency = read(self.frequency.as_ref(), 0);
            // `% Processor Performance` goes above 100 when the core boosts, so it mustn't be capped.
            let performance = read(self.performance.as_ref(), pdh::FMT_NOCAP100);
            topology::core_clocks(&self.cores, &frequency, &performance)
        }
        #[cfg(not(windows))]
        {
            (0..8)
                .map(|core| CoreClock {
                    core,
                    kind: None,
                    mhz: Some(4800 + 50 * core),
                })
                .collect()
        }
    }
}

/// A new sample of every instance of the query's counter; empty when the query isn't open or the sample fails.
#[cfg(windows)]
fn read(query: Option<&pdh::Query>, flags: u32) -> Vec<(String, f64)> {
    query
        .filter(|query| query.collect())
        .and_then(|query| query.formatted_array(flags))
        .unwrap_or_default()
}
```

`crates/kestrel-probe/src/win/mod.rs` — `clocks` sits at the crate root, so `cpu` and `pdh` become crate-visible: replace `mod cpu;` with `pub(crate) mod cpu;` and `mod pdh;` with `pub(crate) mod pdh;`. The module list is then:
```rust
pub(crate) mod cfgmgr;
pub(crate) mod cpu;
mod display;
mod displayconfig;
mod firmware;
pub(crate) mod gpu_usage;
pub(crate) mod pdh;
mod registry_probe;
pub(crate) mod system_info;
mod wmi_probe;
```

How it behaves:
- A `pdh::Query` holds one counter, so there are two queries: `Processor Frequency` (the rated MHz) and `% Processor Performance` with `FMT_NOCAP100`, because it goes above 100 when a core boosts. `PdhAddEnglishCounterW` (inside `Query::open`) keeps the paths English in any Windows language, and the `(*)` instances are "group,number", so more than 64 logical processors still map to cores.
- `new()` reads the topology once and collects once. A query that can't be opened gives an empty list, and `core_clocks` then returns every core with `mhz: None`; without a topology there are no cores and `sample()` returns an empty list (the page shows Not reported).
- `Send`: the sampler holds only `Vec<PhysicalCore>` and `Option<pdh::Query>`, and `pdh::Query`'s `unsafe impl Send` already covers this use (one owner, behind a Mutex), so no new `unsafe` is needed. `the_sampler_can_move_between_threads` fails to compile if that ever changes, on both targets.
- The smoke test asserts only what a hosted runner (a Hyper-V VM without a graphics card) reports: CPUID values, caches, virtualization (on there: the hypervisor bit is set), a known boot mode (UEFI or legacy BIOS, whichever the VM uses), and at least one drive. It doesn't assert PCIe, Resizable BAR, the chipset or monitor names: a Generation 2 Hyper-V VM has no PCI bus, so CI's "Probe this runner" output can show a `chipset` probe error.

- [ ] **Step 4: Verify**

Run:
```bash
cargo test -p kestrel-probe
cargo fmt --all --check
cargo clippy -p kestrel-probe --all-targets -- -D warnings
cargo clippy -p kestrel-probe --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```
Expected: the 2 new Mac tests pass (`off_windows_eight_sample_cores_tick_at_4800_mhz_and_up`, `the_sampler_can_move_between_threads`; 45 in the lib target if Tasks 1–8 added none; `tests/smoke.rs` 2 passed); fmt prints nothing; both clippy runs are clean. The Windows run compiles `windows_gives_a_clock_per_physical_core` and `windows_probe_reads_the_hardware`, which CI runs.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-probe/src/clocks.rs crates/kestrel-probe/src/lib.rs crates/kestrel-probe/src/win/mod.rs crates/kestrel-probe/tests/smoke.rs
git commit -m "feat(probe): live per-core clocks and Windows smoke checks for the new hardware fields" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: `kestrel-bench` — a benchmark records its graphics card and PCIe link; the Copy results line

**Files:**
- Modify: `crates/kestrel-bench/src/model.rs` (`GpuLink`, two `Context` fields)
- Modify: `crates/kestrel-bench/src/text.rs` (the `PCIe link under load` line and its tests)
- Test: `crates/kestrel-bench/src/store.rs` (a benchmark saved by v0.2.0 still loads; the card and link survive a save)
- Modify: `app/src-tauri/src/bench/commands.rs` (`context_from` lists every `Context` field, so it must name the two new ones or the app stops compiling; they stay `None` until Task 14)

**Interfaces:**
- Consumes: nothing from earlier tasks (`kestrel-bench` does not depend on `kestrel-core`, so it has its own link type).
- Produces (Tasks 14, 15 and 17 rely on these):
  ```rust
  // kestrel_bench::GpuLink (model.rs, re-exported by `pub use model::*`). JSON: {"gen": 4, "width": 16}
  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
  #[serde(rename_all = "camelCase", default)]
  pub struct GpuLink { pub gen: u8, pub width: u8 }
  // kestrel_bench::Context gains (JSON "gpuPnpDeviceId", "gpuLink"; both missing in v0.2.0 files → None):
  pub gpu_pnp_device_id: Option<String>,
  pub gpu_link: Option<GpuLink>,
  ```
  `kestrel_bench::text::results_text` ends with the line `PCIe link under load: {gen}.0 x{width}` (e.g. `PCIe link under load: 4.0 x16`), or `PCIe link under load: not reported` when there is no link (or its generation or width is 0). It comes right after the `PC: …` line, and is the last line even when there is no `PC:` line.

- [ ] **Step 1: Write the failing tests**

In `crates/kestrel-bench/src/text.rs`, inside `mod tests`, replace

```rust
    use super::*;
    use crate::model::{BenchResult, Context, MetricDelta, RunMetrics, RunRecord, Spread};
```

with

```rust
    use super::*;
    use crate::model::{BenchResult, Context, GpuLink, MetricDelta, RunMetrics, RunRecord, Spread};

    const CARD: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";
```

In `starting_point()`, the context gains the card and a 4.0 x16 link. Replace

```rust
                gpu: Some("NVIDIA GeForce RTX 3060 Ti".into()),
                gpu_driver: Some("610.74".into()),
                windows_build: 26200,
```

with

```rust
                gpu: Some("NVIDIA GeForce RTX 3060 Ti".into()),
                gpu_driver: Some("610.74".into()),
                gpu_pnp_device_id: Some(CARD.into()),
                gpu_link: Some(GpuLink { gen: 4, width: 16 }),
                windows_build: 26200,
```

In `a_result_reads_as_plain_lines`, the expected text gains a last line. Replace

```rust
             PC: power plan EXM Free Power Plan V8, RAM 3600 MT/s, NVIDIA GeForce RTX 3060 Ti (driver 610.74), Windows build 26200, 360 Hz"
        );
    }
```

with

```rust
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
        // Without any PC context (no PC line), the PCIe line is still there, last.
        bench.context = Context::default();
        let bare = text(&bench);
        assert!(!bare.contains("\nPC: "), "{bare}");
        assert!(
            bare.ends_with("Run 2 kept despite: busy background app (chrome.exe)\nPCIe link under load: not reported"),
            "{bare}"
        );
    }
```

In `crates/kestrel-bench/src/store.rs`, inside `mod tests`, replace

```rust
    use super::*;
    use crate::model::{BenchResult, RunMetrics};
```

with

```rust
    use super::*;
    use crate::model::{BenchResult, GpuLink, RunMetrics};
```

and add, right after the test `saves_lists_and_reads_back` (before `fn clashing_ids_get_a_suffix`):

```rust
    /// A summary.json as v0.2.0 wrote it: its context has no `gpuPnpDeviceId` and no `gpuLink`.
    const SAVED_BY_0_2_0: &str = r#"{
  "id": "20261013-191500",
  "name": "Starting point",
  "createdAt": "2026-10-13T19:15:00Z",
  "profileId": "valorant",
  "routeVersion": 1,
  "kestrelVersion": "0.2.0",
  "presentmonVersion": "2.6.0",
  "runs": [
    {
      "metrics": {
        "frames": 36000,
        "durationS": 90.0,
        "avgFps": 400.0,
        "low1Fps": 280.0,
        "low01Fps": 200.0,
        "stuttersPerMin": 0.4,
        "pcLatencyMs": 9.8,
        "gpuBusyRatio": 0.82,
        "bottleneck": "gpu"
      },
      "flags": [],
      "kept": false
    }
  ],
  "result": {
    "metrics": {
      "frames": 36000,
      "durationS": 90.0,
      "avgFps": 400.0,
      "low1Fps": 280.0,
      "low01Fps": 200.0,
      "stuttersPerMin": 0.4,
      "pcLatencyMs": 9.8,
      "gpuBusyRatio": 0.82,
      "bottleneck": "gpu"
    },
    "spread": {
      "avgFps": 0.0,
      "low1Fps": 0.0,
      "low01Fps": 0.0
    }
  },
  "context": {
    "powerPlan": "EXM Free Power Plan V8",
    "ramMts": 3600,
    "gpu": "NVIDIA GeForce RTX 3060 Ti",
    "gpuDriver": "610.74",
    "windowsBuild": 26200,
    "displayHz": 360,
    "gameSettings": {
      "resolution": [1920, 1080],
      "fullscreenMode": 0,
      "vsync": false,
      "frameRateLimit": 0.0,
      "reflex": 1,
      "multithreadedRendering": true,
      "quality": {
        "sg.ShadowQuality": "0"
      },
      "limits": {},
      "riotSettingsRead": true
    }
  }
}"#;

    #[test]
    fn a_benchmark_saved_by_v0_2_0_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("20261013-191500");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("summary.json"), SAVED_BY_0_2_0).unwrap();
        std::fs::write(
            dir.path().join("index.json"),
            r#"{"ids": ["20261013-191500"], "baseline": "20261013-191500"}"#,
        )
        .unwrap();
        let store = Store::new(dir.path());
        let old = store
            .get("20261013-191500")
            .expect("a v0.2.0 benchmark loads");
        assert_eq!(
            old.context.gpu.as_deref(),
            Some("NVIDIA GeForce RTX 3060 Ti")
        );
        assert_eq!(old.context.windows_build, 26200);
        assert_eq!(
            (old.context.gpu_pnp_device_id, old.context.gpu_link),
            (None, None),
            "the new fields are simply missing"
        );
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert!(list[0].is_baseline);
        assert_eq!(list[0].result.metrics.avg_fps, 400.0);
    }

    #[test]
    fn the_card_and_its_link_survive_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let card = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";
        let mut linked = bench("2026-10-13T20:00:00Z", 410.0);
        linked.context.gpu_pnp_device_id = Some(card.into());
        linked.context.gpu_link = Some(GpuLink { gen: 4, width: 16 });
        let id = store.save(&mut linked, &[]).unwrap();
        let back = store.get(&id).unwrap();
        assert_eq!(back, linked);
        assert_eq!(back.context.gpu_link, Some(GpuLink { gen: 4, width: 16 }));
        // The UI reads these names (camelCase, like every other field).
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.path().join(&id).join("summary.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(json["context"]["gpuPnpDeviceId"], serde_json::json!(card));
        assert_eq!(
            json["context"]["gpuLink"],
            serde_json::json!({"gen": 4, "width": 16})
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel-bench --locked`
Expected: the test build fails to compile: `error[E0432]: unresolved import crate::model::GpuLink` (twice), `error[E0560]: struct model::Context has no field named gpu_pnp_device_id` (and `gpu_link`), `error[E0609]: no field gpu_link on type model::Context`.

- [ ] **Step 3: Implement**

In `crates/kestrel-bench/src/model.rs`, replace

```rust
/// The PC and game when the benchmark ran (spec "Saved data").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Context {
    pub power_plan: Option<String>,
    pub ram_mts: Option<u32>,
    pub gpu: Option<String>,
    pub gpu_driver: Option<String>,
    pub windows_build: u32,
```

with

```rust
/// A PCIe link as Windows reports it: the generation (4 = PCIe 4.0) and the number of lanes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GpuLink {
    pub gen: u8,
    pub width: u8,
}

/// The PC and game when the benchmark ran (spec "Saved data").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Context {
    pub power_plan: Option<String>,
    pub ram_mts: Option<u32>,
    pub gpu: Option<String>,
    pub gpu_driver: Option<String>,
    /// The main graphics card's `Win32_VideoController.PNPDeviceID`, so a link measured on it is only ever applied to
    /// that card. None when the scan had no id for it, and in benchmarks saved by v0.2.0.
    pub gpu_pnp_device_id: Option<String>,
    /// The card's PCIe link during the warm-up and runs, read every 2 s: the highest generation seen, with its lane
    /// count. None when Windows never reported it, and in benchmarks saved by v0.2.0.
    pub gpu_link: Option<GpuLink>,
    pub windows_build: u32,
```

In `crates/kestrel-bench/src/text.rs`, replace the first `use` line

```rust
use crate::model::{Benchmark, Bottleneck, Comparison, Flag, MetricId, Verdict};
```

with

```rust
use crate::model::{Benchmark, Bottleneck, Comparison, Flag, GpuLink, MetricId, Verdict};
```

and the end of `results_text`

```rust
    if !pc.is_empty() {
        lines.push(format!("PC: {}", pc.join(", ")));
    }
    lines.join("\n")
}
```

with

```rust
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
```

In `app/src-tauri/src/bench/commands.rs` (`context_from` names every field, so it would no longer compile), replace

```rust
        gpu_driver: gpu.map(kestrel_core::gpu::display_driver_version),
        windows_build: s.os.build,
```

with

```rust
        gpu_driver: gpu.map(kestrel_core::gpu::display_driver_version),
        // The benchmark doesn't record the card or its PCIe link yet.
        gpu_pnp_device_id: None,
        gpu_link: None,
        windows_build: s.os.build,
```

- [ ] **Step 4: Verify**

Run, from the repo root:
```bash
PATH="$HOME/.cargo/bin:$PATH" cargo fmt --all --check
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel-bench --locked --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel-bench --locked
(cd app && npm run build)
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel --locked
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel --locked --all-targets -- -D warnings
```
Expected: no formatting diff and no clippy warnings; `kestrel-bench`: `test result: ok. 45 passed` (42 before, plus `the_pcie_line_follows_the_pc_line_and_says_when_the_link_wasnt_read`, `a_benchmark_saved_by_v0_2_0_still_loads` and `the_card_and_its_link_survive_a_save_and_a_load`); `kestrel` (the app crate, which embeds `app/dist`, hence the UI build first): `test result: ok. 17 passed`, unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-bench/src/model.rs crates/kestrel-bench/src/text.rs crates/kestrel-bench/src/store.rs app/src-tauri/src/bench/commands.rs
git commit -m "feat(bench): a benchmark records its graphics card and PCIe link; Copy results says the link" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: App — the PCIe link read during passes, every report with the newest link, live CPU clocks

**Files:**
- Modify: `app/src-tauri/src/bench/controller.rs` (`Platform::gpu_link`, link reads in `pace`; tests and their fakes)
- Modify: `app/src-tauri/src/bench/commands.rs` (the real `gpu_link`; `context_from` sets the card; its test)
- Create: `app/src-tauri/src/hardware.rs` (`link_measurements`, `with_links`; tests)
- Modify: `app/src-tauri/src/lib.rs` (reports through `with_links`; `AppState.clocks`; `get_cpu_clocks`)

**Interfaces:**
- Consumes:
  - Task 13: `kestrel_bench::GpuLink { gen: u8, width: u8 }`; `Context.gpu_pnp_device_id: Option<String>`, `Context.gpu_link: Option<GpuLink>`.
  - Task 1: `kestrel_core::{GpuInfo, GpuVendor, PcieLink, PcieInfo, LinkUnderLoad}` with `GpuInfo.pnp_device_id: String`, `GpuInfo.pcie: Option<PcieInfo>`, `GpuInfo.link_under_load: Option<LinkUnderLoad>`; `PcieLink { gen: u8, width: u8 }`; `PcieInfo { card_max, slot_max, now: Option<PcieLink> }`; `LinkUnderLoad { link: PcieLink, measured_at: String, benchmark_id: String }`.
  - Task 3: `kestrel_core::topology::CoreClock { core: u32, kind: Option<CoreKind>, mhz: Option<u32> }` (Serialize).
  - Task 4: `kestrel_core::pcie::LinkMeasurement { pnp_device_id: String, link: PcieLink, measured_at: String, benchmark_id: String }` and `pub fn apply_links(snapshot: &mut SystemSnapshot, measurements: &[LinkMeasurement])` (newest `measured_at` per card, ids compared case-insensitively).
  - Task 6: the `pcie-speed` finding as the contract table words it: Info "Run a benchmark to check the PCIe link under load" without a link; Problem · Low "PCIe link runs at {got} under load instead of {expected}" whose detail starts "Measured during your benchmark on {YYYY-MM-DD}.".
  - Task 10: `kestrel_probe::gpu_link(pnp_device_id: &str) -> Option<kestrel_core::PcieLink>`.
  - Task 12: `kestrel_probe::clocks::ClockSampler` with `new() -> Self` and `sample(&mut self) -> Vec<CoreClock>`; it must be `Send` (it sits in a `Mutex` in Tauri's managed state).
- Produces:
  - `pub trait Platform` gains `fn gpu_link(&self, pnp_device_id: &str) -> Option<GpuLink>;`.
  - A saved benchmark's `context.gpu_link` is the best reading of its warm-up and runs: the highest generation; at the same generation, the most lanes; of equal readings, the first (`fn beats(link: GpuLink, kept: GpuLink) -> bool` in `controller.rs`).
  - `kestrel_app::hardware::link_measurements(store: &Store) -> Vec<LinkMeasurement>` and `kestrel_app::hardware::with_links(report: Report, store: &Store, today: NaiveDate) -> Report`.
  - Tauri command `get_cpu_clocks` → `Vec<CoreClock>`, JSON `[{"core": 0, "kind": null, "mhz": 4800}, …]` (Task 15's `getCpuClocks()`).
  - `get_report`, `rescan` and `get_diagnostics` give the report with `snapshot.gpus[].linkUnderLoad` from the newest benchmark on that card, and findings, score and guide rebuilt to match (Tasks 15–17); new benchmarks save `context.gpuPnpDeviceId` and `context.gpuLink` (Task 17's result line).

- [ ] **Step 1: Write the failing tests**

**`app/src-tauri/src/bench/controller.rs`**, inside `mod tests`:

Replace the imports

```rust
    use super::*;
    use crate::bench::capture::Simulated;
    use std::path::Path;
    use std::sync::atomic::AtomicUsize;
```

with

```rust
    use super::*;
    use crate::bench::capture::{simulated_csv, Simulated};
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::atomic::AtomicUsize;
```

At the end of `struct Fake`, replace

```rust
        unregister_held: AtomicBool,
        release_unregister: AtomicBool,
    }
```

with

```rust
        unregister_held: AtomicBool,
        release_unregister: AtomicBool,
        /// What the card's link reads answer, one per read, in order; None once they run out.
        links: Mutex<VecDeque<Option<GpuLink>>>,
        /// The card each link read asked about.
        link_reads: Mutex<Vec<String>>,
        /// While raised, a link read waits (raising `link_held`) until `release_links` is raised: a read that takes a
        /// while, during which a test can act.
        hold_links: AtomicBool,
        link_held: AtomicBool,
        release_links: AtomicBool,
    }
```

In `impl Platform for Fake`, add the read between `game_settings` and `emit` — replace

```rust
            Some(ValorantSettings {
                resolution: Some([1920, 1080]),
                ..Default::default()
            })
        }
        fn emit(&self, _state: Option<&BenchState>) {
```

with

```rust
            Some(ValorantSettings {
                resolution: Some([1920, 1080]),
                ..Default::default()
            })
        }
        fn gpu_link(&self, pnp_device_id: &str) -> Option<GpuLink> {
            if self.hold_links.load(Ordering::SeqCst) {
                self.link_held.store(true, Ordering::SeqCst);
                while !self.release_links.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            self.link_reads
                .lock()
                .unwrap()
                .push(pnp_device_id.to_string());
            self.links.lock().unwrap().pop_front().flatten()
        }
        fn emit(&self, _state: Option<&BenchState>) {
```

Two new capture fakes, and a `setup` that can take a platform made first. Replace

```rust
    fn setup(capture: Arc<dyn Capture>) -> (Controller, Arc<Fake>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::default());
        fake.key_free.store(true, Ordering::SeqCst);
```

with

```rust
    /// Ends the n-th capture (from 0) once the platform has had n + 1 link reads, or after `limit` when none comes.
    /// The pacer reads at its first step and then only every 2 s, so in a session that knows its card each capture
    /// gets exactly one read, however slowly the threads run.
    struct UntilLinkRead {
        fake: Arc<Fake>,
        limit: Duration,
        runs: AtomicUsize,
    }

    impl Capture for UntilLinkRead {
        fn run(
            &self,
            _p: &str,
            seconds: u32,
            _c: &Path,
            cancel: &AtomicBool,
        ) -> Result<CaptureOutput, String> {
            let reads_wanted = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
            let deadline = Instant::now() + self.limit;
            while self.fake.link_reads.lock().unwrap().len() < reads_wanted
                && Instant::now() < deadline
            {
                if cancel.load(Ordering::SeqCst) {
                    return Err(CANCELLED.into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(CaptureOutput {
                csv: simulated_csv(seconds),
                stderr: String::new(),
            })
        }
        fn clean_up_stale_session(&self) {}
    }

    /// The first capture runs until it's cancelled (raising `started` as it begins); every later one ends at once.
    #[derive(Default)]
    struct FirstBlocks {
        runs: AtomicUsize,
        started: AtomicBool,
    }

    impl Capture for FirstBlocks {
        fn run(
            &self,
            _p: &str,
            seconds: u32,
            _c: &Path,
            cancel: &AtomicBool,
        ) -> Result<CaptureOutput, String> {
            if self.runs.fetch_add(1, Ordering::SeqCst) == 0 {
                self.started.store(true, Ordering::SeqCst);
                while !cancel.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
                return Err(CANCELLED.into());
            }
            Ok(CaptureOutput {
                csv: simulated_csv(seconds),
                stderr: String::new(),
            })
        }
        fn clean_up_stale_session(&self) {}
    }

    fn setup(capture: Arc<dyn Capture>) -> (Controller, Arc<Fake>, tempfile::TempDir) {
        setup_with(Arc::new(Fake::default()), capture)
    }

    /// `setup` with a platform the test made first, for a capture that watches it.
    fn setup_with(
        fake: Arc<Fake>,
        capture: Arc<dyn Capture>,
    ) -> (Controller, Arc<Fake>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        fake.key_free.store(true, Ordering::SeqCst);
```

(The rest of the old `setup` body — `fake.game.store(…)`, the store, `Controller::new(…)`, the returned tuple — stays as it is and is now `setup_with`'s.)

After `fn waiting_at`, add the helpers:

```rust
    const CARD: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";

    fn link(gen: u8, width: u8) -> Option<GpuLink> {
        Some(GpuLink { gen, width })
    }

    /// Presses the start key for the warm-up and the three runs of an armed session; returns the saved result's id.
    fn run_session(c: &Controller) -> String {
        for pass in 0..4u8 {
            wait_for(c, "the next pass", waiting_at(pass));
            c.start_pressed();
        }
        wait_for(c, "the saved result", |s| s.session.result_id.is_some())
            .session
            .result_id
            .unwrap()
    }

    /// A session armed on `CARD` whose link reads answer `links` in order, one read per capture.
    fn session_on_card(links: Vec<Option<GpuLink>>) -> (Controller, Arc<Fake>, tempfile::TempDir) {
        let fake = Arc::new(Fake::default());
        *fake.links.lock().unwrap() = links.into();
        let capture = Arc::new(UntilLinkRead {
            fake: fake.clone(),
            limit: Duration::from_secs(5),
            runs: AtomicUsize::new(0),
        });
        let (c, fake, dir) = setup_with(fake, capture);
        c.start(
            "valorant",
            Context {
                gpu_pnp_device_id: Some(CARD.into()),
                ..Default::default()
            },
        )
        .unwrap();
        (c, fake, dir)
    }
```

and, right after the test `a_whole_session_saves_the_starting_point`, the four tests:

```rust
    #[test]
    fn a_whole_session_saves_the_highest_pcie_link() {
        // One read per capture: the warm-up catches the card still idling at 1.1, the runs read 4.0, 3.0 and nothing.
        let (c, fake, _dir) = session_on_card(vec![link(1, 16), link(4, 16), link(3, 16), None]);
        let id = run_session(&c);
        let saved = c.store().get(&id).unwrap();
        assert_eq!(saved.context.gpu_link, link(4, 16));
        assert_eq!(saved.context.gpu_pnp_device_id.as_deref(), Some(CARD));
        assert_eq!(
            *fake.link_reads.lock().unwrap(),
            vec![CARD.to_string(); 4],
            "one read per capture, of the session's card"
        );
    }

    #[test]
    fn at_the_same_generation_more_lanes_win_and_an_equal_reading_keeps_the_first() {
        // 4.0 x8, then 4.0 x16 (more lanes: it wins), 4.0 x16 again (equal: the first stays), then 4.0 x4.
        let (c, _fake, _dir) =
            session_on_card(vec![link(4, 8), link(4, 16), link(4, 16), link(4, 4)]);
        let id = run_session(&c);
        assert_eq!(c.store().get(&id).unwrap().context.gpu_link, link(4, 16));
        // The rule itself: the generation counts first, then the lanes; an equal reading never replaces the kept one.
        let l = |gen, width| GpuLink { gen, width };
        assert!(beats(l(4, 16), l(4, 8)));
        assert!(
            !beats(l(4, 16), l(4, 16)),
            "the first of equal readings stays"
        );
        assert!(!beats(l(4, 4), l(4, 16)));
        assert!(
            beats(l(5, 1), l(4, 16)),
            "a higher generation wins whatever the lanes"
        );
        assert!(!beats(l(3, 16), l(4, 1)));
    }

    #[test]
    fn without_the_card_s_id_the_link_is_never_read() {
        let fake = Arc::new(Fake::default());
        *fake.links.lock().unwrap() = vec![link(4, 16); 4].into();
        // No read comes, so each capture lasts its 300 ms: long enough for the pacer to take two steps.
        let capture = Arc::new(UntilLinkRead {
            fake: fake.clone(),
            limit: Duration::from_millis(300),
            runs: AtomicUsize::new(0),
        });
        let (c, fake, _dir) = setup_with(fake, capture);
        c.start("valorant", Context::default()).unwrap();
        let id = run_session(&c);
        assert!(fake.link_reads.lock().unwrap().is_empty());
        assert_eq!(c.store().get(&id).unwrap().context.gpu_link, None);
    }

    #[test]
    fn a_late_reading_never_reaches_the_next_session() {
        let fake = Arc::new(Fake::default());
        *fake.links.lock().unwrap() = vec![link(4, 16)].into();
        fake.hold_links.store(true, Ordering::SeqCst);
        let first = Arc::new(FirstBlocks::default());
        let (c, fake, _dir) = setup_with(fake, first.clone());
        c.start(
            "valorant",
            Context {
                gpu_pnp_device_id: Some(CARD.into()),
                ..Default::default()
            },
        )
        .unwrap();
        c.start_pressed();
        // The warm-up's capture is under way (so it is the one that blocks), and its link read is out.
        eventually("the warm-up's capture", || {
            first.started.load(Ordering::SeqCst)
        });
        eventually("the warm-up's link read", || {
            fake.link_held.load(Ordering::SeqCst)
        });
        // The owner cancels while the read is still out, and arms a session that doesn't know its card.
        c.cancel();
        wait_for(&c, "cancelled", |s| s.session.phase == Phase::Cancelled);
        c.start("valorant", Context::default()).unwrap();
        fake.release_links.store(true, Ordering::SeqCst);
        let id = run_session(&c);
        assert_eq!(*fake.link_reads.lock().unwrap(), vec![CARD.to_string()]);
        assert_eq!(
            c.store().get(&id).unwrap().context.gpu_link,
            None,
            "the cancelled session's reading isn't the new one's"
        );
    }
```

**`app/src-tauri/src/bench/commands.rs`**, inside `mod tests`, replace the whole test `the_context_comes_from_the_scan` with:

```rust
    const CARD: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";

    #[test]
    fn the_context_comes_from_the_scan() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let mut snapshot = kestrel_core::owner_pc_fixture();
        for gpu in &mut snapshot.gpus {
            gpu.pnp_device_id = match gpu.vendor {
                kestrel_core::GpuVendor::Nvidia => CARD.into(),
                _ => "ROOT\\DISPLAY\\0000".into(),
            };
        }
        let report = kestrel_core::build_report(snapshot.clone(), date, "test");
        let c = context_from(&report);
        assert_eq!(c.power_plan.as_deref(), Some("Balanced"));
        assert_eq!(c.ram_mts, Some(3600));
        assert_eq!(
            c.gpu.as_deref(),
            Some("NVIDIA GeForce RTX 3060 Ti"),
            "the real GPU, not Parsec's virtual one"
        );
        assert_eq!(c.gpu_driver.as_deref(), Some("610.74"));
        assert_eq!((c.windows_build, c.display_hz), (26200, Some(360)));
        assert!(
            c.game_settings.is_none(),
            "added when the benchmark is saved"
        );
        assert_eq!(
            c.gpu_pnp_device_id.as_deref(),
            Some(CARD),
            "the real card's id, not Parsec's"
        );
        assert!(c.gpu_link.is_none(), "read while the captures run");
        // Windows gave the main card no id: there's no card to read the link of.
        for gpu in &mut snapshot.gpus {
            gpu.pnp_device_id.clear();
        }
        let c = context_from(&kestrel_core::build_report(snapshot, date, "test"));
        assert_eq!(c.gpu_pnp_device_id, None);
    }
```

**`app/src-tauri/src/hardware.rs`** (new; the tests first, the code comes in Step 3):

```rust
//! The graphics card's PCIe link under load. The probe never fills a card's `link_under_load`: it comes from the
//! benchmarks, each of which recorded its card and the best link read during its captures. Every report the app
//! hands out carries the newest one measured on the same card, with the findings, score and guide rebuilt to match.
#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_bench::{Benchmark, Context, GpuLink};
    use kestrel_core::health::{Finding, Severity, Status};
    use kestrel_core::{GpuVendor, LinkUnderLoad, PcieInfo};

    const CARD: &str = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";
    const OTHER_CARD: &str = "PCI\\VEN_1002&DEV_73BF&SUBSYS_23181458&REV_C1\\6&9F8E7D6C&0&00000008";

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 14).unwrap()
    }

    fn pcie(gen: u8, width: u8) -> PcieLink {
        PcieLink { gen, width }
    }

    /// The owner's PC as the probe reports it, its card known by `CARD`: card and slot 4.0 x16, idling at 1.1 now,
    /// and no link under load (the probe never fills that).
    fn scanned() -> Report {
        let mut s = kestrel_core::owner_pc_fixture();
        for gpu in &mut s.gpus {
            gpu.link_under_load = None;
            if gpu.vendor == GpuVendor::Nvidia {
                gpu.pnp_device_id = CARD.into();
                gpu.pcie = Some(PcieInfo {
                    card_max: Some(pcie(4, 16)),
                    slot_max: Some(pcie(4, 16)),
                    now: Some(pcie(1, 16)),
                });
            }
        }
        kestrel_core::build_report(s, today(), "test")
    }

    /// Saves a benchmark that recorded `card` and `link`; returns its id.
    fn save(store: &Store, created_at: &str, card: Option<&str>, link: Option<GpuLink>) -> String {
        let mut bench = Benchmark {
            name: "Benchmark".into(),
            created_at: created_at.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            context: Context {
                gpu_pnp_device_id: card.map(str::to_string),
                gpu_link: link,
                ..Default::default()
            },
            ..Default::default()
        };
        store.save(&mut bench, &[]).unwrap()
    }

    fn finding<'a>(report: &'a Report, id: &str) -> &'a Finding {
        report
            .findings
            .iter()
            .find(|f| f.id == id)
            .unwrap_or_else(|| panic!("no {id} finding"))
    }

    fn card(report: &Report) -> &kestrel_core::GpuInfo {
        report
            .snapshot
            .gpus
            .iter()
            .find(|g| g.pnp_device_id == CARD)
            .unwrap()
    }

    #[test]
    fn only_benchmarks_with_a_card_and_a_link_are_measurements() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let measured = save(
            &store,
            "2026-10-13T20:00:00Z",
            Some(CARD),
            Some(GpuLink { gen: 4, width: 16 }),
        );
        save(&store, "2026-10-13T21:00:00Z", Some(CARD), None);
        save(
            &store,
            "2026-10-13T22:00:00Z",
            None,
            Some(GpuLink { gen: 4, width: 16 }),
        );
        assert_eq!(
            link_measurements(&store),
            vec![LinkMeasurement {
                pnp_device_id: CARD.into(),
                link: pcie(4, 16),
                measured_at: "2026-10-13T20:00:00Z".into(),
                benchmark_id: measured,
            }]
        );
    }

    #[test]
    fn the_newest_benchmark_on_the_same_card_sets_the_link_and_the_findings() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        save(
            &store,
            "2026-10-12T20:00:00Z",
            Some(CARD),
            Some(GpuLink { gen: 4, width: 16 }),
        );
        let newer = save(
            &store,
            "2026-10-13T20:00:00Z",
            Some(&CARD.to_lowercase()),
            Some(GpuLink { gen: 3, width: 16 }),
        );
        // Newer still, but measured on another card: never applied to this one.
        save(
            &store,
            "2026-10-14T09:00:00Z",
            Some(OTHER_CARD),
            Some(GpuLink { gen: 1, width: 8 }),
        );
        let plain = scanned();
        let linked = with_links(plain.clone(), &store, today());
        assert_eq!(
            card(&linked).link_under_load,
            Some(LinkUnderLoad {
                link: pcie(3, 16),
                measured_at: "2026-10-13T20:00:00Z".into(),
                benchmark_id: newer,
            })
        );
        assert!(linked
            .snapshot
            .gpus
            .iter()
            .filter(|g| g.pnp_device_id != CARD)
            .all(|g| g.link_under_load.is_none()));
        let speed = finding(&linked, "pcie-speed");
        assert_eq!(
            (speed.status, speed.severity, speed.title.as_str()),
            (
                Status::Problem,
                Severity::Low,
                "PCIe link runs at 3.0 under load instead of 4.0"
            )
        );
        assert!(speed.detail.contains("2026-10-13"), "{}", speed.detail);
        assert_eq!(
            linked.score.value + 3,
            plain.score.value,
            "a Low problem costs 3 points"
        );
        assert_eq!(linked.app_version, "test");
    }

    #[test]
    fn without_measurements_the_report_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("benchmarks"));
        let plain = scanned();
        let same = with_links(plain.clone(), &store, today());
        assert_eq!(same, plain);
        assert_eq!(
            finding(&same, "pcie-speed").title,
            "Run a benchmark to check the PCIe link under load"
        );
        // A benchmark from v0.2.0 recorded neither the card nor the link: still nothing to apply.
        save(&store, "2026-10-13T19:15:00Z", None, None);
        assert_eq!(with_links(plain.clone(), &store, today()), plain);
    }
}
```

**`app/src-tauri/src/lib.rs`**: so the new tests compile in, replace

```rust
pub mod bench;
```

with

```rust
pub mod bench;
pub mod hardware;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `(cd app && npm run build) && PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel --locked`
Expected: the test build fails to compile: `error[E0407]: method gpu_link is not a member of trait Platform`, `cannot find type GpuLink in this scope` and `cannot find function beats in this scope` (controller tests), `cannot find function with_links in this scope` and `link_measurements`, `cannot find type NaiveDate` / `Store` / `Report` / `PcieLink` and `cannot find struct, variant or union type LinkMeasurement` (hardware tests).

- [ ] **Step 3: Implement**

**`app/src-tauri/src/bench/controller.rs`** — replace the `kestrel_bench` import

```rust
use kestrel_bench::{
    csv, metrics, result, Benchmark, Context, GameProfile, RunRecord, Store, ValorantSettings,
    PRESENTMON_VERSION,
};
```

with

```rust
use kestrel_bench::{
    csv, metrics, result, Benchmark, Context, GameProfile, GpuLink, RunRecord, Store,
    ValorantSettings, PRESENTMON_VERSION,
};
```

replace

```rust
const COUNTDOWN: u32 = 10;
```

with

```rust
const COUNTDOWN: u32 = 10;
/// One step of the pacer: it updates the countdown this often.
const PACE_STEP: Duration = Duration::from_millis(200);
/// The pacer reads the graphics card's PCIe link every this many steps (every 2 s), starting with the first.
const LINK_EVERY: u32 = 10;
```

in `pub trait Platform`, replace

```rust
    fn game_settings(&self) -> Option<ValorantSettings>;
    /// Tells the UI the session changed; None means there's no session.
```

with

```rust
    fn game_settings(&self) -> Option<ValorantSettings>;
    /// The PCIe link of the graphics card with this PCI instance id right now (a device property, nothing from the
    /// game); None when Windows doesn't report it.
    fn gpu_link(&self, pnp_device_id: &str) -> Option<GpuLink>;
    /// Tells the UI the session changed; None means there's no session.
```

and replace the whole `fn pace`

```rust
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
            self.set_seconds_left(
                cancel,
                Some(profile.route.capture_seconds.saturating_sub(elapsed)),
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        self.set_seconds_left(cancel, None);
    }
```

with

```rust
    /// Counts the capture down for the UI, beeps when it's time to switch to the bots, and, when the session knows
    /// its graphics card, reads the card's PCIe link every 2 s (every 10th step, starting with the first).
    fn pace(&self, profile: &GameProfile, done: &AtomicBool, cancel: &Arc<AtomicBool>) {
        let started = Instant::now();
        let mut switched = false;
        let card = self.card(cancel);
        let mut step: u32 = 0;
        while !done.load(Ordering::SeqCst) && !cancel.load(Ordering::SeqCst) {
            let elapsed = started.elapsed().as_secs() as u32;
            if !switched && elapsed >= profile.route.switch_at_seconds {
                switched = true;
                self.platform.beep(Cue::Switch);
            }
            if let Some(card) = card.as_deref().filter(|_| step.is_multiple_of(LINK_EVERY)) {
                // Read with the lock released (see the module docs), then kept if it beats the session's best.
                if let Some(link) = self.platform.gpu_link(card) {
                    self.keep_link(cancel, link);
                }
            }
            step += 1;
            self.set_seconds_left(
                cancel,
                Some(profile.route.capture_seconds.saturating_sub(elapsed)),
            );
            std::thread::sleep(PACE_STEP);
        }
        self.set_seconds_left(cancel, None);
    }

    /// The PCI instance id of the card the session `cancel` belongs to measures; None when the scan had none.
    fn card(&self, cancel: &Arc<AtomicBool>) -> Option<String> {
        self.lock()
            .as_ref()
            .filter(|l| Arc::ptr_eq(&l.cancel, cancel))
            .and_then(|l| l.context.gpu_pnp_device_id.clone())
    }

    /// Keeps `link` in the context of the session `cancel` belongs to when it `beats` the one kept so far.
    fn keep_link(&self, cancel: &Arc<AtomicBool>, link: GpuLink) {
        if let Some(live) = self
            .lock()
            .as_mut()
            .filter(|l| Arc::ptr_eq(&l.cancel, cancel))
        {
            if live.context.gpu_link.is_none_or(|kept| beats(link, kept)) {
                live.context.gpu_link = Some(link);
            }
        }
    }
```

and add the rule as a free function just above the test module, i.e. replace

```rust
#[cfg(test)]
mod tests {
```

with

```rust
/// Whether a link reading beats the one kept so far: a higher generation wins, then, at the same generation, more
/// lanes. An equal reading doesn't, so of equal readings the first is kept.
fn beats(link: GpuLink, kept: GpuLink) -> bool {
    (link.gen, link.width) > (kept.gen, kept.width)
}

#[cfg(test)]
mod tests {
```

The pacer only runs while a capture runs (warm-up and runs), and `spawn_capture` joins it before the capture's result reaches the session, so the context `finished_from` copies into the saved benchmark already holds every reading.

**`app/src-tauri/src/bench/commands.rs`** — replace the `kestrel_bench` import

```rust
use kestrel_bench::{
    result, settings, text, Benchmark, BenchmarkSummary, Comparison, Context, GameProfile, Store,
    ValorantSettings, PROFILES,
};
```

with

```rust
use kestrel_bench::{
    result, settings, text, Benchmark, BenchmarkSummary, Comparison, Context, GameProfile, GpuLink,
    Store, ValorantSettings, PROFILES,
};
```

in `impl Platform for TauriPlatform`, replace

```rust
    fn emit(&self, state: Option<&BenchState>) {
        let _ = self.app.emit("bench-state", state);
    }
```

with

```rust
    fn gpu_link(&self, pnp_device_id: &str) -> Option<GpuLink> {
        kestrel_probe::gpu_link(pnp_device_id).map(|link| GpuLink {
            gen: link.gen,
            width: link.width,
        })
    }

    fn emit(&self, state: Option<&BenchState>) {
        let _ = self.app.emit("bench-state", state);
    }
```

and in `context_from` replace Task 13's placeholder

```rust
        // The benchmark doesn't record the card or its PCIe link yet.
        gpu_pnp_device_id: None,
        gpu_link: None,
```

with

```rust
        // The card whose PCIe link the captures read; an empty id means Windows didn't name it.
        gpu_pnp_device_id: gpu
            .map(|g| g.pnp_device_id.clone())
            .filter(|id| !id.is_empty()),
        // Read while the captures run (Controller::pace).
        gpu_link: None,
```

**`app/src-tauri/src/hardware.rs`** — insert the code between the module doc comment and `#[cfg(test)]`:

```rust
use chrono::NaiveDate;
use kestrel_bench::Store;
use kestrel_core::pcie::{apply_links, LinkMeasurement};
use kestrel_core::{PcieLink, Report};

/// One measurement per saved benchmark that recorded both its card and a link, dated when the benchmark was saved.
pub fn link_measurements(store: &Store) -> Vec<LinkMeasurement> {
    store
        .list()
        .iter()
        .filter_map(|summary| store.get(&summary.id))
        .filter_map(|bench| {
            let pnp_device_id = bench.context.gpu_pnp_device_id?;
            let link = bench.context.gpu_link?;
            Some(LinkMeasurement {
                pnp_device_id,
                link: PcieLink {
                    gen: link.gen,
                    width: link.width,
                },
                measured_at: bench.created_at,
                benchmark_id: bench.id,
            })
        })
        .collect()
}

/// `report` with each card's newest link under load applied, and its findings, score and guide rebuilt for `today`.
pub fn with_links(report: Report, store: &Store, today: NaiveDate) -> Report {
    let Report {
        app_version,
        mut snapshot,
        ..
    } = report;
    apply_links(&mut snapshot, &link_measurements(store));
    kestrel_core::build_report(snapshot, today, &app_version)
}

```

**`app/src-tauri/src/lib.rs`** — the whole file becomes:

```rust
//! Kestrel's Tauri shell: the scan, live load and clocks, and the benchmark with the PCIe link it measures. It writes
//! only inside its own folder.
use chrono::NaiveDate;
use kestrel_core::topology::CoreClock;
use kestrel_core::Report;
use kestrel_probe::clocks::ClockSampler;
use kestrel_probe::load::{LoadSample, LoadSampler};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

pub mod bench;
pub mod hardware;

use bench::commands;
use bench::controller::Controller;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct AppState {
    /// The last scan as the probe returned it; `None` until the first one finishes. Commands hand it out through
    /// `hardware::with_links`, so a benchmark saved since the scan shows at once.
    report: Mutex<Option<Report>>,
    /// One sampler for the whole run, created and primed in `setup`: its first reading is unreliable.
    load: Mutex<LoadSampler>,
    /// One sampler for the whole run, created in `setup`: it collects once then, so the first poll has a baseline.
    clocks: Mutex<ClockSampler>,
}

/// The day the findings are judged on (BIOS and driver age).
fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// The last report, with the newest PCIe link under load; scans first if there isn't one yet.
#[tauri::command]
async fn get_report(
    state: State<'_, AppState>,
    bench: State<'_, Controller>,
) -> Result<Report, String> {
    let cached = state.report.lock().map_err(|e| e.to_string())?.clone();
    match cached {
        Some(report) => Ok(hardware::with_links(report, bench.store(), today())),
        None => rescan(state, bench).await,
    }
}

/// Scans the PC again (on a worker thread) and keeps the probe's report; returns it with the newest link under load.
#[tauri::command]
async fn rescan(
    state: State<'_, AppState>,
    bench: State<'_, Controller>,
) -> Result<Report, String> {
    let report = tauri::async_runtime::spawn_blocking(|| kestrel_probe::collect_report(VERSION))
        .await
        .map_err(|e| e.to_string())?;
    *state.report.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(hardware::with_links(report, bench.store(), today()))
}

/// The last scan, scanning first if needed: a benchmark records the PC's context from it.
pub(crate) async fn latest_report(app: &AppHandle) -> Result<Report, String> {
    get_report(app.state::<AppState>(), app.state::<Controller>()).await
}

/// One live-load reading; the UI polls this about once a second.
// Keep `async`: the PDH and NVML reads must stay off the UI thread.
#[tauri::command(async)]
fn get_load(state: State<'_, AppState>) -> Result<LoadSample, String> {
    let mut sampler = state.load.lock().map_err(|e| e.to_string())?;
    Ok(sampler.sample())
}

/// Each physical core's clock right now; the Hardware page polls this about once a second while it's shown.
// Keep `async`: the PDH read must stay off the UI thread.
#[tauri::command(async)]
fn get_cpu_clocks(state: State<'_, AppState>) -> Result<Vec<CoreClock>, String> {
    let mut sampler = state.clocks.lock().map_err(|e| e.to_string())?;
    Ok(sampler.sample())
}

/// Pretty JSON for the "Copy diagnostics" button: the last scan (with the newest link under load, as the app shows it),
/// the saved benchmarks, VALORANT's settings, and the benchmark session as it is now (a failed one carries
/// PresentMon's error), or null when there is none.
#[tauri::command(async)]
fn get_diagnostics(
    state: State<'_, AppState>,
    bench: State<'_, Controller>,
) -> Result<String, String> {
    let report = state
        .report
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Run a scan first.")?;
    let diagnostics = serde_json::json!({
        "report": hardware::with_links(report, bench.store(), today()),
        "benchmarks": bench.store().list(),
        "valorantSettings": bench.game_settings(),
        // An owned snapshot: no platform call, and the controller's lock is released before this is serialised.
        "benchSession": bench.state(),
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
                    if event.state() == ShortcutState::Pressed && *shortcut == commands::start_key()
                    {
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
                clocks: Mutex::new(ClockSampler::new()),
            });
            app.manage(commands::controller(app.handle())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_report,
            rescan,
            get_load,
            get_cpu_clocks,
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

The cache keeps the probe's own report (its `link_under_load` is always None), and every command applies the saved benchmarks on the way out, so a benchmark saved since the last scan (or one deleted) shows on the next `get_report` — the UI asks again on every tab change (Task 16). `get_report` and `rescan` gain a `State<'_, Controller>` argument, which Tauri fills itself; the UI's calls don't change.

- [ ] **Step 4: Verify**

Run, from the repo root:
```bash
PATH="$HOME/.cargo/bin:$PATH" cargo fmt --all --check
(cd app && npm run build)
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel --locked
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel --locked
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel --locked --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel-bench --locked
```
Expected: no formatting diff; `kestrel`: `test result: ok. 24 passed` both times (17 before, plus the 4 controller tests `a_whole_session_saves_the_highest_pcie_link`, `at_the_same_generation_more_lanes_win_and_an_equal_reading_keeps_the_first`, `without_the_card_s_id_the_link_is_never_read`, `a_late_reading_never_reaches_the_next_session` and the 3 `hardware::tests`; `the_context_comes_from_the_scan` is updated in place), about 2 s each; no clippy warnings; `kestrel-bench`: 45 passed. The Windows-only parts of the app crate are linted by CI's `windows` job (`cargo clippy -p kestrel --locked --all-targets -- -D warnings` there).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/bench/controller.rs app/src-tauri/src/bench/commands.rs app/src-tauri/src/hardware.rs app/src-tauri/src/lib.rs
git commit -m "feat(app): read the PCIe link during benchmark passes, give every report the newest link, live CPU clocks" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: UI data layer — schema-2 types, the clocks call, hardware helpers and Copy specs text

**Files:**
- Regenerate: `app/src/api/mock-report.json` (`npm run mock`, Step 1: the browser preview's sample becomes schema 2)
- Modify: `app/src/api/types.ts` (schema-2 snapshot fields, the `security` and `network` parts, `CoreClock`, `PcieLink`, `PcieInfo`, `LinkUnderLoad`, two `BiosTopic` values, the benchmark context's `gpuPnpDeviceId` / `gpuLink`)
- Modify: `app/src/api/client.ts` (`getCpuClocks`)
- Modify: `app/src/api/sample-bench.ts` (the sample benchmarks' context gets the two new fields, which `BenchContext` now requires)
- Modify: `app/src/api/findings.ts` (`cpuName`, split out of `cpuLabel`: Copy specs needs the name without `· 8C/16T`)
- Modify: `app/src/strings.ts` (`hardwarePage` and `specs` strings the helpers use; Task 16 adds the page's own)
- Create: `app/src/api/hardware.ts` (pure formatting helpers for the Hardware tab and Copy specs)
- Create: `app/src/api/specs.ts` (`specsText`)
- Create: `app/src/api/owner-report.ts` (the owner's PC as a schema-2 `Report`, for tests only)
- Test: `app/src/api/hardware.test.ts`, `app/src/api/specs.test.ts`, `app/src/api/client.test.ts` (new); `app/src/api/findings.test.ts` (one assertion)

**Interfaces:**
- Consumes:
  - Task 1's schema-2 snapshot as the app serialises it (camelCase fields, kebab-case enums): `CpuInfo { …, family, model, stepping, codename, socket, baseMhz, maxMhz, pCores, eCores, caches: CacheInfo[], instructions: string[], virtualization }`, `CacheInfo { level, kind: "data"|"instruction"|"unified"|"unknown", sizeKb, count }`, `BoardInfo { …, biosVendor, chipset, bootMode: "uefi"|"legacy"|"unknown" }`, `MemoryInfo.slotsTotal`, `MemoryInfo.ratedMts` (Task 11 fills it on Windows; the fixture has 3600), `MemoryModule.rank`, `GpuInfo { …, pnpDeviceId, pcie: PcieInfo|null, linkUnderLoad: LinkUnderLoad|null, largestBarMb, resizableBar }`, `PcieLink { gen, width }`, `PcieInfo { cardMax, slotMax, now }`, `LinkUnderLoad { link, measuredAt, benchmarkId }`, `DisplayInfo.monitorName` / `.connection`, `StorageInfo.drives: DriveInfo[]`, `DriveInfo { model, media, bus: "nvme"|"sata"|"usb"|"sas"|"raid"|"virtual"|"other", sizeGb, volumes }`, `VolumeInfo { letter, freeGb, totalGb }` (sizes in GiB).
  - Task 3: `CoreClock { core: u32, kind: Option<CoreKind>, mhz: Option<u32> }` → `{ core, kind: "performance"|"efficiency"|null, mhz }`.
  - Task 6: `BiosTopic::PcieSlot` → `"pcie-slot"`, `BiosTopic::Virtualization` → `"virtualization"`; finding ids `pcie-lanes`, `pcie-speed`, `resizable-bar`, `virtualization`; the existing `ram-speed` and `ram-channels` statuses (and `ram-speed`'s title, "RAM runs at 2133 MT/s, but the kit is rated 3600", which the XMP row shows as its note when it is a Problem; nothing parses it).
  - Tasks 1–14's mock probe: `npm run mock` prints the owner fixture as a schema-2 report with the 31 checks.
  - Task 13: `Context { …, gpu_pnp_device_id: Option<String>, gpu_link: Option<GpuLink> }`, `GpuLink { gen: u8, width: u8 }` → `gpuPnpDeviceId`, `gpuLink`.
  - Task 14: the Tauri command `get_cpu_clocks` → `Vec<CoreClock>`.
- Produces (Tasks 16 and 17 rely on these exact names):
  - `types.ts`: `BiosTopic` (+ `'pcie-slot' | 'virtualization'`), `PcieLink`, `PcieInfo`, `LinkUnderLoad`, `GpuInfo`, `MemoryModule`, `DisplayInfo`, `CacheKind`, `CacheInfo`, `CpuInfo`, `BoardInfo`, `DriveMedia`, `DriveBus`, `VolumeInfo`, `DriveInfo`, `SecurityInfo`, `NetAdapter`, `ProbeError`, `CoreClock`; `SystemSnapshot` gains `security` and `network`; `BenchContext` gains `gpuPnpDeviceId: string | null` and `gpuLink: PcieLink | null`.
  - `client.ts`: `getCpuClocks(): Promise<CoreClock[]>`.
  - `findings.ts`: `cpuName(s: SystemSnapshot): string`.
  - `hardware.ts`: `ghz(mhz: number): string`, `genLabel(gen: number): string`, `linkLabel(link: PcieLink): string`, `kbLabel(kb: number): string`, `cacheLabel(c: CacheInfo): string`, `cacheName(c: CacheInfo): string`, `cacheRows(caches: CacheInfo[]): [string, string][]`, `sizeLabel(gib: number): string`, `coresLabel(cpu: CpuInfo): string`, `ramSpeed(s: SystemSnapshot): number | null`, `memorySummary(s: SystemSnapshot): string | null`, `ramChannels(report: Report): 'dual' | 'single' | null`, `memoryProfile(report: Report): string`, `boardMaker(report: Report): string`, `gpuShortName(name: string): string`, `separateGpu(gpus: GpuInfo[]): GpuInfo | undefined`, `summaryLine(s: SystemSnapshot): string`, `windowsLine(s: SystemSnapshot): string`, `dayMonth(iso: string): string | null`, `dayMonthTime(iso: string): string | null`, `vramLabel(mb: number): string`, `linkSpeedLabel(mbps: number): string`, `mediaLabel(media: DriveMedia): string | null`, `busLabel(bus: DriveBus): string`, `displayName(d: DisplayInfo, n: number): string`, `slotLabel(m: MemoryModule): string | null`, `gamesOn(s: SystemSnapshot, letter: string): string[]`, `type PanelId`, `panelFailed(errors: ProbeError[], panel: PanelId): boolean`, `shouldPoll(tabShown: boolean, visible: boolean, focused: boolean): boolean`, `type SettingState`, `interface BiosSetting { label; value: string | null; state; problem; note; topic }`, `biosSettings(report: Report): BiosSetting[]`.
  - `specs.ts`: `specsText(report: Report): string`.
  - `owner-report.ts`: `ownerReport(): Report` (a fresh copy per call).
  - `strings.ts`: `strings.hardwarePage.{notReported, months, build, cores, hybridCores, threads, cache, rank, media, bus, unnamedDisplay, games, makers, profiles, settings}`, `strings.specs.{labels, driver, pcie, channels, bios, display}`.
  - A schema-2 `app/src/api/mock-report.json` (Tasks 16 and 17's preview and Task 17's `sample-report.test.ts`).

Background for this task:
- Every user-facing word lives in `app/src/strings.ts` (units such as `MT/s`, `GHz`, `GB` stay inline, as in `findings.ts`).
- Vitest runs in node (`environment: 'node'` in `vite.config.ts`), with no DOM library, so everything worth testing is a pure function.
- `app/src/api/mock-report.json` (the browser preview's sample) is regenerated in Step 1, but these tests never read it: they use `ownerReport()` from the new `owner-report.ts`, so they pin exact text whatever the sample holds, and its times carry no zone so they read the same local time on any machine. Vitest doesn't type-check, so the fixture and tests run before the types exist; `tsc` passes after Step 4.

- [ ] **Step 1: Regenerate the browser preview's sample**

Run: `cd app && npm run mock`

This runs `cargo run -q -p kestrel-probe --bin kestrel-probe -- --pretty > src/api/mock-report.json`; on the Mac the probe prints the owner fixture through Tasks 1–14's code, so the sample is schema 2 before any UI code reads the new fields. Check the new file: `"schemaVersion": 2`, 31 findings (`pcie-lanes` Pass "Graphics card uses all its PCIe lanes", `pcie-speed` Info "Run a benchmark to check the PCIe link under load", `resizable-bar` Pass, `virtualization` Pass), the power plan's fix with `"stage": 5` and adapter power saving's with `"stage": 6` (their texts name the same stages), the RTX 3060 Ti with `pcie` 4.0 x16 / 4.0 x16 / now 1.0 x16 and `"linkUnderLoad": null`, `"ratedMts": 3600`, two displays, two drives, the guide's seven topics in the order xmp, memory-gear, update-bios, pcie-slot, resizable-bar, virtualization, iommu. `collectedAt` is now and `appVersion` stays `0.2.0` until Task 18 bumps the workspace version.

Then run `cd app && npm test`. Expected: PASS — `Test Files  5 passed (5)`, `Tests  18 passed (18)`. The tests that read the sample still hold: `findings.test.ts` keeps its three problems (power-plan, bios-age, adapter-power), because the four new checks are Pass or Info on the owner's PC, and its labels and categories are unchanged; `format.test.ts` reads only the OS label. If one of them fails, compare the sample with `fixtures/owner-pc.json`: the sample (that is, Tasks 1–14) is what's wrong, not the test.

- [ ] **Step 2: Write the failing tests**

Create `app/src/api/owner-report.ts` (test data; nothing in the app imports it, so it never reaches the bundle):

```ts
// The owner's PC as a full schema-2 report, for unit tests: the values of crates/kestrel-core/fixtures/owner-pc.json
// plus the link measured by a benchmark. Tests build on this rather than mock-report.json (the browser preview's
// sample), so they pin exact text whatever that sample holds. Times carry no zone, so they read the same local time
// on any machine. Each call returns a fresh copy, free to change.
import type { Finding, Report } from './types';

const check = (id: string, category: Finding['category'], severity: Finding['severity'], status: Finding['status'], title: string, detail = '', fix: Finding['fix'] = { kind: 'nothing' }): Finding =>
  ({ id, category, severity, status, title, detail, fix });

export function ownerReport(): Report {
  return {
    appVersion: '0.3.0',
    snapshot: {
      schemaVersion: 2,
      collectedAt: '2026-10-13T21:04:00',
      os: { caption: 'Microsoft Windows 11 Pro', build: 26200, displayVersion: '25H2' },
      machine: { formFactor: 'desktop', manufacturer: 'ASUS', model: '', onBattery: null },
      cpu: {
        name: '11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz', vendor: 'intel', cores: 8, threads: 16,
        family: 6, model: 167, stepping: 1, codename: 'Rocket Lake-S', socket: 'LGA1200', baseMhz: 3500, maxMhz: 5300,
        pCores: null, eCores: null,
        caches: [
          { level: 1, kind: 'data', sizeKb: 48, count: 8 },
          { level: 1, kind: 'instruction', sizeKb: 32, count: 8 },
          { level: 2, kind: 'unified', sizeKb: 512, count: 8 },
          { level: 3, kind: 'unified', sizeKb: 16384, count: 1 },
        ],
        instructions: ['SSE4.2', 'AVX', 'AVX2', 'AVX-512'],
        virtualization: true,
      },
      gpus: [
        {
          name: 'Parsec Virtual Display Adapter', vendor: 'virtual', driverVersion: '0.45.0.0', driverDate: null, vramMb: null,
          isIntegrated: false, pnpDeviceId: 'ROOT\\DISPLAY\\0000', pcie: null, linkUnderLoad: null, largestBarMb: null, resizableBar: null,
        },
        {
          name: 'NVIDIA GeForce RTX 3060 Ti', vendor: 'nvidia', driverVersion: '32.0.16.1074', driverDate: '2026-09-15', vramMb: 8192,
          isIntegrated: false, pnpDeviceId: 'PCI\\VEN_10DE&DEV_2489&SUBSYS_88221043&REV_A1\\4&2E1F5C5A&0&0008',
          pcie: { cardMax: { gen: 4, width: 16 }, slotMax: { gen: 4, width: 16 }, now: { gen: 1, width: 16 } },
          linkUnderLoad: { link: { gen: 4, width: 16 }, measuredAt: '2026-10-13T21:40:00', benchmarkId: 'sample-1' },
          largestBarMb: 8192, resizableBar: true,
        },
      ],
      memory: {
        totalMb: 32768, ddrGeneration: 4, slotsTotal: 4, ratedMts: 3600,
        modules: [
          { capacityMb: 16384, configuredMts: 3600, maxMts: 2133, partNumber: 'CMG32GX4M2D3600C18', manufacturer: 'Corsair', bankLabel: 'BANK 1', deviceLocator: 'ChannelA-DIMM2', rank: 2 },
          { capacityMb: 16384, configuredMts: 3600, maxMts: 2133, partNumber: 'CMG32GX4M2D3600C18', manufacturer: 'Corsair', bankLabel: 'BANK 3', deviceLocator: 'ChannelB-DIMM2', rank: 2 },
        ],
      },
      displays: [
        { name: '\\\\.\\DISPLAY1', adapter: 'NVIDIA GeForce RTX 3060 Ti', currentHz: 360, maxHz: 360, width: 1920, height: 1080, isPrimary: true, isVirtual: false, monitorName: 'XL2566K', connection: 'DisplayPort' },
        { name: '\\\\.\\DISPLAY2', adapter: 'NVIDIA GeForce RTX 3060 Ti', currentHz: 180, maxHz: 180, width: 1920, height: 1080, isPrimary: false, isVirtual: false, monitorName: '24G4', connection: 'DisplayPort' },
      ],
      board: {
        manufacturer: 'ASUSTeK COMPUTER INC.', product: 'ROG MAXIMUS XIII HERO', biosVersion: '2103', biosDate: '2024-05-20',
        biosVendor: 'American Megatrends Inc.', chipset: 'Intel Z590', bootMode: 'uefi',
      },
      storage: {
        valorant: { path: 'C:/Riot Games/VALORANT/live', driveLetter: 'C', media: 'nvme', freeGb: 210.5, totalGb: 931.5 },
        drives: [
          { model: 'Samsung SSD 980 PRO 1TB', media: 'nvme', bus: 'nvme', sizeGb: 931.5, volumes: [{ letter: 'C', freeGb: 210.5, totalGb: 931.5 }] },
          { model: 'WDC WD20EZBX-00AYRA0', media: 'hdd', bus: 'sata', sizeGb: 1863, volumes: [{ letter: 'D', freeGb: 1228.8, totalGb: 1863 }] },
        ],
      },
      security: { tpm2: true, secureBoot: true, vbsRunning: false, hvciRunning: false, iommu: false, vanguard: 'running' },
      network: {
        adapters: [{ name: 'Intel(R) Ethernet Controller (3) I225-V', kind: 'ethernet', linkMbps: 2500, connected: true, powerSaving: true }],
        deliveryOptimizationInternet: false,
      },
      probeErrors: [],
    },
    findings: [
      check('ram-speed', 'hardware', 'high', 'pass', 'RAM runs at 3600 MT/s, its rated speed', 'The kit is rated 3600 MT/s.'),
      check('ram-channels', 'hardware', 'high', 'pass', 'RAM runs in dual channel'),
      check('bios-age', 'hardware', 'low', 'problem', 'BIOS is from 2024-05-20', '', { kind: 'bios-guide', topic: 'update-bios' }),
      check('pcie-lanes', 'hardware', 'medium', 'pass', 'Graphics card uses all its PCIe lanes', 'x16, as the card supports.'),
      check('pcie-speed', 'hardware', 'low', 'pass', 'PCIe link runs at full speed under load', 'PCIe 4.0 during your benchmark on 2026-10-13.'),
      check('resizable-bar', 'hardware', 'low', 'pass', 'Resizable BAR is on'),
      check('tpm', 'vanguard', 'high', 'pass', 'TPM 2.0 is on'),
      check('secure-boot', 'vanguard', 'high', 'pass', 'Secure Boot is on'),
      check('virtualization', 'vanguard', 'info', 'pass', 'Virtualization is on'),
      check('iommu', 'vanguard', 'info', 'info', 'IOMMU is off', '', { kind: 'bios-guide', topic: 'iommu' }),
      check('adapter-power', 'network', 'low', 'problem', 'Network adapter power saving is on', '', { kind: 'later-stage', stage: 6 }),
    ],
    score: { value: 86, problems: 3, potentialGain: 14 },
    guide: { vendor: 'asus', board: 'ROG MAXIMUS XIII HERO', enterKey: 'Del', tip: '', sections: [] },
  };
}
```

Create `app/src/api/hardware.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  biosSettings, boardMaker, busLabel, cacheLabel, cacheName, cacheRows, coresLabel, dayMonth, dayMonthTime, displayName, gamesOn,
  genLabel, ghz, gpuShortName, kbLabel, linkLabel, linkSpeedLabel, mediaLabel, memoryProfile, memorySummary, panelFailed,
  ramChannels, ramSpeed, separateGpu, shouldPoll, sizeLabel, slotLabel, summaryLine, vramLabel, windowsLine,
} from './hardware';
import { ownerReport } from './owner-report';
import type { CacheInfo, Finding, Report } from './types';

/** The report with some Health Check items changed, by id. */
function withFindings(report: Report, changes: Record<string, Partial<Finding>>): Report {
  return { ...report, findings: report.findings.map((f) => ({ ...f, ...changes[f.id] })) };
}

describe('hardware formatting', () => {
  it('formats clocks and PCIe links', () => {
    expect(ghz(4800)).toBe('4.80 GHz');
    expect(ghz(5312)).toBe('5.31 GHz');
    expect(genLabel(4)).toBe('4.0');
    expect(linkLabel({ gen: 4, width: 16 })).toBe('4.0 x16');
    expect(linkLabel({ gen: 1, width: 8 })).toBe('1.0 x8');
  });

  it('names and sizes caches, one row per name', () => {
    expect(kbLabel(48)).toBe('48 KB');
    expect(kbLabel(16384)).toBe('16 MB');
    expect(kbLabel(1280)).toBe('1.25 MB');
    expect(cacheLabel({ level: 2, kind: 'unified', sizeKb: 512, count: 8 })).toBe('8 × 512 KB');
    expect(cacheLabel({ level: 3, kind: 'unified', sizeKb: 16384, count: 1 })).toBe('16 MB');
    expect(cacheName({ level: 1, kind: 'data', sizeKb: 48, count: 8 })).toBe('L1 data');
    expect(cacheName({ level: 1, kind: 'instruction', sizeKb: 32, count: 8 })).toBe('L1 instruction');
    expect(cacheName({ level: 3, kind: 'unified', sizeKb: 16384, count: 1 })).toBe('L3');
    expect(cacheRows(ownerReport().snapshot.cpu.caches)).toEqual([
      ['L1 data', '8 × 48 KB'], ['L1 instruction', '8 × 32 KB'], ['L2', '8 × 512 KB'], ['L3', '16 MB'],
    ]);
    // A Core i9-13900K in kestrel-core's order: 8 P-cores, and 16 E-cores whose L2 is shared by each cluster of four.
    const hybrid: CacheInfo[] = [
      { level: 1, kind: 'data', sizeKb: 48, count: 8 }, { level: 1, kind: 'data', sizeKb: 32, count: 16 },
      { level: 1, kind: 'instruction', sizeKb: 64, count: 16 }, { level: 1, kind: 'instruction', sizeKb: 32, count: 8 },
      { level: 2, kind: 'unified', sizeKb: 4096, count: 4 }, { level: 2, kind: 'unified', sizeKb: 2048, count: 8 },
      { level: 3, kind: 'unified', sizeKb: 36864, count: 1 },
    ];
    expect(cacheRows(hybrid)).toEqual([
      ['L1 data', '8 × 48 KB + 16 × 32 KB'], ['L1 instruction', '16 × 64 KB + 8 × 32 KB'], ['L2', '4 × 4 MB + 8 × 2 MB'], ['L3', '36 MB'],
    ]);
    expect(cacheRows([])).toEqual([]);
  });

  it('sizes drives the way Windows Explorer does', () => {
    expect(sizeLabel(931.5)).toBe('931 GB');
    expect(sizeLabel(210.5)).toBe('210 GB');
    expect(sizeLabel(1863)).toBe('1.81 TB');
    expect(sizeLabel(1228.8)).toBe('1.20 TB');
    expect(sizeLabel(57.36)).toBe('57.3 GB');
    expect(sizeLabel(7.459)).toBe('7.45 GB');
    expect(sizeLabel(0)).toBe('0.00 GB');
  });

  it('counts cores, a hybrid CPU’s by kind', () => {
    const cpu = ownerReport().snapshot.cpu;
    expect(coresLabel(cpu)).toBe('8 cores · 16 threads');
    expect(coresLabel({ ...cpu, cores: 24, threads: 32, pCores: 8, eCores: 16 })).toBe('8 P-cores + 16 E-cores · 32 threads');
    expect(coresLabel({ ...cpu, threads: 0 })).toBe('8 cores');
  });

  it('sums up memory at the slowest module’s speed', () => {
    const s = ownerReport().snapshot;
    expect(ramSpeed(s)).toBe(3600);
    expect(memorySummary(s)).toBe('32 GB DDR4-3600');
    const slower = { ...s, memory: { ...s.memory, modules: [s.memory.modules[0], { ...s.memory.modules[1], configuredMts: 3200 }] } };
    expect(memorySummary(slower)).toBe('32 GB DDR4-3200');
    expect(memorySummary({ ...s, memory: { ...s.memory, ddrGeneration: null } })).toBe('32 GB 3600 MT/s');
    const noSpeed = { ...s, memory: { ...s.memory, modules: s.memory.modules.map((m) => ({ ...m, configuredMts: null })) } };
    expect(ramSpeed(noSpeed)).toBeNull();
    expect(memorySummary(noSpeed)).toBe('32 GB DDR4');
    expect(memorySummary({ ...s, memory: { ...s.memory, totalMb: 0 } })).toBeNull();
  });

  it('reads the channels from the RAM channel check', () => {
    const owner = ownerReport();
    expect(ramChannels(owner)).toBe('dual');
    expect(ramChannels(withFindings(owner, { 'ram-channels': { status: 'problem' } }))).toBe('single');
    expect(ramChannels(withFindings(owner, { 'ram-channels': { status: 'unknown' } }))).toBeNull();
  });

  it('names the memory profile and the board maker the way the BIOS does', () => {
    const owner = ownerReport();
    expect(memoryProfile(owner)).toBe('XMP');
    const amd = (ddr: number | null, vendor: Report['guide']['vendor']): Report => ({
      ...owner,
      snapshot: { ...owner.snapshot, cpu: { ...owner.snapshot.cpu, vendor: 'amd' }, memory: { ...owner.snapshot.memory, ddrGeneration: ddr } },
      guide: { ...owner.guide, vendor },
    });
    expect(memoryProfile(amd(5, 'asus'))).toBe('EXPO');
    expect(memoryProfile(amd(4, 'asus'))).toBe('D.O.C.P.');
    expect(memoryProfile(amd(null, 'msi'))).toBe('A-XMP');
    expect(memoryProfile(amd(4, 'gigabyte'))).toBe('XMP');
    expect(boardMaker(owner)).toBe('ASUS');
    const dell = { ...owner, guide: { ...owner.guide, vendor: 'other' as const }, snapshot: { ...owner.snapshot, board: { ...owner.snapshot.board, manufacturer: ' Dell Inc. ' } } };
    expect(boardMaker(dell)).toBe('Dell Inc.');
  });

  it('shortens graphics card names and finds the separate card', () => {
    expect(gpuShortName('NVIDIA GeForce RTX 3060 Ti')).toBe('RTX 3060 Ti');
    expect(gpuShortName('AMD Radeon RX 7800 XT')).toBe('RX 7800 XT');
    expect(gpuShortName('Intel(R) Arc(TM) A770 Graphics')).toBe('Arc A770 Graphics');
    expect(gpuShortName('AMD Radeon(TM) Graphics')).toBe('Radeon Graphics');
    const gpus = ownerReport().snapshot.gpus;
    expect(separateGpu(gpus)?.name).toBe('NVIDIA GeForce RTX 3060 Ti');
    expect(separateGpu([{ ...gpus[1], name: 'Intel(R) UHD Graphics 770', vendor: 'intel', isIntegrated: true }])).toBeUndefined();
    expect(separateGpu([gpus[0]])).toBeUndefined();
  });

  it('sums up the PC and Windows in one line each', () => {
    const s = ownerReport().snapshot;
    expect(summaryLine(s)).toBe('Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO');
    expect(summaryLine({ ...s, gpus: [], board: { ...s.board, product: '' } })).toBe('Core i9-11900K · 32 GB DDR4-3600');
    expect(windowsLine(s)).toBe('Windows 11 Pro 25H2 · build 26200');
    expect(windowsLine({ ...s, os: { caption: '', build: 0, displayVersion: null } })).toBe('');
  });

  it('writes dates in local time, day first', () => {
    expect(dayMonth('2026-10-13T21:40:00')).toBe('13 Oct');
    expect(dayMonthTime('2026-10-13T21:04:00')).toBe('13 Oct 21:04');
    expect(dayMonthTime('2026-01-02T03:05:00')).toBe('2 Jan 03:05');
    expect(dayMonth('')).toBeNull();
    expect(dayMonthTime('not a date')).toBeNull();
  });

  it('labels VRAM, link speeds, drives, displays and memory slots', () => {
    expect(vramLabel(8192)).toBe('8 GB');
    expect(vramLabel(512)).toBe('512 MB');
    expect(linkSpeedLabel(2500)).toBe('2.5 Gbps');
    expect(linkSpeedLabel(1000)).toBe('1 Gbps');
    expect(linkSpeedLabel(100)).toBe('100 Mbps');
    expect(mediaLabel('nvme')).toBe('NVMe');
    expect(mediaLabel('hdd')).toBe('HDD');
    expect(mediaLabel('unknown')).toBeNull();
    expect(busLabel('sata')).toBe('SATA');
    expect(busLabel('usb')).toBe('USB');
    const [monitor] = ownerReport().snapshot.displays;
    expect(displayName(monitor, 1)).toBe('XL2566K');
    expect(displayName({ ...monitor, monitorName: null }, 2)).toBe('Display 2');
    expect(displayName({ ...monitor, monitorName: ' ' }, 3)).toBe('Display 3');
    const [module] = ownerReport().snapshot.memory.modules;
    expect(slotLabel(module)).toBe('A2');
    expect(slotLabel({ ...module, deviceLocator: 'DIMM_B1', bankLabel: '' })).toBe('B1');
    expect(slotLabel({ ...module, deviceLocator: 'CPU1 DIMM A1', bankLabel: 'BANK 0' })).toBe('A1');
    expect(slotLabel({ ...module, deviceLocator: 'P0_ChannelA', bankLabel: 'BANK 0' })).toBe('P0_ChannelA');
    expect(slotLabel({ ...module, deviceLocator: '', bankLabel: '' })).toBeNull();
  });

  it('tags a volume with the games installed on it', () => {
    const s = ownerReport().snapshot;
    expect(gamesOn(s, 'C')).toEqual(['VALORANT']);
    expect(gamesOn(s, 'c:')).toEqual(['VALORANT']);
    expect(gamesOn(s, 'D')).toEqual([]);
    expect(gamesOn({ ...s, storage: { ...s.storage, valorant: null } }, 'C')).toEqual([]);
  });

  it('points each failed probe at its panel', () => {
    const failed = (probe: string) => [{ probe, message: 'x' }];
    expect(panelFailed(failed('cpu-id'), 'processor')).toBe(true);
    expect(panelFailed(failed('cpu-id'), 'graphics')).toBe(false);
    expect(panelFailed(failed('pcie'), 'graphics')).toBe(true);
    expect(panelFailed(failed('firmware'), 'board')).toBe(true);
    expect(panelFailed(failed('drives'), 'storage')).toBe(true);
    expect(panelFailed(failed('displays'), 'displays')).toBe(true);
    for (const panel of ['processor', 'graphics', 'memory', 'board', 'displays', 'storage', 'network'] as const) {
      expect(panelFailed(failed('wmi'), panel)).toBe(true);
      expect(panelFailed(failed('probe'), panel)).toBe(true);
      expect(panelFailed(failed('mock'), panel)).toBe(false);
      expect(panelFailed([], panel)).toBe(false);
    }
  });

  it('polls the clocks only while the tab is shown and the window is visible and focused', () => {
    // [tab shown, window visible, window focused, polls]
    const table: [boolean, boolean, boolean, boolean][] = [
      [true, true, true, true],
      [true, true, false, false], // a game in front
      [true, false, true, false], // minimised
      [true, false, false, false],
      [false, true, true, false], // another tab
      [false, true, false, false],
      [false, false, true, false],
      [false, false, false, false],
    ];
    for (const [tab, visible, focused, polls] of table) {
      expect(shouldPoll(tab, visible, focused), `${tab} ${visible} ${focused}`).toBe(polls);
    }
  });
});

describe('BIOS settings Kestrel can see', () => {
  it('lists the owner’s settings, linking the guide only for one that is off', () => {
    expect(biosSettings(ownerReport())).toEqual([
      { label: 'XMP', value: 'On', state: 'on', problem: false, note: 'RAM runs at its rated 3600 MT/s', topic: null },
      { label: 'Resizable BAR', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'Virtualization (VT-x)', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'IOMMU (VT-d)', value: 'Off', state: 'off', problem: false, note: 'Only for Vanguard On-Demand', topic: 'iommu' },
      { label: 'Secure Boot', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'TPM 2.0', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'Boot mode', value: 'UEFI', state: 'on', problem: false, note: null, topic: null },
    ]);
  });

  it('gives XMP its rated-speed note only when the part numbers name a rating', () => {
    const owner = ownerReport();
    const unrated: Report = { ...owner, snapshot: { ...owner.snapshot, memory: { ...owner.snapshot.memory, ratedMts: null } } };
    expect(biosSettings(unrated)[0]).toEqual({ label: 'XMP', value: 'On', state: 'on', problem: false, note: null, topic: null });
  });

  it('marks settings that are off, in amber when their check is a Problem', () => {
    const owner = ownerReport();
    const s = owner.snapshot;
    const report = withFindings({
      ...owner,
      snapshot: {
        ...s,
        cpu: { ...s.cpu, virtualization: false },
        gpus: s.gpus.map((g) => ({ ...g, resizableBar: g.vendor === 'nvidia' ? false : null })),
        board: { ...s.board, bootMode: 'legacy' },
        security: { ...s.security, iommu: true, secureBoot: false, tpm2: false },
      },
    }, {
      'ram-speed': { status: 'problem', title: 'RAM runs at 2133 MT/s, but the kit is rated 3600', fix: { kind: 'bios-guide', topic: 'xmp' } },
      'resizable-bar': { status: 'info', title: 'Resizable BAR is off', fix: { kind: 'bios-guide', topic: 'resizable-bar' } },
      virtualization: { status: 'info', title: 'Virtualization is off', fix: { kind: 'bios-guide', topic: 'virtualization' } },
      iommu: { status: 'info', title: 'IOMMU is on', fix: { kind: 'nothing' } },
      'secure-boot': { status: 'problem', title: 'Secure Boot is off', fix: { kind: 'steps', steps: ['Restart into the BIOS.'] } },
      tpm: { status: 'problem', title: 'TPM 2.0 is off or missing', fix: { kind: 'steps', steps: ['Restart into the BIOS.'] } },
    });
    expect(biosSettings(report).map((r) => [r.label, r.value, r.state, r.problem, r.note, r.topic])).toEqual([
      ['XMP', 'Off', 'off', true, 'RAM runs at 2133 MT/s, but the kit is rated 3600', 'xmp'],
      ['Resizable BAR', 'Off', 'off', false, null, 'resizable-bar'],
      ['Virtualization (VT-x)', 'Off', 'off', false, null, 'virtualization'],
      ['IOMMU (VT-d)', 'On', 'on', false, null, null],
      ['Secure Boot', 'Off', 'off', true, null, null],
      ['TPM 2.0', 'Off', 'off', true, null, null],
      ['Boot mode', 'Legacy (CSM)', 'off', false, null, null],
    ]);
  });

  it('says Not reported for what Windows didn’t report, names AMD’s settings, and skips Resizable BAR without a separate card', () => {
    const owner = ownerReport();
    const s = owner.snapshot;
    const report = withFindings({
      ...owner,
      snapshot: {
        ...s,
        cpu: { ...s.cpu, vendor: 'amd', virtualization: null },
        gpus: [{ ...s.gpus[1], name: 'AMD Radeon(TM) Graphics', vendor: 'amd', isIntegrated: true }],
        board: { ...s.board, bootMode: 'unknown' },
        security: { ...s.security, iommu: null, secureBoot: null, tpm2: null },
      },
    }, { 'ram-speed': { status: 'unknown', title: 'Couldn’t tell your RAM’s rated speed', fix: { kind: 'bios-guide', topic: 'xmp' } } });
    expect(biosSettings(report).map((r) => [r.label, r.value, r.state, r.note, r.topic])).toEqual([
      ['D.O.C.P.', null, 'unknown', null, null],
      ['Virtualization (SVM)', null, 'unknown', null, null],
      ['IOMMU (AMD-Vi)', null, 'unknown', null, null],
      ['Secure Boot', null, 'unknown', null, null],
      ['TPM 2.0', null, 'unknown', null, null],
      ['Boot mode', null, 'unknown', null, null],
    ]);
  });
});
```

Create `app/src/api/specs.test.ts` (the first test is the spec's seven-line example, except that the HDD keeps the full model Windows reports, `WDC WD20EZBX-00AYRA0`, where the spec shortens it to `WDC WD20EZBX`):

```ts
import { describe, expect, it } from 'vitest';
import { ownerReport } from './owner-report';
import { specsText } from './specs';

const lines = (text: string) => text.split('\n');

describe('Copy specs', () => {
  it('writes the owner’s PC in seven lines', () => {
    expect(specsText(ownerReport())).toBe([
      'CPU: Intel Core i9-11900K (8 cores, 16 threads)',
      'GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe 4.0 x16',
      'RAM: 32 GB DDR4-3600 · dual channel · 2 × 16 GB Corsair CMG32GX4M2D3600C18',
      'Board: ASUS ROG MAXIMUS XIII HERO · BIOS 2103 (2024-05-20)',
      'Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz',
      'Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX-00AYRA0 (HDD)',
      'OS: Windows 11 Pro 25H2 (build 26200)',
    ].join('\n'));
  });

  it('counts P-cores and E-cores on a hybrid CPU', () => {
    const r = ownerReport();
    r.snapshot.cpu = { ...r.snapshot.cpu, name: '13th Gen Intel(R) Core(TM) i9-13900K', cores: 24, threads: 32, pCores: 8, eCores: 16 };
    expect(lines(specsText(r))[0]).toBe('CPU: Intel Core i9-13900K (8 P-cores + 16 E-cores, 32 threads)');
  });

  it('lists modules that differ one by one', () => {
    const r = ownerReport();
    const [a] = r.snapshot.memory.modules;
    const b = { ...a, capacityMb: 8192, manufacturer: 'Kingston', partNumber: 'KF3600C17D4/8GX', configuredMts: 3200 };
    r.snapshot.memory = { ...r.snapshot.memory, totalMb: 24576, modules: [a, b] };
    r.findings = r.findings.map((f) => (f.id === 'ram-channels' ? { ...f, status: 'problem' } : f));
    expect(lines(specsText(r))[2]).toBe('RAM: 24 GB DDR4-3200 · single channel · 16 GB Corsair CMG32GX4M2D3600C18 + 8 GB Kingston KF3600C17D4/8GX');
  });

  it('gives only the lanes in use before any benchmark, and no PCIe part without a link', () => {
    const r = ownerReport();
    const gpu = r.snapshot.gpus[1];
    gpu.linkUnderLoad = null;
    expect(lines(specsText(r))[1]).toBe('GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe x16');
    gpu.pcie = null;
    expect(lines(specsText(r))[1]).toBe('GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74');
  });

  it('leaves out every line Kestrel can’t fill', () => {
    const r = ownerReport();
    const s = r.snapshot;
    r.snapshot = {
      ...s,
      cpu: { ...s.cpu, name: '' },
      gpus: [s.gpus[0]], // only the Parsec adapter, which isn't a real card
      memory: { ...s.memory, totalMb: 0 },
      board: { ...s.board, product: '' },
      displays: [{ ...s.displays[0], isVirtual: true }],
      storage: { ...s.storage, drives: [] },
    };
    expect(specsText(r)).toBe('OS: Windows 11 Pro 25H2 (build 26200)');
    r.snapshot.os = { caption: '', build: 0, displayVersion: null };
    expect(specsText(r)).toBe('');
  });

  it('leaves out the parts of a line Windows didn’t report', () => {
    const r = ownerReport();
    const s = r.snapshot;
    r.snapshot = {
      ...s,
      cpu: { ...s.cpu, cores: 0, threads: 0 },
      gpus: [{ ...s.gpus[1], vramMb: null, driverVersion: '', pcie: null, linkUnderLoad: null }],
      memory: { ...s.memory, ddrGeneration: null, modules: [] },
      board: { ...s.board, manufacturer: '', biosVersion: '' },
      os: { ...s.os, build: 0 },
    };
    r.guide = { ...r.guide, vendor: 'other' };
    r.findings = r.findings.filter((f) => f.id !== 'ram-channels');
    expect(lines(specsText(r))).toEqual([
      'CPU: Intel Core i9-11900K',
      'GPU: NVIDIA GeForce RTX 3060 Ti',
      'RAM: 32 GB',
      'Board: ROG MAXIMUS XIII HERO',
      'Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz',
      'Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX-00AYRA0 (HDD)',
      'OS: Windows 11 Pro 25H2',
    ]);
  });

  it('writes a drive without parentheses when its type is unknown', () => {
    const r = ownerReport();
    r.snapshot.storage.drives[1] = { ...r.snapshot.storage.drives[1], model: 'Generic USB Flash Disk', media: 'unknown', bus: 'usb' };
    expect(lines(specsText(r))[5]).toBe('Storage: Samsung SSD 980 PRO 1TB (NVMe) · Generic USB Flash Disk');
  });
});
```

Create `app/src/api/client.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { getCpuClocks } from './client';

describe('the browser preview', () => {
  it('makes up 8 cores around 4.8 GHz that move on every call', async () => {
    const first = await getCpuClocks();
    const second = await getCpuClocks();
    for (const clocks of [first, second]) {
      expect(clocks.map((c) => c.core)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
      for (const c of clocks) {
        expect(c.kind).toBeNull();
        expect(c.mhz).toBeGreaterThanOrEqual(4500);
        expect(c.mhz).toBeLessThanOrEqual(5100);
      }
    }
    expect(second.map((c) => c.mhz)).not.toEqual(first.map((c) => c.mhz));
  });
});
```

In `app/src/api/findings.test.ts`, replace

```ts
import { cpuLabel, displayLabel, driverLabel, groupByCategory, mainGpu, ramLabel, topProblems } from './findings';
```

with

```ts
import { cpuLabel, cpuName, displayLabel, driverLabel, groupByCategory, mainGpu, ramLabel, topProblems } from './findings';
```

and replace

```ts
    expect(cpuLabel(report.snapshot)).toBe('Intel Core i9-11900K · 8C/16T');
```

with

```ts
    expect(cpuLabel(report.snapshot)).toBe('Intel Core i9-11900K · 8C/16T');
    expect(cpuName(report.snapshot)).toBe('Intel Core i9-11900K');
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cd app && npx vitest run src/api/hardware.test.ts src/api/specs.test.ts src/api/client.test.ts src/api/findings.test.ts`

Expected: FAIL, 4 files:
- `src/api/hardware.test.ts` — `Error: Cannot find module './hardware'`
- `src/api/specs.test.ts` — `Error: Cannot find module './specs'`
- `src/api/client.test.ts > the browser preview > makes up 8 cores around 4.8 GHz that move on every call` — `TypeError: getCpuClocks is not a function`
- `src/api/findings.test.ts > findings helpers > formats hardware labels` — `TypeError: cpuName is not a function`

- [ ] **Step 4: Implement**

**4a.** Replace the whole of `app/src/api/types.ts` with:

```ts
// Mirrors the Rust types in crates/kestrel-core (serde camelCase / kebab-case).
export type Severity = 'high' | 'medium' | 'low' | 'info';
export type Category = 'hardware' | 'display' | 'gpu' | 'windows' | 'storage' | 'background' | 'vanguard' | 'network';
export type Status = 'pass' | 'problem' | 'info' | 'unknown';
export type BiosTopic = 'xmp' | 'memory-gear' | 'iommu' | 'resizable-bar' | 'update-bios' | 'pcie-slot' | 'virtualization';

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

/** A PCIe link: generation 1–6 and lane count. */
export interface PcieLink { gen: number; width: number }
/** What the card and its slot support, and the link right now (a card slows it when idle). */
export interface PcieInfo { cardMax: PcieLink | null; slotMax: PcieLink | null; now: PcieLink | null }
/** The highest link seen during the newest benchmark on this card. The app fills it, never the probe. */
export interface LinkUnderLoad { link: PcieLink; measuredAt: string; benchmarkId: string }
export interface GpuInfo {
  name: string;
  vendor: 'nvidia' | 'amd' | 'intel' | 'virtual' | 'other';
  driverVersion: string;
  driverDate: string | null;
  vramMb: number | null;
  isIntegrated: boolean;
  /** Win32_VideoController.PNPDeviceID; empty when unknown. */
  pnpDeviceId: string;
  pcie: PcieInfo | null;
  linkUnderLoad: LinkUnderLoad | null;
  largestBarMb: number | null;
  resizableBar: boolean | null;
}
export interface MemoryModule { capacityMb: number; configuredMts: number | null; maxMts: number | null; partNumber: string; manufacturer: string; bankLabel: string; deviceLocator: string; rank: number | null }
export interface DisplayInfo {
  name: string;
  adapter: string;
  currentHz: number;
  maxHz: number;
  width: number;
  height: number;
  isPrimary: boolean;
  isVirtual: boolean;
  /** The name in the monitor's EDID, e.g. "XL2566K". */
  monitorName: string | null;
  /** "DisplayPort", "HDMI", "DVI", "VGA", "Internal", "USB-C" or "Wireless". */
  connection: string | null;
}
export type CacheKind = 'data' | 'instruction' | 'unified' | 'unknown';
export interface CacheInfo { level: number; kind: CacheKind; sizeKb: number; count: number }
export interface CpuInfo {
  name: string;
  vendor: 'intel' | 'amd' | 'other';
  cores: number;
  threads: number;
  family: number | null;
  model: number | null;
  stepping: number | null;
  codename: string | null;
  socket: string | null;
  baseMhz: number | null;
  maxMhz: number | null;
  /** Set only on hybrid CPUs (cores of more than one efficiency class). */
  pCores: number | null;
  eCores: number | null;
  caches: CacheInfo[];
  /** The sets the CPU has and Windows enables, in this order: SSE4.2, AVX, AVX2, AVX-512, AVX-VNNI, AMX. */
  instructions: string[];
  virtualization: boolean | null;
}
export interface BoardInfo {
  manufacturer: string;
  product: string;
  biosVersion: string;
  biosDate: string | null;
  biosVendor: string;
  chipset: string | null;
  bootMode: 'uefi' | 'legacy' | 'unknown';
}
export type DriveMedia = 'nvme' | 'ssd' | 'hdd' | 'unknown';
export type DriveBus = 'nvme' | 'sata' | 'usb' | 'sas' | 'raid' | 'virtual' | 'other';
/** Sizes are GiB (bytes / 1024³), like the VALORANT install's. */
export interface VolumeInfo { letter: string; freeGb: number; totalGb: number }
export interface DriveInfo { model: string; media: DriveMedia; bus: DriveBus; sizeGb: number; volumes: VolumeInfo[] }
export interface SecurityInfo {
  tpm2: boolean | null;
  secureBoot: boolean | null;
  vbsRunning: boolean | null;
  hvciRunning: boolean | null;
  iommu: boolean | null;
  vanguard: 'running' | 'stopped' | 'not-installed' | 'unknown';
}
export interface NetAdapter { name: string; kind: 'ethernet' | 'wifi' | 'other'; linkMbps: number | null; connected: boolean; powerSaving: boolean | null }
export interface ProbeError { probe: string; message: string }

export interface SystemSnapshot {
  schemaVersion: number;
  collectedAt: string;
  os: { caption: string; build: number; displayVersion: string | null };
  machine: { formFactor: 'desktop' | 'laptop' | 'unknown'; manufacturer: string; model: string; onBattery: boolean | null };
  cpu: CpuInfo;
  gpus: GpuInfo[];
  /** `ratedMts`: the kit's rating from the part numbers (kestrel-core's ram::rated_mts); null when Kestrel doesn't know the kit. */
  memory: { totalMb: number; ddrGeneration: number | null; modules: MemoryModule[]; slotsTotal: number | null; ratedMts: number | null };
  displays: DisplayInfo[];
  board: BoardInfo;
  storage: {
    valorant: { path: string; driveLetter: string; media: DriveMedia; freeGb: number; totalGb: number } | null;
    drives: DriveInfo[];
  };
  security: SecurityInfo;
  network: { adapters: NetAdapter[]; deliveryOptimizationInternet: boolean | null };
  probeErrors: ProbeError[];
}

export interface Report { appVersion: string; snapshot: SystemSnapshot; findings: Finding[]; score: Score; guide: BiosGuide }
export interface LoadSample { cpuPercent: number; ramPercent: number; gpuPercent: number | null; gpuTempC: number | null }
/** One physical core's clock (Rust: kestrel_core::topology::CoreClock). `kind` is set only on hybrid CPUs. */
export interface CoreClock { core: number; kind: 'performance' | 'efficiency' | null; mhz: number | null }

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
  /** The main graphics card's PNPDeviceID when the benchmark ran. */
  gpuPnpDeviceId: string | null;
  /** The highest PCIe link seen during the runs (kestrel-bench's GpuLink has PcieLink's shape). */
  gpuLink: PcieLink | null;
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
export interface MetricDelta { metric: MetricId; baseline: number; current: number; deltaPct: number | null; verdict: Verdict }
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

**4b.** In `app/src/api/client.ts`, replace

```ts
import type { BenchDetail, BenchState, BenchmarkSummary, GameProfile, LoadSample, Report } from './types';
```

with

```ts
import type { BenchDetail, BenchState, BenchmarkSummary, CoreClock, GameProfile, LoadSample, Report } from './types';
```

and replace

```ts
  return Promise.resolve({ cpuPercent: wave(3, 9, 5), ramPercent: 41, gpuPercent: wave(5, 4, 2), gpuTempC: 44 });
};
```

with

```ts
  return Promise.resolve({ cpuPercent: wave(3, 9, 5), ramPercent: 41, gpuPercent: wave(5, 4, 2), gpuTempC: 44 });
};

let clockTick = 0;
/** Each physical core's clock now. The browser preview makes up 8 cores around 4.8 GHz that move on every call. */
export const getCpuClocks = (): Promise<CoreClock[]> => {
  if (inTauri()) return call<CoreClock[]>('get_cpu_clocks');
  clockTick += 1;
  const mhz = (core: number) => Math.round(4800 + 250 * Math.sin(clockTick / 2 + core) + Math.random() * 40);
  return Promise.resolve(Array.from({ length: 8 }, (_, core) => ({ core, kind: null, mhz: mhz(core) })));
};
```

**4c.** In `app/src/api/sample-bench.ts`, replace

```ts
  windowsBuild: 26200, displayHz: 360, gameSettings: null,
};
```

with

```ts
  windowsBuild: 26200, displayHz: 360, gameSettings: null,
  gpuPnpDeviceId: 'PCI\\VEN_10DE&DEV_2489&SUBSYS_88221043&REV_A1\\4&2E1F5C5A&0&0008', gpuLink: { gen: 4, width: 16 },
};
```

(The browser preview's sample benchmarks then show the PCIe line Task 17 adds.)

**4d.** In `app/src/api/findings.ts`, replace

```ts
export function cpuLabel(s: SystemSnapshot): string {
  const name = s.cpu.name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/@.*$/, '')
    .replace(/\s+CPU\b/i, '')
    .replace(/^\d+(st|nd|rd|th) Gen\s+/i, '')
    .replace(/\s+\d+-Core Processor$/i, '')
    .replace(/\s+/g, ' ')
    .trim();
  return s.cpu.threads ? `${name} · ${s.cpu.cores}C/${s.cpu.threads}T` : name;
}
```

with

```ts
/** The CPU's name without trademarks, generation, clock or core-count words: "Intel Core i9-11900K". */
export function cpuName(s: SystemSnapshot): string {
  return s.cpu.name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/@.*$/, '')
    .replace(/\s+CPU\b/i, '')
    .replace(/^\d+(st|nd|rd|th) Gen\s+/i, '')
    .replace(/\s+\d+-Core Processor$/i, '')
    .replace(/\s+/g, ' ')
    .trim();
}

export function cpuLabel(s: SystemSnapshot): string {
  const name = cpuName(s);
  return s.cpu.threads ? `${name} · ${s.cpu.cores}C/${s.cpu.threads}T` : name;
}
```

**4e.** In `app/src/strings.ts`, replace

```ts
  hardware: { cpu: 'CPU', gpu: 'GPU', ram: 'RAM', display: 'Panel', os: 'OS', board: 'Board' },
  health: {
```

with

```ts
  hardware: { cpu: 'CPU', gpu: 'GPU', ram: 'RAM', display: 'Panel', os: 'OS', board: 'Board' },
  hardwarePage: {
    notReported: 'Not reported',
    months: ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'],
    build: (n: number) => `build ${n}`,
    cores: (n: number) => `${n} cores`,
    hybridCores: (p: number, e: number) => `${p} P-cores + ${e} E-cores`,
    threads: (n: number) => `${n} threads`,
    cache: { l1Data: 'L1 data', l1Instruction: 'L1 instruction', level: (n: number) => `L${n}` },
    rank: (n: number) => (n === 1 ? 'Single rank' : n === 2 ? 'Dual rank' : n === 4 ? 'Quad rank' : `${n} ranks`),
    media: { nvme: 'NVMe', ssd: 'SSD', hdd: 'HDD' },
    bus: { nvme: 'NVMe', sata: 'SATA', usb: 'USB', sas: 'SAS', raid: 'RAID', virtual: 'Virtual', other: 'Other' },
    unnamedDisplay: (n: number) => `Display ${n}`,
    games: { valorant: 'VALORANT' },
    makers: { asus: 'ASUS', msi: 'MSI', gigabyte: 'Gigabyte', asrock: 'ASRock' },
    profiles: { xmp: 'XMP', expo: 'EXPO', docp: 'D.O.C.P.', axmp: 'A-XMP' },
    settings: {
      virtualization: { intel: 'Virtualization (VT-x)', amd: 'Virtualization (SVM)', other: 'Virtualization' },
      iommu: { intel: 'IOMMU (VT-d)', amd: 'IOMMU (AMD-Vi)', other: 'IOMMU' },
      resizableBar: 'Resizable BAR',
      secureBoot: 'Secure Boot',
      tpm: 'TPM 2.0',
      bootMode: 'Boot mode',
      on: 'On',
      off: 'Off',
      uefi: 'UEFI',
      legacy: 'Legacy (CSM)',
      ratedSpeed: (mts: number) => `RAM runs at its rated ${mts} MT/s`,
      onDemandOnly: 'Only for Vanguard On-Demand',
    },
  },
  specs: {
    labels: { cpu: 'CPU', gpu: 'GPU', ram: 'RAM', board: 'Board', displays: 'Displays', storage: 'Storage', os: 'OS' },
    driver: (version: string) => `driver ${version}`,
    pcie: (link: string) => `PCIe ${link}`,
    channels: { dual: 'dual channel', single: 'single channel' },
    bios: (version: string) => `BIOS ${version}`,
    display: (width: number, height: number, hz: number) => `${width}×${height} at ${hz} Hz`,
  },
  health: {
```

**4f.** Create `app/src/api/hardware.ts`:

```ts
// Formatting for the Hardware tab and Copy specs. Pure: every value comes from the report; nothing is guessed.
import { strings } from '../strings';
import { cpuName, mainGpu } from './findings';
import { osLabel } from './format';
import type {
  BiosTopic, CacheInfo, CpuInfo, DisplayInfo, DriveBus, DriveMedia, Finding, GpuInfo, MemoryModule, PcieLink, ProbeError,
  Report, SystemSnapshot,
} from './types';

const h = strings.hardwarePage;

/** 4800 → "4.80 GHz". */
export const ghz = (mhz: number): string => `${(mhz / 1000).toFixed(2)} GHz`;

/** A PCIe generation as kestrel-core's pcie::gen_label writes it: 4 → "4.0". */
export const genLabel = (gen: number): string => `${gen}.0`;

/** { gen: 4, width: 16 } → "4.0 x16". */
export const linkLabel = (link: PcieLink): string => `${genLabel(link.gen)} x${link.width}`;

/** A cache size: 48 → "48 KB", 16384 → "16 MB", 1280 → "1.25 MB". */
export const kbLabel = (kb: number): string => (kb < 1024 ? `${kb} KB` : `${Number((kb / 1024).toFixed(2))} MB`);

/** One cache entry: "8 × 48 KB", or just "16 MB" when there's one of it. */
export const cacheLabel = (c: CacheInfo): string => (c.count > 1 ? `${c.count} × ${kbLabel(c.sizeKb)}` : kbLabel(c.sizeKb));

/** "L1 data", "L1 instruction", "L2", "L3". */
export function cacheName(c: CacheInfo): string {
  if (c.level === 1 && c.kind === 'data') return h.cache.l1Data;
  if (c.level === 1 && c.kind === 'instruction') return h.cache.l1Instruction;
  return h.cache.level(c.level);
}

/**
 * One row per cache name, in the order kestrel-core sorts them. A hybrid CPU's two sizes of one level share the row:
 * ["L2", "8 × 2 MB + 4 × 4 MB"].
 */
export function cacheRows(caches: CacheInfo[]): [string, string][] {
  const rows = new Map<string, string[]>();
  for (const c of caches) rows.set(cacheName(c), [...(rows.get(cacheName(c)) ?? []), cacheLabel(c)]);
  return [...rows].map(([name, sizes]) => [name, sizes.join(' + ')]);
}

/**
 * A GiB amount the way Windows Explorer shows it: three significant digits, cut off rather than rounded, so free space
 * is never overstated. 931.5 → "931 GB", 1863 → "1.81 TB".
 */
export function sizeLabel(gib: number): string {
  const tb = gib >= 1000;
  const value = tb ? gib / 1024 : gib;
  const decimals = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  const scale = 10 ** decimals;
  // The nudge keeps a product like 1.2 × 100 = 119.99999… from losing its last digit.
  return `${(Math.floor(value * scale + 1e-9) / scale).toFixed(decimals)} ${tb ? 'TB' : 'GB'}`;
}

/** "8 cores · 16 threads", or "8 P-cores + 16 E-cores · 32 threads" on a hybrid CPU. Needs a core count. */
export function coresLabel(cpu: CpuInfo): string {
  const cores = cpu.pCores !== null && cpu.eCores !== null ? h.hybridCores(cpu.pCores, cpu.eCores) : h.cores(cpu.cores);
  return cpu.threads > 0 ? `${cores} · ${h.threads(cpu.threads)}` : cores;
}

/** The speed the RAM runs at now: the slowest module's, in MT/s. */
export function ramSpeed(s: SystemSnapshot): number | null {
  const speeds = s.memory.modules.map((m) => m.configuredMts ?? 0).filter((v) => v > 0);
  return speeds.length > 0 ? Math.min(...speeds) : null;
}

/** "32 GB DDR4-3600"; "32 GB 3600 MT/s" without a DDR generation; null when Windows reported no memory. */
export function memorySummary(s: SystemSnapshot): string | null {
  if (s.memory.totalMb <= 0) return null;
  const gb = `${Math.round(s.memory.totalMb / 1024)} GB`;
  const ddr = s.memory.ddrGeneration;
  const mts = ramSpeed(s);
  if (ddr) return mts ? `${gb} DDR${ddr}-${mts}` : `${gb} DDR${ddr}`;
  return mts ? `${gb} ${mts} MT/s` : gb;
}

const finding = (report: Report, id: string): Finding | undefined => report.findings.find((f) => f.id === id);

/** Dual or single channel, as the RAM channel check found it; null when it couldn't tell. */
export function ramChannels(report: Report): 'dual' | 'single' | null {
  const status = finding(report, 'ram-channels')?.status;
  return status === 'pass' ? 'dual' : status === 'problem' ? 'single' : null;
}

/** What this board's BIOS calls the memory profile: the same rule as kestrel-core's bios_guide::memory_profile_name. */
export function memoryProfile(report: Report): string {
  const p = h.profiles;
  const { cpu, memory } = report.snapshot;
  if (cpu.vendor !== 'amd') return p.xmp;
  if (memory.ddrGeneration === 5) return p.expo;
  return report.guide.vendor === 'asus' ? p.docp : report.guide.vendor === 'msi' ? p.axmp : p.xmp;
}

/** The board maker's short name (ASUS, MSI, Gigabyte, ASRock), else the name Windows reports. */
export function boardMaker(report: Report): string {
  const vendor = report.guide.vendor;
  return vendor === 'other' ? report.snapshot.board.manufacturer.trim() : h.makers[vendor];
}

/** "NVIDIA GeForce RTX 3060 Ti" → "RTX 3060 Ti", "Intel(R) Arc(TM) A770 Graphics" → "Arc A770 Graphics". */
export const gpuShortName = (name: string): string =>
  name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/\s+/g, ' ')
    .trim()
    // Only when at least two words follow, so "AMD Radeon Graphics" keeps "Radeon".
    .replace(/^(NVIDIA GeForce|NVIDIA|AMD Radeon|AMD|Intel)\s+(?=\S+\s+\S)/i, '');

/** The main graphics card when it is a separate card (not integrated, not virtual), as the PCIe checks see it. */
export function separateGpu(gpus: GpuInfo[]): GpuInfo | undefined {
  const gpu = mainGpu(gpus);
  return gpu && !gpu.isIntegrated ? gpu : undefined;
}

/** The page's one-line summary: "Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO". */
export function summaryLine(s: SystemSnapshot): string {
  const gpu = mainGpu(s.gpus);
  return [cpuName(s).replace(/^(Intel|AMD)\s+/i, ''), gpu ? gpuShortName(gpu.name) : '', memorySummary(s) ?? '', s.board.product.trim()]
    .filter((part) => part !== '')
    .join(' · ');
}

/** "Windows 11 Pro 25H2 · build 26200". */
export const windowsLine = (s: SystemSnapshot): string =>
  [osLabel(s), s.os.build > 0 ? h.build(s.os.build) : ''].filter((part) => part !== '').join(' · ');

function toDate(iso: string): Date | null {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d;
}

/** "2026-10-13T21:40:00Z" → local "13 Oct"; null when it isn't a date. */
export function dayMonth(iso: string): string | null {
  const d = toDate(iso);
  return d ? `${d.getDate()} ${h.months[d.getMonth()]}` : null;
}

/** "2026-10-13T21:04:00Z" → local "13 Oct 21:04"; null when it isn't a date. */
export function dayMonthTime(iso: string): string | null {
  const d = toDate(iso);
  const two = (n: number) => n.toString().padStart(2, '0');
  return d ? `${dayMonth(iso)} ${two(d.getHours())}:${two(d.getMinutes())}` : null;
}

/** 8192 → "8 GB", 512 → "512 MB". */
export const vramLabel = (mb: number): string => (mb >= 1024 ? `${Math.round(mb / 1024)} GB` : `${mb} MB`);

/** 2500 → "2.5 Gbps", 100 → "100 Mbps". */
export const linkSpeedLabel = (mbps: number): string =>
  mbps >= 1000 ? `${Number((mbps / 1000).toFixed(1))} Gbps` : `${mbps} Mbps`;

/** NVMe, SSD or HDD; null when Windows didn't say. */
export const mediaLabel = (media: DriveMedia): string | null => (media === 'unknown' ? null : h.media[media]);

export const busLabel = (bus: DriveBus): string => h.bus[bus];

/** The monitor's EDID name, else "Display 1", "Display 2"… by its place among the real displays (`n` counts from 1). */
export const displayName = (d: DisplayInfo, n: number): string => d.monitorName?.trim() || h.unnamedDisplay(n);

/**
 * A module's slot the way boards print it: "ChannelA-DIMM2" or "DIMM_A2" → "A2". Other locators are shown as Windows
 * reports them; null when it reports none.
 */
export function slotLabel(m: MemoryModule): string | null {
  const text = `${m.deviceLocator} ${m.bankLabel}`;
  const match = /channel\s*([a-h])\W*dimm\s*(\d)/i.exec(text) ?? /\bdimm[\s_-]*([a-h])(\d)\b/i.exec(text);
  if (match) return `${match[1].toUpperCase()}${match[2]}`;
  return m.deviceLocator.trim() || m.bankLabel.trim() || null;
}

/** The games Kestrel found installed on a volume: VALORANT, from its install folder. */
export function gamesOn(s: SystemSnapshot, letter: string): string[] {
  const norm = (l: string) => l.replace(':', '').trim().toUpperCase();
  const valorant = s.storage.valorant;
  return valorant && norm(letter) !== '' && norm(valorant.driveLetter) === norm(letter) ? [h.games.valorant] : [];
}

export type PanelId = 'processor' | 'graphics' | 'memory' | 'board' | 'displays' | 'storage' | 'network';

/** The probes behind each panel. A failed "wmi" or "probe" sits behind every panel. */
const PANEL_PROBES: Record<PanelId, string[]> = {
  processor: ['cpu', 'cpu-id', 'topology'],
  graphics: ['gpus', 'pcie'],
  memory: ['memory'],
  board: ['board', 'chipset', 'firmware'],
  displays: ['displays'],
  storage: ['storage', 'drives'],
  network: ['network'],
};

/** True when a probe behind this panel failed, so the panel shows the notice. "mock" only shows the sample banner. */
export const panelFailed = (errors: ProbeError[], panel: PanelId): boolean =>
  errors.some((e) => e.probe === 'wmi' || e.probe === 'probe' || PANEL_PROBES[panel].includes(e.probe));

/** Live clocks poll only while the Hardware tab is shown and Kestrel's window is visible and focused (a game in front isn't Kestrel). */
export const shouldPoll = (tabShown: boolean, visible: boolean, focused: boolean): boolean => tabShown && visible && focused;

export type SettingState = 'on' | 'off' | 'unknown';

/** One row of "BIOS settings Kestrel can see". */
export interface BiosSetting {
  label: string;
  /** On, Off, UEFI or Legacy (CSM); null when Windows didn't report it. */
  value: string | null;
  state: SettingState;
  /** Its Health Check item is a Problem, so the row shows in amber. */
  problem: boolean;
  note: string | null;
  /** The BIOS-guide topic to open: set only while the setting is off and its Health Check item points at the guide. */
  topic: BiosTopic | null;
}

/**
 * XMP (from the RAM speed check), Resizable BAR (from the main graphics card; left out without a separate card),
 * virtualization, IOMMU, Secure Boot, TPM 2.0 and the boot mode.
 */
export function biosSettings(report: Report): BiosSetting[] {
  const s = report.snapshot;
  const t = h.settings;
  const row = (label: string, on: boolean | null, checkId: string, note: string | null = null): BiosSetting => {
    const check = finding(report, checkId);
    const state: SettingState = on === null ? 'unknown' : on ? 'on' : 'off';
    return {
      label,
      value: on === null ? null : on ? t.on : t.off,
      state,
      problem: check?.status === 'problem',
      note,
      topic: state === 'off' && check?.fix.kind === 'bios-guide' ? check.fix.topic : null,
    };
  };
  const ram = finding(report, 'ram-speed');
  const xmp = ram?.status === 'pass' ? true : ram?.status === 'problem' ? false : null;
  const rated = s.memory.ratedMts;
  const xmpNote = xmp === true && rated !== null ? t.ratedSpeed(rated) : xmp === false ? (ram?.title ?? null) : null;
  const gpu = separateGpu(s.gpus);
  const boot = s.board.bootMode;
  return [
    row(memoryProfile(report), xmp, 'ram-speed', xmpNote),
    ...(gpu ? [row(t.resizableBar, gpu.resizableBar, 'resizable-bar')] : []),
    row(t.virtualization[s.cpu.vendor], s.cpu.virtualization, 'virtualization'),
    row(t.iommu[s.cpu.vendor], s.security.iommu, 'iommu', s.security.iommu === false ? t.onDemandOnly : null),
    row(t.secureBoot, s.security.secureBoot, 'secure-boot'),
    row(t.tpm, s.security.tpm2, 'tpm'),
    {
      label: t.bootMode,
      value: boot === 'uefi' ? t.uefi : boot === 'legacy' ? t.legacy : null,
      state: boot === 'uefi' ? 'on' : boot === 'legacy' ? 'off' : 'unknown',
      problem: false,
      note: null,
      topic: null,
    },
  ];
}
```

Notes on choices the tests pin:
- The kit's rated speed is `snapshot.memory.ratedMts`, which the probe reads from the part numbers (Task 11); no helper parses a finding's wording. A kit Kestrel doesn't know (`null`) gives the XMP row no note, and the Memory panel says "Rating unknown" (Task 16).
- `sizeLabel` cuts off rather than rounds, like Windows Explorer: the 2 TB WD drive (1863 GiB) reads "1.81 TB", as Explorer shows it.
- `memoryProfile` mirrors `kestrel_core::bios_guide::memory_profile_name` the same way `mainGpu` mirrors `gpu::main_gpu`; the board maker comes from `report.guide.vendor`, which Rust already worked out.
- `biosSettings` takes each row's BIOS-guide topic from its Health Check item's fix, so the checklist links exactly where the Health tab does (NVIDIA's Resizable BAR Info links the guide; a card that doesn't use Resizable BAR doesn't), and only while the setting is off. Without a separate graphics card there is no Resizable BAR row.

**4g.** Create `app/src/api/specs.ts`:

```ts
import { strings } from '../strings';
import { cpuName, driverLabel, mainGpu } from './findings';
import { osLabel } from './format';
import { boardMaker, linkLabel, mediaLabel, memorySummary, ramChannels } from './hardware';
import type { MemoryModule, Report, SystemSnapshot } from './types';

const t = strings.specs;
const h = strings.hardwarePage;

const line = (label: string, text: string) => `${label}: ${text}`;

/** Copy specs (decision 77): one plain-text line per part, leaving out any line Kestrel can't fill. */
export function specsText(report: Report): string {
  const s = report.snapshot;
  return [cpuLine(s), gpuLine(s), ramLine(report), boardLine(report), displaysLine(s), storageLine(s), osLine(s)]
    .filter((text): text is string => text !== null)
    .join('\n');
}

/** "CPU: Intel Core i9-11900K (8 cores, 16 threads)"; hybrid CPUs count P-cores and E-cores. */
function cpuLine(s: SystemSnapshot): string | null {
  const name = cpuName(s);
  if (!name) return null;
  const { cores, threads, pCores, eCores } = s.cpu;
  const counts: string[] = [];
  if (cores > 0) counts.push(pCores !== null && eCores !== null ? h.hybridCores(pCores, eCores) : h.cores(cores));
  if (threads > 0) counts.push(h.threads(threads));
  return line(t.labels.cpu, counts.length > 0 ? `${name} (${counts.join(', ')})` : name);
}

/** The main graphics card. Its PCIe part is the link under load, or only the lanes in use before any benchmark. */
function gpuLine(s: SystemSnapshot): string | null {
  const gpu = mainGpu(s.gpus);
  if (!gpu) return null;
  const vram = gpu.vramMb !== null && gpu.vramMb >= 1024 ? ` ${Math.round(gpu.vramMb / 1024)} GB` : '';
  const parts = [`${gpu.name}${vram}`];
  if (gpu.driverVersion) parts.push(t.driver(driverLabel(gpu)));
  const now = gpu.pcie?.now;
  if (gpu.linkUnderLoad) parts.push(t.pcie(linkLabel(gpu.linkUnderLoad.link)));
  else if (now) parts.push(t.pcie(`x${now.width}`));
  return line(t.labels.gpu, parts.join(' · '));
}

/** "2 × 16 GB Corsair CMG32GX4M2D3600C18", or each module on its own, joined with " + ", when they differ. */
function modulesText(modules: MemoryModule[]): string | null {
  if (modules.length === 0) return null;
  const texts = modules.map((m) => [`${Math.round(m.capacityMb / 1024)} GB`, m.manufacturer.trim(), m.partNumber.trim()].filter(Boolean).join(' '));
  return texts.every((text) => text === texts[0]) ? `${texts.length} × ${texts[0]}` : texts.join(' + ');
}

function ramLine(report: Report): string | null {
  const total = memorySummary(report.snapshot);
  if (!total) return null;
  const channels = ramChannels(report);
  const modules = modulesText(report.snapshot.memory.modules);
  return line(t.labels.ram, [total, channels ? t.channels[channels] : null, modules].filter(Boolean).join(' · '));
}

function boardLine(report: Report): string | null {
  const { board } = report.snapshot;
  const product = board.product.trim();
  if (!product) return null;
  const version = board.biosVersion.trim();
  const bios = version ? ` · ${t.bios(version)}${board.biosDate ? ` (${board.biosDate})` : ''}` : '';
  return line(t.labels.board, `${[boardMaker(report), product].filter(Boolean).join(' ')}${bios}`);
}

function displaysLine(s: SystemSnapshot): string | null {
  const real = s.displays.filter((d) => !d.isVirtual && d.width > 0 && d.height > 0 && d.currentHz > 1);
  if (real.length === 0) return null;
  return line(t.labels.displays, real.map((d) => t.display(d.width, d.height, d.currentHz)).join(' · '));
}

function storageLine(s: SystemSnapshot): string | null {
  const drives = s.storage.drives.filter((d) => d.model.trim() !== '');
  if (drives.length === 0) return null;
  return line(t.labels.storage, drives.map((d) => {
    const media = mediaLabel(d.media);
    return media ? `${d.model.trim()} (${media})` : d.model.trim();
  }).join(' · '));
}

function osLine(s: SystemSnapshot): string | null {
  const os = osLabel(s);
  if (!os) return null;
  return line(t.labels.os, s.os.build > 0 ? `${os} (${h.build(s.os.build)})` : os);
}
```

- [ ] **Step 5: Verify**

Run: `cd app && npm test`
Expected: PASS — `Test Files  8 passed (8)`, `Tests  44 passed (44)`.

Run: `cd app && npx tsc --noEmit`
Expected: no output, exit 0.

Run: `cd app && npm run build`
Expected: `✓ built in …`, no errors (`owner-report.ts` isn't imported by the app, so it isn't bundled).

Nothing visible changes yet: Task 16 adds the page.

- [ ] **Step 6: Commit**

```bash
git add app/src/api/mock-report.json app/src/api/types.ts app/src/api/client.ts app/src/api/client.test.ts app/src/api/sample-bench.ts app/src/api/findings.ts app/src/api/findings.test.ts app/src/api/hardware.ts app/src/api/hardware.test.ts app/src/api/specs.ts app/src/api/specs.test.ts app/src/api/owner-report.ts app/src/strings.ts
git commit -m "feat(app): hardware types, the CPU clocks call, formatting helpers and Copy specs text" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 16: The Hardware tab

**Files:**
- Create: `app/src/components/hardware/view.ts` (what each panel shows, as plain data; pure and tested)
- Create: `app/src/components/hardware/HwPanel.tsx` (`HwPanel`, `DeviceName`, `Facts`: the parts every panel shares)
- Create: `app/src/components/hardware/HardwareTop.tsx` (summary line, Windows line, Last scan, Scan again, Copy specs)
- Create: `app/src/components/hardware/CopySpecs.tsx`
- Create: `app/src/components/hardware/ProcessorPanel.tsx`, `LiveClocks.tsx`, `GraphicsPanels.tsx`, `MemoryPanel.tsx`, `BoardPanel.tsx`, `DisplayPanels.tsx`, `StoragePanels.tsx`, `NetworkPanels.tsx`
- Create: `app/src/pages/Hardware.tsx`
- Create: `app/src/styles/hardware.css`
- Modify: `app/src/components/TitleBar.tsx` (the `hardware` tab between Health and Tweaks; `TABS` exported for its test)
- Modify: `app/src/App.tsx` (the route; the quiet report refresh on every tab change)
- Modify: `app/src/pages/Home.tsx`, `app/src/components/HardwarePanel.tsx` (See all)
- Modify: `app/src/strings.ts` (tab label, See all, the page's strings, Copy specs button)
- Modify: `app/src/main.tsx` (imports `hardware.css`), `app/src/styles/base.css` (`.panel-head`)
- Test: `app/src/components/hardware/view.test.ts`, `app/src/components/TitleBar.test.ts` (new)

**Interfaces:**
- Consumes (Task 15): `getCpuClocks(): Promise<CoreClock[]>`; `cpuName`, `driverLabel`, `mainGpu` (`findings.ts`); from `hardware.ts`: `biosSettings`, `boardMaker`, `busLabel`, `cacheRows`, `coresLabel`, `dayMonth`, `dayMonthTime`, `displayName`, `gamesOn`, `ghz`, `gpuShortName`, `linkLabel`, `linkSpeedLabel`, `mediaLabel`, `ramChannels`, `ramSpeed`, `sizeLabel`, `slotLabel`, `vramLabel`, `summaryLine`, `windowsLine`, `panelFailed`, `type PanelId`, `shouldPoll`; `specsText(report: Report): string`; `ownerReport(): Report`; the types `BiosTopic`, `CoreClock`, `GpuInfo`, `PcieLink`, `Report`, `SystemSnapshot` (with `memory.ratedMts`); `strings.hardwarePage.{notReported, rank, settings.on/off}`; the schema-2 sample Task 15 regenerated, for the browser preview.
- Produces:
  - `TitleBar.tsx`: `export type Tab = 'home' | 'health' | 'hardware' | 'tweaks' | 'lockIn' | 'benchmark' | 'toolbox'`, `export const TABS: Tab[]`.
  - `pages/Hardware.tsx`: `Hardware({ report, onGuide, onRescan }: { report: Report; onGuide: (topic: BiosTopic) => void; onRescan: () => void })`.
  - `components/hardware/view.ts`: `interface Fact { label; value: string | null; tone?: 'good' | 'warn' | 'muted'; note?; topic?: BiosTopic }`, `interface Card { name: string | null; facts: Fact[] }`, `processorView`, `clockView(clocks: CoreClock[], maxMhz: number | null, polling: boolean)`, `graphicsView`, `memoryView`, `boardView`, `displaysView`, `storageView`, `networkView`.
  - `HardwarePanel({ snapshot, onTab })` (new required prop); CSS class `.panel-head` in `base.css`.
  - `strings.tabs.hardware`, `strings.home.seeHardware`, `strings.hardwarePage.{title, lastScan, panelFailed, panels, processor, clocks, graphics, memory, board, displays, storage, network}`, `strings.specs.{copy, copied, failed}`.

Background for this task:
- The page uses the current look: `.panel` sections with a `.label` heading (as on Health), values in the mono `.num` font, lime `--accent` for good or live, amber `--warn` for a Health Check problem, grey `--muted` for Not reported. Two columns from 1100 px (the window opens at 1180 × 760, minimum 1024 × 680), one below.
- Vitest has no DOM here, so the components stay thin: `view.ts` turns the report into lines (label, value, tone, note, BIOS-guide topic) and is unit-tested; the `.tsx` files only lay them out.
- `App` mounts a page only while its tab is shown (`<main key={tab}>`), so `LiveClocks` passes `true` as `shouldPoll`'s tab argument and leaving the tab unmounts it, which stops its timer. It listens to `visibilitychange`, `focus` and `blur` for the other two arguments.
- Reduced motion needs nothing new: `base.css` already turns every transition off under `prefers-reduced-motion: reduce`, so the clock bars jump instead of sliding.
- `App` only renders tabs once there is a report (the first run makes the user scan, and the loading and failed states already exist), so the page never shows without a scan.

- [ ] **Step 1: Write the failing tests**

Create `app/src/components/hardware/view.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { shouldPoll } from '../../api/hardware';
import { ownerReport } from '../../api/owner-report';
import type { CoreClock, Finding, Report } from '../../api/types';
import { boardView, clockView, displaysView, graphicsView, memoryView, networkView, processorView, storageView, type Fact } from './view';

/** [label, value] of each line, plus its tone and BIOS-guide topic when it has them. */
const lines = (facts: Fact[]) => facts.map((f) => [f.label, f.value, ...(f.tone ? [f.tone] : []), ...(f.topic ? [f.topic] : [])]);

function withFindings(report: Report, changes: Record<string, Partial<Finding>>): Report {
  return { ...report, findings: report.findings.map((f) => ({ ...f, ...changes[f.id] })) };
}

/** The owner's 8 cores, C7 the fastest. */
const ownerClocks: CoreClock[] = [4800, 4850, 4750, 4900, 4800, 4950, 4700, 5200].map((mhz, core) => ({ core, kind: null, mhz }));

describe('Live clocks', () => {
  it('shows one cell per core with Average and Highest while polling', () => {
    const view = clockView(ownerClocks, 5300, shouldPoll(true, true, true));
    expect(view.cells.map((c) => [c.label, c.tag, c.text])).toEqual([
      ['C0', null, '4.80 GHz'], ['C1', null, '4.85 GHz'], ['C2', null, '4.75 GHz'], ['C3', null, '4.90 GHz'],
      ['C4', null, '4.80 GHz'], ['C5', null, '4.95 GHz'], ['C6', null, '4.70 GHz'], ['C7', null, '5.20 GHz'],
    ]);
    expect(view.cells[7].percent).toBe(98); // 5200 of the 5300 MHz max boost
    expect([view.average, view.highest, view.badge, view.paused]).toEqual(['4.87 GHz', '5.20 GHz', '1 Hz', false]);
  });

  it('pauses, dimmed, whenever Kestrel’s window is hidden or not focused', () => {
    for (const [visible, focused] of [[true, false], [false, true], [false, false]]) {
      const view = clockView(ownerClocks, 5300, shouldPoll(true, visible, focused));
      expect([view.badge, view.paused]).toEqual(['Paused', true]);
      // The last values stay on screen, dimmed.
      expect(view.cells[7].text).toBe('5.20 GHz');
    }
  });

  it('tags P- and E-cores, and scales to the fastest core when the max boost is unknown', () => {
    const hybrid: CoreClock[] = [
      { core: 0, kind: 'performance', mhz: 5500 }, { core: 1, kind: 'efficiency', mhz: 4300 }, { core: 2, kind: 'efficiency', mhz: null },
    ];
    const view = clockView(hybrid, null, true);
    expect(view.cells.map((c) => [c.label, c.tag, c.text, c.percent])).toEqual([
      ['C0', 'P', '5.50 GHz', 100], ['C1', 'E', '4.30 GHz', 78], ['C2', 'E', '—', 0],
    ]);
    expect([view.average, view.highest]).toEqual(['4.90 GHz', '5.50 GHz']);
    const none = clockView([{ core: 0, kind: null, mhz: null }], 5300, true);
    expect([none.average, none.highest]).toEqual([null, null]);
  });
});

describe('Hardware panels', () => {
  it('shows the processor, its caches and its instruction sets', () => {
    const view = processorView(ownerReport().snapshot);
    expect(view.name).toBe('Intel Core i9-11900K');
    expect(lines(view.facts)).toEqual([
      ['Codename', 'Rocket Lake-S'], ['Socket', 'LGA1200'], ['Cores', '8 cores · 16 threads'], ['Base clock', '3.50 GHz'],
      ['Max boost', '5.30 GHz'], ['L1 data', '8 × 48 KB'], ['L1 instruction', '8 × 32 KB'], ['L2', '8 × 512 KB'], ['L3', '16 MB'],
    ]);
    expect(view.chips).toEqual([
      { name: 'SSE4.2', on: true }, { name: 'AVX', on: true }, { name: 'AVX2', on: true }, { name: 'AVX-512', on: true },
      { name: 'AVX-VNNI', on: false }, { name: 'AMX', on: false },
    ]);
  });

  it('says Not reported for processor values Windows didn’t give, never all chips dimmed', () => {
    const s = ownerReport().snapshot;
    const view = processorView({ ...s, cpu: { ...s.cpu, codename: null, socket: null, maxMhz: null, caches: [], instructions: [] } });
    expect(lines(view.facts).filter(([, value]) => value === null).map(([label]) => label)).toEqual(['Codename', 'Socket', 'Max boost', 'Caches']);
    expect(view.chips).toBeNull();
  });

  it('shows each real graphics card with its PCIe link, and the virtual ones by name', () => {
    const view = graphicsView(ownerReport());
    expect(view.cards.map((c) => c.name)).toEqual(['NVIDIA GeForce RTX 3060 Ti']);
    expect(lines(view.cards[0].facts)).toEqual([
      ['VRAM', '8 GB'], ['Driver', '610.74 (2026-09-15)'], ['PCIe link', 'Card 4.0 x16 · Slot 4.0 x16'], ['Now', '1.0 x16'],
      ['Under load', '4.0 x16 · from your benchmark on 13 Oct'], ['Resizable BAR', 'On', 'good'],
    ]);
    expect(view.cards[0].facts[3].note).toBe('The card slows its link when idle.');
    expect(view.virtual).toEqual(['Parsec Virtual Display Adapter']);
  });

  it('asks for a benchmark before any, and flags only the main card’s PCIe problems', () => {
    const owner = ownerReport();
    const [parsec, rtx] = owner.snapshot.gpus;
    const igpu = { ...rtx, name: 'Intel(R) UHD Graphics 750', vendor: 'intel' as const, driverVersion: '31.0.101.2127', vramMb: 128, isIntegrated: true };
    const report = withFindings(
      { ...owner, snapshot: { ...owner.snapshot, gpus: [parsec, igpu, { ...rtx, linkUnderLoad: null }] } },
      { 'pcie-lanes': { status: 'problem', fix: { kind: 'bios-guide', topic: 'pcie-slot' } } },
    );
    const view = graphicsView(report);
    expect(view.cards.map((c) => c.name)).toEqual(['NVIDIA GeForce RTX 3060 Ti', 'Intel UHD Graphics 750']);
    expect(lines(view.cards[0].facts).slice(3)).toEqual([
      ['Now', '1.0 x16', 'warn', 'pcie-slot'], ['Under load', 'Run a benchmark to check the link under load.', 'muted'], ['Resizable BAR', 'On', 'good'],
    ]);
    // Integrated graphics have no PCIe slot or Resizable BAR to show.
    expect(lines(view.cards[1].facts)).toEqual([['VRAM', '128 MB'], ['Driver', '31.0.101.2127 (2026-09-15)']]);
  });

  it('shows memory per module, with the kit rating from the part numbers', () => {
    const view = memoryView(ownerReport());
    expect(view.name).toBe('32 GB DDR4');
    expect(lines(view.facts)).toEqual([['Channels', 'Dual channel'], ['Slots', '2 of 4'], ['Speed now', '3600 MT/s'], ['Kit rating', '3600 MT/s']]);
    expect(view.modules).toEqual([
      { slot: 'A2', size: '16 GB', maker: 'Corsair', part: 'CMG32GX4M2D3600C18', speed: '3600 MT/s', rank: 'Dual rank' },
      { slot: 'B2', size: '16 GB', maker: 'Corsair', part: 'CMG32GX4M2D3600C18', speed: '3600 MT/s', rank: 'Dual rank' },
    ]);
  });

  it('flags slow RAM with the XMP guide, and says Rating unknown for a kit Kestrel doesn’t know', () => {
    const owner = ownerReport();
    const slow = withFindings(owner, {
      'ram-speed': { status: 'problem', title: 'RAM runs at 2133 MT/s, but the kit is rated 3600', detail: '', fix: { kind: 'bios-guide', topic: 'xmp' } },
    });
    expect(lines(memoryView(slow).facts)[2]).toEqual(['Speed now', '3600 MT/s', 'warn', 'xmp']);
    const s = owner.snapshot;
    const unknown: Report = { ...owner, snapshot: { ...s, memory: { ...s.memory, ratedMts: null } } };
    expect(lines(memoryView(unknown).facts)[3]).toEqual(['Kit rating', 'Rating unknown', 'muted']);
    const bare = memoryView({ ...owner, snapshot: { ...s, memory: { ...s.memory, slotsTotal: null, modules: [{ ...s.memory.modules[0], rank: null, manufacturer: '' }] } } });
    expect(lines(bare.facts)[1]).toEqual(['Slots', '1 used']);
    expect([bare.modules[0].maker, bare.modules[0].rank]).toEqual([null, null]);
  });

  it('shows the board, its BIOS and the BIOS settings Kestrel can see', () => {
    const view = boardView(ownerReport());
    expect(view.name).toBe('ASUS ROG MAXIMUS XIII HERO');
    expect(lines(view.facts)).toEqual([
      ['Chipset', 'Intel Z590'], ['BIOS vendor', 'American Megatrends Inc.'], ['BIOS version', '2103'], ['BIOS date', '2024-05-20', 'warn', 'update-bios'],
    ]);
    expect(lines(view.settings)).toEqual([
      ['XMP', 'On', 'good'], ['Resizable BAR', 'On', 'good'], ['Virtualization (VT-x)', 'On', 'good'], ['IOMMU (VT-d)', 'Off', 'iommu'],
      ['Secure Boot', 'On', 'good'], ['TPM 2.0', 'On', 'good'], ['Boot mode', 'UEFI', 'good'],
    ]);
    expect(view.settings.map((f) => f.note ?? null)).toEqual(['RAM runs at its rated 3600 MT/s', null, null, 'Only for Vanguard On-Demand', null, null, null]);
  });

  it('shows each monitor by name, amber below its top refresh rate', () => {
    const s = ownerReport().snapshot;
    const view = displaysView(s);
    expect(view.cards.map((c) => [c.name, c.primary])).toEqual([['XL2566K', true], ['24G4', false]]);
    expect(lines(view.cards[0].facts)).toEqual([
      ['Resolution', '1920×1080'], ['Refresh rate', '360 Hz of 360 Hz'], ['Connection', 'DisplayPort'], ['Graphics card', 'RTX 3060 Ti'],
    ]);
    const parsec = { ...s.displays[1], name: '\\\\.\\DISPLAY3', adapter: 'Parsec Virtual Display Adapter', isVirtual: true, monitorName: null };
    const slow = displaysView({ ...s, displays: [s.displays[0], { ...s.displays[1], currentHz: 60, monitorName: null }, parsec] });
    expect(slow.cards.map((c) => c.name)).toEqual(['XL2566K', 'Display 2']);
    expect(lines(slow.cards[1].facts)[1]).toEqual(['Refresh rate', '60 Hz of 180 Hz', 'warn']);
    expect(slow.virtual).toEqual(['Parsec Virtual Display Adapter']);
  });

  it('shows every drive with its volumes and the games on them', () => {
    const view = storageView(ownerReport().snapshot);
    expect(view.map((d) => [d.name, lines(d.facts)])).toEqual([
      ['Samsung SSD 980 PRO 1TB', [['Type', 'NVMe'], ['Bus', 'NVMe'], ['Size', '931 GB']]],
      ['WDC WD20EZBX-00AYRA0', [['Type', 'HDD'], ['Bus', 'SATA'], ['Size', '1.81 TB']]],
    ]);
    expect(view.map((d) => d.volumes)).toEqual([
      [{ letter: 'C:', space: '210 GB free of 931 GB', games: ['VALORANT'] }],
      [{ letter: 'D:', space: '1.20 TB free of 1.81 TB', games: [] }],
    ]);
  });

  it('shows each network adapter, power saving in amber', () => {
    expect(networkView(ownerReport().snapshot).map((a) => [a.name, lines(a.facts)])).toEqual([
      ['Intel Ethernet Controller I225-V', [['Type', 'Ethernet'], ['Link speed', '2.5 Gbps'], ['Status', 'Connected'], ['Power saving', 'On', 'warn']]],
    ]);
  });
});
```

Create `app/src/components/TitleBar.test.ts`:

```ts
import { expect, it } from 'vitest';
import { strings } from '../strings';
import { TABS } from './TitleBar';

it('puts the Hardware tab between Health and Tweaks', () => {
  expect(TABS.map((tab) => strings.tabs[tab])).toEqual(['Home', 'Health', 'Hardware', 'Tweaks', 'Lock In', 'Benchmark', 'Toolbox']);
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd app && npx vitest run src/components`

Expected: FAIL, 2 files:
- `src/components/hardware/view.test.ts` — `Error: Cannot find module './view'`
- `src/components/TitleBar.test.ts > puts the Hardware tab between Health and Tweaks` — `TypeError: Cannot read properties of undefined (reading 'map')` (`TABS` isn't exported yet)

- [ ] **Step 3: Implement**

**3a.** In `app/src/strings.ts`, replace

```ts
  tabs: { home: 'Home', health: 'Health', tweaks: 'Tweaks', lockIn: 'Lock In', benchmark: 'Benchmark', toolbox: 'Toolbox' },
```

with

```ts
  tabs: { home: 'Home', health: 'Health', hardware: 'Hardware', tweaks: 'Tweaks', lockIn: 'Lock In', benchmark: 'Benchmark', toolbox: 'Toolbox' },
```

replace

```ts
    hardware: 'Hardware',
    lockIn: 'Lock In',
```

with

```ts
    hardware: 'Hardware',
    seeHardware: 'See all',
    lockIn: 'Lock In',
```

replace

```ts
  hardwarePage: {
    notReported: 'Not reported',
```

with

```ts
  hardwarePage: {
    title: 'Hardware',
    notReported: 'Not reported',
    lastScan: (when: string) => `Last scan ${when}`,
    panelFailed: 'Kestrel couldn’t read everything here. Use Copy diagnostics on Home to share the details.',
    panels: {
      processor: 'Processor', clocks: 'Live clocks', graphics: 'Graphics', memory: 'Memory', board: 'Motherboard and BIOS',
      displays: 'Displays', storage: 'Storage', network: 'Network',
    },
    processor: { codename: 'Codename', socket: 'Socket', cores: 'Cores', base: 'Base clock', maxBoost: 'Max boost', caches: 'Caches', instructions: 'Instructions' },
    clocks: {
      live: '1 Hz', paused: 'Paused', average: 'Average', highest: 'Highest',
      core: (n: number) => `C${n}`,
      kinds: { performance: 'P', efficiency: 'E' },
    },
    graphics: {
      vram: 'VRAM', driver: 'Driver', pcie: 'PCIe link', now: 'Now', underLoad: 'Under load', resizableBar: 'Resizable BAR',
      supports: (card: string, slot: string) => `Card ${card} · Slot ${slot}`,
      idle: 'The card slows its link when idle.',
      fromBenchmark: (link: string, day: string) => `${link} · from your benchmark on ${day}`,
      runBenchmark: 'Run a benchmark to check the link under load.',
      virtual: (names: string) => `Virtual adapters: ${names}`,
    },
    memory: {
      channels: 'Channels', dual: 'Dual channel', single: 'Single channel', slots: 'Slots', speed: 'Speed now', kit: 'Kit rating',
      slotsUsed: (used: number, total: number | null) => (total ? `${used} of ${total}` : `${used} used`),
      ratingUnknown: 'Rating unknown',
      columns: { slot: 'Slot', size: 'Size', maker: 'Maker', part: 'Part number', speed: 'Speed', rank: 'Rank' },
      note: 'Timings and voltages need a driver, which Kestrel never loads.',
    },
    board: { chipset: 'Chipset', biosVendor: 'BIOS vendor', biosVersion: 'BIOS version', biosDate: 'BIOS date', settings: 'BIOS settings Kestrel can see' },
    displays: {
      resolution: 'Resolution', refresh: 'Refresh rate', connection: 'Connection', gpu: 'Graphics card', primary: 'Primary',
      refreshOf: (now: number, max: number) => `${now} Hz of ${max} Hz`,
      virtual: (names: string) => `Virtual displays: ${names}`,
    },
    storage: { type: 'Type', bus: 'Bus', size: 'Size', free: (free: string, total: string) => `${free} free of ${total}` },
    network: {
      type: 'Type', link: 'Link speed', status: 'Status', connected: 'Connected', disconnected: 'Not connected', powerSaving: 'Power saving',
      kinds: { ethernet: 'Ethernet', wifi: 'Wi-Fi', other: 'Other' },
    },
```

and replace

```ts
  specs: {
    labels: { cpu: 'CPU', gpu: 'GPU', ram: 'RAM', board: 'Board', displays: 'Displays', storage: 'Storage', os: 'OS' },
```

with

```ts
  specs: {
    copy: 'Copy specs',
    copied: 'Copied to clipboard',
    failed: 'Couldn’t copy. Try again.',
    labels: { cpu: 'CPU', gpu: 'GPU', ram: 'RAM', board: 'Board', displays: 'Displays', storage: 'Storage', os: 'OS' },
```

**3b.** Create `app/src/components/hardware/view.ts`:

```ts
// What each Hardware panel shows, as plain data: the components only lay it out. Pure, so it's tested in node.
import { cpuName, driverLabel, mainGpu } from '../../api/findings';
import {
  biosSettings, boardMaker, busLabel, cacheRows, coresLabel, dayMonth, displayName, gamesOn, ghz, gpuShortName, linkLabel,
  linkSpeedLabel, mediaLabel, ramChannels, ramSpeed, sizeLabel, slotLabel, vramLabel,
} from '../../api/hardware';
import type { BiosTopic, CoreClock, GpuInfo, PcieLink, Report, SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';

const h = strings.hardwarePage;

/** One label–value line of a panel. A null value shows Not reported. */
export interface Fact {
  label: string;
  value: string | null;
  /** good: lime (on, or as good as it gets); warn: amber (a Health Check problem); muted: a hint rather than a value. */
  tone?: 'good' | 'warn' | 'muted';
  note?: string;
  /** Shows the BIOS guide link for this topic. */
  topic?: BiosTopic;
}

/** A device's panel: its name (null: Not reported) and its lines. */
export interface Card { name: string | null; facts: Fact[] }

/** The instruction sets the Processor panel always shows, in the probe's order; the ones the CPU lacks are dimmed. */
const INSTRUCTIONS = ['SSE4.2', 'AVX', 'AVX2', 'AVX-512', 'AVX-VNNI', 'AMX'];

const onOff = (value: boolean | null): string | null => (value === null ? null : value ? h.settings.on : h.settings.off);

/** Trademarks and Windows' instance numbers out: "Intel(R) Ethernet Controller (3) I225-V" → "Intel Ethernet Controller I225-V". */
const cleanName = (name: string): string => name.replace(/\((R|TM)\)/gi, '').replace(/\s*\(\d+\)/g, '').replace(/\s+/g, ' ').trim();

/** A Health Check item that is a Problem: amber, plus its BIOS-guide topic when it has one. */
function flagged(report: Report, id: string): Pick<Fact, 'tone' | 'topic'> {
  const f = report.findings.find((x) => x.id === id);
  if (f?.status !== 'problem') return {};
  return f.fix.kind === 'bios-guide' ? { tone: 'warn', topic: f.fix.topic } : { tone: 'warn' };
}

export function processorView(s: SystemSnapshot): Card & { chips: { name: string; on: boolean }[] | null } {
  const { cpu } = s;
  const p = h.processor;
  const caches = cacheRows(cpu.caches);
  return {
    name: cpuName(s) || null,
    facts: [
      { label: p.codename, value: cpu.codename },
      { label: p.socket, value: cpu.socket },
      { label: p.cores, value: cpu.cores > 0 ? coresLabel(cpu) : null },
      { label: p.base, value: cpu.baseMhz === null ? null : ghz(cpu.baseMhz) },
      { label: p.maxBoost, value: cpu.maxMhz === null ? null : ghz(cpu.maxMhz) },
      ...(caches.length > 0 ? caches.map(([label, value]) => ({ label, value })) : [{ label: p.caches, value: null }]),
    ],
    // No sets at all means CPUID couldn't be read, not a CPU without SSE4.2: Not reported, never all dimmed.
    chips: cpu.instructions.length > 0 ? INSTRUCTIONS.map((name) => ({ name, on: cpu.instructions.includes(name) })) : null,
  };
}

export interface ClockCell { label: string; tag: string | null; text: string; percent: number }

/**
 * The live clocks: one cell per physical core (P or E on hybrid CPUs) with a bar scaled to the max boost clock, else to
 * the fastest core; Average and Highest over the cores Windows reported; the 1 Hz badge, or Paused (cells dimmed).
 */
export function clockView(clocks: CoreClock[], maxMhz: number | null, polling: boolean) {
  const c = h.clocks;
  const values = clocks.map((x) => x.mhz).filter((v): v is number => v !== null);
  const top = maxMhz ?? Math.max(0, ...values);
  const cells: ClockCell[] = clocks.map((x) => ({
    label: c.core(x.core),
    tag: x.kind === null ? null : c.kinds[x.kind],
    text: x.mhz === null ? '—' : ghz(x.mhz),
    percent: x.mhz === null || top <= 0 ? 0 : Math.min(100, Math.round((x.mhz / top) * 100)),
  }));
  return {
    cells,
    average: values.length > 0 ? ghz(values.reduce((a, b) => a + b, 0) / values.length) : null,
    highest: values.length > 0 ? ghz(Math.max(...values)) : null,
    badge: polling ? c.live : c.paused,
    paused: !polling,
  };
}

function gpuFacts(report: Report, gpu: GpuInfo, main: boolean): Fact[] {
  const g = h.graphics;
  const facts: Fact[] = [
    { label: g.vram, value: gpu.vramMb === null ? null : vramLabel(gpu.vramMb) },
    { label: g.driver, value: gpu.driverVersion ? `${driverLabel(gpu)}${gpu.driverDate ? ` (${gpu.driverDate})` : ''}` : null },
  ];
  if (gpu.isIntegrated) return facts;
  // The PCIe and Resizable BAR checks look at the main card only, so only its lines can be flagged.
  const flag = (id: string) => (main ? flagged(report, id) : {});
  const { pcie, linkUnderLoad } = gpu;
  const link = (l: PcieLink | null) => (l ? linkLabel(l) : h.notReported);
  if (pcie) {
    facts.push(
      { label: g.pcie, value: g.supports(link(pcie.cardMax), link(pcie.slotMax)) },
      { label: g.now, value: pcie.now ? linkLabel(pcie.now) : null, ...(pcie.now ? { note: g.idle } : {}), ...flag('pcie-lanes') },
    );
  } else {
    facts.push({ label: g.pcie, value: null });
  }
  const day = linkUnderLoad ? (dayMonth(linkUnderLoad.measuredAt) ?? linkUnderLoad.measuredAt) : '';
  facts.push(
    linkUnderLoad
      ? { label: g.underLoad, value: g.fromBenchmark(linkLabel(linkUnderLoad.link), day), ...flag('pcie-speed') }
      : { label: g.underLoad, value: g.runBenchmark, tone: 'muted' },
    { label: g.resizableBar, value: onOff(gpu.resizableBar), ...(gpu.resizableBar ? { tone: 'good' as const } : {}), ...flag('resizable-bar') },
  );
  return facts;
}

/** One card per real graphics card, the main one first; virtual adapters (Parsec…) only by name, for the dim line. */
export function graphicsView(report: Report): { cards: Card[]; virtual: string[] } {
  const { gpus } = report.snapshot;
  const main = mainGpu(gpus);
  const real = gpus.filter((g) => g.vendor !== 'virtual').sort((a, b) => Number(b === main) - Number(a === main));
  return {
    cards: real.map((gpu) => ({ name: cleanName(gpu.name) || null, facts: gpuFacts(report, gpu, gpu === main) })),
    virtual: gpus.filter((g) => g.vendor === 'virtual').map((g) => cleanName(g.name)),
  };
}

export interface ModuleRow { slot: string | null; size: string; maker: string | null; part: string | null; speed: string | null; rank: string | null }

export function memoryView(report: Report): Card & { modules: ModuleRow[] } {
  const s = report.snapshot;
  const m = h.memory;
  const { totalMb, ddrGeneration, modules, slotsTotal, ratedMts: rated } = s.memory;
  const channels = ramChannels(report);
  const speed = ramSpeed(s);
  return {
    name: totalMb > 0 ? `${Math.round(totalMb / 1024)} GB${ddrGeneration ? ` DDR${ddrGeneration}` : ''}` : null,
    facts: [
      { label: m.channels, value: channels === null ? null : m[channels], ...flagged(report, 'ram-channels') },
      { label: m.slots, value: modules.length > 0 ? m.slotsUsed(modules.length, slotsTotal) : null },
      { label: m.speed, value: speed === null ? null : `${speed} MT/s`, ...flagged(report, 'ram-speed') },
      rated === null ? { label: m.kit, value: m.ratingUnknown, tone: 'muted' } : { label: m.kit, value: `${rated} MT/s` },
    ],
    modules: modules.map((mod) => ({
      slot: slotLabel(mod),
      size: `${Math.round(mod.capacityMb / 1024)} GB`,
      maker: mod.manufacturer.trim() || null,
      part: mod.partNumber.trim() || null,
      speed: mod.configuredMts ? `${mod.configuredMts} MT/s` : null,
      rank: mod.rank ? h.rank(mod.rank) : null,
    })),
  };
}

/** The board, its BIOS, and "BIOS settings Kestrel can see": On in lime, a Problem in amber, Off with its guide link. */
export function boardView(report: Report): Card & { settings: Fact[] } {
  const { board } = report.snapshot;
  const b = h.board;
  return {
    name: [boardMaker(report), board.product.trim()].filter(Boolean).join(' ') || null,
    facts: [
      { label: b.chipset, value: board.chipset },
      { label: b.biosVendor, value: board.biosVendor.trim() || null },
      { label: b.biosVersion, value: board.biosVersion.trim() || null },
      { label: b.biosDate, value: board.biosDate, ...flagged(report, 'bios-age') },
    ],
    settings: biosSettings(report).map((row) => ({
      label: row.label,
      value: row.value,
      ...(row.problem ? { tone: 'warn' as const } : row.state === 'on' ? { tone: 'good' as const } : {}),
      ...(row.note ? { note: row.note } : {}),
      ...(row.topic ? { topic: row.topic } : {}),
    })),
  };
}

/** One card per monitor (amber when it runs below its top rate); virtual displays only by adapter, for the dim line. */
export function displaysView(s: SystemSnapshot): { cards: (Card & { primary: boolean })[]; virtual: string[] } {
  const d = h.displays;
  return {
    cards: s.displays.filter((x) => !x.isVirtual).map((x, i) => {
      const known = x.currentHz > 1; // Windows says 0 or 1 Hz for "hardware default"
      const slow = known && x.maxHz > 0 && x.currentHz + 1 < x.maxHz;
      return {
        name: displayName(x, i + 1),
        primary: x.isPrimary,
        facts: [
          { label: d.resolution, value: x.width > 0 && x.height > 0 ? `${x.width}×${x.height}` : null },
          { label: d.refresh, value: known ? (x.maxHz > 0 ? d.refreshOf(x.currentHz, x.maxHz) : `${x.currentHz} Hz`) : null, ...(slow ? { tone: 'warn' as const } : {}) },
          { label: d.connection, value: x.connection },
          { label: d.gpu, value: x.adapter.trim() ? gpuShortName(x.adapter) : null },
        ],
      };
    }),
    virtual: s.displays.filter((x) => x.isVirtual).map((x) => cleanName(x.adapter) || x.name),
  };
}

export interface VolumeRow { letter: string; space: string; games: string[] }

/** One card per drive, with each lettered volume's free space and the games installed on it. */
export function storageView(s: SystemSnapshot): (Card & { volumes: VolumeRow[] })[] {
  const st = h.storage;
  return s.storage.drives.map((drive) => ({
    name: drive.model.trim() || null,
    facts: [
      { label: st.type, value: mediaLabel(drive.media) },
      { label: st.bus, value: busLabel(drive.bus) },
      { label: st.size, value: drive.sizeGb > 0 ? sizeLabel(drive.sizeGb) : null },
    ],
    volumes: drive.volumes.map((v) => ({
      letter: `${v.letter.replace(':', '')}:`,
      space: v.totalGb > 0 ? st.free(sizeLabel(v.freeGb), sizeLabel(v.totalGb)) : h.notReported,
      games: gamesOn(s, v.letter),
    })),
  }));
}

/** One card per adapter. Power saving on a connected adapter and a slow cable link are Health Check problems: amber. */
export function networkView(s: SystemSnapshot): Card[] {
  const n = h.network;
  return s.network.adapters.map((a) => ({
    name: cleanName(a.name) || null,
    facts: [
      { label: n.type, value: n.kinds[a.kind] },
      {
        label: n.link,
        value: a.linkMbps === null ? null : linkSpeedLabel(a.linkMbps),
        ...(a.kind === 'ethernet' && a.connected && a.linkMbps !== null && a.linkMbps < 1000 ? { tone: 'warn' as const } : {}),
      },
      { label: n.status, value: a.connected ? n.connected : n.disconnected },
      { label: n.powerSaving, value: onOff(a.powerSaving), ...(a.connected && a.powerSaving ? { tone: 'warn' as const } : {}) },
    ],
  }));
}
```

**3c.** Create `app/src/components/hardware/HwPanel.tsx`:

```tsx
import type { ReactNode } from 'react';
import type { BiosTopic } from '../../api/types';
import { strings } from '../../strings';
import type { Fact } from './view';

const h = strings.hardwarePage;

type PanelProps = { title: string; failed?: boolean; aside?: ReactNode; className?: string; children: ReactNode };

/** One Hardware panel: its label (and whatever sits at the right of it), the notice when its probe failed, its content. */
export function HwPanel({ title, failed = false, aside, className, children }: PanelProps) {
  return (
    <section className={`panel hw-panel${className ? ` ${className}` : ''}`}>
      <div className="panel-head">
        <h2 className="label">{title}</h2>
        {aside}
      </div>
      {failed && <p className="notice cut-s">{h.panelFailed}</p>}
      {children}
    </section>
  );
}

/** A device's name at the top of its panel; Not reported, in grey, when Windows gave none. */
export function DeviceName({ name, children }: { name: string | null; children?: ReactNode }) {
  return (
    <h3 className={`hw-name${name === null ? ' tone-muted' : ''}`}>
      {name ?? h.notReported}
      {children}
    </h3>
  );
}

/** A panel's label–value lines. A missing value says Not reported, in grey; a flagged line links its BIOS-guide topic. */
export function Facts({ facts, onGuide }: { facts: Fact[]; onGuide?: (topic: BiosTopic) => void }) {
  return (
    <dl className="facts">
      {facts.map(({ label, value, tone, note, topic }) => (
        <div key={label} className="fact">
          <dt>{label}</dt>
          <dd>
            <span className={value === null || tone === 'muted' ? 'tone-muted' : `num${tone ? ` tone-${tone}` : ''}`}>{value ?? h.notReported}</span>
            {note && <span className="muted small">{note}</span>}
            {topic && onGuide && <button className="link" onClick={() => onGuide(topic)}>{strings.health.biosGuide}</button>}
          </dd>
        </div>
      ))}
    </dl>
  );
}
```

**3d.** Create `app/src/components/hardware/CopySpecs.tsx` (the same clipboard and 3-second note as `CopyDiagnostics`):

```tsx
import { IconClipboard } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { specsText } from '../../api/specs';
import type { Report } from '../../api/types';
import { strings } from '../../strings';

/** Copies a short plain-text summary of the PC, to paste in a chat or a forum (decision 77). */
export function CopySpecs({ report }: { report: Report }) {
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    if (!note) return;
    const id = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(id);
  }, [note]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(specsText(report));
      setNote(strings.specs.copied);
    } catch {
      setNote(strings.specs.failed);
    }
  };

  return (
    <span className="copy">
      <button className="btn btn-primary cut-s" onClick={() => void copy()}>
        <IconClipboard size={14} aria-hidden /> {strings.specs.copy}
      </button>
      {note && <span className="toast" role="status">{note}</span>}
    </span>
  );
}
```

**3e.** Create `app/src/components/hardware/HardwareTop.tsx`:

```tsx
import { IconRefresh } from '@tabler/icons-react';
import { dayMonthTime, summaryLine, windowsLine } from '../../api/hardware';
import type { Report } from '../../api/types';
import { strings } from '../../strings';
import { CopySpecs } from './CopySpecs';

const h = strings.hardwarePage;

/** The page head: the PC in one line, Windows, when it was scanned, Scan again and Copy specs. */
export function HardwareTop({ report, onRescan }: { report: Report; onRescan: () => void }) {
  const s = report.snapshot;
  const summary = summaryLine(s);
  const windows = windowsLine(s);
  return (
    <header className="page-head hw-head">
      <div className="hw-title">
        <h1>{h.title}</h1>
        {summary && <p className="num">{summary}</p>}
        {windows && <p className="muted small num">{windows}</p>}
      </div>
      <div className="hw-actions">
        <span className="label">{h.lastScan(dayMonthTime(s.collectedAt) ?? h.notReported)}</span>
        <button className="btn btn-ghost cut-s" onClick={onRescan}>
          <IconRefresh size={14} aria-hidden /> {strings.home.rescan}
        </button>
        <CopySpecs report={report} />
      </div>
    </header>
  );
}
```

**3f.** Create `app/src/components/hardware/ProcessorPanel.tsx`:

```tsx
import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { processorView } from './view';

const h = strings.hardwarePage;

/** Name, codename, socket, cores, clocks, caches, and a chip per instruction set (dimmed when the CPU lacks it). */
export function ProcessorPanel({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  const view = processorView(snapshot);
  return (
    <HwPanel title={h.panels.processor} failed={failed}>
      <DeviceName name={view.name} />
      <Facts facts={view.facts} />
      <div>
        <h4 className="label">{h.processor.instructions}</h4>
        {view.chips ? (
          <ul className="chips">
            {view.chips.map((chip) => (
              <li key={chip.name} className={`chip cut-s${chip.on ? '' : ' chip-off'}`}>{chip.name}</li>
            ))}
          </ul>
        ) : (
          <p className="tone-muted small">{h.notReported}</p>
        )}
      </div>
    </HwPanel>
  );
}
```

**3g.** Create `app/src/components/hardware/LiveClocks.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { getCpuClocks } from '../../api/client';
import { shouldPoll } from '../../api/hardware';
import type { CoreClock } from '../../api/types';
import { strings } from '../../strings';
import { HwPanel } from './HwPanel';
import { clockView } from './view';

const h = strings.hardwarePage;

/**
 * Each physical core's clock, read once a second, but only while Kestrel's window is visible and focused (a game in
 * front means it isn't); otherwise the last values stay, dimmed, under Paused. The page is mounted only while the
 * Hardware tab is shown, so leaving the tab stops the reads as well.
 */
export function LiveClocks({ maxMhz }: { maxMhz: number | null }) {
  const [clocks, setClocks] = useState<CoreClock[] | null>(null);
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');
  const [focused, setFocused] = useState(() => document.hasFocus());
  const polling = shouldPoll(true, visible, focused);

  useEffect(() => {
    const onVisibility = () => setVisible(document.visibilityState === 'visible');
    const onFocus = () => setFocused(true);
    const onBlur = () => setFocused(false);
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('focus', onFocus);
    window.addEventListener('blur', onBlur);
    return () => {
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('blur', onBlur);
    };
  }, []);

  useEffect(() => {
    if (!polling) return;
    let alive = true;
    const tick = async () => {
      try {
        const next = await getCpuClocks();
        if (alive) setClocks(next);
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
  }, [polling]);

  const view = clockView(clocks ?? [], maxMhz, polling);
  const c = h.clocks;
  return (
    <HwPanel
      title={h.panels.clocks}
      className={view.paused ? 'clocks-paused' : undefined}
      aside={<span className={`small num ${view.paused ? 'muted' : 'live'}`}>{view.badge}</span>}
    >
      {clocks === null ? (
        <p className="muted small num">…</p>
      ) : view.highest === null ? (
        <p className="tone-muted small">{h.notReported}</p>
      ) : (
        <>
          <ol className="clock-cells">
            {view.cells.map((cell) => (
              <li key={cell.label} className="clock-cell cut-s">
                <span className="muted small">
                  {cell.label}
                  {cell.tag && <span className="clock-kind"> {cell.tag}</span>}
                </span>
                <span className="num">{cell.text}</span>
                <span className="clock-bar" aria-hidden="true">
                  <span style={{ width: `${cell.percent}%` }} />
                </span>
              </li>
            ))}
          </ol>
          <p className="small num">
            <span className="muted">{c.average}</span> {view.average} · <span className="muted">{c.highest}</span> {view.highest}
          </p>
        </>
      )}
    </HwPanel>
  );
}
```

**3h.** Create `app/src/components/hardware/GraphicsPanels.tsx`:

```tsx
import type { BiosTopic, Report } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { graphicsView } from './view';

const h = strings.hardwarePage;

/** One panel per real graphics card, the main one first; virtual adapters (Parsec…) on one dim line at the end. */
export function GraphicsPanels({ report, failed, onGuide }: { report: Report; failed: boolean; onGuide: (topic: BiosTopic) => void }) {
  const { cards, virtual } = graphicsView(report);
  const virtualLine = virtual.length > 0 && <p className="muted small">{h.graphics.virtual(virtual.join(', '))}</p>;
  if (cards.length === 0) {
    return (
      <HwPanel title={h.panels.graphics} failed={failed}>
        <DeviceName name={null} />
        {virtualLine}
      </HwPanel>
    );
  }
  return (
    <>
      {cards.map((card, i) => (
        <HwPanel key={`${card.name}-${i}`} title={h.panels.graphics} failed={failed && i === 0}>
          <DeviceName name={card.name} />
          <Facts facts={card.facts} onGuide={onGuide} />
          {i === cards.length - 1 && virtualLine}
        </HwPanel>
      ))}
    </>
  );
}
```

**3i.** Create `app/src/components/hardware/MemoryPanel.tsx`:

```tsx
import type { BiosTopic, Report } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { memoryView } from './view';

const h = strings.hardwarePage;

/** Total, channels, slots, speed and kit rating, then one row per module. */
export function MemoryPanel({ report, failed, onGuide }: { report: Report; failed: boolean; onGuide: (topic: BiosTopic) => void }) {
  const view = memoryView(report);
  const col = h.memory.columns;
  return (
    <HwPanel title={h.panels.memory} failed={failed}>
      <DeviceName name={view.name} />
      <Facts facts={view.facts} onGuide={onGuide} />
      {view.modules.length > 0 && (
        <table className="modules">
          <thead>
            <tr>
              {[col.slot, col.size, col.maker, col.part, col.speed, col.rank].map((name) => (
                <th key={name}>{name}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {view.modules.map((m, i) => (
              <tr key={`${m.slot}-${i}`}>
                {[m.slot, m.size, m.maker, m.part, m.speed, m.rank].map((value, j) => (
                  <td key={j} className={value === null ? 'tone-muted' : 'num'}>{value ?? h.notReported}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="muted small">{h.memory.note}</p>
    </HwPanel>
  );
}
```

**3j.** Create `app/src/components/hardware/BoardPanel.tsx`:

```tsx
import type { BiosTopic, Report } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { boardView } from './view';

const h = strings.hardwarePage;

/** The board, chipset and BIOS, then the BIOS settings Kestrel can see, each off one linking its BIOS-guide topic. */
export function BoardPanel({ report, failed, onGuide }: { report: Report; failed: boolean; onGuide: (topic: BiosTopic) => void }) {
  const view = boardView(report);
  return (
    <HwPanel title={h.panels.board} failed={failed}>
      <DeviceName name={view.name} />
      <Facts facts={view.facts} onGuide={onGuide} />
      <h4 className="label">{h.board.settings}</h4>
      <Facts facts={view.settings} onGuide={onGuide} />
    </HwPanel>
  );
}
```

**3k.** Create `app/src/components/hardware/DisplayPanels.tsx`:

```tsx
import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { displaysView } from './view';

const h = strings.hardwarePage;

/** One panel per monitor; virtual displays on one dim line at the end. */
export function DisplayPanels({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  const { cards, virtual } = displaysView(snapshot);
  const virtualLine = virtual.length > 0 && <p className="muted small">{h.displays.virtual(virtual.join(', '))}</p>;
  if (cards.length === 0) {
    return (
      <HwPanel title={h.panels.displays} failed={failed}>
        <DeviceName name={null} />
        {virtualLine}
      </HwPanel>
    );
  }
  return (
    <>
      {cards.map((card, i) => (
        <HwPanel key={`${card.name}-${i}`} title={h.panels.displays} failed={failed && i === 0}>
          <DeviceName name={card.name}>
            {card.primary && <span className="tag tag-info cut-s">{h.displays.primary}</span>}
          </DeviceName>
          <Facts facts={card.facts} />
          {i === cards.length - 1 && virtualLine}
        </HwPanel>
      ))}
    </>
  );
}
```

**3l.** Create `app/src/components/hardware/StoragePanels.tsx`:

```tsx
import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { storageView } from './view';

const h = strings.hardwarePage;

/** One panel per drive: type, bus and size, then each volume with its free space and the games installed there. */
export function StoragePanels({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  const cards = storageView(snapshot);
  if (cards.length === 0) {
    return (
      <HwPanel title={h.panels.storage} failed={failed}>
        <DeviceName name={null} />
      </HwPanel>
    );
  }
  return (
    <>
      {cards.map((card, i) => (
        <HwPanel key={`${card.name}-${i}`} title={h.panels.storage} failed={failed && i === 0}>
          <DeviceName name={card.name} />
          <Facts facts={card.facts} />
          {card.volumes.length > 0 && (
            <ul className="volumes">
              {card.volumes.map((v) => (
                <li key={v.letter}>
                  <span className="num">{v.letter}</span>
                  <span className="num">{v.space}</span>
                  {v.games.map((game) => (
                    <span key={game} className="tag tag-info cut-s">{game}</span>
                  ))}
                </li>
              ))}
            </ul>
          )}
        </HwPanel>
      ))}
    </>
  );
}
```

**3m.** Create `app/src/components/hardware/NetworkPanels.tsx`:

```tsx
import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { networkView } from './view';

const h = strings.hardwarePage;

/** One panel per network adapter: kind, link speed, connected, power saving. */
export function NetworkPanels({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  const cards = networkView(snapshot);
  if (cards.length === 0) {
    return (
      <HwPanel title={h.panels.network} failed={failed}>
        <DeviceName name={null} />
      </HwPanel>
    );
  }
  return (
    <>
      {cards.map((card, i) => (
        <HwPanel key={`${card.name}-${i}`} title={h.panels.network} failed={failed && i === 0}>
          <DeviceName name={card.name} />
          <Facts facts={card.facts} />
        </HwPanel>
      ))}
    </>
  );
}
```

**3n.** Create `app/src/pages/Hardware.tsx` (Live clocks sit in their own panel beside Processor, so the first row pairs up):

```tsx
import { panelFailed, type PanelId } from '../api/hardware';
import type { BiosTopic, Report } from '../api/types';
import { BoardPanel } from '../components/hardware/BoardPanel';
import { DisplayPanels } from '../components/hardware/DisplayPanels';
import { GraphicsPanels } from '../components/hardware/GraphicsPanels';
import { HardwareTop } from '../components/hardware/HardwareTop';
import { LiveClocks } from '../components/hardware/LiveClocks';
import { MemoryPanel } from '../components/hardware/MemoryPanel';
import { NetworkPanels } from '../components/hardware/NetworkPanels';
import { ProcessorPanel } from '../components/hardware/ProcessorPanel';
import { StoragePanels } from '../components/hardware/StoragePanels';

type Props = { report: Report; onGuide: (topic: BiosTopic) => void; onRescan: () => void };

/** Every part of the PC Windows reports without a driver (decisions 52 and 74), from the last scan; only the clocks move. */
export function Hardware({ report, onGuide, onRescan }: Props) {
  const s = report.snapshot;
  const failed = (panel: PanelId) => panelFailed(s.probeErrors, panel);
  return (
    <div className="page hardware">
      <HardwareTop report={report} onRescan={onRescan} />
      <div className="hw-grid">
        <ProcessorPanel snapshot={s} failed={failed('processor')} />
        <LiveClocks maxMhz={s.cpu.maxMhz} />
        <GraphicsPanels report={report} failed={failed('graphics')} onGuide={onGuide} />
        <MemoryPanel report={report} failed={failed('memory')} onGuide={onGuide} />
        <BoardPanel report={report} failed={failed('board')} onGuide={onGuide} />
        <DisplayPanels snapshot={s} failed={failed('displays')} />
        <StoragePanels snapshot={s} failed={failed('storage')} />
        <NetworkPanels snapshot={s} failed={failed('network')} />
      </div>
    </div>
  );
}
```

**3o.** Create `app/src/styles/hardware.css`:

```css
/* The Hardware tab: a scrolling page of panels, in two columns from 1100 px. */
.hardware { max-width: 1180px; }
.hw-head { display: flex; align-items: flex-end; gap: 12px 20px; flex-wrap: wrap; }
.hw-title { display: grid; gap: 4px; flex: 1; min-width: 0; }
.hw-actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.hw-grid { display: grid; grid-template-columns: minmax(0, 1fr); gap: 10px; align-items: start; }
@media (min-width: 1100px) { .hw-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
.hw-panel { display: grid; gap: 10px; align-content: start; }
.hw-panel h4 { margin: 0 0 6px; font-weight: 400; }
.hw-name { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 16px; font-weight: 600; }
.facts { display: grid; grid-template-columns: 130px minmax(0, 1fr); gap: 6px 12px; margin: 0; font-size: 12.5px; }
.fact { display: contents; }
.facts dt { color: var(--muted); }
.facts dd { margin: 0; display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 10px; min-width: 0; }
.tone-good { color: var(--accent); }
.tone-warn { color: var(--warn); }
.tone-muted { color: var(--muted); }
.chips { list-style: none; display: flex; flex-wrap: wrap; gap: 6px; padding: 0; margin: 0; }
.chip { padding: 3px 9px; background: var(--panel-2); color: var(--accent); font-family: var(--font-num); font-size: 11.5px; }
.chip-off { color: var(--line-strong); }
.live { color: var(--accent); }
.clock-cells { list-style: none; display: grid; grid-template-columns: repeat(auto-fill, minmax(104px, 1fr)); gap: 6px; padding: 0; margin: 0; }
.clock-cell { background: var(--panel-2); padding: 8px 10px; display: grid; gap: 4px; }
.clock-kind { color: var(--text); }
.clock-bar { height: 3px; background: var(--line); }
/* base.css drops this transition when Windows asks for reduced motion, so the bars jump instead. */
.clock-bar span { display: block; height: 100%; background: var(--accent); transition: width 0.4s ease; }
.clocks-paused .clock-cells { opacity: 0.45; }
.modules { width: 100%; border-collapse: collapse; font-size: 12px; }
.modules th { text-align: left; font-weight: 400; font-size: 11px; letter-spacing: 0.08em; color: var(--muted); padding: 0 10px 4px 0; }
.modules td { padding: 6px 10px 6px 0; border-top: 1px solid var(--line); }
.volumes { list-style: none; display: grid; gap: 6px; padding: 0; margin: 0; font-size: 12.5px; }
.volumes li { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding-top: 6px; border-top: 1px solid var(--line); }
```

In `app/src/main.tsx`, replace

```ts
import './styles/bench.css';
```

with

```ts
import './styles/bench.css';
import './styles/hardware.css';
```

In `app/src/styles/base.css`, replace

```css
.label { font-size: 11px; letter-spacing: 0.14em; color: var(--muted); }
```

with

```css
.label { font-size: 11px; letter-spacing: 0.14em; color: var(--muted); }
.panel-head { display: flex; align-items: baseline; gap: 8px; }
.panel-head > :last-child:not(:first-child) { margin-left: auto; }
```

**3p.** In `app/src/components/TitleBar.tsx`, replace

```ts
export type Tab = 'home' | 'health' | 'tweaks' | 'lockIn' | 'benchmark' | 'toolbox';
const TABS: Tab[] = ['home', 'health', 'tweaks', 'lockIn', 'benchmark', 'toolbox'];
```

with

```ts
export type Tab = 'home' | 'health' | 'hardware' | 'tweaks' | 'lockIn' | 'benchmark' | 'toolbox';
export const TABS: Tab[] = ['home', 'health', 'hardware', 'tweaks', 'lockIn', 'benchmark', 'toolbox'];
```

**3q.** In `app/src/App.tsx`, replace

```tsx
import { ComingSoon } from './pages/ComingSoon';
import { Health } from './pages/Health';
```

with

```tsx
import { ComingSoon } from './pages/ComingSoon';
import { Hardware } from './pages/Hardware';
import { Health } from './pages/Health';
```

replace

```tsx
  const openGuide = (topic: BiosTopic) => {
    setGuideTopic(topic);
    setTab('toolbox');
  };

  const chooseTab = (next: Tab) => {
    if (firstRun) return;
    if (next !== 'toolbox') setGuideTopic(null);
    setTab(next);
  };
```

with

```tsx
  // A benchmark saved since the last look changes the report (its PCIe link under load), so each tab change fetches it
  // again, quietly: no loading screen, and a scan under way or a failed one is left as it is.
  const refresh = () => {
    void getReport().then(
      (report) => setScan((current) => (current.state === 'ready' ? { state: 'ready', report } : current)),
      () => undefined,
    );
  };

  const openGuide = (topic: BiosTopic) => {
    refresh();
    setGuideTopic(topic);
    setTab('toolbox');
  };

  const chooseTab = (next: Tab) => {
    if (firstRun) return;
    if (next !== 'toolbox') setGuideTopic(null);
    if (next !== tab) refresh();
    setTab(next);
  };
```

and replace

```tsx
          {report && tab === 'health' && <Health report={report} onGuide={openGuide} />}
```

with

```tsx
          {report && tab === 'health' && <Health report={report} onGuide={openGuide} />}
          {report && tab === 'hardware' && <Hardware report={report} onGuide={openGuide} onRescan={() => void run(true)} />}
```

(`chooseTab` already returns early during the first run, so the refresh starts after it. Inside Tauri, `get_report` returns the cached scan with the newest link under load applied (Task 14), so Health and Hardware catch up with a benchmark saved meanwhile.)

**3r.** In `app/src/pages/Home.tsx`, replace

```tsx
        <HardwarePanel snapshot={report.snapshot} />
```

with

```tsx
        <HardwarePanel snapshot={report.snapshot} onTab={onTab} />
```

In `app/src/components/HardwarePanel.tsx`, replace

```tsx
import type { SystemSnapshot } from '../api/types';
import { strings } from '../strings';

export function HardwarePanel({ snapshot: s }: { snapshot: SystemSnapshot }) {
```

with

```tsx
import type { SystemSnapshot } from '../api/types';
import { strings } from '../strings';
import type { Tab } from './TitleBar';

export function HardwarePanel({ snapshot: s, onTab }: { snapshot: SystemSnapshot; onTab: (tab: Tab) => void }) {
```

and replace

```tsx
      <div className="label">{strings.home.hardware}</div>
```

with

```tsx
      <div className="panel-head">
        <span className="label">{strings.home.hardware}</span>
        <button className="link" onClick={() => onTab('hardware')}>{strings.home.seeHardware}</button>
      </div>
```

- [ ] **Step 4: Verify**

Run: `cd app && npm test`
Expected: PASS — `Test Files  10 passed (10)`, `Tests  58 passed (58)`.

Run: `cd app && npx tsc --noEmit`
Expected: no output, exit 0.

Run: `cd app && npm run build`
Expected: `✓ built in …`, no errors.

Manual check in the browser preview, on the schema-2 sample (`src/api/mock-report.json`) Task 15 regenerated:

1. `cd app && npm run dev` and open http://localhost:1420.
2. If the first-run screen shows, click **Scan my PC**, then **Later**. The tabs read Home · Health · Hardware · Tweaks · Lock In · Benchmark · Toolbox.
3. Home: the Hardware panel has **See all** on its right; it opens the Hardware tab.
4. With the window at least 1100 px wide: the head reads "Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO", then "Windows 11 Pro 25H2 · build 26200", "Last scan" with the sample's date and time, **Scan again** and **Copy specs**. Panels in two columns: Processor | Live clocks, Graphics | Memory, Motherboard and BIOS | Displays (XL2566K, Primary), Displays (24G4) | Storage (Samsung SSD 980 PRO 1TB, `C:` with a VALORANT tag), Storage (WDC) | Network (Power saving On, amber).
5. Processor: Rocket Lake-S, LGA1200, 8 cores · 16 threads, 3.50 / 5.30 GHz, the four caches; chips SSE4.2, AVX, AVX2, AVX-512 lit, AVX-VNNI and AMX dimmed.
6. Live clocks: C0–C7 change every second under a lime "1 Hz". Click another window: the cells dim and the badge says Paused; click back: they move again. In DevTools → Rendering, emulate `prefers-reduced-motion: reduce`: the bars jump instead of sliding.
7. Graphics: "Card 4.0 x16 · Slot 4.0 x16", Now 1.0 x16 with "The card slows its link when idle.", Under load "Run a benchmark to check the link under load.", Resizable BAR On in lime, and the dim line "Virtual adapters: Parsec Virtual Display Adapter".
8. Motherboard and BIOS: BIOS date 2024-05-20 in amber with **Open BIOS guide** (opens Toolbox at "Update the BIOS"); in "BIOS settings Kestrel can see", IOMMU (VT-d) Off with "Only for Vanguard On-Demand" and **Open BIOS guide** (Toolbox scrolls to the IOMMU topic); the other rows On / UEFI in lime.
9. **Copy specs** shows "Copied to clipboard" for 3 s; pasted, it's the seven lines, the GPU line ending "PCIe x16" (the sample has no benchmark link).
10. Narrow the window below 1100 px: one column. Switch tabs a few times: "Scanning your PC…" never flashes. The DevTools console shows no warnings.
11. Stop the dev server.

- [ ] **Step 5: Commit**

```bash
git add app/src/components/hardware app/src/pages/Hardware.tsx app/src/components/TitleBar.tsx app/src/components/TitleBar.test.ts app/src/App.tsx app/src/pages/Home.tsx app/src/components/HardwarePanel.tsx app/src/strings.ts app/src/main.tsx app/src/styles/base.css app/src/styles/hardware.css
git commit -m "feat(app): Hardware tab with live clocks, BIOS settings and Copy specs; See all on Home" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 17: PCIe link in benchmark results, stage numbers from one constant, docs, and a test of the sample

**Files:**
- Modify: `app/src/api/bench.ts` (`contextRows`, moved out of `ResultDetail` so the new line is unit-tested), `app/src/components/bench/ResultDetail.tsx`, `app/src/styles/bench.css` (wider label column)
- Modify: `app/src/strings.ts` (`STAGE`; every stage mention uses it; the PCIe line's label)
- Modify (comments only): `app/src/components/LockInCard.tsx`, `app/src/pages/Benchmark.tsx`, `app/src/api/client.ts`, `app/src/api/types.ts`, `app/src-tauri/src/bench/mod.rs`, `app/src-tauri/src/bench/controller.rs` (`StartMode`), `crates/kestrel-bench/src/profile.rs`
- Modify (docs): `README.md`, `docs/superpowers/specs/2026-10-09-kestrel-design.md`, `docs/superpowers/specs/2026-10-12-kestrel-stage-2-benchmark-design.md`
- Test: `app/src/api/bench.test.ts` (one test), `app/src/strings.test.ts`, `app/src/api/sample-report.test.ts` (new)

**Interfaces:**
- Consumes: Task 15's `BenchContext.gpuLink: PcieLink | null` and `linkLabel(link: PcieLink): string`, `specsText(report: Report): string`, `summaryLine(s: SystemSnapshot): string`, `SystemSnapshot.memory.ratedMts`; Task 8's `TWEAKS_STAGE = 5` / `COMPETITIVE_STAGE = 6` in `health.rs` (they reach the sample through `Fix::LaterStage`); the schema-2 `app/src/api/mock-report.json` Task 15 regenerated (`npm run mock`: the owner fixture with the 31 checks); Task 13/14's benchmark context (`gpuLink` in every saved benchmark, `null` in ones saved by v0.2.0).
- Produces: `export const STAGE = { tweaks: 5, competitive: 6, lockIn: 7, fixes: 8 } as const` (`strings.ts`); `contextRows(ctx: BenchContext): [string, string][]` (`api/bench.ts`); `strings.bench.contextRows.pcieLink = 'PCIe link under load'`; `app/src/api/sample-report.test.ts`, which pins what the sample says.

Background for this task:
- Decision 66 reordered the stages: tweak engine 5, Competitive tier 6, Lock In 7, fixes and the troubleshooter 8. Task 8 moved the Rust side to `TWEAKS_STAGE` / `COMPETITIVE_STAGE`; this task gives the UI its one constant. A comment that only labels code with the stage it was built in ("Stage 2: the benchmark") now names the feature instead, so no comment can go stale again.
- The browser preview's sample is printed by the Rust mock probe from `crates/kestrel-core/fixtures/owner-pc.json`; Task 15 regenerated it, and this task only tests it. The probe never fills `linkUnderLoad` (the app does, from benchmarks), so the sample's GPU line in Copy specs ends "PCIe x16".

- [ ] **Step 1: Write the failing tests**

In `app/src/api/bench.test.ts`, replace

```ts
import { canStartRun, flagText, graphShape, metricText, passLabel, pct, sessionHeadline, settingsText, spreadText, verdictTone } from './bench';
import type { BenchState, ValorantSettings } from './types';
```

with

```ts
import { canStartRun, contextRows, flagText, graphShape, metricText, passLabel, pct, sessionHeadline, settingsText, spreadText, verdictTone } from './bench';
import type { BenchContext, BenchState, ValorantSettings } from './types';
```

and replace

```ts
  it('signs percentages and colours verdicts', () => {
```

with

```ts
  it('lists the PC at the time, with the PCIe link measured under load', () => {
    const ctx: BenchContext = {
      powerPlan: 'EXM Free Power Plan V8', ramMts: 3600, gpu: 'NVIDIA GeForce RTX 3060 Ti', gpuDriver: '610.74', windowsBuild: 26200,
      displayHz: 360, gameSettings: null, gpuPnpDeviceId: 'PCI\\VEN_10DE&DEV_2489', gpuLink: { gen: 4, width: 16 },
    };
    expect(contextRows(ctx)).toEqual([
      ['Power plan', 'EXM Free Power Plan V8'], ['RAM', '3600 MT/s'], ['Graphics', 'NVIDIA GeForce RTX 3060 Ti · 610.74'],
      ['PCIe link under load', '4.0 x16'], ['Windows build', '26200'], ['Monitor', '360 Hz'], ['VALORANT', 'Settings not found'],
    ]);
    // Saved by v0.2.0, or Windows didn't report the link during the runs.
    expect(contextRows({ ...ctx, gpuPnpDeviceId: null, gpuLink: null })[3]).toEqual(['PCIe link under load', 'Not reported']);
  });

  it('signs percentages and colours verdicts', () => {
```

Create `app/src/strings.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { STAGE, strings } from './strings';

describe('stage numbers', () => {
  it('follow the stage order of decision 66', () => {
    expect(STAGE).toEqual({ tweaks: 5, competitive: 6, lockIn: 7, fixes: 8 });
    expect(strings.home.lockInSoon).toBe('Arrives in stage 7');
    expect(strings.toolbox.later).toBe('Fixes and the VALORANT troubleshooter arrive in stage 8.');
    expect(strings.comingSoon.tweaks.stage).toBe(5);
    expect(strings.comingSoon.lockIn.stage).toBe(7);
  });

  it('are only ever one of STAGE’s', () => {
    const named = [...JSON.stringify(strings).matchAll(/stage (\d+)/gi)].map((m) => Number(m[1]));
    expect(named.length).toBeGreaterThan(0);
    for (const n of named) expect(Object.values(STAGE)).toContain(n);
  });
});
```

Create `app/src/api/sample-report.test.ts` (it pins what the sample Task 15 regenerated must say, end to end through the Task 15 helpers):

```ts
import { describe, expect, it } from 'vitest';
import { STAGE } from '../strings';
import { summaryLine } from './hardware';
import mock from './mock-report.json';
import { specsText } from './specs';
import type { Report } from './types';

// The browser preview's sample, printed by `npm run mock` from the owner's fixture (crates/kestrel-core/fixtures/owner-pc.json).
const sample = mock as unknown as Report;

describe('the sample report', () => {
  it('is schema 2 with all 31 checks, its later-stage fixes in the new stage order', () => {
    expect(sample.snapshot.schemaVersion).toBe(2);
    expect(sample.findings).toHaveLength(31);
    const stage = (id: string) => {
      const fix = sample.findings.find((f) => f.id === id)?.fix;
      return fix?.kind === 'later-stage' ? fix.stage : null;
    };
    expect(stage('power-plan')).toBe(STAGE.tweaks);
    expect(stage('adapter-power')).toBe(STAGE.competitive);
  });

  it('fills the Hardware tab and Copy specs from what the probe prints', () => {
    expect(summaryLine(sample.snapshot)).toBe('Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO');
    // The probe prints the kit's rating from the part numbers; the Memory panel reads it from there.
    expect(sample.snapshot.memory.ratedMts).toBe(3600);
    // Only the app fills the link under load (from benchmarks), never the probe, so the GPU line gives the lanes in use.
    expect(specsText(sample)).toBe([
      'CPU: Intel Core i9-11900K (8 cores, 16 threads)',
      'GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe x16',
      'RAM: 32 GB DDR4-3600 · dual channel · 2 × 16 GB Corsair CMG32GX4M2D3600C18',
      'Board: ASUS ROG MAXIMUS XIII HERO · BIOS 2103 (2024-05-20)',
      'Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz',
      'Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX-00AYRA0 (HDD)',
      'OS: Windows 11 Pro 25H2 (build 26200)',
    ].join('\n'));
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd app && npx vitest run src/api/bench.test.ts src/strings.test.ts src/api/sample-report.test.ts`

Expected: FAIL, 4 tests in 3 files:
- `bench.test.ts > … with the PCIe link measured under load` — `TypeError: contextRows is not a function`
- `strings.test.ts > stage numbers > follow the stage order of decision 66` — `AssertionError: expected undefined to deeply equal { tweaks: 5, competitive: 6, …(2) }`
- `strings.test.ts > stage numbers > are only ever one of STAGE’s` — `TypeError: Cannot convert undefined or null to object`
- `sample-report.test.ts > the sample report > is schema 2 …` — `TypeError: Cannot read properties of undefined (reading 'tweaks')` (`STAGE` isn't exported yet; the schema and the 31 checks already hold on the sample Task 15 regenerated)

`sample-report.test.ts > the sample report > fills the Hardware tab …` already passes: it pins the sample Task 15 committed.

- [ ] **Step 3: Implement**

**3a. The PCIe line in a benchmark result.** In `app/src/api/bench.ts`, replace

```ts
import { strings } from '../strings';
import type { BenchState, Flag, MetricId, RunMetrics, ValorantSettings, Verdict } from './types';
```

with

```ts
import { strings } from '../strings';
import { linkLabel } from './hardware';
import type { BenchContext, BenchState, Flag, MetricId, RunMetrics, ValorantSettings, Verdict } from './types';
```

and replace

```ts
/** VALORANT's recorded settings in one line: "1920×1080 · V-Sync off · No FPS limit". */
```

with

```ts
/** A result's "PC at the time" lines. The PCIe link is the highest the graphics card reached during the runs. */
export function contextRows(ctx: BenchContext): [string, string][] {
  const r = strings.bench.contextRows;
  return [
    [r.powerPlan, ctx.powerPlan ?? '—'],
    [r.ram, ctx.ramMts ? `${ctx.ramMts} MT/s` : '—'],
    [r.gpu, ctx.gpu ? `${ctx.gpu}${ctx.gpuDriver ? ` · ${ctx.gpuDriver}` : ''}` : '—'],
    [r.pcieLink, ctx.gpuLink ? linkLabel(ctx.gpuLink) : strings.bench.notReported],
    [r.windows, ctx.windowsBuild ? String(ctx.windowsBuild) : '—'],
    [r.display, ctx.displayHz ? `${ctx.displayHz} Hz` : '—'],
    [r.game, settingsText(ctx.gameSettings)],
  ];
}

/** VALORANT's recorded settings in one line: "1920×1080 · V-Sync off · No FPS limit". */
```

In `app/src/components/bench/ResultDetail.tsx`, replace

```tsx
import { dateText, flagText, METRIC_ORDER, metricText, metricValue, pct, settingsText, spreadText, verdictText, verdictTone } from '../../api/bench';
```

with

```tsx
import { contextRows, dateText, flagText, METRIC_ORDER, metricText, metricValue, pct, spreadText, verdictText, verdictTone } from '../../api/bench';
```

delete these lines (the rows now come from `contextRows`):

```tsx
  const ctx = bench.context;
  const contextRows: [string, string][] = [
    [b.contextRows.powerPlan, ctx.powerPlan ?? '—'],
    [b.contextRows.ram, ctx.ramMts ? `${ctx.ramMts} MT/s` : '—'],
    [b.contextRows.gpu, ctx.gpu ? `${ctx.gpu}${ctx.gpuDriver ? ` · ${ctx.gpuDriver}` : ''}` : '—'],
    [b.contextRows.windows, ctx.windowsBuild ? String(ctx.windowsBuild) : '—'],
    [b.contextRows.display, ctx.displayHz ? `${ctx.displayHz} Hz` : '—'],
    [b.contextRows.game, settingsText(ctx.gameSettings)],
  ];

```

and replace

```tsx
          {contextRows.map(([name, value]) => (
```

with

```tsx
          {contextRows(bench.context).map(([name, value]) => (
```

In `app/src/styles/bench.css`, replace

```css
.detail .hw { grid-template-columns: 110px minmax(0, 1fr); }
```

with

```css
.detail .hw { grid-template-columns: 160px minmax(0, 1fr); }
```

(the 20-character "PCIe link under load" needs about 145 px in the mono font).

**3b. One constant for stage numbers.** In `app/src/strings.ts`, replace

```ts
/** Every user-facing string lives here (spec decision 17). Sentence case, no exclamation marks. */
export const strings = {
```

with

```ts
/** The stage each later feature arrives in (decision 66). kestrel-core's health.rs keeps TWEAKS_STAGE and COMPETITIVE_STAGE. */
export const STAGE = { tweaks: 5, competitive: 6, lockIn: 7, fixes: 8 } as const;

/** Every user-facing string lives here (spec decision 17). Sentence case, no exclamation marks. */
export const strings = {
```

then make these four replacements in the same file:

```ts
    lockInSoon: 'Arrives in stage 5',
```
→
```ts
    lockInSoon: `Arrives in stage ${STAGE.lockIn}`,
```

```ts
    later: 'Fixes and the VALORANT troubleshooter arrive in stage 6.',
```
→
```ts
    later: `Fixes and the VALORANT troubleshooter arrive in stage ${STAGE.fixes}.`,
```

```ts
    tweaks: { stage: 3, title: 'Tweaks', body: 'Reversible tweaks picked for your hardware, with Preview and one-click undo.' },
    lockIn: { stage: 5, title: 'Lock In', body: 'A tiny helper that boosts your PC while VALORANT runs and undoes everything when you quit.' },
```
→
```ts
    tweaks: { stage: STAGE.tweaks, title: 'Tweaks', body: 'Reversible tweaks picked for your hardware, with Preview and one-click undo.' },
    lockIn: { stage: STAGE.lockIn, title: 'Lock In', body: 'A tiny helper that boosts your PC while VALORANT runs and undoes everything when you quit.' },
```

```ts
    contextRows: { powerPlan: 'Power plan', ram: 'RAM', gpu: 'Graphics', windows: 'Windows build', display: 'Monitor', game: 'VALORANT' },
```
→
```ts
    contextRows: {
      powerPlan: 'Power plan', ram: 'RAM', gpu: 'Graphics', pcieLink: 'PCIe link under load', windows: 'Windows build', display: 'Monitor',
      game: 'VALORANT',
    },
```

The two ComingSoon pages (Tweaks, Lock In) take their stage from `strings.comingSoon.*`, so `ComingSoon.tsx` itself doesn't change.

**3c. Comments that name stages.** Run `grep -rniI "stage" app/src app/src-tauri/src crates --exclude=mock-report.json`. Besides the `strings.ts` lines changed in 3b, change these hits, each a comment that labels code with a stage number:

`app/src/components/LockInCard.tsx`, line 3:

```tsx
/** Placeholder until stage 5: shows where Lock In will live, switched off. */
```
→
```tsx
/** Placeholder until Lock In arrives (STAGE.lockIn in strings.ts): shows where it will live, switched off. */
```

`app/src/pages/Benchmark.tsx`, line 10:

```tsx
/** Stage 2: start a benchmark, follow the session, and look through saved results (newest first). */
```
→
```tsx
/** The Benchmark tab: start a benchmark, follow the session, and look through saved results (newest first). */
```

`app/src/api/client.ts`:

```ts
// Stage 2: the benchmark. In the browser preview these use sample data and a fake session.
```
→
```ts
// The benchmark. In the browser preview these use sample data and a fake session.
```

`app/src/api/types.ts`:

```ts
// Stage 2: the benchmark (Rust: crates/kestrel-bench and app/src-tauri/src/bench).
```
→
```ts
// The benchmark (Rust: crates/kestrel-bench and app/src-tauri/src/bench).
```

(`app/src-tauri/src/lib.rs` needs nothing: Task 14 rewrote its first line without a stage number.)

`app/src-tauri/src/bench/mod.rs`, line 1:

```rust
//! Stage 2's benchmark: capturing frames, pacing runs, and the commands the UI calls.
```
→
```rust
//! The benchmark: capturing frames, pacing runs (sampling the graphics card's PCIe link meanwhile), and the commands
//! the UI calls.
```

`crates/kestrel-bench/src/profile.rs`, line 1:

```rust
//! Game profiles (decision 55). Stage 2 has one: VALORANT with Route v1.
```
→
```rust
//! Game profiles (decision 55). VALORANT with Route v1 is the only one so far; FiveM, Rainbow Six Siege and Warzone
//! follow (decision 56).
```

Leave the other hits as they are: `home.stage(n)`, `health.laterStage(n)` and `comingSoon.arrives(n)` in `strings.ts` (formatters for a number that comes from data); `FindingRow.tsx`, `Health.tsx` and `ComingSoon.tsx` (they render `fix.stage` or the `stage` prop); `types.ts`'s `{ kind: 'later-stage'; stage: number }`; `owner-report.ts`'s `stage: 6` (test data: the value Task 8 gives adapter power saving); `strings.test.ts` and `sample-report.test.ts`; and `crates/kestrel-core/src/health.rs` (Task 8 already moved its numbers to `TWEAKS_STAGE` / `COMPETITIVE_STAGE`). If an earlier stage-3 task added another comment that labels code with a stage number, give it the same treatment: name the feature instead.

**3d. `StartMode` docs match the shipped behaviour.** `start_countdown` is offered in either mode and `canStartRun` (UI) shows Start run whenever a pass waits with the game running. In `app/src-tauri/src/bench/controller.rs`, replace

```rust
pub enum StartMode {
    /// The start key works.
    Hotkey,
    /// Another app holds the key: the UI's "Start run" button starts a 10-second countdown instead.
    Countdown,
}
```

with

```rust
pub enum StartMode {
    /// Kestrel holds the start key. The UI still offers "Start run" (a 10-second countdown) whenever a pass waits with
    /// the game running, because a game can swallow the key.
    Hotkey,
    /// Another app holds the start key, so "Start run" is the only way to start a pass.
    Countdown,
}
```

In `docs/superpowers/specs/2026-10-12-kestrel-stage-2-benchmark-design.md` ("Errors" table), replace

```markdown
| F10 can't be registered | Fallback: **Start run** in Kestrel, then a 10-second beep countdown |
```

with

```markdown
| F10 can't be registered, or the game swallows it | **Start run** in Kestrel, offered whenever a pass waits with the game running (in both cases), then a 10-second beep countdown |
```

**3e. Docs in the new stage order.** In `docs/superpowers/specs/2026-10-09-kestrel-design.md`, replace

```markdown
## Tweak list (stages 3–4)
```

with

```markdown
## Tweak list (stages 5–6)
```

In `README.md`, replace

```markdown
## Status

Stage 2 (v0.2.0): the benchmark. A warm-up and 3 runs on a fixed route in VALORANT's Range measure average FPS,
1% and 0.1% lows, stutters and PC latency, and compare every later result with your starting point. Kestrel
changes nothing on your PC; it only saves benchmark results in its own folder.

Stage 1 (v0.1.0): hardware detection, Health Check and BIOS guide.
```

with

```markdown
## Status

Stage 3 (v0.3.0): the Hardware tab. Every part of the PC that Windows reports without a driver, each core's clock
live, and Copy specs for a short summary to paste anywhere. Four new Health Check items look at the graphics card's
PCIe link, Resizable BAR and virtualization, with BIOS-guide steps for your board, and benchmarks record the PCIe
link under load. Kestrel still changes nothing on your PC.

Stage 2 (v0.2.0): the benchmark. A warm-up and 3 runs on a fixed route in VALORANT's Range measure average FPS,
1% and 0.1% lows, stutters and PC latency, and compare every later result with your starting point. Kestrel
changes nothing on your PC; it only saves benchmark results in its own folder.

Stage 1 (v0.1.0): hardware detection, Health Check and BIOS guide.

Next, in this order:

- Stage 4: FiveM, Rainbow Six Siege and Warzone
- Stage 5: the tweak engine, with reversible tweaks picked for your PC
- Stage 6: the Competitive and Extreme tiers
- Stage 7: Lock In
- Stage 8: fixes and the troubleshooter
- Stage 9: the redesign, as v1.0
```

- [ ] **Step 4: Verify**

Run: `cd app && npm test`
Expected: PASS — `Test Files  12 passed (12)`, `Tests  63 passed (63)`.

Run: `cd app && npx tsc --noEmit` and `cd app && npm run build`
Expected: no errors; `✓ built in …`.

Run: `cargo fmt --all --check` and `cargo test -p kestrel-bench --locked`
Expected: no formatting changes; all kestrel-bench tests pass (the Rust edits are doc comments).

Run: `grep -rniI "stage [0-9]" app/src --exclude=mock-report.json --exclude="*.test.ts"`
Expected: no output (exit 1): every stage number the UI shows now comes from `STAGE` or from a finding's fix.

Manual check: `cd app && npm run dev`, open http://localhost:1420:
1. Benchmark tab, open a result: "PC at the time" lists "PCIe link under load 4.0 x16" (the sample benchmarks carry the link) on one line beside the other rows.
2. Home: the Lock In card says "Arrives in stage 7"; the top fixes show "Stage 5" (power plan) and "Stage 6" (network power saving).
3. Tweaks tab "Arrives in stage 5", Lock In tab "Arrives in stage 7", the Toolbox's last line "Fixes and the VALORANT troubleshooter arrive in stage 8."
4. Health: the four new checks under Hardware and BIOS and Vanguard readiness; Toolbox: the two new topics.
5. Hardware tab: the page from Task 16 on the sample Task 15 committed.

- [ ] **Step 5: Commit**

```bash
git add app/src/api/bench.ts app/src/api/bench.test.ts app/src/components/bench/ResultDetail.tsx app/src/styles/bench.css app/src/strings.ts app/src/strings.test.ts app/src/api/sample-report.test.ts app/src/components/LockInCard.tsx app/src/pages/Benchmark.tsx app/src/api/client.ts app/src/api/types.ts app/src-tauri/src/bench/mod.rs app/src-tauri/src/bench/controller.rs crates/kestrel-bench/src/profile.rs README.md docs/superpowers/specs/2026-10-09-kestrel-design.md docs/superpowers/specs/2026-10-12-kestrel-stage-2-benchmark-design.md
git commit -m "feat(app): PCIe link under load in results; stage numbers from one constant; a test of the sample report; docs follow" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 18: CI job for the app crate on macOS; version 0.3.0

**Files:**
- Modify: `.github/workflows/ci.yml` (a new `app-macos` job)
- Modify: `Cargo.toml` (workspace version), `crates/kestrel-probe/Cargo.toml` (its `kestrel-core` dependency's `version`, the only path dependency that carries one), `Cargo.lock` (through `cargo update -w`), `app/src-tauri/tauri.conf.json`, `app/package.json` and `app/package-lock.json` (through `npm version`), `app/src/api/mock-report.json` (`appVersion`, which the browser preview's About shows; stage 2 bumped it with the version too)

**Interfaces:**
- Consumes: Tasks 1–17 committed and green.
- Produces: an `App tests (macOS)` check on every pull request (`cargo test -p kestrel` and its clippy); version 0.3.0 everywhere, so `CARGO_PKG_VERSION` is "0.3.0" (the report's `appVersion`, every new benchmark's `kestrelVersion`) and the Release workflow names the installer `Kestrel_0.3.0_x64-setup.exe` (Task 19).

- [ ] **Step 1: Write the failing checks**

There is no unit test for a version or a workflow; these commands are the checks. From the repo root:
```bash
PATH="$HOME/.cargo/bin:$PATH" cargo metadata --format-version 1 --no-deps --locked | jq -r '.packages[] | "\(.name) \(.version)"' | sort
grep -n -A1 -E '^name = "kestrel(-core|-probe|-bench)?"$' Cargo.lock
grep -n '0\.2\.0' Cargo.toml crates/*/Cargo.toml app/src-tauri/Cargo.toml app/src-tauri/tauri.conf.json app/package.json app/src/api/mock-report.json
node -p 'const l = require("./app/package-lock.json"); l.version + " " + l.packages[""].version'
ruby -ryaml -e 'puts YAML.load_file(ARGV[0])["jobs"].keys.inspect' .github/workflows/ci.yml
```

- [ ] **Step 2: Run them to verify they fail**

Expected now: `kestrel 0.2.0`, `kestrel-bench 0.2.0`, `kestrel-core 0.2.0`, `kestrel-probe 0.2.0`; the four `Cargo.lock` entries at `version = "0.2.0"`; the grep prints `Cargo.toml:8:version = "0.2.0"`, `crates/kestrel-probe/Cargo.toml:9:kestrel-core = { version = "0.2.0", path = "../kestrel-core" }`, `app/src-tauri/tauri.conf.json:5:  "version": "0.2.0",`, `app/package.json:4:  "version": "0.2.0",` and `app/src/api/mock-report.json:2:  "appVersion": "0.2.0",` (line numbers may have moved); node prints `0.2.0 0.2.0`; the jobs are `["core", "ui", "windows"]`.

- [ ] **Step 3: Implement**

`.github/workflows/ci.yml` — add the job between `ui` and `windows`, i.e. replace

```yaml
      - run: npm ci
      - run: npm test
      - run: npx tsc --noEmit

  windows:
```

with

```yaml
      - run: npm ci
      - run: npm test
      - run: npx tsc --noEmit

  app-macos:
    name: App tests (macOS)
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - uses: actions/setup-node@v7
        with:
          node-version: lts/*
          cache: npm
          cache-dependency-path: app/package-lock.json
      - run: npm ci
        working-directory: app
      # The app crate embeds app/dist, so the UI is built first. Its tests use fakes, not Windows code, so they run here.
      - run: npm run build
        working-directory: app
      - run: cargo test -p kestrel --locked
      - run: cargo clippy -p kestrel --locked --all-targets -- -D warnings

  windows:
```

(The same actions and versions as the `core`, `ui` and `windows` jobs; the cargo steps run from the repo root like theirs.)

Version 0.3.0:

- `Cargo.toml`, under `[workspace.package]`: `version = "0.2.0"` → `version = "0.3.0"`.
- `crates/kestrel-probe/Cargo.toml`: `kestrel-core = { version = "0.2.0", path = "../kestrel-core" }` → `kestrel-core = { version = "0.3.0", path = "../kestrel-core" }`. (`grep -n 'path = ' crates/*/Cargo.toml app/src-tauri/Cargo.toml` shows the other path dependencies, the app's three, have no `version`.)
- `app/src-tauri/tauri.conf.json`: `"version": "0.2.0",` → `"version": "0.3.0",`.
- `app/src/api/mock-report.json`: `"appVersion": "0.2.0",` → `"appVersion": "0.3.0",`.
- Run `cd app && npm version 0.3.0 --no-git-tag-version && cd ..` — expected: prints `v0.3.0`; `app/package.json` and both own-version fields of `app/package-lock.json` (top level and `packages[""]`) say 0.3.0.
- Run `PATH="$HOME/.cargo/bin:$PATH" cargo update -w` — expected: `Updating kestrel v0.2.0 (…) -> v0.3.0` for `kestrel`, `kestrel-bench`, `kestrel-core` and `kestrel-probe`, and nothing else.

- [ ] **Step 4: Verify**

Re-run Step 1's checks. Expected: `kestrel 0.3.0`, `kestrel-bench 0.3.0`, `kestrel-core 0.3.0`, `kestrel-probe 0.3.0`; the four `Cargo.lock` entries at `version = "0.3.0"`; the grep prints nothing (exit code 1); node prints `0.3.0 0.3.0`; the jobs are `["core", "ui", "app-macos", "windows"]`. `git diff --stat` lists only `.github/workflows/ci.yml`, `Cargo.toml`, `Cargo.lock` (`8 ++++----`), `crates/kestrel-probe/Cargo.toml`, `app/src-tauri/tauri.conf.json`, `app/package.json`, `app/package-lock.json` (`4 ++--`) and `app/src/api/mock-report.json`. Test data that names the version a benchmark was saved with stays `"0.2.0"` on purpose (`bench/controller.rs` and `kestrel-bench`'s `text.rs`/`store.rs` tests, `app/src/api/sample-bench.ts`).

Then the whole local suite, as CI runs it:
```bash
PATH="$HOME/.cargo/bin:$PATH" cargo fmt --all --check
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel-core -p kestrel-probe -p kestrel-bench --locked --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel-core -p kestrel-probe -p kestrel-bench --locked
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel-probe -p kestrel-bench --target x86_64-pc-windows-msvc --locked --all-targets -- -D warnings
(cd app && npm ci && npm test && npx tsc --noEmit && npm run build)
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel --locked && PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel --locked --all-targets -- -D warnings
```
Expected: all clean (`--locked` passes, so `Cargo.lock` is in step with the manifests).

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(app): run the app crate's tests and clippy on macOS" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git add Cargo.toml Cargo.lock crates/kestrel-probe/Cargo.toml app/src-tauri/tauri.conf.json app/package.json app/package-lock.json app/src/api/mock-report.json
git commit -m "chore(release): version 0.3.0" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 19: Publish v0.3.0, open the pull request, hand it to the owner

**Files:** none in the repo. GitHub: the `stage-3` branch, a pull request, the `v0.3.0` tag and pre-release. The owner's tracker doc.

**Interfaces:**
- Consumes: Tasks 1–18 committed on `stage-3` (version 0.3.0, the `app-macos` job); `.github/workflows/release.yml` as it is (a `v*` tag builds the NSIS installer with tauri-action and publishes it as a pre-release, not a draft); the tracker doc https://claude.ai/artifact/G53gWshuJMv9FeQpzwEaNf (doc id `7a09b517-430a-482d-a0b1-05395c826cd4`, one tab, body node `c691f7ec-8686`).
- Produces: the `v0.3.0` pre-release with `Kestrel_0.3.0_x64-setup.exe`; an open PR `stage-3` → `main` titled "Stage 3: Hardware & BIOS", bound to this session, auto-merge off; the tracker's stage 3 row at "Ready to test" with the release link, a new "Now:" line and a "What to test in stage 3" checklist; one message to the owner in Portuguese.

Owner decisions 72C/73A: the test build is published as soon as it is ready and CI is green, without asking. Merging stays the owner's call: never merge the PR, never enable auto-merge. `ci.yml` runs on pull requests and on pushes to `main` only, so pushing `stage-3` starts no CI run; opening the PR is what starts it, which is why the PR is opened before the CI check and the tag.

- [ ] **Step 1: Final local checks**

From the repo root:
```bash
git status --short
git rev-parse --abbrev-ref HEAD
PATH="$HOME/.cargo/bin:$PATH" cargo fmt --all --check
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel-core -p kestrel-probe -p kestrel-bench --locked --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel-core -p kestrel-probe -p kestrel-bench --locked
PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel-probe -p kestrel-bench --target x86_64-pc-windows-msvc --locked --all-targets -- -D warnings
(cd app && npm test && npx tsc --noEmit && npm run build)
PATH="$HOME/.cargo/bin:$PATH" cargo test -p kestrel --locked && PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p kestrel --locked --all-targets -- -D warnings
grep -n '"version"' app/src-tauri/tauri.conf.json
```
Expected: `git status` prints nothing, the branch is `stage-3`, every check is clean, and `tauri.conf.json` says `"version": "0.3.0",`. Anything red is fixed (test first) and committed before going on.

- [ ] **Step 2: Push the branch**

```bash
git push -u origin stage-3
git rev-parse HEAD
git ls-remote origin refs/heads/stage-3
```
Expected: the push succeeds and both SHAs are the same. That SHA is `<HEAD>` below.

- [ ] **Step 3: Open the pull request and bind it**

```bash
gh pr create --base main --head stage-3 --title "Stage 3: Hardware & BIOS" --body "$(cat <<'EOF'
Stage 3 of Kestrel: Hardware & BIOS (decisions 51–54 and 72–78). Plan: docs/superpowers/plans/2026-10-13-kestrel-stage-3-hardware.md. Spec: docs/superpowers/specs/2026-10-13-kestrel-stage-3-hardware-design.md.

- **Hardware tab**, between Health and Tweaks: every part Windows reports without a kernel driver. Processor (codename, socket, cores with the P/E split, caches, instruction sets, live per-core clocks that pause when Kestrel isn't in front), graphics cards with their PCIe link and Resizable BAR, memory per module, board, chipset, BIOS and the BIOS settings Kestrel can see, displays, every drive and the network adapters. Copy specs copies a short summary.
- **Health Check:** four new checks (PCIe lanes, PCIe speed under load, Resizable BAR, virtualization), 31 in all, and two BIOS-guide topics (PCIe slot, virtualization).
- **Benchmark:** the graphics card's PCIe link is read every 2 s during the warm-up and runs; the result and Copy results show it, and the PCIe speed check uses the newest benchmark on the same card.
- **Also:** stage numbers follow the new stage order; the stage-1 fixes in this area (RAM channels and XMP readings, placeholder board names, virtual displays, BIOS-guide wording); a macOS CI job runs the app crate's tests; version 0.3.0.

The new reads are all user mode (CPUID, processor topology, PDH clocks, Configuration Manager device properties, display config, WMI). Kestrel still changes nothing on the PC: it only saves benchmark results in its own folder. The v0.3.0 test build is published from this branch once CI is green; merge after the owner's test passes.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
Expected: prints the PR's URL (`https://github.com/gabryelvs/kestrel/pull/<n>`).

Then, with the session's PR tools (load them first: ToolSearch `select:mcp__ccd_pr__get_status,mcp__ccd_pr__bind_pr`):
1. Call `mcp__ccd_pr__get_status` with no arguments.
2. If it shows no PR, or a different one, call `mcp__ccd_pr__bind_pr` with `{"url": "<the PR URL>"}`, then `mcp__ccd_pr__get_status` once more.

Expected: the PR is open, head `stage-3`, base `main`, auto-merge off. Never call `mcp__ccd_pr__set_auto_merge`, and leave the monitor switches as they are.

- [ ] **Step 4: Check CI on `<HEAD>` (once; no polling loop)**

```bash
gh run list --branch stage-3 --limit 1 --json databaseId,status,conclusion,headSha
```
(`databaseId` is added to the fields so the run can be watched.) Expected: one run whose `headSha` is `<HEAD>`.
- `"status":"completed"` and `"conclusion":"success"`: go on.
- Not finished yet: run `gh run watch <databaseId> --exit-status` once, as a background command (Bash with `run_in_background: true`), and wait for its notification. Exit code 0: go on.
- The list is empty or its `headSha` is older (GitHub hadn't queued the run yet): read `mcp__ccd_pr__get_status` once, then run the same `gh run list` once more and continue as above.

Then confirm all four jobs passed:
```bash
gh run view <databaseId> --json jobs --jq '.jobs[] | "\(.name): \(.conclusion)"'
```
Expected: `Core logic (Linux): success`, `UI tests: success`, `App tests (macOS): success`, `Windows build: success` (in any order). A failure stops the release: read `gh run view <databaseId> --log-failed`, fix it on `stage-3` (test first), commit, push (the PR's CI runs again on the new head) and repeat this step for the new `<HEAD>`.

- [ ] **Step 5: Tag and publish v0.3.0 (no need to ask: decision 73A)**

```bash
git fetch origin --tags
git tag -l v0.3.0
git ls-remote --tags origin v0.3.0
git tag -a v0.3.0 -m "Kestrel v0.3.0 — stage 3 test build (Hardware & BIOS)" HEAD
git push origin v0.3.0
gh run list --workflow Release --branch v0.3.0 --limit 1 --json databaseId,status,conclusion,headSha
```
Expected: the two tag lookups print nothing (no such tag yet); the push creates the tag; the Release run's `headSha` is `<HEAD>` (if it isn't listed yet, run the `gh run list` once more). Watch it with one background `gh run watch <databaseId> --exit-status` (it builds the installer, about 10–20 minutes); expected exit code 0. Then:
```bash
gh release view v0.3.0 --json isPrerelease,isDraft,assets,url --jq '{pre: .isPrerelease, draft: .isDraft, files: [.assets[].name], url: .url}'
```
Expected: `{"draft":false,"files":["Kestrel_0.3.0_x64-setup.exe"],"pre":true,"url":"https://github.com/gabryelvs/kestrel/releases/tag/v0.3.0"}`. If the Release run fails, read `gh run view <databaseId> --log-failed`; when `gh release view v0.3.0` shows no release, fix the cause on `stage-3`, get Step 4 green again, then move the tag (`git tag -d v0.3.0 && git push origin :refs/tags/v0.3.0`) and repeat this step; if a broken release does exist, stop and tell the owner instead of deleting it.

- [ ] **Step 6: Update the owner's tracker**

Use the docs connector (load the `anthropic-skills:docs` skill, or call the connector's `guide` with `["topic.index", "topic.editing"]`, before the first docs call). Never web-fetch the doc.

1. Read the outline (container `{"kind":"project","id":"7a09b517-430a-482d-a0b1-05395c826cd4"}`): `read(ref={"object":"node","id":"c691f7ec-8686"}, engine="prose", container=…, payload={"projection":"outline"})`, then `payload={"kind":"view","parentId":"<the Build plan table's id>"}` and `payload={"kind":"view","parentId":"<the Now paragraph's id>"}`. Note the read's `rev` and each block's `h`. When this plan was written (rev 46) they were: Build plan table `mr1kt2gempj.27190`; stage 3's row `mr1kt2gempj.27538`, its "What you get" paragraph `mr1kt2gempj.27543` (`h` `5e4c4fe8`) and its Status paragraph `mr1kt2gempj.32546` (`h` `03646fec`, a dropdown on enum `3d0483a0-17df`: index 0 Not started, 1 In progress, 2 Ready to test, 3 Done); the "Now:" paragraph `mr1kt2gempj.25779` (`h` `69096d25`); the stage-2 checklist `mr1kt2gempj.30978`, then its G-SYNC note `mr1kt2gempj.31968`, then the `Results log` heading `mr1kt2gempj.6628`. Use the ids, hashes and rev your read shows.
2. Pick the "Now:" text. If stage 2's Status still says "Ready to test" (the owner hasn't finished v0.2.0):
   `**Now:** stages 2 and 3 are ready to test. First stage 2 (benchmark): install the [v0.2.0 test build](https://github.com/gabryelvs/kestrel/releases/tag/v0.2.0) and work through "What to test in stage 2". Then stage 3 (Hardware & BIOS): install the [v0.3.0 test build](https://github.com/gabryelvs/kestrel/releases/tag/v0.3.0) over it and work through "What to test in stage 3". Both lists are under How testing works.`
   If stage 2 is "Done":
   `**Now:** stage 3 (Hardware & BIOS) is ready to test: install the [v0.3.0 test build](https://github.com/gabryelvs/kestrel/releases/tag/v0.3.0) and work through "What to test in stage 3" under How testing works.`
   Then add the sentences after the current paragraph's "ready to test" sentence as they read now (today: `Your Claude Design handoff (Redesign v4) is in and gets built in the last stage. Stage 1 ([v0.1.0](https://github.com/gabryelvs/kestrel/releases/tag/v0.1.0)) is done: 97/100 on your PC.`), re-spelling their links.
3. One `update(ref={"object":"node","id":"c691f7ec-8686"}, engine="prose", container=…, payload={"ops":[…]})` with these four ops (ids, `ifHash` and `ifRev` from step 1's read):
   ```json
   {"op":"replace","target":{"kind":"blocks","ids":["mr1kt2gempj.27543"]},"ifHash":"5e4c4fe8","ifRev":46,
    "with":{"as":"markdown","from":{"kind":"inline","content":"Hardware & BIOS: CPU-Z-style tab, PCIe link, Resizable BAR and virtualization checks ([v0.3.0 test build](https://github.com/gabryelvs/kestrel/releases/tag/v0.3.0))"}}}
   ```
   ```json
   {"op":"replace","target":{"kind":"blocks","ids":["mr1kt2gempj.32546"]},"ifHash":"03646fec","ifRev":46,
    "with":{"as":"markdown","blocks":{"st":{"type":"dropdown","enum":"3d0483a0-17df","index":2}},"from":{"kind":"inline","content":"<?claude block st?>"}}}
   ```
   ```json
   {"op":"replace","target":{"kind":"blocks","ids":["mr1kt2gempj.25779"]},"ifHash":"69096d25","ifRev":46,
    "with":{"as":"markdown","from":{"kind":"inline","content":"<the Now: text from step 2>"}}}
   ```
   ```json
   {"op":"insert","target":{"kind":"blocks","ids":["mr1kt2gempj.6628"]},"side":"before",
    "source":{"as":"markdown","from":{"kind":"inline","content":"### What to test in stage 3\n\n- [ ] Install the [v0.3.0 test build](https://github.com/gabryelvs/kestrel/releases/tag/v0.3.0) over v0.2.0 (if Windows warns, More info → Run anyway); the Hardware tab sits between Health and Tweaks\n- [ ] Processor: i9-11900K, Rocket Lake-S, LGA1200, 8 cores / 16 threads, caches 48 KB, 32 KB, 512 KB and 16 MB, AVX-512 lit\n- [ ] Live clocks move on 8 cores and pause when another window is in front\n- [ ] Graphics: RTX 3060 Ti, 8 GB, current driver, card and slot PCIe 4.0 x16; Resizable BAR matches NVIDIA Control Panel → System Information\n- [ ] Memory: both modules in the right slots, 3600 MT/s, kit rated 3600\n- [ ] Board: ROG MAXIMUS XIII HERO, Z590, BIOS version and date as in the BIOS, UEFI; Virtualization matches Task Manager → Performance → CPU\n- [ ] Both monitors with their names, at 360 Hz and 180 Hz\n- [ ] Every drive with the right type and free space; the network adapter at its link speed\n- [ ] Health Check shows the four new checks\n- [ ] Run a benchmark: the result shows \"PCIe link under load: 4.0 x16\", and the PCIe check passes\n- [ ] Copy specs and Copy diagnostics, then paste both in chat\n- [ ] Nothing outside Kestrel's folder changed"}}}
   ```
   A refused op changes nothing: follow its `code` (`guide(["refusal.<code>"])`); on `guard_mismatch` the owner edited that block, so redo it from what stands now and keep their words.
4. Check: `read(…, payload={"kind":"view","sinceRev":<the rev from step 1>})` shows the stage 3 row at "Ready to test" with the link, the new "Now:" line, and the 12-item checklist between the stage-2 note and `Results log`.

- [ ] **Step 7: Tell the owner**

One short message, in Portuguese, with the links; put the PR's URL from Step 3 where the text says `<URL do PR>`. If stage 2 is already "Done" in the tracker, leave out step 1 and renumber.

```text
O stage 3 (Hardware & BIOS) está pronto para testar: https://github.com/gabryelvs/kestrel/releases/tag/v0.3.0

1. Se ainda não terminou o teste do v0.2.0 (benchmark), faça primeiro "What to test in stage 2" no tracker.
2. Nesse link, baixe Kestrel_0.3.0_x64-setup.exe e instale por cima da versão atual. Se o Windows avisar, clique em "Mais informações" (More info) e depois em "Executar assim mesmo" (Run anyway).
3. Abra o tracker (https://claude.ai/artifact/G53gWshuJMv9FeQpzwEaNf) e siga "What to test in stage 3", em How testing works, marcando cada item.
4. No fim, clique em Copy specs e em Copy diagnostics e cole os dois aqui no chat.

O Kestrel continua sem mudar nada no seu PC: ele só lê o hardware e guarda os benchmarks na pasta dele. O PR do stage 3 (<URL do PR>) fica aberto: o merge é com você, depois do teste.
```
