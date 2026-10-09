# Kestrel — design spec

Agreed with the owner (gabryelvs) on 2026-10-09 through a grilling session. The live tracker is
https://claude.ai/artifact/G53gWshuJMv9FeQpzwEaNf. This file is the in-repo copy.

## What it is

Kestrel is a free Windows FPS optimiser for VALORANT, shared with friends, built so it can go
public later. It detects the PC's hardware, finds misconfigurations (Health Check), applies
reversible tweaks picked for that hardware (Smart Plan), runs a while-you-play booster (Lock In),
and proves gains with a benchmark. Kestrel is not affiliated with Riot Games.

## Product decisions

| # | Topic | Decision |
| --- | --- | --- |
| 1 | Audience | Owner + friends, free; can go public later |
| 2 | Games | VALORANT first; other games later as profiles |
| 3 | Tweak tiers | Essential (default), Competitive (opt-in), Extreme (opt-in, warning each); each tweak explained, impact-labelled, reversible |
| 3 | Never does | Read/write the game's memory or inject; load its own kernel driver; touch Vanguard (vgc/vgk); advise turning off TPM or Secure Boot; permanently disable Defender, UAC or Windows Update |
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

## Stages

| Stage | Contents |
| --- | --- |
| 1 | App shell, hardware detection, Health Check, BIOS guide, Copy diagnostics. Read-only. |
| 2 | Benchmark |
| 3 | Tweak engine, undo journal, restore points, Essential tier, Smart Plan |
| 4 | Competitive + Extreme tiers, NVIDIA profile, network, debloat |
| 5 | Lock In |
| 6 | Fixes, VALORANT troubleshooter, auto-update, installer polish → v1.0 |

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
VALORANT; trim unused services (Fax, Remote Registry, Print Spooler without printer…); HAGS and
fullscreen-optimizations toggles decided by the benchmark.

Extreme: Memory Integrity/VBS off (decision 29); 0.5 ms timer resolution during Lock In (unproven).

Left out on purpose: Defender/UAC/Windows Update off; Spectre/Meltdown mitigations off; GPU MSI-mode
edits; Nagle/TcpAckFrequency/TCP autotuning; HPET/dynamic-tick boot tweaks; clearing shader caches;
removing Xbox Game Bar; third-party timer or memory-cleaner drivers.

## Lock In (stage 5)

Kestrel power plan; pause Windows Search indexing and Windows Update downloads; close user-chosen
apps (none by default); purge standby memory when free RAM is low on PCs with ≤16 GB; 0.5 ms timer
resolution only if that Extreme tweak is on; never touches VALORANT or Vanguard processes; everything
reverted on game exit or at next start-up after a crash.

## Known limits

- No CPU temperature (needs a kernel driver; Vanguard blocks those). GPU temperature on NVIDIA via NVML.
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
