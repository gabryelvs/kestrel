# Stage 1 review backlog

Findings the stage 1 reviews deferred to later stages. v0.1.0 ships without them; none breaks the
read-only guarantee. Stage 2 planning should start from this list. Recorded 2026-10-10.

## Security hardening

- Call `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR)` once at
  start-up. nvml.dll now loads by full System32 path, but this closes the whole DLL-planting class for the
  elevated app (nvml.dll's own imports included).
- Narrow `opener:default` (any http/https/mailto/tel plus reveal-in-folder) to an https host list, and open links
  through the user's unelevated shell so a browser that isn't running doesn't start elevated.
- Move the CSP's `'unsafe-inline'` to `devCsp` if production doesn't need it.

## Health Check accuracy

- A wholesale probe failure scores 100 ("Nothing to fix") on Home; show the probe notice on Home and on the
  first-run result too.
- Monitors on a DisplayLink dock or on the Microsoft Basic Display driver are classed virtual, so they get
  "a virtual display is active" and skip the refresh check.
- Firmware that reports `ConfiguredClockSpeed` in MHz (half the MT/s) could raise a false XMP alarm on a
  recognised kit; treat configured × 2 ≈ rated as Unknown.
- The TPM Unknown text blames missing admin rights even when Kestrel is elevated and the query failed for
  another reason.
- A failed battery query on a PC whose `PCSystemType` is 0 or missing counts as Desktop, and on-battery then
  passes as "Running on mains power"; `FormFactor::Unknown` would be honest.
- Drive-space rounding can read "50 GB free… keep at least 50 GB" just under a limit.
- Laptops' Windows 11 settings page is "Power & battery", not "Power".
- A custom power plan whose name can't be read shows "Custom power plan: Custom plan".
- display-refresh names the first slow display in enumeration order rather than the primary; its Unknown text
  says no monitors were reported when monitors exist at 0/1 Hz; unreadable displays are dropped when another is
  readable; an empty display list gives Pass in virtual-display and valorant-gpu.
- Unknown form factor plus hybrid GPUs gives Unknown even when a Windows GPU preference is set.
- "VALORANT may run on the integrated graphics" is High on hybrid laptops with no Windows preference, although
  NVIDIA's Optimus profile usually picks the dGPU; judge from friends' diagnostics.
- Network checks disagree on which adapters count (adapter-power includes Other); link-speed says "No cable
  connected" when the adapter list is empty or unreadable; EEE is flagged on most Realtek/Intel cards (Low,
  revisit in stage 4).
- `startup_folder_count` ignores `StartupApproved\StartupFolder`, so disabled shortcuts still count.
- `firmware::iommu_enabled` can't say "couldn't tell"; VBS/Hyper-V might hide the DMAR table (unverified).
- valorant.rs doesn't strip YAML comments or unescape `\\`; `game_exe_path("")` is unguarded.
- `main_gpu` ties depend on order and unknown VRAM counts as 0; prefer known vendors over Other (QEMU QXL);
  the Intel Arc regex misses "Arc(TM) Pro A60/B50".
- Dual-socket (`CPU1 DIMM A1`) and underscore (`P0_ChannelA`) channel labels still alias to single channel.
- Excluding Kestrel by exe name also hides other apps' msedgewebview2.exe and wmiprvse.exe load.
- A standard user who elevates with an admin's password makes HKCU and LocalAppData the admin's, so per-user
  checks read the wrong settings; stage 3 must handle this before writing per-user settings.

## BIOS guide

- The ASUS laptop page shows enterKey F2 but step 1 says "press Del".
- Unknown-maker update copy reads awkwardly; "Advanced mode" vs "Advanced Mode"; the update caution omits the
  Gear 1 reset on 11th gen; SMBIOS placeholder board names ("To Be Filled By O.E.M.") are shown as-is.
- ROG Strix boards may also say "Extreme Tweaker" (unconfirmed); no `Fix::BiosGuide` points at MemoryGear or
  ResizableBar, so those sections are reachable only by scrolling.

## Probes and runtime

- Scan timeout: a hung WMI provider leaves "Scanning…" forever with no report and no Copy diagnostics.
- `collect()` uses `thread::spawn` (can panic; use `Builder::spawn` and stamp `collected_at` in the fallback);
  a panic in one probe discards all data; `rows_in` reconnects per call.
- GPU % sums every adapter while temperature is NVML device 0; no temperature when the GPU Engine counter is
  missing; an empty sum gives -0.0.
- Measure the cost of 1 Hz PDH and NVML polling while Kestrel stays open behind the game; check whether NVML
  polling wakes an Optimus dGPU.
- Confirm with Process Explorer that perfproc.dll opens no VALORANT handle while the game runs.
- Priming the load sampler in `setup` may delay first paint (measure); there is no in-flight scan dedupe; a
  panic inside `sample()` would poison the load Mutex.
- The app crate lacks `[[bin]] test = false`, so `cargo test --workspace` from a non-elevated Windows shell hits
  the requireAdministrator manifest (os error 740).
- WebView2's own network traffic isn't Kestrel's code, but the "no network calls" promise should say so.

## UI

- Accessibility pass: focus drops to `<body>` after the first-run buttons and when Toolbox focuses a section;
  the scanning line's `aria-live` updates every 450 ms; the failure message lacks `role="alert"`; the first-run
  branch has no `<main>`; Health status icons are `aria-hidden` with only a title; the window buttons' focus
  ring falls off-screen; smooth scrolling ignores reduced motion.
- The Copy-diagnostics toast is in flow and pushes the footer for 3 s; a same-text second click doesn't restart
  its timer; its `role="status"` mounts with its text.
- A persistent `get_load` failure leaves "…" forever with nothing logged.
- FindingRow and CheckRow have no exhaustive switch over `Fix` kinds, so a new kind would render nothing.
- First-run result spacing ("86 /100 3 fixes found"), an empty list on a perfect score, and title-bar tabs that
  look clickable during the first run.
- `types.ts` `SystemSnapshot` is a subset of the Rust snapshot; the TS `mainGpu` tie-break and `driverLabel`'s
  missing >9999 guard differ from Rust.
- `npm run mock` uses the clock: regenerating after about 2027-03-18 makes gpu-driver-age a problem and breaks
  findings.test.ts.
- The sample report is bundled into production (14 KB); a few hex colours bypass the tokens; `.small` lives in
  home.css; the score's 20 segments are hard-coded in three places.

## CI and tooling

- The probe step's `Tee-Object` decodes the JSON with the console code page; set
  `[Console]::OutputEncoding` to UTF-8 first.
- `lts/*`, `ubuntu-latest` (Ubuntu 26 from 2026-10-19) and `stable` with `-D warnings` float; pin when one bites.
- Add clippy for the app crate after `tauri build`; add a tag-to-version guard to release.yml; a failed asset
  upload leaves an empty published pre-release to delete by hand.
- Crate manifests don't inherit `repository.workspace`; LICENSE may have a trailing blank line.

## Tests to add

- `fixture_loses_nothing_when_parsed`; RAM regex negative tests and speed-bin snap boundaries; GPU id/name
  fallbacks; the health arms and boundaries listed above; format.ts branches; first-run gating (needs jsdom);
  a `tauri::test` of the command surface.
