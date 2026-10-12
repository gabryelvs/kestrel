# Kestrel stage 2 — benchmark design

Agreed with the owner on 2026-10-10 to 2026-10-12 (decisions 36–66, see the main spec
`2026-10-09-kestrel-design.md`). Stage 2 ships as the v0.2.0 test pre-release.

## Goal

Measure VALORANT's smoothness and input delay with a repeatable routine, save the result as the
owner's **starting point**, and compare every later result against it, telling real changes from
noise. The capture engine is built for any game; VALORANT is its first and only profile in stage 2.

## Decisions this stage implements

| # | Decision |
| --- | --- |
| 36 | "Starting point" = the PC as it is now (EXM's tweaks included), i.e. before Kestrel's tweaks |
| 37 | A run starts with a start key in the game; beeps mark start and end |
| 38 | One benchmark = 3 runs; the result is the middle value of each metric, with the spread shown |
| 39 | Average FPS, 1% low, 0.1% low, frame-time stability, PC latency where reported, CPU- or GPU-bound verdict |
| 40 | Each run's summary and every frame time are kept until the owner deletes them |
| 41 | Results compare against the starting point by default (any run can be picked); each change is called "real" or "within noise" |
| 42 | The first run offers "Run your starting-point benchmark now" / "Later" |
| 43 | VALORANT's video settings are read (never written) and saved with each run; comparisons warn when they differ |
| 44 | Untrustworthy runs are flagged "not comparable" with the reason |
| 45 | "Copy results", and benchmark summaries in Copy diagnostics |
| 46 | Capture with Intel PresentMon 2.6.0, bundled at a pinned version; it opens a query-only handle to the game (owner approved) |
| 47 | Input delay = NVIDIA Reflex "PC latency" when the game reports it, else "not reported" |
| 48 | Route v1: 30 s walking loop + 60 s on strafing bots (bot count 5, Vandal); route version recorded |
| 49 | One unrecorded warm-up pass before the 3 runs |
| 50 | No free capture in stage 2 (FiveM gets one in the multi-game stage) |
| 55 | Built on game profiles; VALORANT is the first profile |
| 64 | Visual redesign moved to the last stage (owner designs it in Claude Design); stage 2 uses the current look |

## What the owner sees

1. **Benchmark tab** (replaces "Arrives in stage 2"): Route v1 steps, results list, **Start benchmark**.
2. **Start benchmark** arms a session. Kestrel waits until VALORANT is running (it checks the process
   list every 2 s) and says: "Open the Range, stand at the start, press F10."
3. **Session** = warm-up + runs 1–3. Each starts on F10:
   - one long beep → capture starts (90 s);
   - one short beep at 30 s → switch from the walking loop to the bots;
   - two short beeps at 90 s → run done; walk back to the start for the next one.
   The Kestrel window shows the stage ("Warm-up", "Run 2 of 3"), a countdown and each run's status.
   A flagged run shows its reason with **Redo run N** and **Keep anyway**. **Cancel** stops the session.
4. **Result**: the middle of the 3 runs for each metric, the spread, the bottleneck verdict, a
   frame-time graph, the context (power plan, RAM speed, driver, Windows build, VALORANT settings).
5. **Comparison** against the starting point: per metric, the difference in % and "real change" or
   "within noise"; a warning when VALORANT's settings differ between the two.
6. The first saved benchmark is named "Starting point" and becomes the baseline. Any result can be
   renamed, deleted or set as the baseline.
7. **Home** "Last benchmark" card: latest result vs the starting point (average FPS, 1% low), or
   "No benchmark yet" with **Run starting point**.
8. **First run**: after "Here's what Kestrel found", a step offers the starting-point benchmark now or later.
9. **Copy results** copies a plain-text summary; Copy diagnostics includes all benchmark summaries.

### Route v1 (shown in the app; the version is saved with every run)

Before the warm-up: open VALORANT → Practice → The Range. Take a Vandal from the weapon wall. In the
bot settings choose **Strafe** and bot count **5**.

Each pass (warm-up and runs 1–3):
1. Stand on the spawn platform facing the range. Press **F10** (long beep).
2. 0–30 s: walk straight down the main path to the bot area without stopping, looking ahead.
3. At the short beep (30 s): start the strafing bots and shoot them with the Vandal.
4. At the double beep (90 s): stop and walk back to the spawn platform.

The owner tries Route v1 during the stage-2 test; any wording change to the steps that changes what
the player does becomes Route v2, and results from different route versions are never compared.

## Measurements

PresentMon writes one CSV row per frame. Kestrel parses it by header name (the column set changed
across 2.x), with a UTF-8 BOM and `NA` for missing values. Frame time = `MsBetweenPresents`.

| Metric | Definition |
| --- | --- |
| Average FPS | 1000 / mean frame time |
| 1% low | 1000 / mean of the slowest 1% of frame times (NVIDIA FrameView's definition) |
| 0.1% low | 1000 / mean of the slowest 0.1% of frame times |
| Stutters per minute | frames slower than 2× the median frame time, per minute of capture |
| PC latency | mean of `MsPCLatency` over frames that have it; "not reported" when no frame has it |
| Bottleneck | median over frames of `MsGPUBusy / MsBetweenPresents`: ≥ 0.90 GPU-bound, ≤ 0.75 CPU-bound, otherwise mixed |

A benchmark's value for each metric is the median of its 3 runs; the spread is (max − min) / median.

**Real change or noise:** comparing result B with baseline A, a metric is a **real change** when all
three of B's runs are better than all three of A's (or all worse); otherwise it is **within noise**.
The difference shown is between the two medians, in %.

### When a run is flagged (decision 44)

| Flag | Rule |
| --- | --- |
| Too short | fewer than 85 s of frames, or fewer than 1,000 frames |
| Not the game in front | more than 5% of frames presented as "Composed" (windowed, overlay, alt-tab) |
| FPS cap or V-Sync | VALORANT reports V-Sync on or a frame-rate limit, or frame times are flat (5th and 95th percentile within 5% of each other) |
| Dropped data | PresentMon reports lost events or buffer overflow |
| Busy background app | right after the run, a non-game process used ≥ 10% CPU (Kestrel's existing background sampler) |

A session completes with 3 unflagged runs, or flagged runs the owner chose to keep (marked in the result).

## VALORANT settings (read-only, decision 43)

Files: the most recently written `GameUserSettings.ini` and `RiotUserSettings.ini` under
`%LOCALAPPDATA%\VALORANT\Saved\Config` (one folder per Riot account). Keys confirmed on the owner's PC
are listed in `docs/superpowers/notes/2026-10-12-valorant-settings-keys.md`. Kestrel records
resolution, fullscreen mode, V-Sync, frame-rate limit, Reflex mode, multithreaded rendering and the
quality settings. A key that is absent means "default", never an error. The files are never written.

## Architecture

- **`crates/kestrel-bench`** (new, pure Rust, tested on macOS): game profiles, PresentMon CSV parsing,
  metrics, run flags, the middle-of-three result, comparison verdicts, VALORANT settings parsing,
  the saved-data model and the plain-text summary for Copy results.
  - `GameProfile { id, name, process_name, route }`; one entry: VALORANT
    (`VALORANT-Win64-Shipping.exe`, Route v1).
- **`crates/kestrel-probe`**: `game_running(process_name)` from a Toolhelp32 process snapshot (no
  process handle is ever opened); the existing background-CPU sampler reused after each run.
- **Tauri app** (`app/src-tauri`): the session controller (state machine Idle → Armed → Capturing →
  … → Done), F10 through the official global-shortcut plugin (registered only while a session is
  armed), beeps through Windows `Beep`, PresentMon as a hidden child process, storage, commands and
  events. A `Capture` trait with a real PresentMon implementation and a fake used by tests.
- **UI** (`app/src`): Benchmark page, Home card, first-run step, Copy results — in the current style.

### PresentMon

- Version 2.6.0, asset `PresentMon-2.6.0-x64.exe` (980,320 bytes), SHA-256
  `b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af`, MIT licence.
- `app/scripts/fetch-presentmon.mjs` downloads the exe and its `LICENSE.txt` from the v2.6.0 release at
  build time and refuses a file whose SHA-256 differs. CI runs it before `tauri build`; nothing is
  downloaded on the user's PC.
- Bundled through `app/src-tauri/tauri.windows.conf.json` (merged on Windows builds only, so macOS
  development builds don't need the file) as resources `presentmon/PresentMon.exe` and
  `presentmon/LICENSE.txt`. Toolbox gets an "About Kestrel" section listing PresentMon and its licence.
- Each capture runs:
  `PresentMon.exe --process_name VALORANT-Win64-Shipping.exe --output_file <run>.csv --timed 90
  --terminate_after_timed --session_name KestrelBench --stop_existing_session --no_console_stats
  --set_circular_buffer_size 8192 --track_pc_latency`, hidden, with stderr captured.
- Exit code 6 (session name in use) → run once with `--terminate_existing_session --session_name
  KestrelBench`, then retry the capture once.
- A marker file exists while a capture runs. If Kestrel starts and finds it (a crash mid-run), it
  terminates the `KestrelBench` session first.

### Saved data (decision 40)

`%APPDATA%\io.github.gabryelvs.kestrel\benchmarks\`:
- `index.json`: the list of benchmarks and the baseline id.
- `<id>\summary.json`: name, created-at, profile id, route version, Kestrel and PresentMon versions,
  per-run metrics and flags, the result, and the context snapshot (power plan, RAM speed, GPU and
  driver, Windows build, primary display Hz, VALORANT settings).
- `<id>\run-<n>.bin`: every frame time of run n as little-endian f32 (about 0.2 MB per run).

Files are written to a temporary name and renamed, so a crash never leaves a half-written file.
Deleting a benchmark removes its folder. Temporary CSVs are deleted after parsing.

### Commands and events

`bench_profiles`, `bench_start(profile)`, `bench_cancel`, `bench_redo(run)`, `bench_keep(run)`,
`bench_list`, `bench_get(id)`, `bench_rename(id, name)`, `bench_delete(id)`,
`bench_set_baseline(id)`, `bench_compare(a, b)`, `bench_copy_results(id)`; event `bench-state` on every
state change and once a second while capturing. `get_diagnostics` adds benchmark summaries.

## Errors

| Situation | What happens |
| --- | --- |
| VALORANT isn't running | Armed state waits with "Start VALORANT and open the Range" |
| F10 can't be registered | Fallback: **Start run** in Kestrel, then a 10-second beep countdown |
| PresentMon missing or fails to start | Session stops with the error and "Copy diagnostics" |
| No frames captured | The run is flagged "Too short" with Redo |
| Kestrel closes mid-session | The session is discarded; the stale capture session is cleaned up at next start |
| AMD or Intel graphics | PC latency shows "not reported" |
| Saving fails | The error is shown; nothing outside Kestrel's folder is touched |

## Testing

- `kestrel-bench` unit tests: CSV parsing (BOM, `NA`, missing and reordered columns, a real PresentMon
  2.6.0 header), each metric on synthetic frame times with known answers, every flag rule, the
  middle-of-three result and spread, the real-or-noise verdict, settings parsing on fixture files
  (absent keys = default), save and load round trip, the Copy results text.
- Session controller tests with the fake capture: warm-up → 3 runs → result; redo; keep anyway;
  cancel; game not running; capture failure.
- UI unit tests for the comparison and formatting helpers; the browser preview shows sample
  benchmarks so the page can be checked on the Mac.
- CI (Windows job): fetch PresentMon with the hash check, run a 3-second capture of `dwm.exe` and
  assert the CSV has PresentMon's header, and check that the build output has
  `target/release/presentmon/PresentMon.exe` beside `Kestrel.exe`.

## Rules for stage 2 (changes from stage 1 in bold)

- **Kestrel may write files, but only inside its own app-data folder.**
- **It runs the bundled PresentMon only during captures; PresentMon opens a query-only handle to the
  game and creates an ETW session that ends with the capture.**
- **It registers F10 only while a benchmark session is armed.**
- Unchanged: never read or write a game's memory, never inject, never load a kernel driver, never
  start, stop or change Vanguard; no network calls at runtime except opening links; no telemetry;
  VALORANT's settings files are never written.

## What the owner tests (tracker checklist for v0.2.0)

- Install v0.2.0; the Benchmark tab shows Route v1
- Start a benchmark; F10 starts each pass and the beeps are audible in the game
- Warm-up and 3 runs complete; numbers are close to VALORANT's own FPS counter
- PC latency shows a number (Reflex On)
- The result says CPU- or GPU-bound
- Results are still there after closing and reopening Kestrel
- Copy results, then paste the text in chat
- Route v1's steps were clear (note anything to change)
- Nothing outside Kestrel's folder changed

## Not in stage 2

Visual redesign (last stage), other games and FiveM free capture (stage 4), Hardware & BIOS
(stage 3), tweaks (stages 5–6), Lock In (stage 7).
