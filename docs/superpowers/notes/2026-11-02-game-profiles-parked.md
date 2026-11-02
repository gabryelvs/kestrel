# Game profiles (stage 4): parked

Stage 4 (FiveM, Rainbow Six Siege, Warzone) was postponed on 2026-11-02 (decision 85A): the owner has only
VALORANT installed (79) and no friend can test the other games for now (86). Nothing was specced or built. This
note keeps the open questions and the research so the stage can restart from here. The research dates from
2026-10-14; re-check it before use, especially the anti-cheat facts and the Call of Duty file names (MW4 was
due on 2026-10-23).

## Questions still open

Asked on 2026-10-14, parked unanswered. The recommended answer comes first in each.

- **Q80, split the stage.** A: 4a is the multi-game foundation, FiveM and telagem safety; 4b is R6 and Warzone, each
  with its own test build. B: one stage.
- **Q81, what the player does in FiveM's 90-second capture (decision 59).** A: drive the same repeatable loop in the
  same car, which tests the stutter from decision 57. B: stand at a busy spot and turn the camera slowly. C: play
  normally.
- **Q82, FiveM's read-only checks.** A: this list: FiveM or GTA V on a hard drive; under 16 GB of RAM; beta or canary
  update channel; the heaviest GTA settings, as Info (Extended Distance Scaling, Ultra grass, MSAA, high
  population density); ReShade or ENB, as Info; ShadowPlay turned back on in FiveM; leftover V8 DLLs in the Windows
  folder; FiveM on integrated graphics on hybrid laptops. B: also the player-report-only leads, as Info (Vanguard
  linked to FiveM crashes, page file). C: only the first three. An FPS cap belongs with the GTA settings work (58).
- **Q83, check what RP staff look at.** SysMain, Data Usage (DusmSvc), PcaSvc, DPS, BAM, Event Log, Prefetch and
  the Recycle Bin are on, as Windows ships them; anything off shows amber with steps to turn it back on. A: a
  "Telagem" group only on PCs with FiveM, outside the score. B: the same, counted as High. C: no check.
- **Q84, FiveM's cache.** A: Kestrel never suggests deleting anything in FiveM's folders (cache, logs, crashes). B:
  it suggests deleting only the server cache, with a telagem warning.

Still to ask for R6 and Warzone: which benchmark each uses (Siege has a user-started built-in one; Call of Duty's runs
only from the Multiplayer menu and has no Warzone map, so Warzone needs a free capture), and whether Kestrel may
write their settings files (decision 58 left that to their stage).

## Rulings proposed with those questions

- Each game has its own starting point; in FiveM, one per server, named by the player and remembered. Results from
  different servers are never compared.
- In FiveM the player joins the server before starting a benchmark, because FiveM restarts the game on connect.
- The Benchmark tab gets a game picker (games not installed show "Not found"); Health shows one group per installed
  game; Home's Last benchmark card compares with that game's starting point.
- With FiveM installed, rule 61 is on and Health says so in one line.
- The Benchmark tab says, for every game: no known bans, but no official guarantee.
- FiveM for GTAV Enhanced is detected and benchmarked with fewer checks, because official facts are thin.
- Capture finds the game by process-name pattern: `FiveM_b<build>_GTAProcess.exe` (also match `GameProcess`),
  `RainbowSix.exe`, `cod*.exe` confirmed by present events.

## What has to generalise in the code

Everything below is VALORANT-only today (map of 2026-10-14):

- `kestrel-bench` doesn't depend on `kestrel-core`. `Route` models only an F10 timed capture with a switch beep;
  `RUNS` is global; `TooShort` is hard-coded to 85 s and 1,000 frames.
- `ValorantSettings` is threaded through the model, flags, `Platform`, the saved JSON and the UI types;
  `Platform::game_settings()` takes no profile.
- The baseline is global: the first save of any game becomes "Starting point". `storage.valorant` is one snapshot
  field. The `NOT_BACKGROUND` list holds VALORANT and Riot processes only.
- Tests pin one profile (`PROFILES.len() == 1`, `start("fivem")` fails). The UI takes `profiles[0]` with no picker,
  and RouteCard's text is VALORANT-specific. Home's Last benchmark uses `list[0]` against the global baseline.
- Foreground-game detection doesn't exist yet (decision 69, stage 9).

## Research: Rainbow Six Siege and Warzone (2026-10-14)


### Premises that changed
- Siege is D3D12-only: Vulkan removed in Y9S3 (10 Sep 2024), DX11 removed in Y10S1 (4 Mar 2025, "single executable") [2][3]. Steam/Ubisoft call it "Tom Clancy's Rainbow Six Siege" again (PCGW dates the rename-back 3 Mar 2026) [1][16]: key off app ids, not titles.
- Siege anti-cheat is mid-change: Ubisoft's own kernel-level "Ubisoft Anti-Cheat" (Sentinel) shipped with Y11S3 (1 Sep 2026) beside BattlEye [5][6]; Ubisoft plans to drop BattlEye once Sentinel can stand alone [7]. A `Sentinel\UbisoftAntiCheatInstaller.exe` inside the install folder shows it is present [5]. Secure Boot + TPM 2.0 + Memory Integrity are now required for Legend Division [4].
- "Fenris" is Diablo IV, not COD [19]. COD HQ on Battle.net is AUKS [18].
- TPM 2.0 + Secure Boot are required for BO7, Warzone and MW4 (MW4 ships 23 Oct 2026) [20][30].

### Rainbow Six Siege
**1. Install detection**
- Ubisoft Connect: `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>\InstallDir` is what GOG Galaxy's plugin reads (`SOFTWARE\Ubisoft\Launcher\Installs\<launch_id>`, from a 32-bit process, i.e. the WOW6432Node view) [17]. Siege id 635 (Steam edition 1843): from Ubisoft save paths and Steam launch args [1][15]. I could not confirm against a live registry.
- Steam: app 359550, installdir `Tom Clancy's Rainbow Six Siege`, launches `RainbowSix.exe -uplay_steam_mode` [15]. Free Ultra HD Texture Pack DLC 377560 (36 GB) [1][16].
- Epic slug `rainbow-six-siege-x`; Microsoft Store 9N0JJTC4TZV5 (Game Pass Elite; still needs Ubisoft Connect) [1]. Xbox install path not verified.
- The running exe can be a staged copy in `%LOCALAPPDATA%\Ubisoft\r6s\` ("quickboot", `R6Status.ini`; older `...\r6siege\<build>\`) [13][14]. Match by process name, not install dir.

**2. Process names**
- `RainbowSix.exe` is the only game exe since Y10S1. `RainbowSix_BE.exe` is the BattlEye launcher/installer stub (inferred: never presents) [13]. Retired: `RainbowSix_Vulkan.exe` (Y9S3), `RainbowSix_DX11.exe` (existed Y9S3-Y9S4) [2][3][40]. One unverified user report shows a byte-identical `RainbowSixHelper.exe` in Y11S3 [13].

**3. Settings**
- `%USERPROFILE%\Documents\My Games\Rainbow Six - Siege\<ubisoft-user-id>\GameSettings.ini` (Ubisoft Support) [41]. One folder per account: take the newest; resolve Documents through the known-folder API (OneDrive redirects).
- INI, `;` comments. Newest full sample I found is May 2024 (Vulkan era), so re-verify on a D3D12 build [9]: `[DISPLAY]` FPSLimit (0 = off, below 30 = off), NVReflex (0/1 on/2 on+boost), NVReflexIndicator, FSRMode, FSR2Mode. `[DISPLAY_SETTINGS]` WindowMode (0 full/1 windowed/2 borderless), VSync (0/1/2 frames), RefreshRate, Resolution*, MaxGPUBufferedFrame, DefaultFOV. `[CUSTOM_QUALITY]` Texture, TextureFiltering, Shadow, AO, Reflection, AntiAliasing, Sharpness, RenderScalingFactor, AdaptiveRenderScalingTargetFPS, DLSSPerfQual (opaque ints). `[HARDWARE_INFO]` GPUDedicatedMemoryMB, SystemMemoryMB, GPUScore, CPUScore. 2018-19 files had `RenderScaling=<percent>` [10]. Ubisoft says an FPS limit stops the game using more cores than needed (2018 post) [8].

**4. Benchmark**
- An in-game benchmark exists (DSOGaming 2015, PC Gamer 2018, UL "Benchmark mode") [12][42][44]; it is a button in the settings screen (reported greyed out for some Ubisoft-Connect-purchased copies in 2020); users ran it on DX12 in Jan-Mar 2025 [11]. No Ubisoft documentation, duration or output format found. Community only: results go to `Documents\My Games\Rainbow Six - Siege\Benchmark`, and the summary may need the Ubisoft Connect overlay (a guess in the thread) [11]. Reviewers say its scene is lighter than real maps [42][44].
- Treat as user-started; capture with PresentMon, never parse game output.

**5. Low-FPS causes a read-only tool can check**
- SSD required: 65 GB (110 GB for 4K/120 Ultra); RAM 8 GB min/16 GB rec; DX12 GPU [4].
- Texture setting vs VRAM (2018 sizes: Ultra 6 GB, VH 4, H 3, M 2, L 1; re-verify) [42]; compare `Texture` with `GPUDedicatedMemoryMB`.
- Render scale/upscaler, FPSLimit, VSync, MaxGPUBufferedFrame as above.
- DX12 stutter: Ubisoft acknowledged post-Y9S3 DX12 drops and patched across Y9S3.1-.3; no API choice remains [39]. PresentMon 2.6.0 can attribute D3D12 PSO-compile time per frame [31].
- Memory Integrity (HVCI): now required for Legend; about 5% average fps cost in Tom's Hardware's 2021 test, more on CPUs without MBEC [35]. Report it, never advise disabling.
- Ubisoft's own PC checklist (updated 12 Oct 2026): requirements, Windows update, admin rights, verify files, GPU driver, background apps [36].

### Call of Duty / Warzone
**1. Install detection**
- Steam: HQ app 1938090 (installdir `Call of Duty HQ`, runs `bootstrapper.exe cod.exe`). Warzone 1962663 (free) and BO7 3606480 are DLC apps of 1938090, i.e. the same install; MW4 4435490 is DLC-typed too [15][16]. BO6 is now standalone 4384550 (`Call of Duty Black Ops 6`, `bootstrapper.exe cod-cod24.exe`); MWII 3595230 (`cod22-cod.exe`); MWIII 3595270 (`cod.exe`) [15].
- Battle.net: `%ALLUSERSPROFILE%\Battle.net\Agent\product.db` (protobuf: `ProductInstall{uid, product_code, settings.install_path, installed/playable}`; schema in GOG plugin) [17]. Community launch codes: AUKS = Call of Duty HQ (Warzone and BO7 are toggles), BTLR = BO6 (standalone since Jul 2026), NINA = MWII, PNTA = MWIII, ODIN = MW2019, FORE, ZEUS [18]. Default dir `C:\Program Files (x86)\Call of Duty` [43].
- Xbox: Store ids Warzone 9NNFG8BQRCXL, BO7 9PLBM5790L94, BO6 9N58QRBG9WDG, MW4 9MTFTB2WNZFM [30]; install path and package family not verified.

**2. Process names**
- `cod.exe` (HQ: Warzone/BO7/MW4, and MWIII) behind non-presenting `bootstrapper.exe`; standalone `cod-cod24.exe` (BO6), `cod22-cod.exe` (MWII) [15]. Not checked on a live PC: match `cod*.exe` and confirm via PresentMon present events.

**3. Settings**
- Folder: `Documents\Call of Duty\players\` (Steam/Battle.net); Game Pass: `%LOCALAPPDATA%\Activision\Call of Duty\players\<XUID>\`; PCGW and a 2025 guide also list LocalAppData for BO7. Check both, take newest [28][29][30].
- Files (Warzone uses whichever title drives it: BO7 now): MWIII/Warzone-2023 `options.4.cod23.cst`; BO6 `s.1.0.cod24.txt0/.txt1` (double-buffered; read the newer) + `g.1.0.l.txt`; BO7/Warzone-2025 `s.1.0.cod25.txt0/.txt1` (beta: `s.1.0.bt.cod25.txt1`) + `g.p.cod25.1.0.l.txt`. MW4 probably `cod26` (inference). [28][29][45]
- Format: plain text, `// Section` headers; `.cst` lines `Key:0.0 = "value" // range`; `.txt` lines `Key@0 = "value"` [28][29]. Key names (community-documented, confirm on a real file): CapFps, MaxFpsInGame/InMenu/OutOfFocus, VSync, VSyncInMenu, NvidiaReflex, AmdAntilag2, DLSSMode(MP), DLSSPerfMode, DLSSFrameGeneration, FSRFrameInterpolation, XeSSQuality, AATechniquePreferred(MP), ResolutionMultiplier, DynamicSceneResolution, VideoMemoryScale(MP), TextureQuality, VirtualTexturingMemoryMode, WorldStreamingQuality, HTTPStreamLimitMBytes/HTTPStreamUsageLimit (in the profile file), RendererWorkerCount, GPUUploadHeaps, DisplayMode, RefreshRate; read-only: GPUName, DetectedMemoryAmountMB, LastSavedDriveType [28][29]. PCGW: V-Sync options off/100/50/33/25%; separate gameplay/menu/out-of-focus caps [30].

**4. Benchmark**
- Activision advertises a built-in "Call of Duty PC Benchmark Tool" for BO7 [21]. Community wiki: also MWII, MWIII, BO6, MW4; BO6/BO7 via Options > Graphics > Benchmark, only while in the Multiplayer menu, on a scripted MP map (Lowtown/Raid); MWII/MWIII via Private Match [25]. Output is an on-screen summary (avg CPU/GPU fps, bottleneck %, VRAM/RAM, settings). DSOGaming: Multiplayer only [27].
- No Warzone-map benchmark, no CLI, no results file found, duration undocumented. PCGH's BO6 CPU tests used the integrated MP benchmark, measured with CapFrameX over a 20 s window, so PresentMon-based capture during it works in practice [26]. For Warzone, Kestrel needs a free-play capture mode.

**5. Low-FPS causes**
- SSD required (BO7: SSD, 116 GB; HQ listing 161 GB) [21][16]; BO7 RAM 8/12/16 GB; Arc needs Resizable BAR; AVX CPU [21].
- VRAM: community fix for stutter/"packet burst" is lowering `VideoMemoryScale` to what the game really uses [29]. `GPUUploadHeaps=true` with Resizable BAR hurts some GPUs [29].
- On-demand streaming: Activision renamed it "On-Demand High-Quality Streaming" with Optimized (more bandwidth) / Minimal (10 Nov 2025) [24]; community says Off no longer exists and `HTTPStreamLimitMBytes` is the cap [29].
- `RendererWorkerCount` left from an old CPU causes low fps/stutter [29]. Shaders preload only once a 3D scene is up [29].
- Reflex "Enabled + Boost" raises clocks for CPU-bound cases [38]; check caps/VSync keys.
- TPM/Secure Boot do not cost fps per Activision, but missing ones restrict modes [20][22]; Memory Integrity as above [35].

### Anti-cheat and PresentMon / CapFrameX / FrameView (both games)
- Bottom line: ETW-only capture is routine for reviewers and I found no ban or block reports for it in Siege or COD, but no vendor documents it as allowed. The documented friction is session start timing and injection overlays.
- No vendor statement (Ubisoft, BattlEye, Activision) allows or forbids ETW capture. Activision says Ricochet's driver "monitors and reports applications that attempt to interact with protected Call of Duty titles" and lists no banned tools [23].
- Mechanism: PresentMon only consumes ETW events, no injection [31]. CapFrameX captures with a PresentMon build (CX-PresentMon 2.6.1, based on Intel 2.6.0) and its overlay is a separate component (RTSS or a "hook-free" option) [33]. Practice: PCGH benchmarked BO6 (Ricochet) with CapFrameX [26]. I found no ban or block report for ETW-only capture in Siege or COD.
- Real friction found: PresentMon #573, EA AntiCheat returns ACCESS_DENIED to `StartTrace` once the game runs (BF6); fixed in v2.5.0 ("Anticheat Mitigation": session created first, providers toggled later). Latest is v2.6.0 (21 Sep 2026); v2.5.0 binaries were pulled, use 2.5.1+ [31]. Do the same: start the ETW session before launching the game and handle ACCESS_DENIED.
- FrameView: overlay (not capture) problems; "not available with Battlefield 6 anticheat"; legacy injection overlay still exists [32].
- Anecdotes: a 2023 Siege ban blamed on Afterburner/GPU Tweak overlays (unverified) [34]; Ricochet false-positive claims on ReFS system volumes (reportedly fixed by early 2026, no Activision confirmation) [37]. Sentinel plus BattlEye interplay with ETW tools is untested: test on the owner PC.
- PresentMon CLI: `--process_name` (repeatable), `--output_file`, `--timed`, `--terminate_on_proc_exit`, unique `--session_name`, `--track_frame_type` (marks driver-generated frames when DLSS/FSR frame generation is on); needs admin or Performance Log Users [31].
- Activision's own checks for the TPM/Secure Boot requirement: `msinfo32` (BIOS Mode UEFI, Secure Boot State On) and `tpm.msc`; Windows 10 needs 22H2+ [20].
- Design: ETW-only capture, no overlay or injection, and say "no vendor guarantee" in the UI.

### Gaps
Siege benchmark menu/duration/file format; COD benchmark file and duration; Game Pass paths; live exe names; SteamDB pages were Cloudflare-blocked.

### Sources
[1] https://www.pcgamingwiki.com/wiki/Tom_Clancy%27s_Rainbow_Six_Siege (read via its public API)
[2] https://www.ubisoft.com/en-gb/game/rainbow-six/siege/news-updates/seasons/twinshells
[3] https://www.ubisoft.com/en-gb/game/rainbow-six/siege/news-updates/seasons/prepphase
[4] https://www.ubisoft.com/en-gb/help/rainbow-six-siege/connectivity-and-performance/article/system-requirements-for-rainbow-six-siege/000080364 (page also lists "Security requirements", 4 Sep 2026)
[5] https://www.ubisoft.com/en-gb/help/connectivity-and-performance/article/information-about-battleye-anti-cheat/000064020 (lists Ubisoft Anti-Cheat troubleshooting, 25 Sep 2026)
[6] https://news.ubisoft.com/en-us/article/4CHPYPn72TtqMJoYJRUU2m/rainbow-six-siege-community-checkpoint-new-anticheat-measures-balancing-updates-more
[7] https://bo3.gg/r6siege/news/ubisoft-will-launch-its-own-anti-cheat-in-rainbow-six-siege-with-y11s3
[8] https://www.ubisoft.com/en-gb/game/rainbow-six/siege/news-updates/7MPHeuO6hPcy49Mdc3rbDg/100-cpu-usage
[9] https://steamcommunity.com/app/359550/discussions/0/4361250086037392581/
[10] https://steamcommunity.com/app/359550/discussions/0/1636417554425686905/
[11] https://steamcommunity.com/app/359550/discussions/0/2939119747686986096/ , .../2847921219519051382/ , .../715609860235894461/
[12] https://support.benchmarks.ul.com/support/solutions/articles/44002158428-rainbow-six-siege
[13] https://github.com/brandenbailey23/r6-siege-battleye-launch-bug/blob/main/ISSUE_REPORT.md (single unverified user)
[14] https://steamcommunity.com/app/359550/discussions/0/3828663748259564057/
[15] https://api.steamcmd.net/v1/info/<appid> for 359550, 1938090, 1962663, 3606480, 4384550, 3595230, 3595270, 4435490
[16] https://store.steampowered.com/api/appdetails?appids=<appid>
[17] https://github.com/FriendsOfGalaxy/galaxy-integration-uplay and https://github.com/FriendsOfGalaxy/galaxy-integration-battlenet
[18] https://steamcommunity.com/sharedfiles/filedetails/?id=1113049716
[19] https://wowdev.wiki/TACT
[20] https://support.activision.com/tpm
[21] https://www.callofduty.com/blog/2025/11/call-of-duty-black-ops-7-pc-trailer-specs-preloading-handheld-systems
[22] https://www.callofduty.com/blog/2025/08/call-of-duty-black-ops-6-ricochet-anti-cheat-season-05
[23] https://support.activision.com/articles/ricochet-overview
[24] https://support.activision.com/articles/managing-content-in-call-of-duty-games
[25] https://callofduty.fandom.com/wiki/Benchmark_Tool (community)
[26] https://www.pcgameshardware.de/Call-of-Duty-Black-Ops-6-Spiel-74688/Specials/PC-Test-Benchmarks-GPU-CPU-Release-1458561/
[27] https://www.dsogaming.com/pc-performance-analyses/call-of-duty-black-ops-7-pc-performance-analysis/
[28] https://github.com/bradselph/COD-Settings-Changer (community editor; README and source)
[29] https://steamcommunity.com/sharedfiles/filedetails/?id=3217810419 (community guide, updated 5 Oct 2025)
[30] https://www.pcgamingwiki.com/wiki/Call_of_Duty:_Black_Ops_7 , .../Call_of_Duty:_Warzone , .../Call_of_Duty:_Modern_Warfare_4
[31] https://github.com/GameTechDev/PresentMon/releases , .../issues/573 , .../blob/main/README-ConsoleApplication.md
[32] https://www.nvidia.com/en-us/geforce/technologies/frameview/release-notes/
[33] https://newreleases.io/project/github/CXWorld/CapFrameX/release/v1.9.2_beta
[34] https://steamcommunity.com/app/359550/discussions/0/3872588429048262476/
[35] https://www.tomshardware.com/news/windows-11-gaming-benchmarks-performance-vbs-hvci-security
[36] https://www.ubisoft.com/en-us/help/connectivity-and-performance/article/troubleshooting-technical-issues-in-ubisoft-games/000106026
[37] https://habr.com/ru/articles/923190/
[38] https://www.nvidia.com/en-gb/geforce/news/nvidia-reflex-call-of-duty-modern-warfare-warzone-out-now
[39] https://siege.gg/news/rainbow-six-siege-release-new-update-regarding-dx12-performance-issues
[40] https://steamcommunity.com/app/359550/discussions/0/4756452715170991918
[41] https://x.com/UbisoftSupport/status/1933535447679946931
[42] https://www.pcgamer.com/rainbow-six-siege-benchmarks/
[43] https://us.forums.blizzard.com/en/blizzard/t/blzbntbna00000012/40639
[44] https://www.dsogaming.com/pc-performance-analyses/tom-clancys-rainbow-six-siege-pc-performance-analysis/
[45] https://jordantbh.tv/title/blackops6/settings/configuration/

## Research: FiveM and telagem (2026-10-14)

CAVEATS. support.cfx.re, SteamDB and PCGamingWiki are Cloudflare-blocked and Rockstar pages time out, so those are cited from docs links or search snippets only. File and registry facts come from reading the open-source Legacy client (citizenfx/fivem master, Oct 2026). Enhanced client code is not public, so Enhanced facts are thinner and partly forum-sourced (marked unverified). Nothing was written to /Users/gabs/Optimisation.

KEY POINTS
1. Legacy FiveM keeps in-game graphics in %APPDATA%\CitizenFX\gta5_settings.xml (installs since about June 2020), not Documents\Rockstar Games\GTA V\settings.xml [17,24].
2. The frame-presenting process is FiveM_b<build>_GTAProcess.exe; the name changes with the server-enforced game build [16,21,22,23].
3. FiveM for GTAV Enhanced is a separate early-access client (since 21 Jul 2026); pure mode is forced and graphic mods are blocked [8,9,10].
4. Cfx.re's own docs tell users with FPS drops to capture an ETW trace. I found no ban or crash report tied to PresentMon, CapFrameX or FrameView [4,35,36].
5. Telagem-style rules explicitly punish disabled services, edited registry keys, deleted .exe files and disabled Prefetch/Recycle Bin [38].

1. DETECTION WITHOUT LAUNCHING (Legacy)
Install: %LOCALAPPDATA%\FiveM\FiveM.exe plus FiveM.app\ (CitizenFX.ini, data\, logs\, crashes\, plugins\). Running the installer from an empty folder gives a portable install there [2]. FiveM.app\FiveM.installroot exists only for the standard install [13].
Registry, all HKCU [13,14,15]:
- SOFTWARE\CitizenFX\FiveM, value Last Run Location = FiveM.app\ path, rewritten each launch (best locator, covers portable).
- SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\CitizenFX_FiveM (DisplayName, Publisher Cfx.re, InstallLocation, UninstallString); only for the %LOCALAPPDATA% install.
- Protocol: SOFTWARE\Classes\fivem (URL Protocol marker) and SOFTWARE\Classes\FiveM.ProtocolHandler\shell\open\command = quoted FiveM.exe path plus "%1"; also SOFTWARE\FiveM\Capabilities\URLAssociations. Written when the game process first starts, so absent on never-launched installs.
CitizenFX.ini (FiveM.app\CitizenFX.ini) [1,6]: [Game] IVPath (PathCL2 for -cl2), UpdateChannel (production, beta, canary), SavedBuildNumber, DefaultBuild, PoolSizesIncrease, DisableNVSP, EnableFullMemoryDump, DisableOSVersionCheck, DisableCrashUpload; [Addons] ReShade5 override. Shortcut flags: -bNNNN, -pure_N, -cl2.
GTA V path logic [12]: IVPath must hold GTA5.exe. Otherwise FiveM proposes, in order: %ProgramData%\Rockstar Games\Launcher\titles.dat (encrypted JSON, entry gta5); HKLM\SOFTWARE\WOW6432Node\Rockstar Games\GTAV InstallFolderSteam (value ends in \GTAV); HKLM\SOFTWARE\[WOW6432Node\]Rockstar Games\Grand Theft Auto V InstallFolderEpic, then InstallFolderXboxPc under WOW6432Node. A candidate is validated by x64a/b/g.rpf, common.rpf, bink2w64.dll, audio_rel.rpf, GTA5.exe and mpheist3/mptuner/mpsum2 dlc.rpf; otherwise a file picker opens. Community reports Grand Theft Auto V\InstallFolder for Rockstar Launcher copies (unverified). Steam app 271590 = GTA V Legacy, 3240220 = Enhanced [32]; read steamapps\appmanifest_<id>.acf installdir via libraryfolders.vdf [40]. Epic: ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item (GTA V AppName 9d2d0eb64d5c44529cece33fe2a46482, one community source) [40]. FiveM's ROS check also needs the Rockstar Games Launcher (Launcher.exe under Program Files\Rockstar Games\Launcher, seen in logs) [22].
Legacy vs Enhanced: the free Enhanced upgrade shipped 4 Mar 2025; the original became GTAV Legacy, stays supported, and the two have no cross-play; Enhanced needs an SSD and 8 GB minimum, Legacy 4 GB [31]. Documents folders: ...\Rockstar Games\GTA V vs ...\GTAV Enhanced [30]. FiveM was Legacy-only until 21 Jul 2026 (Cfx.re thread, Mar 2025: no ETA, use Legacy) [11]. fivem.net now offers FiveM (needs Legacy) and FiveM for GTAV Enhanced (Early Access, needs Enhanced) [9,10]; most servers still run Legacy. The Legacy client rejects any folder whose name contains V Enhanced and probes the ...\GTAV Enhanced registry keys only to warn [12]. Enhanced client, unverified: %APPDATA%\FiveM for GTAV Enhanced\config.toml with [gta5enhanced] gamepath; logs in the same folder; cache in %LOCALAPPDATA%\FiveM for GTAV Enhanced\gamecache [25,41].

2. PROCESS NAMES
- FiveM.exe: launcher/master; spawns the game process and waits [16]. Not a frame source.
- Game (creates the swap chain, calls Present): FiveM_b<build>_GTAProcess.exe in FiveM.app\data\cache\subprocess\ (older clients: FiveM.app\cache\subprocess). Seen for b2545, b2699, b2944, b3258 [22,23]. Clients before June 2026 omitted the b<build>_ part for the default build; now always present (default 3258, newest in client 3889) [16]. -cl2 adds cl2_. Source builds it from a type string GameProcess.exe while CI symbol aliases and logs say GTAProcess [16,21], so match both.
- Helpers (no frames): FiveM_ChromeBrowser (CEF, no extension), FiveM_ROSLauncher, FiveM_ROSService, FiveM_SteamChild.exe, FiveM_DumpServer.
- The server picks the build (sv_enforceGameBuild) and the client restarts the game on connect for build or pure-mode transitions [6,7], so resolve the name after joining. PresentMon takes exact exe names (--process_name, repeatable); without elevation some processes show as unknown and cannot be targeted by name [37].
- Enhanced: unknown. One 23 Jul 2026 crash report names GTA5_Enhanced.exe as the faulting app [26]; treat as a lead and discover the presenter from unfiltered PresentMon output.

3. SETTINGS FILES
- GTA Legacy: Documents\Rockstar Games\GTA V\settings.xml (use the Known Folder API; OneDrive may redirect). FiveM Legacy: %APPDATA%\CitizenFX\gta5_settings.xml when FiveM.app\crashes was created after about 3 Jun 2020 or citizen\new_vid_behavior.txt exists; the Documents file is copied once on first run [17]. Forum users confirm the path (no staff reply) [24]. FiveM ignores commandline.txt [17].
- Format [27,28]: Settings root with version, configSource, graphics, system, audio, video, VideoCardDescription; each setting is a Key element with a value attribute. Keys: MaxLodScale = Extended Distance Scaling (0-1); LodScale = Distance Scaling; Shadow_Distance = Extended Shadows Distance (0-2); Shadow_LongShadows; UltraShadows_Enabled = High Resolution Shadows; GrassQuality 0-3; ShadowQuality 0-3; ReflectionQuality 0-3; TextureQuality/ShaderQuality/ParticleQuality/WaterQuality 0-2; PostFX 0-3; MSAA and ReflectionMSAA 0/2/4/8; FXAA_Enabled; TXAA_Enabled; SamplingMode = Frame Scaling (0 off, 1-8 = 0.5x-2.0x); SSAO; AnisotropicFiltering; Tessellation; CityDensity = Population Density; PedVarietyMultiplier/VehicleVarietyMultiplier = Population Variety; HdStreamingInFlight; DX_Version (0 DX10, 1 DX10.1, 2 DX11). Video: ScreenWidth/Height, RefreshRate, Windowed (0 fullscreen, 1 windowed, 2 borderless), VSync (0/1/2 half), AdapterIndex, PauseOnFocusLoss.
- Enhanced is the same family: version 34 in a 2025 sample, extra keys (SSAOType, ResScalingType, AAType, TAA_*, fsr/fsr3/dlss Quality, Raytracing_*/RT*, FrameLimit, ReflexMode) [28]. Where FiveM-Enhanced stores its graphics file: not found.
- Other FiveM files: %APPDATA%\CitizenFX\settings.json (main-menu settings), fivem.cfg (console vars, binds), kvs\; FiveM.app\logs\CitizenFX_log_<timestamp>.log; FiveM.app\crashes\ [15]. Streaming memory and Light Client settings: not found in docs, forum or source; closest are console var str_maxVehicleTextureRes (default 1024) and the strmem debug view [5].

4. CAUSES AND FIXES ([R] = Kestrel can check read-only)
- Heavy GTA settings. NVIDIA's guide: Extended Distance Scaling costs the most, Grass Quality is large, Population Density costs 2-3x its benchmark figure in play (75% suggested), MSAA costs performance and VRAM, Reflection MSAA is safe off, Long Shadows about 3 FPS, HD streaming in flight about 4 FPS; Shadow Quality and Population Variety eat VRAM and overflow causes stutter [29]. [R] settings XML plus DXGI VRAM.
- Custom-asset-heavy servers and HDD: forum replies call big drops normal on asset-heavy servers and advise an SSD for the FiveM folder [33]; Enhanced requires SSD [31]. [R] drive type, cache sizes.
- CPU-bound or uncapped FPS: a regular says capping at 120 removed most stutter; VSync reports conflict; Ryzen two-CCD affinity is a guess [33]. [R] VSync/FrameLimit, CPU topology, power plan.
- RAM/page file: official 8 GB minimum (4 may work), 16 GB recommended, 4-8 GB extra for cache [3]; str_maxVehicleTextureRes helps when physical memory runs out [5]; page file/commit limit is a forum lead only [43]. [R] RAM, commit limit.
- Overlays/mods: FiveM disables NVIDIA ShadowPlay per session by default (DisableNVSP=0 stops that; docs say the overlay causes crashes) and blocks the NVIDIA Freestyle DLL (nvppex); ReShade 5.x+ is blocked unless acknowledged in CitizenFX.ini, 5.9.0 outright [1,18,19,20]. sv_pureLevel 1 allows known graphics mods and audio, 2 blocks all modified files; Enhanced early access blocks graphics mods [7,8]. [R] DisableNVSP, reshade/dxgi DLLs, overlay processes.
- Crashes (official [4]): often server-specific (test on the FiveM.net test server); adhesive.dll load error means Windows 10 older than 1703; antivirus blocks (ROS token, stuck on Starting FiveM, Moving of xyz.exe failed); leftover v8*.dll in System32; delete caches.xml; canary/beta unstable [1]. Community only: Riot Vanguard linked to adhesive.dll crashes, Jul-Aug 2026 [34]. [R] Windows build, UpdateChannel, v8 DLLs, third-party AV, vgk service.
- Drivers: Rockstar says update, or revert if the problem followed an update, and verify files [31]. [R] driver version/date. HAGS, Game Mode, Game DVR: no FiveM-specific source.
- Cache: hosting guides (not Cfx) say clear data\server-cache, data\server-cache-priv, data\nui-storage and parts of data\cache, never the game cache (data\game-storage, multi-GB, forces re-download); first load is 2-3 minutes slower; they call it diagnostic, not an FPS boost [18,39]. Cfx docs only name caches.xml, ros_id.dat and %LOCALAPPDATA%\digitalentitlements for specific errors [4]. [R] sizes only.

5. ANTI-CHEAT AND TELAGEM
- Cfx Bans FAQ: automated tripwire for external programs injecting into the client; bans arrive in delayed waves; "Graphic and sound packs or modifications do not cause global bans." [35] Adhesive is not documented further.
- Cfx support asks players to record an ETW trace (UIforETW) for FPS drops [4,36]. PresentMon is ETW-only (no injection, no driver) but needs admin or Performance Log Users membership [37]; Kestrel should not edit groups. CapFrameX installs a kernel driver (WinRing0.sys, replaced by the PawnIO installer in its Oct 2025 beta) [42].
- Telagem (BR rules; Rede Evo and Redeyu are Minecraft networks, same culture): staff connect via AnyDesk and run public scanners (anticheat.ac, Echo) [38]. Punished: disabled pcasvc/sysmain/dps/bam/dusmsvc, altered or deleted registry keys, running or modifying files after a request, deleting an .exe after use, VMs, disabling Prefetch or Recycle Bin, editing logs or temp files, changing permissions to dodge audits; refusing means a ban [38]. Echo says it logs recent program usage and running-process memory [38]. An audit script checks SysMain, PcaSvc, BAM, EventLog, DPS, Schedule, event IDs 1102/104/4616, Prefetch state, USN journal, Recycle Bin, PowerShell history [38]. One BR FiveM server bans VPNs and external programs but allows ENB/ReShade [38].
- Implications: Kestrel must not stop/disable services, touch Prefetch/SysMain/registry, clean temp/recent/logs/USN/Recycle Bin, delete or rename PresentMon or itself, hook or overlay, or advise any of that (generic FPS-booster lists do). Leave its own ordinary traces alone and ship PresentMon unmodified.

6. GAPS
No official source for the Enhanced process name, install folder, graphics file or registry keys; no FiveM-specific evidence for Game DVR/HAGS; Cfx support articles were read via docs repo history or snippets.

SOURCES
[1] https://docs.fivem.net/docs/client-manual/citizenfx/
[2] https://docs.fivem.net/docs/client-manual/installing-fivem/
[3] https://docs.fivem.net/docs/client-manual/system-requirements/
[4] https://docs.fivem.net/docs/support/client-issues/
[5] https://docs.fivem.net/docs/client-manual/console-commands/
[6] https://docs.fivem.net/docs/client-manual/shortcut/
[7] https://docs.fivem.net/docs/server-manual/server-commands/
[8] https://docs.fivem.net/docs/developers/legacy-vs-enhanced/
[9] https://forum.cfx.re/t/fivem-for-gtav-enhanced-is-available-now-in-early-access/5412858
[10] https://fivem.net/
[11] https://forum.cfx.re/t/when-will-gta-v-enhanced-be-supported-in-fivem/5311704
[12-21] FiveM source, base https://github.com/citizenfx/fivem/blob/master/ then: [12] code/client/launcher/GameSelect.cpp; [13] code/client/launcher/Installer.cpp; [14] code/client/launcher/Main.cpp; [15] code/components/glue/src/ConnectToNative.cpp; [16] code/client/shared/CfxSubProcess.h, code/components/sticky/src/LaunchIntercept.cpp, code/client/shared/CrossBuildRuntime.h; [17] code/components/gta-core-five/src/VidBehavior.cpp, BlockLoadSetters.cpp; [18] code/client/launcher/ViabilityChecks.cpp; [19] code/components/rage-graphics-five/src/ReShadeFixups.cpp; [20] code/client/launcher/DisableNVSP.cpp; [21] code/tools/ci/psm1/cfxUploadClientSymbols.psm1
[22] https://forum.cfx.re/uploads/short-url/AfZkc5bwkIX9pwrDE9ISlCAz3WW.log (Sep 2026 CitizenFX log, b3258)
[23] https://forum.cfx.re/t/limit-your-fps-quickly-and-easily-nvidia-amd/4824400
[24] https://forum.cfx.re/t/fivem-is-not-following-or-using-settings-xml/1369432 ; https://forum.cfx.re/t/my-fivem-graphics-wont-apply-to-game-even-though-they-show-in-the-settings/4823770
[25] https://forum.cfx.re/t/how-to-change-the-default-game-folder-from-steam-to-epic-games-in-fivem-for-enhanced/5415959
[26] https://forum.cfx.re/t/weird-bug-crash/5416252
[27] https://fivelaunch.help/reference/gta-settings/
[28] https://github.com/gearguy365/gta5_config_files/blob/master/settings.xml ; https://pastebin.com/AuRPSWEL (Enhanced-format sample, anonymous)
[29] https://www.nvidia.com/en-us/geforce/news/grand-theft-auto-v-pc-graphics-and-performance-guide/
[30] https://www.corsair.com/ww/en/explorer/gamer/gaming-pcs/how-to-benchmark-gta-v-enhanced/
[31] https://www.guru3d.com/story/rockstar-games-releases-free-upgrade-for-grand-theft-auto-v-on-pc/ ; https://support.rockstargames.com/articles/lMQXeP2Z1mN3g9oZiBZFR/grand-theft-auto-v-pc-system-requirements ; https://support.rockstargames.com/articles/1yIMMXYwArdpjqwe6RDItY/gtav-pc-general-troubleshooting-installation-issues-crashes-freezes-poor-performance
[32] https://store.steampowered.com/app/271590/ ; https://store.steampowered.com/app/3240220/
[33] https://forum.cfx.re/t/fps-drops-on-a-good-pc/5218443 ; https://forum.cfx.re/t/fivem-low-performance-with-mid-range-pc/5304026 ; https://forum.cfx.re/t/fivem-when-i-drive-i-get-fps-drops/2081685
[34] https://forum.cfx.re/t/fivem-crashing/5422257 ; https://forum.cfx.re/t/consistent-crashing-at-menu-adhesive-dll-b59672/5418561
[35] https://github.com/citizenfx/fivem-docs/blob/c5c7a97645fed2e64574c71ff69de2802158de2c/content/docs/support/ban-faq.md (last docs version) ; live: https://support.cfx.re/hc/en-us/articles/8444465475356-Bans-FAQ
[36] https://support.cfx.re/hc/en-us/articles/8366604193436-Creating-an-ETW-trace
[37] https://github.com/GameTechDev/PresentMon/blob/main/README.md ; https://github.com/GameTechDev/PresentMon/blob/main/README-ConsoleApplication.md
[38] https://wiki.rederevo.com/regras/screenshare ; https://wiki.redeyu.com/regras/anti-jogo ; https://github.com/tqmgxd/loghunter ; https://forum.eclipse-rp.net/topic/172222-echo-anti-cheat-pc-scans/ ; https://forum.ad-rp.com/index.php?%2Fregrasfivem%2F=
[39] https://shockbyte.com/help/knowledgebase/articles/how-to-clear-the-cache-on-your-fivem-client ; https://fatduckgaming.com/server-docs/troubleshooting/how-to-clear-fivem-cache
[40] https://unpkg.com/@ciberus/find-steam-app@4.0.0/README.md ; https://jayd.ml/games/2020/05/16/epic-games-store-steam-libraries.html
[41] https://gist.github.com/Amzi-01/7ad2ea3f15c4aa2e855c03c71575139a (Wine write-up of the Enhanced client)
[42] https://github.com/CXWorld/CapFrameX/releases/tag/v1.7.7beta
[43] https://forum.cfx.re/t/out-of-game-memory/74674 (snippet-level only)
