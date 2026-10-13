# Kestrel — design spec

Agreed with the owner (gabryelvs) on 2026-10-09 through a grilling session. The live tracker is
https://claude.ai/artifact/G53gWshuJMv9FeQpzwEaNf. This file is the in-repo copy.

## What it is

Kestrel is a free Windows FPS optimiser, shared with friends, built so it can go public later.
VALORANT comes first; FiveM, Rainbow Six Siege and Call of Duty: Warzone follow as game profiles
(decisions 55, 56, 62). It detects the PC's hardware, finds misconfigurations (Health Check), applies
reversible tweaks picked for that hardware (Smart Plan), runs a while-you-play booster (Lock In),
and proves gains with a benchmark. Kestrel is not affiliated with Riot Games, Cfx.re/Rockstar
Games, Ubisoft or Activision.

## Product decisions

| # | Topic | Decision |
| --- | --- | --- |
| 1 | Audience | Owner + friends, free; can go public later |
| 2 | Games | VALORANT first; then FiveM, Rainbow Six Siege and Warzone as game profiles (revised by 55, 56, 62) |
| 3 | Tweak tiers | Essential (default), Competitive (opt-in), Extreme (opt-in, warning each); each tweak explained, impact-labelled, reversible |
| 3 | Never does | Read/write any game's memory or inject; load its own kernel driver; touch Vanguard (vgc/vgk) or any other anti-cheat; modify game files; advise turning off TPM or Secure Boot; permanently disable Defender, UAC or Windows Update; on a PC with FiveM, stop or disable Windows services or clean traces (decision 61) |
| 4 | Detection | Info + Health Check + Smart Plan (explains why each tweak was picked) |
| 5 | Apply model | Permanent tweaks + opt-in Lock In booster |
| 6 | Measuring | Before/after benchmark with PresentMon (ETW, passive) |
| 8 | Owner's role | Hands-off: working .exe + simple instructions |
| 9 | Stack | Tauri 2 (Rust + web UI); Lock In is a separate tiny helper exe |
| 10 | Distribution | Public GitHub repo github.com/gabryelvs/kestrel, GPL-3.0, GitHub Releases, auto-update (asks before installing), no telemetry, "Copy diagnostics" button |
| 11 | Install | NSIS installer, per-machine; requireAdministrator (one UAC prompt at launch); Lock In starts at sign-in via scheduled task; main window never auto-starts |
| 12 | GPU driver | Automatic NVIDIA profile for VALORANT only; guide for AMD/Intel |
| 13 | Debloat | Light clean-up + optional app removal from a list; never Store, Xbox/Game Bar, Edge/WebView2 |
| 14 | Network | Diagnostics + adapter tweaks in Competitive |
| 15 | Safety net | Undo journal per change, Revert everything, restore point, crash recovery |
| 16 | Extras | Fixes toolbox, read-only VALORANT troubleshooter, BIOS guide |
| 17 | Language | English, all strings translation-ready |
| 18 | Optimise for | Smoothness (1% lows) and low input delay first; average FPS second |
| 19 | Delivery | Six test stages, safest first, each a GitHub pre-release |
| 20 | Look | Tactical: near-black #0C0E11, one acid-lime accent #C8F53C, chamfered panels, monospace numerals, tabs along the top |
| 21 | Home | Status first: score, top fixes with one-click buttons, hardware, Lock In status, small live-load graph, last benchmark; help link to GitHub; no account/greeting |
| 22 | First run | Guided: scan → findings → benchmark first → proposed plan with Preview → restore point → apply or "Not now" |
| 23 | Name | Kestrel |
| 24 | Logo | "Hover": a kestrel hovering, seen head-on, acid lime |
| 25 | Booster | Lock In |
| 26 | Booster visibility | Tray icon: grey waiting, lime active; right-click to pause/open; no pop-ups |
| 27 | Names | Kestrel.exe; repo github.com/gabryelvs/kestrel |
| 28 | Windows | Windows 11 fully supported; Windows 10 22H2 runs with a warning banner, Win11-only tweaks hidden, untested |
| 29 | Memory Integrity (VBS) | Extreme tier, manual only, warns about Riot account restrictions and Vanguard On-Demand mode |
| 33 | Benchmark | Fixed 90-second routine in the Range |
| 34 | VALORANT settings | Guide + read-only check of the local settings file; never written |
| 35 | FPS cap | Uncapped, NVIDIA Reflex On + Boost |

### Decisions of 2026-10-10 to 2026-10-12

| # | Topic | Decision |
| --- | --- | --- |
| 36–50 | Benchmark | See `2026-10-12-kestrel-stage-2-benchmark-design.md` (PresentMon 2.6.0, Range Route v1, warm-up + 3 runs, starting point, real-or-noise comparisons) |
| 51 | Hardware & BIOS | Its own stage, right after the benchmark |
| 52 | Hardware tab | CPU-Z-style tab showing everything readable without a kernel driver: CPU (codename, cores, caches, live per-core clock), board and BIOS, memory per module, GPU with PCIe link. No timings, voltages or CPU temperature (they need a driver) |
| 53 | BIOS checks | New Health Check items: GPU below its PCIe link, Resizable BAR off (where Windows reports it), virtualization off; each with a BIOS-guide section for the owner's board |
| 54 | CPU score | No CPU-Z-style CPU benchmark |
| 55 | Multi-game | Everything from stage 2 on is built on game profiles; VALORANT stays the first complete game |
| 56 | Game order | VALORANT → FiveM → Rainbow Six Siege → Call of Duty |
| 57 | FiveM goals | Low FPS, stutter while moving around the city, crashes |
| 58 | Game settings writes | Allowed for FiveM/GTA V (graphics settings and FiveM's own config) with automatic backup and undo, game closed only; VALORANT stays read-only; R6 and Warzone decided in their stage |
| 59 | Other benchmarks | The game's built-in benchmark where it has one; FiveM gets a 90-second free capture, comparable only on the same server |
| 60 | Stage 2 design | Approved |
| 61 | Telagem safety | FiveM RP staff inspect players' PCs ("telagem") and treat stopped services, cleaned traces or edited forensic registry keys as hiding cheats. When FiveM is installed, Kestrel never stops or disables a Windows service and never cleans traces, PC-wide; if FiveM appears after such tweaks, Kestrel warns and offers to undo them. Without FiveM, service tweaks stay opt-in. A "Kestrel report" lists every change for everyone |
| 62 | Call of Duty | Warzone |
| 63 | Animation | Animated UI that pauses while a game runs or Kestrel is in the background, respects reduced motion, and has an off switch |
| 64 | Redesign timing | The visual redesign is the last stage; the owner designs it in Claude Design (handoff: `docs/superpowers/notes/2026-10-12-claude-design-handoff.md`) |
| 65 | Home hero | Decided in the owner's Claude Design work (preview options: particle falcon, tactical radar, wind currents) |
| 66 | Stage order | As in the table below |
| 67 | Redesign handoff | Arrived 2026-10-12 (Claude Design "Kestrel Redesign v4"): icon rail, status bar, HUD look, a motion system that pauses while a game is in front; Home hero is a hex shield over a rotating radar sweep (settles 65). The redesign stays the last stage (64) |
| 68 | Benchmark in the redesign | Keeps the approved stage-2 flow (warm-up + 3 runs on Route v1, F10, beeps, review, starting point, real gain vs noise) in the design's visual language: an overlay per pass with ring and timer, a before→after chart, history. No live FPS during a pass |
| 69 | Multi-game copy | The redesign's copy is game-neutral ("FPS optimiser for your games", "While your game runs"); animations pause when any supported game (VALORANT, FiveM, Rainbow Six Siege, Warzone) is in the foreground, found from the foreground window's process id in a Toolhelp32 snapshot (no handle to the game) |
| 70 | Design data | Real data in the redesign stage: points per finding from `kestrel-core` ("What if you fix"), Kestrel's own CPU and RAM in the status bar, and status, note and menu path per BIOS-guide topic (menu paths only for boards Kestrel knows, ASUS first; hidden otherwise) |
| 71 | Handoff in the repo | When the redesign stage starts, the handoff is copied to `docs/design/redesign-v4/` (public in the repo) |
| 72 | Build cadence | Stages run as a queue: the next stage is built while the owner tests the last one. Each stage still ships as its own test build, and its fixes land before the owner merges it. Stages 6–8 start only after stage 5 passes the owner's test on the real PC |
| 73 | Test builds | Each stage's test build (a GitHub pre-release) is published as soon as it is ready and CI is green, without asking first. Merging stays the owner's call |
| 74 | Hardware tab scope | Processor, graphics, memory per module, board and BIOS (52), plus displays, every drive and network adapters |
| 75 | PCIe link check | Lanes are checked at any time; the PCIe generation is checked under load, sampled during benchmark runs and shown in the result (a card slows its link when idle) |
| 76 | BIOS check weights | Fewer lanes than the card supports: Medium (x4 or less: High). Generation below what card and slot support, under load: Low (1.0 or 2.0: High). Resizable BAR off: High on Intel Arc, Low on AMD, Info on NVIDIA. Virtualization off: Info |
| 77 | Copy specs | A Copy specs button on the Hardware tab copies a short plain-text summary of the PC |
| 78 | Hardware screen design | The owner designs the Hardware screen in Claude Design from Kestrel's brief (`docs/superpowers/notes/2026-10-13-hardware-tab-brief.md`); stage 3 builds the tab in the current look and stage 9 builds the owner's design |

## Stages

| Stage | Contents |
| --- | --- |
| 1 | App shell, hardware detection, Health Check, BIOS guide, Copy diagnostics. Read-only. (v0.1.0, done) |
| 2 | Benchmark, built on game profiles, VALORANT first |
| 3 | Hardware & BIOS: Hardware tab and BIOS checks (51–53) |
| 4 | Game profiles: FiveM, Rainbow Six Siege, Warzone (detection, checks, benchmarks); telagem safety (61) |
| 5 | Tweak engine, undo journal, restore points, Essential tier, Smart Plan, Kestrel report |
| 6 | Competitive + Extreme tiers, NVIDIA profiles per game, network, debloat, FiveM/GTA V settings (58) |
| 7 | Lock In |
| 8 | Fixes, troubleshooters per game, auto-update, installer polish |
| 9 | Visual redesign from the owner's Claude Design work (63–65) → v1.0 |

## Health Check (all read-only)

- Hardware and BIOS: RAM below rated speed; single-channel RAM; BIOS older than 2025-12-01 (Riot has asked some players to update)
- Display: monitor below its top refresh rate; active virtual displays (e.g. Parsec)
- GPU: driver older than ~6 months; VALORANT on integrated graphics on hybrid PCs; laptop on battery
- Windows: power plan; Game Mode; background recording; pending restart; Windows build vs Vanguard minimums
- Storage: VALORANT on a hard drive; nearly full drive
- Background: apps using CPU at idle; number of startup apps
- Vanguard readiness: TPM 2.0, Secure Boot, Memory Integrity, IOMMU, Vanguard installed, On-Demand eligibility
- Network: Wi-Fi instead of cable; link speed; adapter power saving; Delivery Optimization internet uploads

## Tweak list (stages 3–4)

Essential: Kestrel power plan (High performance base, no core parking, USB/PCIe power saving off;
laptops only on charger); VALORANT on "High performance" GPU; Game Mode on; Game DVR background
recording off (Game Bar stays); Win11 "Optimizations for windowed games" on; NVIDIA VALORANT profile
(max-performance power, high-performance texture filtering, 10 GB shader cache, V-Sync off);
"Enhance pointer precision" off.

Competitive: VALORANT High CPU priority via IFEO PerfOptions (skipped under 6 cores); adapter power
saving / EEE / interrupt moderation off; Delivery Optimization internet uploads off; QoS DSCP 46 for
VALORANT; trim unused services (Fax, Remote Registry, Print Spooler without printer…; opt-in, never on
a PC with FiveM — decision 61); HAGS and fullscreen-optimizations toggles decided by the benchmark.

Extreme: Memory Integrity/VBS off (decision 29); 0.5 ms timer resolution during Lock In (unproven).

Left out on purpose: Defender/UAC/Windows Update off; Spectre/Meltdown mitigations off; GPU MSI-mode
edits; Nagle/TcpAckFrequency/TCP autotuning; HPET/dynamic-tick boot tweaks; clearing shader caches;
removing Xbox Game Bar; third-party timer or memory-cleaner drivers.

## Lock In (stage 7)

Kestrel power plan; pause Windows Search indexing and Windows Update downloads (not on a PC with
FiveM — decision 61); close user-chosen
apps (none by default); purge standby memory when free RAM is low on PCs with ≤16 GB; 0.5 ms timer
resolution only if that Extreme tweak is on; never touches VALORANT or Vanguard processes; everything
reverted on game exit or at next start-up after a crash. The redesign mock-up's "Raise VALORANT
priority to High" row is not built: changing a game's priority needs a handle to the game process.

## Known limits

- No CPU temperature, memory timings or voltages (they need a kernel driver; Vanguard blocks those).
  GPU temperature on NVIDIA via NVML.
- RAM rated speed comes from the module part number; unusual kits may be missed (reported as Unknown).
- Windows 10 untested. Unsigned builds show SmartScreen on first install.

## Platform facts (researched 2026-10-09)

- VALORANT moved to Unreal Engine 5.3 in patch 11.02 (Jul 2025). Requires Win10 19041+ or Win11 (TPM 2.0 + Secure Boot).
- VAN:RESTRICTION can demand TPM, Secure Boot, Memory Integrity, IOMMU, or Windows build 19045/26100 — for flagged accounts only.
- Vanguard On-Demand mode (Jun 2026) needs Win11 25H2+, Secure Boot, TPM 2.0, VBS, HVCI, IOMMU.
- Vanguard blocks known vulnerable drivers (e.g. WinRing0). No Riot statements or ban reports about IFEO priority, ETW FPS capture, GPU preference or fullscreen optimizations.

## Owner's PC (test machine)

Desktop · i9-11900K (8C/16T) · RTX 3060 Ti · 32 GB DDR4 Corsair CMG32GX4M2D3600C18 (now 3600 MT/s Gear 1) ·
ASUS ROG Maximus XIII Hero · 360 Hz monitor · Windows 11 Pro 25H2 (26200) · Parsec installed · ~320 FPS in Deathmatch.
