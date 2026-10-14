# Kestrel stage 3 — Hardware & BIOS design

Agreed with the owner on 2026-10-13 (decisions 51–54 and 72–78, see the main spec
`2026-10-09-kestrel-design.md`). Stage 3 ships as the v0.3.0 test pre-release.

## Goal

Show every part of the PC that Windows reports without a kernel driver on a new **Hardware** tab, with
each core's clock live. Add four BIOS-related checks to the Health Check, each pointing at a
BIOS-guide topic for the owner's board, and measure the graphics card's PCIe link under load during
benchmark runs. Along the way, fix the app's stale stage numbers and the hardware-area items left
from stage 1.

## Decisions this stage implements

| # | Decision |
| --- | --- |
| 51 | Hardware & BIOS is its own stage, right after the benchmark |
| 52 | A CPU-Z-style tab with everything readable without a kernel driver: CPU (codename, cores, caches, live per-core clock), board and BIOS, memory per module, GPU with PCIe link. No timings, voltages or CPU temperature |
| 53 | New Health Check items for the GPU's PCIe link, Resizable BAR and virtualization, each with a BIOS-guide section for the owner's board |
| 54 | No CPU benchmark score |
| 72 | Stages run as a queue; stage 3 is built while the owner tests v0.2.0 |
| 73 | v0.3.0 is published as soon as it is ready and CI is green |
| 74 | The tab also lists displays, every drive and the network adapters |
| 75 | PCIe lanes are checked at any time; the PCIe generation is checked under load, during benchmark runs |
| 76 | Weights of the new checks (see "Health Check") |
| 77 | A Copy specs button copies a short plain-text summary |
| 78 | The owner designs the Hardware screen in Claude Design (brief: `docs/superpowers/notes/2026-10-13-hardware-tab-brief.md`); stage 3 builds the tab in the current look, stage 9 builds the owner's design |

## What the owner sees

### Hardware tab

The tabs become Home · Health · **Hardware** · Tweaks · Lock In · Benchmark · Toolbox. The page uses
the current look: a scrolling page of panels, two columns at 1180 px, one column below 1100 px. Labels
follow the brief. Every value comes from the latest scan, except the live clocks. A value Windows
doesn't report shows **Not reported**; nothing is guessed.

- **Top:** a one-line summary (`Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII
  HERO`), the Windows edition, version and build, "Last scan" with the time and **Scan again**, and
  **Copy specs**.
- **Processor:** name, codename, socket; cores and threads (`8 P-cores + 16 E-cores · 32 threads` on
  hybrid CPUs); base and max boost clock; L1 data, L1 instruction, L2 and L3 (count × size);
  instruction chips SSE4.2, AVX, AVX2, AVX-512, AVX-VNNI and AMX (missing ones dimmed).
- **Live clocks:** one cell per physical core with its clock in GHz (two decimals), P or E on hybrid
  CPUs, plus Average and Highest. The page asks for new clocks once a second only while the Hardware
  tab is shown and Kestrel's window is visible and focused; otherwise the cells dim and say
  **Paused** (a game in front means Kestrel isn't focused). With reduced motion on, values change
  without animation.
- **Graphics:** one panel per graphics card (other adapters — virtual ones such as Parsec, USB docks, cards without a driver — on one dim line at the
  end): name, VRAM, driver version and date; PCIe link: what the card and the slot support
  (`Card 4.0 x16 · Slot 4.0 x16`), the link now (with "The card slows its link when idle.") and under
  load (`Under load 4.0 x16 · from your benchmark on 13 Oct`, or "Run a benchmark to check the link
  under load."); Resizable BAR On / Off / Not reported.
- **Memory:** total, DDR generation, channel layout, slots used of total, speed now, kit rating from
  the part number ("Rating unknown" when not recognised); one row per module: slot, size, maker,
  part number, speed, rank. A note: "Timings and voltages need a driver, which Kestrel never loads."
- **Motherboard and BIOS:** maker, model, chipset; BIOS vendor, version and date; **BIOS settings
  Kestrel can see:** XMP (from the RAM check), Resizable BAR, Virtualization, IOMMU, Secure Boot,
  TPM 2.0, Boot mode. Each item shows its state; one that is off or wrong links to its BIOS-guide
  topic.
- **Displays:** one panel per monitor: its name, resolution, refresh rate now and maximum, primary,
  connection (DisplayPort, HDMI…) and the graphics card it's connected to.
- **Storage:** one panel per drive: model, type (NVMe, SSD, HDD), bus, size; each volume with its
  letter and free space; tags for the games installed there (VALORANT in stage 3).
- **Network:** one panel per adapter: name, Ethernet or Wi-Fi, link speed, connected, power saving.
- A panel whose probe failed shows "Kestrel couldn't read everything here. Use Copy diagnostics on
  Home to share the details." Before the first scan the page asks for a scan.

Home's Hardware panel gets a **See all** link to the tab.

### Copy specs

Copies plain text, one line per part, leaving out a line Kestrel can't fill, then shows "Copied to
clipboard" for 3 s:

```
CPU: Intel Core i9-11900K (8 cores, 16 threads)
GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe 4.0 x16
RAM: 32 GB DDR4-3600 · dual channel · 2 × 16 GB Corsair CMG32GX4M2D3600C18
Board: ASUS ROG MAXIMUS XIII HERO · BIOS 2103 (2024-05-20)
Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz
Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX-00AYRA0 (HDD)
OS: Windows 11 Pro 25H2 (build 26200)
```

The GPU line names the main graphics card; its PCIe part is the link measured under load, or just
the lane count (`PCIe x16`) before any benchmark. Modules that differ are listed one by one.

### Health Check: four new checks (decision 76)

Only a Problem lowers the score (High −15, Medium −8, Low −3), as today. The three graphics checks
look at the main graphics card (the existing `main_gpu`); without a separate card they say "No
separate graphics card to check" (Info).

| Check (group) | Pass | Problem or Info | Weight |
| --- | --- | --- | --- |
| `pcie-lanes` (Hardware and BIOS) | Graphics card uses all its PCIe lanes | Graphics card runs at x8 instead of x16 (the lane counts are the real ones) | Medium; High at x4 or fewer |
| `pcie-speed` (Hardware and BIOS) | PCIe link runs at full speed under load | PCIe link runs at 3.0 under load instead of 4.0 | Low; High at 1.0 or 2.0. Before a benchmark: Info, "Run a benchmark to check the PCIe link under load" |
| `resizable-bar` (Hardware and BIOS) | Resizable BAR is on | Resizable BAR is off | High on Intel Arc, Low on AMD (Smart Access Memory); Info on NVIDIA; Info without a fix on cards that don't use it |
| `virtualization` (Vanguard readiness) | Virtualization is on | Virtualization is off | Info |

- `pcie-lanes` compares the lanes in use with the lanes the card supports. Its text lists the usual
  causes: the wrong slot, a slot that shares lanes with an M.2 drive or second card, a card not
  pushed fully in; and says an external (Thunderbolt) enclosure limits a card to x4 by design.
- On a laptop, a lane shortfall is Info with no fix: the laptop's design sets the lanes. Its PCIe BIOS-guide steps skip the slot and M.2 checks.
- `pcie-speed` compares the highest generation seen during a benchmark with the lower of what the
  card and the slot support. It uses the latest benchmark measured on the same card.
- `resizable-bar` treats NVIDIA RTX 30 and newer, AMD RX 6000 and newer and Intel Arc as cards that
  use it. NVIDIA's text says NVIDIA only uses it in games on its own list, so most games, VALORANT
  included, gain little and turning it on is optional.
- `virtualization` says VALORANT works fine without it; Windows needs it for Memory Integrity, which
  Riot can require on flagged accounts; it's called Intel Virtualization Technology (VT-x) or SVM on
  AMD.
- When Windows doesn't report a value the check says so (Unknown), never Pass.

The Health Check grows from 27 to 31 checks.

### BIOS guide

Two new topics, with steps and menu paths for ASUS, MSI, Gigabyte and ASRock boards (Intel and AMD),
checked against each vendor's current BIOS manual; other boards get the generic text:

- **Run the graphics card at full PCIe speed** (`pcie-slot`): the top long slot, pushed fully in; the
  slot's PCIe speed set to Auto; an M.2 drive that shares the slot's lanes moved to another M.2 slot.
- **Turn on virtualization (only if you need it)** (`virtualization`): Intel Virtualization
  Technology or SVM Mode set to Enabled.

`pcie-lanes` and `pcie-speed` point at `pcie-slot`, `resizable-bar` at the existing Resizable BAR
topic, `virtualization` at the new one. Topic order: XMP, memory gear, BIOS update, PCIe slot,
Resizable BAR, virtualization, IOMMU.

### Benchmark

During the warm-up and the runs, Kestrel reads the main graphics card's PCIe link every 2 s and keeps
the highest generation seen, with its lane count. The result shows "PCIe link under load: 4.0 x16"
(or "not reported"), Copy results gets the same line, and the Health Check updates as soon as the
benchmark is saved.

## How each value is read

Everything is a user-mode read: no driver, no handle to any game, nothing written.

| Value | Source |
| --- | --- |
| CPU vendor, family, model, stepping, brand, instruction sets, hypervisor present; Intel base and max clock | The `CPUID` instruction (leaves 0, 1, 7 with sub-leaves 0 and 1, 0x16, 0x80000002–4). AVX, AVX-512 and AMX count only when the OS saves their registers (`XGETBV`) |
| Codename and socket | A table in `kestrel-core` keyed by vendor, family and model: Intel Core 6th gen to Core Ultra 200, AMD Zen to Zen 5, desktop and mobile, its sources cited above it and each entry pinned by a test with a real chip's signature. Unknown models: Not reported |
| Cores, threads, P/E split, caches | `GetLogicalProcessorInformationEx` (cores with their efficiency class; caches with level, type, size) |
| Base clock (when CPUID has none), live per-core clocks | PDH `\Processor Information(*)\Processor Frequency` × `% Processor Performance` / 100 per logical processor, through `PdhAddEnglishCounterW` (works in any Windows language), mapped to physical cores |
| Virtualization | On when a hypervisor is present (CPUID) or VBS runs; otherwise `IsProcessorFeaturePresent(PF_VIRT_FIRMWARE_ENABLED)` |
| BIOS vendor | `Win32_BIOS.Manufacturer` |
| Chipset | A chipset token in the board name (`Z590`, `B550`, `X670E`…), else in the description of the Intel LPC/eSPI controller (PCI class 06/01) |
| Boot mode | `GetFirmwareType` |
| Slots, rank | `Win32_PhysicalMemoryArray.MemoryDevices`; `Win32_PhysicalMemory.Attributes` |
| PCIe link | Configuration Manager device properties `DEVPKEY_PciDevice_CurrentLinkSpeed/Width` and `MaxLinkSpeed/Width` of the card's PCI device, found from `Win32_VideoController.PNPDeviceID`. Through a card's own internal PCIe switch (AMD Radeon) Kestrel walks up to the device under the root port. The slot's maximum is the root port's. Speed codes 1–6 are PCIe 1.0–6.0. For an NVIDIA card, the link now and under load come from NVML (the NVIDIA driver's own library, loaded from System32) when it reports exactly that one card, because the stored property may not follow the live link |
| Resizable BAR | The card's allocated memory ranges (`CM_Get_First_Log_Conf` with `ALLOC_LOG_CONF`, memory and large-memory descriptors): on when a range is larger than 256 MB, off when none is, Not reported when Windows lists none |
| Monitor names and connection | `QueryDisplayConfig` and `DisplayConfigGetDeviceInfo` (target and source names), joined to the existing display list by GDI name |
| Drives and volumes | `MSFT_PhysicalDisk` and `MSFT_Partition` (Windows storage WMI), free space per volume |

## Data

The scan snapshot moves to schema version 2. Every new field is optional or defaults to empty, so a
version-1 snapshot still loads. In short:

- `cpu`: codename, socket, family/model/stepping, base and max MHz, P-cores and E-cores, caches
  (level, kind, size, count), instruction sets, virtualization.
- `board`: BIOS vendor, chipset, boot mode.
- `memory`: slots in total; each module: rank.
- `gpus[]`: PCI instance id, PCIe link (card max, slot max, now: generation and lanes), link under
  load (generation, lanes, when, which benchmark — filled by the app from the latest benchmark, not
  by the probe), largest memory range and Resizable BAR.
- `displays[]`: monitor name, connection.
- `storage.drives[]`: model, type, bus, size, volumes (letter, free, total). The UI tags a volume with the games Kestrel knows are installed there (VALORANT's drive in stage 3).

The TypeScript snapshot type gains these fields plus the `security` and `network` parts the Hardware
tab shows. Benchmarks gain `context.gpuLink`. Copy diagnostics carries all of it unchanged.

## Architecture

- **`crates/kestrel-core`** (pure, tested on macOS): decoding `CPUID` registers, the codename table,
  cache and core grouping, the P/E split, clock maths, PCIe code decoding and the walk from card to
  slot over a list of devices, the Resizable BAR rule over memory ranges, the chipset token,
  placeholder names (`To be filled by O.E.M.`, `Default string`, `System Product Name`…), drive and
  volume grouping, the four checks and the two guide topics.
- **`crates/kestrel-probe`**: new Windows readers for `CPUID`, processor topology, Configuration
  Manager (PCI properties, parents, allocated resources, the LPC/eSPI controller), display config,
  firmware type and the new WMI queries; a persistent `ClockSampler` like the load sampler; one
  `gpu_link(instance_id)` read for the benchmark. Each failure becomes a `ProbeError` named after its
  panel, never a failed scan. `windows-sys` gains the Configuration Manager, display and threading
  features; the PCI property keys are declared locally.
- **`crates/kestrel-bench`**: `Context.gpu_link`; the Copy results line.
- **Tauri app**: a `get_cpu_clocks` command; the benchmark `Platform` gets `gpu_link()`, sampled by
  the controller every 2 s while a pass runs; after a scan and after each saved benchmark the cached
  report is rebuilt with the latest link under load.
- **UI**: the Hardware page and its panels, Copy specs (built in TypeScript from the report with the
  existing formatting helpers), the tab, Home's See all, the benchmark result line. New checks and
  guide topics appear in Health and Toolbox through the existing components.

## Also in stage 3

- **Stage numbers.** The app still uses the old order (decision 66 changed it). Every "stage N" in
  the app and the Health Check moves to the new order: power plan, Game Mode and background recording
  → 5; startup apps, adapter power saving and Delivery Optimization → 6; Lock In → 7; fixes and the
  troubleshooter → 8. The numbers live in one constant per language so they can't drift again. The
  sample report, the README and code comments follow, and the main spec's "Tweak list (stages 3–4)"
  heading becomes "(stages 5–6)".
- **Stage-1 items in this area** (`docs/superpowers/notes/2026-10-10-stage-1-backlog.md`): RAM
  locators such as `CPU1 DIMM A1` and `P0_ChannelA` read as dual channel when they are; no false XMP
  alarm when Windows reports the RAM clock in MHz (half the MT/s); placeholder board names never shown
  or matched; DisplayLink and Microsoft Basic Display adapters no longer counted as virtual displays;
  BIOS-guide wording (ASUS laptops say F2 consistently, "Advanced Mode" spelled one way, the update
  caution also mentions resetting Gear 1 on 11th-gen Intel). The rest of that list stays open.
- **Stage-2 loose ends:** the `StartMode` doc comments and the stage-2 spec's errors table match the
  shipped behaviour (Start run is offered whenever a session waits); a macOS CI job runs the app
  crate's tests. Behaviour changes to the benchmark wait for the owner's v0.2.0 test.

## Errors and edge cases

| Situation | What happens |
| --- | --- |
| A probe fails or times out | Its panel shows the notice; the rest of the page and the scan carry on |
| PCI properties missing (old Windows, a virtual machine, a laptop card switched off while idle) | PCIe shows Not reported; `pcie-lanes` is Unknown with the reason |
| AMD Radeon with an internal PCIe switch | The link is read at the device under the root port, not the card's own internal link |
| The slot's maximum can't be read | `pcie-speed` is Unknown, so a 4.0 card on a 3.0 board is never flagged |
| No benchmark yet, or no link read during it | `pcie-speed` is Info and asks for a benchmark |
| A benchmark measured on another card | It isn't used for this card |
| Hypervisor running (Hyper-V, VBS) | Virtualization is On (the firmware flag is hidden then) |
| CPU not in the codename table | Codename and socket show Not reported |
| More than 64 logical processors | Processor groups are mapped, so every core has a cell |
| PDH counters missing | Live clocks show Not reported; base clock falls back to CPUID or Not reported |
| Monitor without a name in its EDID | The panel shows "Display 1", "Display 2"… |
| USB or virtual drives | Listed with their bus (USB, Virtual) |

## Testing

- `kestrel-core` unit tests: `CPUID` decoding (extended family and model maths, brand string, each
  feature bit with and without OS support, hypervisor bit), every codename table entry plus unknown
  models, the owner's i9-11900K (family 6, model 0xA7 → Rocket Lake-S, LGA1200), caches and cores from
  synthetic topology (hybrid, AMD with two L3s, more than 64 threads), clock maths and PDH instance
  names, PCIe codes, the card-to-slot walk (NVIDIA direct, AMD through its switch, Thunderbolt), the
  Resizable BAR rule (256 MB, 8 GB, none), chipset tokens and placeholders, drive grouping, each new
  check in every state and weight, the guide's new topics per vendor, the new stage numbers. The owner
  fixture gains the new fields.
- `kestrel-probe`: pure decoders tested everywhere; the Windows smoke test checks the new fields read
  without a panic; CI's "Probe this runner" step prints them (a hosted runner has no real GPU, so
  PCIe shows Not reported there).
- App: the controller test samples links from the fake platform and saves the highest; the report
  rebuild uses the latest link for the same card; the macOS CI job runs these.
- UI: Copy specs text (full, missing lines, mixed modules, no benchmark), the hardware formatting
  helpers, the live-clock pause rule; the browser preview shows the sample PC.

## Rules for stage 3 (changes from stage 2 in bold)

- **Kestrel runs the `CPUID` instruction (the CPU's identification, readable in user mode) and reads
  device properties through Configuration Manager; still no driver.**
- **During benchmark passes it reads the graphics card's PCIe link (a device property, or NVML for an NVIDIA card), never
  anything from the game.**
- Unchanged: writes only inside its own app-data folder; runs the bundled PresentMon only during
  captures; registers F10 only while a session is armed; never reads or writes a game's memory, never
  injects, never loads a kernel driver, never touches Vanguard or any anti-cheat; no network calls at
  runtime except opening links; no telemetry; VALORANT's settings files are only read.

## What the owner tests (tracker checklist for v0.3.0)

- Install v0.3.0 over v0.2.0; the Hardware tab sits between Health and Tweaks
- Processor: i9-11900K, Rocket Lake-S, LGA1200, 8 cores / 16 threads, caches 48 KB, 32 KB, 512 KB
  and 16 MB, AVX-512 lit
- Live clocks move on 8 cores and pause when another window is in front
- Graphics: RTX 3060 Ti, 8 GB, current driver, card and slot PCIe 4.0 x16; Resizable BAR matches
  NVIDIA Control Panel → System Information
- Memory: both modules in the right slots, 3600 MT/s, kit rated 3600
- Board: ROG MAXIMUS XIII HERO, Z590, BIOS version and date as in the BIOS, UEFI; Virtualization
  matches Task Manager → Performance → CPU
- Both monitors with their names, at 360 Hz and 180 Hz
- Every drive with the right type and free space; the network adapter at its link speed
- Health Check shows the four new checks
- Run a benchmark: the result shows "PCIe link under load: 4.0 x16", and the PCIe check passes
- Copy specs and Copy diagnostics, then paste both in chat
- Nothing outside Kestrel's folder changed

## Not in stage 3

The owner's Hardware screen design and the rest of the redesign (stage 9), other games (stage 4),
tweaks (stages 5–6), Lock In (stage 7), NVMe and network chip details, Wi-Fi band and signal, X3D
cache labels per core, per-core load. CPU temperature, voltages, RAM timings, fans and power limits
never (decision 52); no CPU score (decision 54).
