# Kestrel stage 5: Essential tweak research

Date 2026-11-02. Target: Windows 11 25H2 (build 26200), test PC i9-11900K, RTX 3060 Ti, NVIDIA 617.42, desktop.
No Windows PC was available: everything marked REAL PC still needs a run on one.

Evidence labels:

| Label | Meaning |
| --- | --- |
| DOC | Read in Microsoft or NVIDIA documentation |
| CODE | Read in open-source code or headers |
| DECOMP | Read in decompiled 25H2 pseudocode (nohuto/decompiled-pseudocode, IDA output of Microsoft binaries). Strong, but not a Microsoft promise |
| REPORT | A third party says so; not checked by me |
| EXCERPT | Seen only as a search-result excerpt; the page itself could not be read (usually a 403) |
| UNVERIFIED | No primary evidence found |
| REAL PC | Can only be settled on a real Windows PC |

---

## 0. What changes the plan

| # | Assumption in the brief or spec | What I found | Label |
| --- | --- | --- | --- |
| 1 | `AllowAutoGameMode` may live under HKCU GameBar | 25H2's shell reads `AutoGameModeEnabled` from HKCU and `AllowAutoGameMode` only from the **HKLM policy** key `Software\Policies\Microsoft\Windows\GameDVR`. Write only `AutoGameModeEnabled` | DOC + DECOMP |
| 2 | Turn off `GameDVR_Enabled`, `AppCaptureEnabled` and `HistoricalCaptureEnabled` while Game Bar stays | `AppCaptureEnabled=0` turns off all Game Bar capture (manual recording and screenshots). Only `HistoricalCaptureEnabled` is background recording. The 25H2 shell treats it as off when absent, so this tweak is usually a no-op | DECOMP + REPORT |
| 3 | Shader cache 10 GB is an increase | R615's header gives a default of `0x4000` MB = 16 GB, and XDA relays NVIDIA's support article saying 16 GB. **10 GB lowers it** on 617.42 | CODE + REPORT |
| 4 | "Prefer maximum performance" works | NVIDIA's own 617.42 release notes list it as the one open issue: it may not be applied correctly [6007998]. A read-back will look fine even if the driver ignores it | DOC |
| 5 | A High performance base is safe everywhere | Modern Standby PCs may only use Balanced or derived plans. AMD reportedly requires Balanced on dual-CCD X3D. No rigorous 2025-26 desktop FPS evidence exists either way | DOC + EXCERPT |
| 6 | Stock High performance exists | Some optimisers delete the stock plans, so Kestrel must not assume they exist, including on the owner's PC | REAL PC |
| 7 | GPU preference helps | Microsoft describes it as a choice for PCs with more than one GPU. Nothing to gain on a one-GPU desktop | DOC |
| 8 | Mouse tweak helps aim | Riot's notes confirm raw input, always on since 11.06. Microsoft says WM_INPUT has no ballistics. It only affects the desktop and possibly menus | DOC |
| 9 | `SRSetRestorePointW` success means a point exists | Inside the 24-hour window it returns TRUE and ERROR_SUCCESS with the **old** sequence number. Windows-sys also binds it to `sfc.dll` at load time, which Microsoft's page says not to do | DOC + CODE |
| 10 | NVIDIA Control Panel is the reference UI | NVIDIA announced on 2026-05-26 that it is retiring; Graphics > Program Settings in the NVIDIA App replaces Manage 3D Settings | DOC |

Two wider points:

- The owner's benchmark spread (runs 178/199/227, and runs 2 and 3 differ by 14%) is far larger than any plausible effect here (0-3%). Kestrel cannot show a gain from these tweaks until that variance drops.
- If the owner's EXM plan already turns off parking, Kestrel's plan may change little on that PC; the Preview shows it.

### Suggested Essential list after this research

| Tweak | Suggestion |
| --- | --- |
| Game Mode | Keep. Write only `AutoGameModeEnabled`. Usually a no-op |
| Background recording | Keep. Write only `HistoricalCaptureEnabled=0`. Usually a no-op |
| Windowed optimisations | Keep on Windows 11 only, and measure `PresentMode` before claiming anything |
| GPU preference | Apply only with two or more hardware GPUs; otherwise show "not needed" |
| Pointer precision | Keep, with honest copy: not an aim or FPS change in VALORANT |
| Power plan | Skip on dual-CCD X3D. Balanced-derived on Modern Standby. On hybrid CPUs leave parking at the base plan's value until measured |
| NVIDIA profile | Max performance plus an NVML check, V-Sync force off, texture filtering (no CPU-bound benefit expected). **Drop the 10 GB shader cache** |

---

## 1. Game Mode

**Which value 25H2 uses.** `AutoGameModeEnabled`.

- Microsoft's Windows 11 settings reference names `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled` (REG_DWORD, 0 or 1) as the key behind Gaming > Game Mode, and does not list `AllowAutoGameMode` (DOC).
- 25H2's `twinui.pcshell!BroadcastDVRComponent::EvaluateAppForGameMode` checks `HKLM\Software\Policies\Microsoft\Windows\GameDVR\AllowAutoGameMode` first, then `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled`. A missing value passes; 0 blocks (DECOMP).
- Folklore explained: WinUtil and TrayTrigger write both values under HKCU (CODE). OBS reads `AutoGameModeEnabled` and falls back to `AllowAutoGameMode` (CODE). A 2020 OBS forum post suspected Microsoft retired the HKCU value around 1903-1909 (REPORT).

| Item | Detail |
| --- | --- |
| Mechanism | `HKCU\Software\Microsoft\GameBar`, `AutoGameModeEnabled`, REG_DWORD, 1 |
| Read | Absent means on (DECOMP; OBS logs "probably on" for build 19042+). Also read the HKLM policy: if it is 0, say "blocked by policy" and do not fight it |
| Verify | Read back; Settings > Gaming > Game Mode. Whether VALORANT actually registers is a separate question, see below |
| Undo | Absent before: `RegDeleteValueW`. Present before: write the old DWORD back. Only if the value is still what Kestrel wrote |
| Takes effect | No reboot. Evaluated on app and view changes, so on next focus change. Per win-config's reading of the 23H2 code, turning it **off** needs a game restart because the registered PID stays (REPORT from decompiled code) |
| Scope | Per user. HKLM policy is read-only for Kestrel |
| Windows 10 22H2 | Same value in all guides; OBS treats it as the main one. Win10's `twinui.pcshell` in that repo has no `BroadcastDVRComponent`, so not checked in code (UNVERIFIED). Write the same value |

**Does VALORANT register?** Game Mode only applies to apps the shell classes as games, via the `expandedResources` capability or game config store data. win-config's author saw CS2 and Overwatch register but not Forza Horizon 5 or Cyberpunk 2077 (REPORT). REAL PC: use nohuto's `gm_effects` (reads `WNF_RM_GAME_MODE_ACTIVE`) with VALORANT in front. The public `HasExpandedResources` only answers for the calling process, so Kestrel cannot use it for the game.

**Benefit or harm.**

- MakeUseOf, 2026-07-12, one laptop (i7-13700H, RTX 4060, 25H2), CapFrameX: averages barely moved. CS2 1% lows rose from 142.3 to 154.6. Forza with Discord open rose from 48.2 to 71.5 (REPORT).
- An aggregator cites a Ryzen 5 5600 regression in one game: 52.4/36.8 off versus 49.8/33.3 on (EXCERPT, weak).
- NVIDIA's latency guide recommends it on, without numbers (DOC).
- Dual-CCD X3D placement depends on Game Mode and Game Bar (section 6). **Never turn these off.**

---

## 2. Game DVR background recording

**What the system actually reads on 25H2.**

- `BroadcastDVRComponent::RegistryChangeNotificationCallback` reads `AppCaptureEnabled` and `HistoricalCaptureEnabled` (plus battery, wireless, mic and camera values) from `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR`, and re-arms `RegNotifyChangeKeyValue`. So changes are picked up live (DECOMP).
- `IsGameDVREnabled` = `AppCaptureEnabled` AND policy AND an internal check. It does **not** read `GameDVR_Enabled` (DECOMP).
- The policy check reads `HKLM\Software\Policies\Microsoft\Windows\GameDVR\AllowGameDVR`, then CSP `ApplicationManagement/AllowGameDVR`, and caches for 12 hours (DECOMP; policy documented, DOC).
- The constructor's defaults are `AppCaptureEnabled`=on and `HistoricalCaptureEnabled`=off. So **absent history means off** (DECOMP, my reading of the byte offsets).
- `Windows.Media.Capture.AppCaptureSettings` stores `IsAppCaptureEnabled` and `IsHistoricalCaptureEnabled` in the same key (DECOMP via win-config). The WinRT class needs the restricted `appCaptureSettings` capability, so it is not a route for Kestrel (DOC).
- `GameDVR_Enabled`: `IsGameManager::Get/SetGameDvrEnabledProperty` reaches global property 0 of the game config store. That this is `GameDVR_Enabled` is my inference (UNVERIFIED). Sophia writes it next to `AppCaptureEnabled`. Writing both to 0 silences the ms-gamingoverlay popup on LTSC (REPORT).

**Which values the Settings page writes on 25H2: not provable from here.** Microsoft's reference lists only the hotkey values in that key, and the Settings binary is not in the decompiled set. The ElevenForum tutorial maps "Record what happened" to `HistoricalCaptureEnabled` (EXCERPT). REAL PC, two minutes: Procmon, filter on Path containing `GameDVR` or `GameConfigStore`, then flip "Record what happened" in Settings > Gaming > Captures.

| Item | Detail |
| --- | --- |
| Mechanism | `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR`, `HistoricalCaptureEnabled`, REG_DWORD, 0. **Do not write** `AppCaptureEnabled`, `GameDVR_Enabled` or `AllowGameDVR` |
| Why not the others | `AppCaptureEnabled=0` kills Win+Alt+R and Game Bar screenshots (TrayTrigger docs; WinUtil #609). Policy values cause "managed by your organization" and are the classic undo failure (WinUtil #254, #4242) |
| Read | Absent or 0: already off, record a no-op, write nothing. If HKLM `AllowGameDVR`=0, capture is already off; just report it |
| Verify | Read back; Settings > Gaming > Captures |
| Undo | Prior 1: write 1, only if still 0. No-op or absent: nothing to do |
| Takes effect | Immediately, via the shell's registry watcher (DECOMP) |
| Scope | Per user |
| Windows 10 22H2 | Same key and values; `AppCaptureSettings` dates from 10240 and the CSP from 1507. Absent default not checked (UNVERIFIED) |

**Benefit.** TrayTrigger's docs say background recording keeps a hardware encoder running and writes a rolling clip (REPORT). I found no 2025-26 measurement. It is off by default, so most PCs gain nothing.

---

## 3. Optimizations for windowed games

| Item | Detail |
| --- | --- |
| Mechanism | `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`, value `DirectXUserGlobalSettings`, REG_SZ list of `Name=Value;` tokens. Token `SwapEffectUpgradeEnable=1` |
| Read | Parse tokens. **Absent-token default is unknown**: ElevenForum says off, others report on (EXCERPT). REAL PC: delete nothing, just compare Settings with the raw string |
| Verify | Read back; System > Display > Graphics > Default settings. Real effect: `PresentMode`, see below |
| Takes effect | Next game launch; Microsoft's steps say restart the game (DOC). No reboot |
| Scope | Per user |
| Windows 10 22H2 | No such setting; TrayTrigger says no effect there. Skip below build 22000. The feature arrived around build 22557 (EXCERPT) |

**Merging tokens safely.**

1. Split on `;`, ignore empties, split each on the first `=`.
2. Compare keys case-insensitively. Collapse duplicates with last-wins, in place.
3. Keep unknown tokens and their values exactly (`VRROptimizeEnable`, `AutoHDREnable`, anything new).
4. Re-serialise as `k=v;`.

TrayTrigger documents why: blind appending produced `SwapEffectUpgradeEnable=0;SwapEffectUpgradeEnable=1;` after Windows had disabled it (CODE, MIT).

**Undo at token level, never the whole string.** A whole-string restore would undo a VRR or Auto HDR change the user made later.

- Record the prior token (value or absent) and whether the registry value existed.
- If the token is still Kestrel's: restore it or remove it.
- If the string becomes empty and the value did not exist before: delete the value.
- If Kestrel created the key and it is empty: delete the key.
- Auto HDR on forces this optimisation on (DOC), so a token that reads back unexpectedly may be Windows' doing.

**Does it matter for VALORANT?**

- Microsoft scopes it to DirectX 10 and 11 games in windowed and borderless modes, moving blt-model presents to flip-model (DOC). So: no effect in true fullscreen.
- Riot lists DirectX 11 as the requirement (EXCERPT of Riot's specs page); patch 11.02 moved to UE 5.3 and the notes do not mention the graphics API (DOC). Which API it actually runs on is UNVERIFIED.
- If VALORANT already presents with flip-model, this does nothing. I expect that but could not confirm it, because Unreal's source is not public.
- REAL PC, and cheap: Kestrel already parses PresentMon's `PresentMode` (`crates/kestrel-bench/src/csv.rs`). `Composed: Copy with GPU GDI` or `Composed: Copy with CPU GDI` means the tweak can help. `Hardware: Independent Flip`, `Hardware Composed: Independent Flip` or `Composed: Flip` means it is already flip-model. `Hardware: Legacy Flip` means classic fullscreen, where the tweak does not apply (strings checked in PresentMon's `CsvOutput.cpp`; the interpretation is mine). Check the owner's existing benchmark data first, per display mode.

**Evidence.** No VALORANT measurement found. Coverage of Microsoft's blog says some games may tear (EXCERPT).

---

## 4. Per-app GPU preference

| Item | Detail |
| --- | --- |
| Mechanism | Same key. Value **name** is the full exe path; REG_SZ data `GpuPreference=2;`, merged like section 3 since it can hold other tokens. ShieldBattery's example: `SwapEffectUpgradeEnable=1;GpuPreference=2;` |
| Exe | `<product_install_full_path>\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe`, default `C:\Riot Games\VALORANT\live\...`. **Kestrel's `valorant.rs` returns forward slashes: convert to backslashes for the value name.** Case sensitivity: REAL PC |
| Values | 0 Windows decides, 1 power saving, 2 high performance (community and ninjaOne). A Microsoft Q&A answer gives a contradictory reading; treat that answer as unreliable |
| Read | Absent means Windows decides. If it is already 1 or 2, the user chose: leave it (ShieldBattery's rule) |
| Verify | Read back; Settings > Display > Graphics > Custom options; Task Manager's GPU column while playing |
| Undo | Token level. Value absent before: delete the value, **not** `GpuPreference=0;` |
| Takes effect | Next game launch |
| Scope | Per user |
| Windows 10 22H2 | Works: `EnumAdapterByGpuPreference` dates from 1803 (DOC); same key (community) |

**One active GPU gains nothing.**

- Microsoft describes the GPU choice as applying if the PC has more than one GPU (DOC).
- DXGI's high-performance order is xGPU, dGPU, iGPU (DOC).
- ShieldBattery notes DXGI only reorders for apps using the default adapter (CODE comment, unverified).
- Unreal may pick its own adapter, so it may be moot even on hybrids (UNVERIFIED, from memory).
- The i9-11900K does have an iGPU (UHD 750), which may be enabled. The owner also has a Parsec virtual adapter. Suggest: apply only with two or more hardware GPUs, reusing stage 3's adapter list.

---

## 5. Enhance pointer precision

| Item | Detail |
| --- | --- |
| Mechanism | `SystemParametersInfoW(SPI_SETMOUSE (4), 0, &[i32;3] {0,0,0}, SPIF_UPDATEINIFILE (1))`, then `SendNotifyMessageW` (Kestrel dropped `SPIF_SENDCHANGE (2)`, which can block on a hung window). Array order is threshold1, threshold2, acceleration (DOC) |
| Registry equivalent | `HKCU\Control Panel\Mouse`, **REG_SZ** `MouseSpeed`, `MouseThreshold1`, `MouseThreshold2`. Off: `0/0/0`. On (default): `1/6/10` (procmon capture in win-config). WinUtil once wrote these as numbers and had to fix it (PR #5188) |
| Read | `SPI_GETMOUSE (3)` for live state, plus the registry for persisted state. Report any mismatch |
| Verify | Both reads; Mouse Properties > Pointer Options |
| Undo | Restore the three old strings exactly (absent: delete) and call `SPI_SETMOUSE` with the old ints. Only if still `0,0,0` |
| Takes effect | Immediately via SPI. A registry-only write waits for next sign-in |
| Scope | Per user on disk; in-memory state is per session |
| Windows 10 22H2 | Same |

**Trap when the elevated account differs (section 9).** `SPIF_UPDATEINIFILE` writes the caller's profile (DOC), so it would hit the wrong account. In that case call SPI with only `SPIF_SENDCHANGE` and write `HKEY_USERS\<SID>\Control Panel\Mouse` yourself. Whether SPI succeeds from another account's process on that desktop: REAL PC.

**VALORANT and raw input: confirmed.** Riot's 3.07 notes describe the setting as choosing APIs for processing Raw Input, and 11.06 removed the setting and keeps it on at all times (DOC). Microsoft says WM_INPUT has no ballistics applied, while WM_MOUSEMOVE does (DOC). So this changes nothing for in-game aim. Menu cursor behaviour is UNVERIFIED. No FPS effect, and the copy should say so.

---

## 6. Kestrel power plan

### GUIDs: all correct

Checked against `windows-sys` 0.61.2 constants (Kestrel's own dependency, generated from Windows metadata) and Microsoft docs.

| Purpose | GUID | Constant / source | Value |
| --- | --- | --- | --- |
| High performance | `8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c` | `GUID_MIN_POWER_SAVINGS` | base |
| Balanced | `381b4222-f694-41f0-9685-ff5bb260df2e` | `GUID_TYPICAL_POWER_SAVINGS` | fallback base |
| Processor subgroup | `54533251-82be-4824-96c1-47b60b740d00` | `GUID_PROCESSOR_SETTINGS_SUBGROUP` | |
| CPMINCORES | `0cc5b647-c1df-4637-891a-dec35c318583` | `GUID_PROCESSOR_CORE_PARKING_MIN_CORES` | **100** (not 0). Microsoft: parking is disabled at 100% |
| CPMINCORES1 (efficiency class 1) | `0cc5b647-c1df-4637-891a-dec35c318584` | `..._MIN_CORES_1` | hybrid only |
| USB subgroup | `2a737441-1930-4402-8d77-b2bebba308a3` | Microsoft support article (Teams Rooms) | |
| USB selective suspend | `48e6b7a6-50f5-4782-a5d4-53bb8f07e226` | same; 0 disabled, 1 enabled | 0 |
| PCIe subgroup | `501a4d13-42af-4429-9fd1-a8218c268e20` | `GUID_PCIEXPRESS_SETTINGS_SUBGROUP` | |
| ASPM | `ee12f906-d277-404b-b6da-e5fa1a576df5` | `GUID_PCIEXPRESS_ASPM_POLICY`; 0 None, 1 Moderate, 2 Max | 0 |

CPMinCores is a hidden setting and works per NUMA node. The API writes it regardless of visibility.

### API route (`powrprof.dll`; windows-sys `Win32_System_Power`, GUIDs in `Win32_System_SystemServices`)

1. `PowerGetActiveScheme(NULL, &mut *mut GUID)`; journal it; `LocalFree`.
2. `PowerDuplicateScheme(NULL, &base, &mut dest)`. NULL `dest` allocates a new GUID. `powercfg /duplicatescheme` accepts a destination GUID, so a fixed Kestrel GUID through a non-null pointer should work (REAL PC). Otherwise journal the allocated one.
3. `PowerWriteFriendlyName(NULL, &scheme, NULL, NULL, wide_bytes, len)`. Wide string, size includes the terminator; the binding takes bytes.
4. `PowerReadValueMin` and `PowerReadValueMax`, then `PowerWriteACValueIndex(NULL, &scheme, &sub, &setting, v)`. TrayTrigger reports some machines accept only a narrower range for parking and clamps; do the same and record it.
5. `PowerSetActiveScheme(NULL, &scheme)`. Microsoft: changes to the active scheme need this call. No reboot.
6. Verify: `PowerGetActiveScheme`, `PowerReadACValueIndex` for each value. `powercfg /q`. Parking: Perfmon's parking status counter (REAL PC). ASPM: `powercfg /energy`; firmware may keep ASPM control, in which case the plan has no effect (from memory, REAL PC).

**DC values.** `PowerWriteDCValueIndex` is irrelevant on desktops. Suggest leaving DC at the base plan's values, given the spec's laptop-only-on-charger rule. TrayTrigger notes its values apply on battery too, and calls that a cost.

### Undo

1. `PowerSetActiveScheme(prior)`. If prior is gone, use Balanced and say so.
2. `PowerDeleteScheme(kestrel)`, only for a scheme the journal says Kestrel created.
3. **Never** run `powercfg /restoredefaultschemes`: it removes all custom plans (win-config), which on the owner's PC includes EXM's.

If the Kestrel plan has vanished because something reset schemes, drop the journal entry and report it.

**Scope.** Machine-wide. Microsoft says each computer has one active plan, although the API page says "for the current user". Read `HKLM\SOFTWARE\Policies\Microsoft\Power\PowerSettings\ActivePowerScheme`: if a policy sets it, report it; do not delete it as Sophia does.

**Windows 10 22H2.** Same API and settings.

### Machines that need different handling

**Modern Standby (S0).**

- Microsoft: such devices only allow Balanced or plans derived from it (DOC).
- Detect with `GetPwrCapabilities`: `AoAc` TRUE means S0 low-power idle (DOC).
- `PowerDuplicateScheme` of High performance has mixed reports: `powercfg -duplicatescheme` failed for a Surface user; `PlatformAoAcOverride` works for some (EXCERPT). Do not touch that value.
- On these PCs base on Balanced and write only the three toggles.

**High-performance-derived plans lose the power slider.** Microsoft says the slider appears only on Balanced or derived plans, and advises gaming OEMs to copy their settings onto Balanced (DOC). Game Mode works in any slider mode (DOC).

**Hybrid CPUs (P/E).**

- No 2025-26 measurement of parking on versus off found. Vendor pages claiming stutter fixes carry no data.
- What exists: AnandTech relayed Intel saying Balanced favours in-focus apps for P-cores (EXCERPT). XDA relays Alois Kraus: on Windows 10, Balanced could push lower-priority work to E-cores, far less so on Windows 11 (REPORT).
- Intel's Arrow Lake field update said a missing PPM package hurt core parking and ramp, and that afterwards both Balanced and High performance deliver its performance. Intel also ties APO to that package (EXCERPT).
- Reasoning, unmeasured: parked E-cores help keep game threads on P-cores, so forcing all awake could hurt. High performance itself unparks everything (next point).
- Suggestion: on hybrid, do not force parking values; use a Balanced-derived plan with USB and ASPM only, and let the benchmark decide.

**What High performance already does.** Microsoft's server tuning page says processors lock at the highest state, turbo included, and all cores are unparked (DOC, server-focused). So on this base CPMINCORES is likely already 100, and the write is an explicit guarantee (read it first; REAL PC).

**Ryzen X3D.**

- Dual-CCD (7900X3D, 7950X3D, 9900X3D, 9950X3D): the chipset driver uses Game Bar's game detection to park the non-cache CCD.
- AMD's guidance as relayed: Balanced plan, Game Mode on, current Game Bar (Notebookcheck, Tom's Hardware: EXCERPT; AMD's partner PDF lists Balanced for test systems).
- HWBusters says that changing the plan through the legacy Control Panel breaks parking on its 9950X3D; use Settings > System > Power (REPORT, read). XDA adds that owners on High performance report parking does not engage; reports conflict (REPORT).
- TrayTrigger keeps its plan on these chips but leaves parking alone. Given AMD's Balanced guidance, I would **skip Kestrel's plan on dual-CCD X3D entirely**.
- Single-CCD X3D (7800X3D, 9800X3D): no such scheduler; normal handling. The dual-cache 9950X3D2 is unchecked (UNVERIFIED).

### Does High performance beat Balanced on a desktop in 2025-26?

**I found no rigorous answer.**

| Source | Result | Quality |
| --- | --- | --- |
| GO-EUC, 2026-09-15 | Slight gain, no measurable benefit. Azure VMs, no games | Good method, wrong workload |
| XDA, 2026-10-23 | Advises Balanced. No benchmarks of its own | Opinion with citations |
| XDA, recent | High performance versus Ultimate within margin of error in three games. Does not cover Balanced | Narrow (EXCERPT) |
| dtptips, 2025 | -1% to +3.4% average: SotTR 169.3 vs 172.2, RDR2 65.8 vs 68.1. Setup unclear | Weak (EXCERPT) |
| Laptop blogs | 5-15% | No method; not desktop (EXCERPT) |

Mechanism: Speed Shift responds in about 1 ms (Microsoft's HWP text; XDA), so pinning clocks has little to win. Expect 0-3% at most, with higher idle power and heat. Kestrel should present this as "measured on your PC", not as a promised gain.

**USB and ASPM.** No gaming evidence for either. Microsoft recommends selective suspend off on Teams Rooms devices, a device-reliability reason (DOC).

### EXM's free power plan

- EXM does not publicly document its contents, and it is closed source and proprietary. **Do not copy from it.**
- Consequences for Kestrel on the owner's PC: undo must return to the EXM plan by GUID; enumerate with `PowerEnumerate` and don't assume stock plans exist; try `PowerDuplicateScheme` of High performance and handle failure. WinHelpOnline says this restores a missing plan on non-S0 PCs (EXCERPT, REAL PC).
- REAL PC: `powercfg /l`, and `powercfg /qh` for the three toggles on the EXM plan.

---

## 7. NVIDIA per-game profile through NVAPI DRS

### Loading

`LoadLibraryExW("nvapi64.dll", LOAD_LIBRARY_SEARCH_SYSTEM32)`, `GetProcAddress("nvapi_QueryInterface")`, type `extern "C" fn(u32) -> *mut c_void`. A null return means the driver lacks that id. This export is not in `nvapi.h`, but NVIDIA publishes the id table, and NPI, TrayTrigger, nvapi-rs and Zed all do this (CODE). Zed is a Rust example.

All functions return `i32` (NvAPI_Status). Call `NvAPI_Initialize` before anything, and `NvAPI_Unload` in a pair with it.

### Interface ids (from `nvapi_interface.h`, R615, commit `70d337db91`, read directly)

| Function | Id | Function | Id |
| --- | --- | --- | --- |
| NvAPI_Initialize | `0x0150E828` | NvAPI_DRS_GetSetting | `0x73BF8338` |
| NvAPI_Unload | `0xD22BDD7E` | NvAPI_DRS_SetSetting | `0x577DD202` |
| NvAPI_DRS_CreateSession | `0x0694D52E` | NvAPI_DRS_DeleteProfileSetting | `0xE4A26362` |
| NvAPI_DRS_LoadSettings | `0x375DBD6B` | NvAPI_DRS_RestoreProfileDefaultSetting | `0x53F0381E` |
| NvAPI_DRS_FindApplicationByName | `0xEEE566B2` | NvAPI_DRS_SaveSettings | `0xFCBC7E14` |
| NvAPI_DRS_FindProfileByName | `0x7E4A9A0B` | NvAPI_DRS_DestroySession | `0xDAD9CFF8` |
| NvAPI_DRS_CreateProfile | `0xCC176068` | NvAPI_DRS_DeleteProfile | `0x17093206` |
| NvAPI_DRS_CreateApplication | `0x4347A9DE` | NvAPI_DRS_GetProfileInfo | `0x61CD6FD6` |
| NvAPI_DRS_GetBaseProfile | `0xDA8466A0` | NvAPI_DRS_GetSettingNameFromId | `0xD61CBE6E` |
| NvAPI_DRS_EnumSettings | `0xAE3039DA` | NvAPI_GetErrorMessage | `0x6C2D048C` |

NPI first tries other ids (for example `0x8A2CF5F5` for SetSetting) and falls back to these. Those are not in NVIDIA's table, and TrayTrigger manages with the public ones. **Use the public ones.**

### Struct sizes (compiled from the header's layout; assert these in Rust)

| Struct | sizeof | Version word | Layout note |
| --- | --- | --- | --- |
| `NVDRS_SETTING_V1` | 12320 | `0x00013020` | **pack(4)**. settingId at 4100, type 4104, location 4108, isCurrentPredefined 4112, isPredefinedValid 4116, predefined union 4120, current union 8220 |
| `NVDRS_APPLICATION_V4` | 20492 | `0x0004500C` | appName at 8, friendly 4104, launcher 8200, fileInFolder 12296, bitfield word 16392, commandLine 16396 |
| `NVDRS_PROFILE_V1` | 4116 | `0x00011014` | name at 4, gpuSupport 4100, isPredefined 4104, numOfApps 4108, numOfSettings 4112 |

Strings are `[u16; 2048]`; pass name parameters as a pointer to a full buffer. Enum `NVDRS_SETTING_TYPE`: DWORD is 0. Enum `NVDRS_SETTING_LOCATION`: 0 current, 1 global, 2 base, 3 default.

Status codes: `NVAPI_SETTING_NOT_FOUND` -160, `NVAPI_PROFILE_NOT_FOUND` -163, `NVAPI_PROFILE_NAME_IN_USE` -164, `NVAPI_EXECUTABLE_NOT_FOUND` -166, `NVAPI_EXECUTABLE_ALREADY_IN_USE` -167, `NVAPI_PROFILE_REMOVED` -169, `NVAPI_INVALID_USER_PRIVILEGE` -137, `NVAPI_EXECUTABLE_PATH_IS_AMBIGUOUS` -182. The header's doc comment names `NVAPI_APPLICATION_NOT_FOUND` but the enum has none; treat -166 as not found.

### Settings (from `NvApiDriverSettings.h`, R615)

| Setting | Id | Target value | Header default |
| --- | --- | --- | --- |
| Power management mode, `PREFERRED_PSTATE_ID` | `0x1057EB71` | `PREFER_MAX` = 1 | `OPTIMAL_POWER` = 5 |
| Texture filtering quality, `QUALITY_ENHANCEMENTS_ID` | `0x00CE2691` | `HIGHPERFORMANCE` = `0x14` | `QUALITY` = 0 |
| Vertical sync, `VSYNCMODE_ID` | `0x00A879CF` | `FORCEOFF` = `0x08416747` | `PASSIVE` = `0x60925292` |
| Shader cache size, `PS_SHADERDISKCACHE_MAX_SIZE_ID` | `0x00AC8497` | 10 GB would be `0x2800` (MB; value names from NPI, not NVIDIA) | **`0x4000` = 16 GB**; older reference headers show `0x1000` |

Note the `VSYNCMODE_ID` is one digit away from `VRR_APP_OVERRIDE_ID` `0x10A879CF`. Compare carefully.

### Shader cache: drop it

- **Smaller than the default**: 16 GB per the R615 header and XDA relaying NVIDIA's support article (that article returned 403 to me, EXCERPT). 10 GB would lower the limit, and hitting a limit means evicting shaders and recompiling them, which is what the tweak aimed to avoid.
- **Per-application honouring: UNVERIFIED.**
  - NVIDIA's help says the size limits the driver's disk use and lists it among features on the Global and Program tabs, without saying more (DOC).
  - XDA finds it in the NVIDIA App under Global Settings (REPORT).
  - NPI's text says "for this profile" and TrayTrigger says a game's own profile keeps its value (tool authors, not NVIDIA). Guides conflict.
  - A global write would also break decision 12 (VALORANT's profile only). Testing it would need clearing the cache, which the spec rules out.
- Also: installing a new driver deletes the cache regardless (DOC). Suggest reading the effective value and only showing a hint if it is under 10 GB.

### Flow

Sessions are private copies, and Save writes the whole copy back (TrayTrigger code comment, consistent with NPI's locking; not stated by NVIDIA). So: one session at a time, short-lived, and **verify in a second session**. A load takes over 100 ms (same source).

1. CreateSession, LoadSettings.
2. FindApplicationByName with the **full path**. NVIDIA says a fully qualified path always returns the profile the driver will apply on launch (DOC). A bare name can return -182.
3. If found, use that profile. GetProfileInfo gives its name and `isPredefined`; never hard-code a profile name.
4. If -166: FindProfileByName for Kestrel's name, or CreateProfile (`version`, `profileName`) then CreateApplication (`version`, `appName`). If CreateApplication fails, delete the profile before any Save (TrayTrigger). NVAPI's note prefers absolute paths; TrayTrigger uses the exe name so a moved install keeps working. I would use the exe name, then confirm with step 2. -167 needs care: see NPI's `MitigateNvapiAlreadyInUseBug`.
5. For each setting: GetSetting, classify prior (next table), **journal to disk**, then SetSetting with `version`, `settingId`, `settingType`=0, `settingLocation`=0, `u32CurrentValue`.
6. SaveSettings, DestroySession. Reopen and verify. Unload.

### Undo, exactly

Classify before writing. **A failed read is not "absent"**: abort, or undo would delete a real value (TrayTrigger's rule).

| GetSetting result | Journal as | Undo |
| --- | --- | --- |
| -160, or `settingLocation` != current | absent or inherited | `DeleteProfileSetting` |
| current, `isCurrentPredefined` != 0 | predefined | `RestoreProfileDefaultSetting` |
| current, `isCurrentPredefined` == 0 | user value `u32CurrentValue` | `SetSetting` with it |
| Already equals target | no-op | nothing |

- Before any undo, re-read. If the value is not what Kestrel wrote, it is someone else's now: leave it and report it (TrayTrigger's rule).
- **Profile Kestrel created**: delete its settings; then if `isPredefined`==0, `numOfApps`==1 and `numOfSettings`==0, `DeleteProfile`. If others added settings, keep the profile.
- **Avoid `RestoreProfileDefault`** on a predefined profile: it removes all user changes there, not only Kestrel's. On a user profile it can delete the whole profile (DOC, -169).

### Read, verify, scope

| Item | Detail |
| --- | --- |
| Read | `GetSetting` on the profile from step 2; `GetProfileInfo` for the profile itself |
| Verify | Reopen a new session and read again. For max performance, also check the effect through NVML |
| Takes effect | Treated as next game launch (see "Game closed?" below) |
| Scope | Machine-wide: one driver database (under `C:\ProgramData\NVIDIA Corporation\Drs`, from memory) rather than per user |
| Windows 10 22H2 | The header lists these calls as Windows 10 and later; same flow. R615 supports Windows 10 per its release notes |

### Questions from the brief

- **Predefined VALORANT profile: UNVERIFIED.** No public dump or NVIDIA statement found. Guides say to pick VALORANT in the list and add it if missing. One REAL PC NVAPI call settles it: FindApplicationByName with the full path to `VALORANT-Win64-Shipping.exe`, then GetProfileInfo. Also check whether NVIDIA qualifies the entry with `fileInFolder` or `launcher`.
- **Licence**: `nvapi.h`, `nvapi_interface.h`, `NvApiDriverSettings.h` and `nvapi_lite_common.h` carry SPDX MIT. `License.txt` (updated 2026-09-18) puts the `.lib` files under MIT, and the README says MIT. Compatible with GPL-3.0: keep NVIDIA's notice in third-party notices if header text is copied; ids are facts. `nvapi64.dll` stays NVIDIA's driver file, loaded at runtime and not shipped, as with NVML in stage 3. Not legal advice.
- **NVIDIA App**: Graphics > Program Settings replaces Manage 3D Settings, and the classic Control Panel is retiring for Game Ready and Studio drivers (NVIDIA, 2026-05-26, 610.47). TrayTrigger states the App and Control Panel share this database. So users can change values there, which is why undo needs the "still ours?" check. "Optimize" is described as configuring in-game settings and enabling DLSS and Reflex; whether it rewrites these driver values: **no NVIDIA statement found**. REAL PC: press Optimize, re-read. Also re-read after a driver update; a clean install probably resets the database (from memory).
- **Vanguard**: no Riot statement found, for or against. A community reply guesses it is not bannable (REPORT, worthless as proof). Technically Kestrel changes NVIDIA's database in its own process and touches neither the game nor Vanguard, like the NVIDIA App. A separate Blur Busters thread titled "Riot's custom profile" appears to be by a forum user named Riot, not Riot Games (EXCERPT). REAL PC: launch VALORANT after a write.
- **Game closed?** NVIDIA does not say. TrayTrigger says games started afterwards use the new value. Treat it as next launch, and refuse or warn while VALORANT runs, which also keeps Kestrel away from the game (UNVERIFIED).
- **Elevation**: NPI requires elevation to write; TrayTrigger says some settings do not need it and some return -137. Kestrel's manifest is `requireAdministrator`, so this is moot.

### Real benefit in a CPU-bound game

| Setting | Expectation | Evidence |
| --- | --- | --- |
| Prefer max performance | The one plausible gain: holds GPU clocks when the GPU is underused, trimming latency slightly | NVIDIA describes Reflex's Boost doing this when heavily CPU-bound, with more power use and slight latency gain (EXCERPT). TheFPSReview 2019: no consistent gain in GPU-limited tests, an occasional clock drop, idle power up in global use (REPORT, old). **Open issue 6007998 on 617.42** (DOC) |
| Texture filtering high performance | None when CPU-bound; slightly worse texture quality | NVIDIA only says it gives the highest frame rate, no numbers (DOC). No 2025-26 test found |
| V-Sync force off | None if in-game V-Sync is already off; only a guard | NVIDIA recommends off if you accept tearing (DOC). On G-SYNC displays forcing it off may cost the usual V-Sync pairing (from memory, UNVERIFIED) |
| Shader cache 10 GB | Worse or nothing | See above |

**Verify the max-performance setting by effect, not value:** read clocks and P-state through NVML during a run (Kestrel already has NVML). Consider also pointing to VALORANT's own Reflex "On + Boost", which NVIDIA's guide shows as an option (EXCERPT; REAL PC to confirm it is still there). Per-app scope avoids the idle-power cost.

---

## 8. System restore points

### SRSetRestorePointW versus WMI

| | `SRSetRestorePointW` | WMI `root\default:SystemRestore.CreateRestorePoint` |
| --- | --- | --- |
| Result | BOOL plus `STATEMGRSTATUS` (`nStatus`, `llSequenceNumber`) | S_OK or COM error |
| Throttled case | TRUE, ERROR_SUCCESS, **sequence of the earlier point** (DOC) | S_OK; no sequence (DOC) |
| Failure detail | `ERROR_SERVICE_DISABLED` (1058), `ERROR_DISK_FULL`, `ERROR_BAD_ENVIRONMENT` (safe mode), `ERROR_TIMEOUT`, `ERROR_INTERNAL_ERROR`, `ERROR_INVALID_DATA` (DOC) | Generic |
| From Rust | Dynamic load, simple | Needs WMI COM |

**Use `SRSetRestorePointW`, with WMI only for listing.**

- Microsoft says not to load-time link: `LoadLibrary` `SrClient.dll`, then `GetProcAddress`. **`windows-sys` binds the function to `sfc.dll`, so do not call its binding.** Use its struct types only.
- `RESTOREPOINTINFOW` is `packed(1)` in windows-sys. Fields: `dwEventType` `BEGIN_SYSTEM_CHANGE` (100), `dwRestorePtType` `MODIFY_SETTINGS` (12), `llSequenceNumber` 0, description of up to 256 wide chars.
- After applying, call again with `END_SYSTEM_CHANGE` (101) and the returned sequence. Do not END a sequence you did not open, that is, the throttled case (PC Tweaker's PR, REPORT). On cancel, `CANCELLED_OPERATION` (13) removes the point (DOC).
- **COM security**: Microsoft says the process must allow NetworkService, LocalService and System to call back, with a `CoInitializeSecurity` sample. That call is process-wide and can only happen once, so it may clash with WebView2 (from memory). Data point: PC Tweaker (Rust and Tauri) has no such call in its repo, and its VM test still created real points (REPORT). REAL PC: test without it first; if it fails, use a short-lived helper process.

### The 24-hour throttle

`HKLM\Software\Microsoft\Windows NT\CurrentVersion\SystemRestore`, DWORD `SystemRestorePointCreationFrequency`, **in minutes**. Absent means 24 hours; 0 means never skip (DOC). Applications must create it themselves.

Prior art:

- Sophia sets 0, creates, then removes the value.
- WinUtil sets 0 with a hard-coded `OriginalValue` of 1440, for a value that did not exist.
- PC Tweaker never changes it and reports "reused".

Suggestion: leave it alone. If a point exists from the last 24 hours, it is older than this change, and that is acceptable; tell the user its time.

### Telling a point was really created

1. Before the call, read the highest `SequenceNumber` and its `CreationTime` from WMI `SELECT * FROM SystemRestore` in `root\default`.
2. After the call, a point counts as created only if a **higher sequence** appears with your description.
3. Re-poll once a second for up to 30 seconds, only while a point is expected. PC Tweaker's clean Windows 11 VM test took about 30 seconds for the call and the listing lagged it (REPORT; the PR dates the test "2/11/2026").

**How long.** "A few seconds" (TrayTrigger docs) to about 30 seconds (PC Tweaker VM). Microsoft: a full snapshot of the registry and system databases. A Microsoft Q&A thread reports hangs. Run it off the UI thread, with progress and a timeout.

### System Protection

- **Detect**: `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SPP\Clients`, value `{09F7EDC5-294E-4180-AF6A-FB0E6A0E9513}`, must contain the C: volume's GUID (`GetVolumeNameForVolumeMountPointW`). This is Sophia's method (CODE); undocumented. Runtime truth: `ERROR_SERVICE_DISABLED`.
- **Avoid WinUtil's test** (`Get-ComputerRestorePoint` returning nothing): zero points is not the same as protection off.
- **Policy**: `HKLM\SOFTWARE\Policies\Microsoft\Windows NT\SystemRestore`, `DisableSR` and `DisableConfig`. Sophia deletes them. Kestrel should only report them. Sophia also sets `VSS` and `swprv` to Manual if disabled; report that instead too.
- **Turn on**: WMI `SystemRestore.Enable("C:\\")`. It returns immediately, before monitoring is fully on, and fails in safe mode (DOC). Ask first: it takes disk.
- **Size**: `vssadmin resize shadowstorage /for=C: /on=C: /maxsize=...`. Microsoft warns that resizing may make shadow copies disappear (DOC). Only do this if no storage is allocated. REAL PC: `vssadmin list shadowstorage`.

### Failure modes on 25H2

- **60-day retention**: 24H2 and later keep restore points for up to 60 days since June 2025 (Microsoft KB text, EXCERPT).
- **Space**: oldest points are purged; up to 5% of disk or 10 GB on large disks (DOC).
- **Point-in-time restore** shares VSS space with System Restore (Microsoft Learn, EXCERPT). Its default limit is 2% of disk and it is on by default from 26H2 (support page, EXCERPT). A Tech Community thread from 2026-07-20 reports System Protection's maximum snapping back to 2% and old points being purged (REPORT, no Microsoft reply).
- **Also**: System Protection is often off, VSS may be disabled, safe mode, and mutex timeouts.

**Conclusion: the journal is the real undo; the restore point is a bonus that may vanish.** Never promise it.

**Windows 10 22H2**: same API and throttle, and it can hit the documented Stop 0xc000021a when restoring across updates (DOC).

---

## 9. Per-user settings when the elevated account differs

Kestrel's manifest is `requireAdministrator`. For a standard user, consent means typing an admin's credentials, and the process then runs **as that admin**: Raymond Chen spells this out, and removing elevation would only yield the admin's own unelevated token. HKCU is then the wrong hive, so Game Mode, DVR, DirectX and mouse writes would land on the wrong account.

**Finding the interactive user, no SYSTEM needed.**

1. `GetShellWindow()`, then `GetWindowThreadProcessId`, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`, `OpenProcessToken(TOKEN_QUERY)`, `GetTokenInformation(TokenUser)`, `ConvertSidToStringSidW`. This is the shell of Kestrel's own desktop. NULL means no shell (DOC). Opening another user's token as admin should work, as elevated `Get-Process -IncludeUserName` does (REAL PC).
2. **Fallback**: `ProcessIdToSessionId(GetCurrentProcessId())`, then `WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, id, WTSUserName / WTSDomainName)`, then `LookupAccountNameW`.
3. **Not recommended first**:
   - Toolhelp snapshot looking for `explorer.exe`: several instances are possible, it can run as another user, and the name is spoofable. If used, match the session and `%WINDIR%` path.
   - `WTSGetActiveConsoleSessionId`: it gives the physical console, not Kestrel's session (wrong under RDP), and 0xFFFFFFFF during transitions (DOC).
   - `WTSQueryUserToken`: needs LocalSystem and SE_TCB (DOC).

**Detecting the mismatch.** Compare `OpenProcessToken(GetCurrentProcess())`'s TokenUser with the shell SID. Different means the over-the-shoulder case: tell the user which account gets the changes, and include it in Copy diagnostics.

**Writing.** `RegOpenKeyExW(HKEY_USERS, "<SID>\\Software\\Microsoft\\GameBar", ...)`.

- The hive is loaded while that user is signed in.
- Suggest **always** using this path, even when the SIDs match, so there is one code path. A process's HKCU mapping is cached, and `RegOpenCurrentUser` only helps with impersonation.
- Journal the SID with each change. Undo for a signed-out user stays pending.
- Unaffected: power plan (machine) and DRS (ProgramData). DXGI reads the game's own HKCU, which is the interactive user.
- SPI: see section 5.

Windows-sys modules: `Win32_UI_WindowsAndMessaging`, `Win32_System_RemoteDesktop`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_System_Registry`, `Win32_System_Threading`.

**Windows 10 22H2**: the same APIs and the same elevation behaviour; none of these pages limit them to Windows 11.

---

## 10. Undo prior art and journal lessons

| Tool | How it undoes | What goes wrong |
| --- | --- | --- |
| **WinUtil** (MIT) | `tweaks.json` holds fixed `Value` and `OriginalValue`; undo writes `OriginalValue`. Nothing recorded | A user's earlier custom value is lost. #609: DVR undo missing entirely. #254, #4242: policies left, "managed by your organization". #487: Xbox overlay stayed broken. PR #5188 (merged 2026-10-21) added a current-state check and admits it is not ownership: a service already at the applied type can still be reset |
| **Sophia** (MIT) | Paired functions, for example `XBOXGameBar -Enable` writes 1 and 1. Restores defaults, not prior values | Prior state is unknowable. Deletes policies to make changes visible. README favours a fresh install (excerpt) |
| **O&O ShutUp10++** | Best design. Sessions store each touched setting's prior value, and "none" returns to none. History is a **stack**: rolling back an entry rolls back newer ones. Entries are removed only after a successful restore. 10 sessions kept, plus a first-run baseline, plus a separate factory reset that is itself undoable and removes empty policy subkeys | Windows' own resets are outside history. Toggling off differs from restoring. Runs through a background service |
| **TrayTrigger** (MIT, closest analog) | Captures prior once, restores only while the system still holds what it wrote, keeps the record if restore fails | Worth copying patterns; see below |
| **PC Tweaker** (source-available, **all rights reserved: read, do not copy**) | Honest restore-point audit | Its PRs document the throttle lies |

**Lessons for Kestrel's journal.**

1. **Write-ahead.** Read prior, fsync the intent (`FlushFileBuffers`), write to the OS, read back, fsync the outcome. TrayTrigger records before the save so a crash still leaves a record. For compaction use a temp file plus `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`. Tolerate a truncated last line in append-only JSONL.
2. **Crash recovery by comparison.** For an intent with no outcome, compare the current value: equal to planned means applied (keep it undoable); equal to prior means not applied (drop it); anything else means someone else changed it (flag it, never touch it).
3. **A failed read is not absent.** Distinct states: absent, value, predefined, inherited, read failed. Abort on read failure.
4. **No-op is a state.** If already at target, record no-op and never undo it. This fixes WinUtil's admitted hole.
5. **Three-way check on undo.** Current equals written: restore. Current equals prior: done. Otherwise: leave it, report "changed by something else", and offer an explicit force.
6. **Capture once.** Re-apply must not overwrite the first prior (TrayTrigger).
7. **Delete the record only after success, and always then.** A stale record makes a later identical user choice look like Kestrel's (TrayTrigger).
8. **LIFO undo per session**, with Revert everything walking the stack (O&O).
9. **Token-level records** for `DirectXUserGlobalSettings` and per-app GPU strings.
10. **Record created keys and profiles**, and remove them only if empty and Kestrel-created (O&O's empty-subkey point).
11. **Never write Policies keys.**
12. **Keep the journal admin-only and treat it as untrusted.** An elevated app acting on a user-writable file is a privilege escalation path, so undo must accept only known tweak ids and whitelisted targets, never a path read from the file. TrayTrigger notes its snapshot is user-writable and defaults bad tokens safely. Suggest `%ProgramData%\Kestrel` with an Administrators and SYSTEM ACL.
13. **Key on stable identity**: SID, power scheme GUID, DRS setting id and exe name. Store Windows build, driver version and restore-point result (created or reused, with sequence).
14. **Startup drift check.** Re-read everything and show what changed: feature updates can reset settings (O&O's FAQ), and the App or a driver update can change DRS. Offer re-apply, do not silently re-apply.
15. **One apply at a time**: DRS saves are whole-database. Warn if NVIDIA App or Control Panel is open, since its later Save may overwrite (inference).

---

## 11. REAL PC checklist

| # | Check | How |
| --- | --- | --- |
| 1 | Do stock plans exist? Current plan's parking, USB, ASPM | `powercfg /l`, `powercfg /qh` |
| 2 | Does `PowerDuplicateScheme` work from High performance with a fixed GUID? Can active be deleted? | Small test build |
| 3 | Which values Settings writes for captures | Procmon on `GameDVR`, `GameConfigStore` |
| 4 | Does VALORANT register for Game Mode | `gm_effects` |
| 5 | VALORANT `PresentMode` per display mode; absent-token default of windowed optimisation | Kestrel benchmark; Settings versus raw string |
| 6 | VALORANT DRS profile: name, predefined, exe entry | FindApplicationByName + GetProfileInfo |
| 7 | Does Prefer Max take effect on 617.42 | NVML clocks during a CPU-bound run, with and without |
| 8 | NVIDIA App Optimize and driver update effect on DRS values | Re-read after each |
| 9 | Launch VALORANT after DRS writes; any VAN error | Play a Range session |
| 10 | Restore point: with and without COM security; duration; `SPP\Clients` matches Settings; `vssadmin list shadowstorage` | Small test build |
| 11 | Over-the-shoulder case: second admin account; token read; HKU writes; SPI from other account | Needs a standard user account |
| 12 | Value name case for per-app GPU preference | Settings then compare |

---

## 12. Licences of prior art

NVAPI headers MIT. NPI MIT. TrayTrigger MIT. WinUtil MIT. Sophia MIT. ShieldBattery: check before copying. OBS GPL-2.0+. win-config AGPL-3.0 and decompiled pseudocode: use as facts, do not paste. PC Tweaker: all rights reserved. EXM: proprietary with a DMCA history; do not copy, including its plan.

---

## 13. Sources

"Read" means I read the page or file. "Excerpt" means only a search excerpt was visible.

### Windows settings, Game Mode, DVR
- Microsoft, Reference for Windows 11 settings (read): https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11
- Microsoft, ApplicationManagement Policy CSP, AllowGameDVR (read): https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-applicationmanagement
- Microsoft, AppCaptureSettings (read): https://learn.microsoft.com/en-us/uwp/api/windows.media.capture.appcapturesettings
- Decompiled 25H2 twinui.pcshell (read: EvaluateAppForGameMode, IsGameDVREnabled, IsGameDVRGroupPolicyEnabled, RegistryChangeNotificationCallback, constructor, IsGameManager accessors): https://github.com/nohuto/decompiled-pseudocode/tree/main/11-25H2/twinui-pcshell
- win-config docs, Game Mode, Game Bar, mouse, power (read; notes that its analysis is from 23H2): https://github.com/nohuto/win-config
- OBS Studio `libobs/obs-windows.c` (read): https://github.com/obsproject/obs-studio/blob/master/libobs/obs-windows.c
- OBS forum, 2020 (read): https://obsproject.com/forum/threads/game-mode-detection-with-25-0-4-broken.118565/
- Microsoft, HasExpandedResources: https://learn.microsoft.com/en-us/windows/win32/api/expandedresources/nf-expandedresources-hasexpandedresources
- MakeUseOf Game Mode test, 2026-07-12 (read): https://www.makeuseof.com/i-tested-windows-game-mode-for-month-what-benchmarks-actually-showed/
- Windows Forum, Game Mode can hurt CPU-bound FPS (excerpt): https://windowsforum.com/windows-news.4/windows-11-game-mode-can-hurt-cpu-bound-fps-in-some-games.441277/
- NVIDIA System Latency Optimization Guide (read): https://www.nvidia.com/en-us/geforce/guides/system-latency-optimization-guide/
- ElevenForum, Record What Happened (excerpt): https://www.elevenforum.com/t/enable-or-disable-record-what-happened-for-gaming-captures-in-windows-11.17900
- Bob Pony on LTSC popup (excerpt): https://x.com/TheBobPony/status/1881997480864469382

### DirectX and GPU preference
- Microsoft support, Optimizations for windowed games (read): https://support.microsoft.com/en-us/topic/3f006843-2c7e-4ed0-9a5e-f9389e535952
- TrayTrigger `Services/DirectXGlobalSettings.cs` and `Help/tweaks/*` (read): https://github.com/stephenh678/TrayTrigger
- TrayTrigger wiki, VRR for windowed games (read): https://github.com/stephenh678/TrayTrigger/wiki/Variable-Refresh-Rate-for-Windowed-Games
- ElevenForum, windowed optimisations (excerpt): https://www.elevenforum.com/t/turn-on-or-off-optimizations-for-windowed-games-in-windows-11.4943/
- ShieldBattery `app/game/gpu-preference.ts` (read): https://github.com/ShieldBattery/ShieldBattery/blob/master/app/game/gpu-preference.ts
- ninjaOne, GPU preferences (read): https://www.ninjaone.com/blog/restore-gpu-preferences-for-apps-in-windows/
- Microsoft Q&A answer, contradictory (read): https://learn.microsoft.com/en-us/answers/a/6800271
- DXGI EnumAdapterByGpuPreference (read): https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_6/nf-dxgi1_6-idxgifactory6-enumadapterbygpupreference
- PresentMon capture docs (read): https://github.com/GameTechDev/PresentMon/blob/main/README-CaptureApplication.md

### VALORANT and input
- Patch notes 11.06 (read): https://playvalorant.com/en-us/news/game-updates/valorant-patch-notes-11-06/
- Patch notes 11.02 (read): https://playvalorant.com/en-us/news/game-updates/valorant-patch-notes-11-02/
- Patch notes 3.07 (excerpt): https://playvalorant.com/en-us/news/game-updates/valorant-patch-notes-3-07/
- Riot minimum specs (excerpt): https://support-valorant.riotgames.com/hc/en-us/articles/360044136134-Minimum-Recommended-PC-Specs
- Microsoft, High-definition mouse movement (read): https://learn.microsoft.com/en-us/windows/win32/dxtecharts/taking-advantage-of-high-dpi-mouse-movement
- Microsoft, SystemParametersInfoW (read): https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow

### Power
- Microsoft, Power Policy Settings (read): https://learn.microsoft.com/en-us/windows/win32/power/power-policy-settings
- Microsoft, Power Setting GUIDs (read): https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids
- Microsoft, CPMinCores (read): https://learn.microsoft.com/en-us/windows-hardware/customize/power-settings/options-for-core-parking-cpmincores
- Microsoft, Link state power management (read): https://learn.microsoft.com/en-us/windows-hardware/customize/power-settings/pci-express-settings-link-state-power-management
- Microsoft, USB selective suspend GUIDs (read): https://github.com/MicrosoftDocs/SupportArticles-docs/blob/main/Teams/teams-rooms-and-devices/usb-selective-suspend-status-unhealthy.md
- Microsoft, Processor power management options (read via win-config's copy): https://learn.microsoft.com/en-us/windows-hardware/customize/power-settings/configure-processor-power-management-options
- Microsoft, Powercfg options (read): https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/powercfg-command-line-options
- Microsoft, Power slider (read): https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/customize-power-slider
- Microsoft, Windows Server power tuning (read): https://learn.microsoft.com/en-us/windows-server/administration/performance-tuning/hardware/power/power-performance-tuning
- Power API pages (read): https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powerduplicatescheme, https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powerwriteacvalueindex, https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powerwritefriendlyname
- SYSTEM_POWER_CAPABILITIES (read): https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-system_power_capabilities
- WinHelpOnline, only Balanced listed (excerpt): https://www.winhelponline.com/blog/power-options-only-balanced-plan/
- XDA, High performance plan, 2026-10-23 (read): https://www.xda-developers.com/your-high-performance-power-plan-might-be-hurting-your-cpus-gaming-performance/
- XDA, hidden plan (excerpt): https://www.xda-developers.com/this-hidden-windows-11-power-plan-improved-my-games-performance/
- GO-EUC, 2026-09-15 (read): https://www.go-euc.com/beyond-balanced-benchmarking-windows-11-power-plans/
- dtptips (excerpt): https://dtptips.com/windows-high-performance-vs-balanced-power-plan-which-is-better-in-2025/
- HWBusters 9950X3D (read): https://hwbusters.com/cpu/amd-ryzen-9-9950x3d-cpu-review-the-top-multi-core-gaming-processor/13/
- Notebookcheck 9950X3D (excerpt): https://www.notebookcheck.net/AMD-Ryzen-9-9950X3D-Review-Ryzen-7-9800X3D-levels-of-gaming-performance-and-power-efficiency-for-50-markup-over-the-Ryzen-9-9950X.976049.0.html
- AMD partner PDF (excerpt): https://www.amd.com/content/dam/amd/en/documents/partner-hub/ryzen/ryzen-9-9900x3d-9950x3d-how-to-sell-non-competitive.pdf
- Tom's Hardware, 7000X3D install guide (excerpt): https://www.tomshardware.com/news/amd-ryzen-7000x3d-install-guide
- Intel field update (excerpt): https://community.intel.com/t5/Blogs/Tech-Innovation/Client/Field-Update-1-of-2-Intel-Core-Ultra-200S-Series-Performance/post/1650490
- Tom's Hardware, Arrow Lake fix (excerpt): https://www.tomshardware.com/pc-components/cpus/intels-arrow-lake-performance-fix-is-now-available-another-update-coming-next-month
- AnandTech, Alder Lake (excerpt): https://www.anandtech.com/show/16959/intel-innovation-alder-lake-november-4th/3

### NVIDIA
- NVAPI repo: `nvapi.h`, `nvapi_interface.h`, `NvApiDriverSettings.h`, `nvapi_lite_common.h`, `License.txt`, README (read): https://github.com/NVIDIA/nvapi
- NVAPI reference, DRS (read): https://archive.docs.nvidia.com/gameworks/content/gameworkslibrary/coresdk/nvapi/group__drsapi.html
- NVIDIA 617.42 release notes, open issue 6007998 (read): https://us.download.nvidia.com/Windows/617.42/617.42-win11-win10-release-notes.pdf
- Mixed News on the issue's history (excerpt): https://mixed-news.com/en/?p=35384
- NVIDIA, Control Panel retiring, 2026-05-26 (read): https://www.nvidia.com/en-us/geforce/news/007-first-light-geforce-game-ready-driver/
- NVIDIA App release highlights (read): https://www.nvidia.com/en-us/software/nvidia-app/release-highlights/
- NVIDIA App launch article (excerpt): https://www.nvidia.com/en-us/geforce/news/nvidia-app-download-and-features/
- NVIDIA, Manage 3D Settings reference (read): https://www.nvidia.com/content/Control-Panel-Help/vLatest/en-us/mergedProjects/nv3d/Manage_3D_Settings_(reference).htm
- NVIDIA Reflex overview (excerpt): https://www.nvidia.com/en-us/geforce/news/reflex-low-latency-platform/
- NVIDIA support, shader cache (403, excerpt only): https://nvidia.custhelp.com/app/answers/detail/a_id/5735/~/deleting-nvidia-shader-cache-files
- XDA, shader cache default, 2026-09-27 (read): https://www.xda-developers.com/nvidia-had-quietly-taken-40gb-of-my-c-drive-for-a-feature-that-i-was-barely-using-at-all/
- NVIDIA Profile Inspector: `NvapiDrsWrapper.cs`, `DrsSettingsService.cs`, `DrsSettingsServiceBase.cs`, `CustomSettingNames.xml` (read): https://github.com/Orbmu2k/nvidiaProfileInspector
- TrayTrigger: `NvApi.cs`, `IDrsBackend.cs`, `NvidiaGlobalSetting.cs`, `PerformanceProfileService.Nvidia.cs` (read): https://github.com/stephenh678/TrayTrigger
- Zed `directx_renderer.rs` (read): https://github.com/zed-industries/zed/blob/main/crates/gpui_windows/src/directx_renderer.rs
- nvapi-rs (read): https://github.com/arcnmx/nvapi-rs
- TheFPSReview power modes, 2019 (read): https://www.thefpsreview.com/2019/12/04/nvidia-geforce-driver-power-mode-settings-compared/11
- Community reply on bans (excerpt, weak): https://www.answeroverflow.com/m/1040568974217334834
- Blur Busters thread (excerpt): https://forums.blurbusters.com/viewtopic.php?p=99464

### Restore points
- SRSetRestorePointW (read): https://learn.microsoft.com/en-us/windows/win32/api/srrestoreptapi/nf-srrestoreptapi-srsetrestorepointw
- STATEMGRSTATUS (read): https://learn.microsoft.com/en-us/windows/win32/api/srrestoreptapi/ns-srrestoreptapi-statemgrstatus
- CreateRestorePoint (read): https://learn.microsoft.com/en-us/windows/win32/sr/createrestorepoint-systemrestore
- Enable (read): https://learn.microsoft.com/en-us/windows/win32/sr/enable-systemrestore
- SystemRestore class (read): https://learn.microsoft.com/en-us/windows/win32/sr/systemrestore
- Using System Restore, COM security sample (read): https://learn.microsoft.com/en-us/windows/win32/sr/using-system-restore
- Restore points, space and known issue (read): https://learn.microsoft.com/en-us/windows/win32/sr/restore-points
- vssadmin resize (read): https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/vssadmin-resize-shadowstorage
- Microsoft KB, 60 days (excerpt): https://support.microsoft.com/help/5062271
- Point-in-time restore, Learn (excerpt): https://learn.microsoft.com/en-us/windows/configuration/point-in-time-restore
- Point-in-time restore, support (excerpt): https://support.microsoft.com/en-us/windows/experience/backup-recovery/point-time-restore-for-windows
- Tech Community thread (read): https://techcommunity.microsoft.com/discussions/windows11/windows-11-25h2-point-in-time-restore-seems-to-reset-system-protection-to-2/4539095
- Microsoft Q&A, Checkpoint-Computer hangs (excerpt): https://learn.microsoft.com/en-us/answers/questions/83785/checkpoint-computer-hangs-sometimes
- PC Tweaker PRs #253, #349 (read): https://github.com/AurelioAvila/pc-tweaker-app/pull/253, https://github.com/AurelioAvila/pc-tweaker-app/pull/349
- `windows-sys` 0.61.2 bindings (read locally from cargo registry): https://docs.rs/windows-sys/0.61.2/

### Interactive user
- WTSQueryUserToken (read): https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsqueryusertoken
- WTSQuerySessionInformationW (read): https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsquerysessioninformationw
- WTSGetActiveConsoleSessionId (read): https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-wtsgetactiveconsolesessionid
- GetShellWindow (read): https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getshellwindow
- RegOpenCurrentUser (read): https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regopencurrentuser
- Raymond Chen, launching unelevated (read): https://devblogs.microsoft.com/oldnewthing/20131118-00/?p=2643

### Undo prior art
- WinUtil `config/tweaks.json`, `Invoke-WinUtilTweaks.ps1`, `Invoke-WPFundoall.ps1` (read): https://github.com/ChrisTitusTech/winutil
- WinUtil issues and PR (read): https://github.com/ChrisTitusTech/winutil/issues/609, https://github.com/ChrisTitusTech/winutil/issues/254, https://github.com/ChrisTitusTech/winutil/issues/4242, https://github.com/ChrisTitusTech/winutil/issues/487, https://github.com/ChrisTitusTech/winutil/pull/5188
- Sophia Script `src/Sophia_Script_for_Windows_11/Module/Sophia.psm1` (read): https://github.com/farag2/Sophia-Script-for-Windows
- O&O ShutUp10++ Undo history (read): https://manuals.oo-software.com/ooshutup10/en/docs/common-features/undo-history
- O&O ShutUp10++ FAQ (read): https://manuals.oo-software.com/ooshutup10/en/docs/faq
