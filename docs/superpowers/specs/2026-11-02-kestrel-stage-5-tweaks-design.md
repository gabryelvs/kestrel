# Kestrel stage 5 — Tweaks design

Written on 2026-11-02 from the owner's answers (decisions 85–89 and earlier ones, see the main spec
`2026-10-09-kestrel-design.md`) and from the research in
`docs/superpowers/notes/2026-11-02-stage-5-tweak-research.md`. Stage 5 ships as the v0.5.0 test pre-release; stage 4
is postponed, so there is no v0.4.0.

## Why this stage exists

The owner wants VALORANT to run smoother and faster on their PC without putting that PC at risk. Stages 1–3 only read
the PC. Stage 5 is the first stage that changes it, so its real product is trust: every change is shown first, recorded
before it's made, and can be undone, leaving the PC as it was.

The research was blunt about the Essential tier itself: on a PC already tuned by another tool it will barely move the
benchmark. Game Mode and background recording are usually already right, the power plan is worth 0–3% at best, and
nothing else in the tier raises FPS in a CPU-bound game. For scale, a screen share cost the owner 11–22% in their first
benchmark. So Kestrel never promises a gain: it says what each tweak does, and the benchmark judges. The tweaks more
likely to move FPS come in stages 6 and 7, on top of this engine.

## Goals

- A **Tweaks** tab where a person sees the Essential tweaks picked for their PC, why each was picked, and each one's
  state; ticks what they want; sees every change in a **Preview**; and only then presses **Apply**.
- An undo that works one item at a time or for everything, and never overwrites something it didn't write without
  asking.
- A record that survives crashes: Kestrel writes down the old value before each change.
- A restore point before changes, reported truthfully.
- A **Kestrel report** listing every change.
- An uninstaller that asks whether to undo Kestrel's changes.
- Benchmarks that remember which tweaks were active, so a comparison says what changed between two results.

**Success on the owner's PC:** they tick Kestrel's plan, read the Preview, apply, see it in Windows, undo it and find
their EXM plan active again, with nothing left behind. On GitHub, a fresh Windows machine applies, checks and undoes
every tweak, and the values read afterwards equal the values read before.

## Non-goals

- The NVIDIA profile: it moves to stage 6 (decision 90).
- The Competitive and Extreme tiers, network tweaks, debloat (stage 6). FiveM detection and rule 61 also arrive before
  stage 6: nothing in this tier stops a service, disables one or cleans traces.
- Lock In (stage 7), auto-update (stage 8), the new look (stage 9). The tab uses the current look, as the Hardware tab
  did. The owner may design it in Claude Design later from a brief.
- Undoing what other tools changed. Kestrel's undo covers only Kestrel's own changes.
- Game profiles (stage 4, postponed).

## Anti-goals

Failure even if everything else works:

- Anything changes before Apply is pressed.
- A value Kestrel didn't write gets overwritten without asking, including when something else changed a setting after
  Kestrel did.
- Kestrel re-applies a tweak on its own.
- The app implies a gain it didn't measure.
- Kestrel writes a `Policies` key, runs `powercfg /restoredefaultschemes` (it would delete the owner's EXM plan),
  deletes a plan it didn't create, or copies anything from EXM.
- Kestrel stops or disables a Windows service, cleans traces, touches Vanguard, or opens a handle to the game. It finds
  the game by name in a process list, as today.
- Turning off Game Bar's manual recording or screenshots: background recording goes off, Game Bar stays.

## Decisions this stage implements

| # | Decision |
| --- | --- |
| 3 | Tiers: Essential, Competitive, Extreme; each tweak explained, impact-labelled, reversible. Stage 5 builds Essential |
| 4 | Smart Plan: picks tweaks for the hardware and explains why |
| 15 | Undo journal per change, Revert everything, restore point, crash recovery |
| 21 | Home's top fixes get buttons; Apply plan leads to the plan |
| 22 | First run ends with the proposed plan, Preview, restore point, then apply or Not now |
| 28 | Windows 10 22H2: Windows 11-only tweaks hidden |
| 30 | The tweak list, corrected by the research (see "The Essential tier") |
| 61 | The Kestrel report lists every change, for everyone |
| 85 | Stage 4 is postponed; stage 5 is next |
| 87 | Another tool's power plan: Kestrel's plan is offered unticked, the Preview shows what would change, Kestrel suggests measuring before and after, and undo returns to the old plan |
| 88 | The uninstaller asks whether to undo Kestrel's changes |
| 89 | System Protection off: ask at that moment (turn it on and create the point, recorded and undoable; or go on with undo only) |
| 90 | **Kestrel's call, approved with this spec:** the NVIDIA profile moves to stage 6. It is the riskiest item: it writes to the driver's database, GitHub's machine has no NVIDIA card, and the gain is small on a CPU-bound PC (NVIDIA lists "Prefer maximum performance" as an open issue in driver 617.42). The 10 GB shader cache is dropped for good: the current driver's default is 16 GB, so 10 GB would lower it |

## What a person sees

### Tweaks tab

Replaces the "Arrives in stage 5" page. Top to bottom:

- **Intro:** "Kestrel picked these for your PC. Nothing changes until you press Apply."
- **Measure first:** a note, with a button to the Benchmark tab, in either of these cases. It never blocks Apply.
  - No starting point yet: "Run a benchmark first, so you can compare afterwards."
  - The starting point's runs vary by more than 10% (the owner's case): "Your starting point's runs vary by 24.9%, so
    small changes won't show. Run a clean one before applying." The Benchmark tab's **Use as baseline** makes the
    new run the starting point.
- **Another account:** when Kestrel runs as a different account from the signed-in one (see "Which account gets the
  changes"): "Kestrel is running as another account. Changes go to the account signed in now."
- **Essential panel:** one row per tweak: a tick box when it can be applied, its title, a state tag, an impact tag, and
  one line saying why it was picked or why it isn't needed.
- **Competitive and Extreme:** one line each, "Arrives in stage 6".
- **Preview** button with the number of ticked items, and **Revert everything** when any change is active. When Apply
  can't run, the button is off and says why: "Close VALORANT first." or "Finish the benchmark first."
- **Kestrel report** panel.

Row states:

| State | Meaning | Controls |
| --- | --- | --- |
| Available | Can be applied | Tick box |
| Already set | Already at the target; Kestrel records nothing and never undoes it | None |
| Not needed | Doesn't apply to this PC, with the reason | None |
| Blocked by policy | An organisation policy decides it; Kestrel reports and doesn't fight it | None |
| Applied | Kestrel changed it and it still holds | **Undo** |
| Changed by something else | Kestrel changed it, and it now holds a third value: neither Kestrel's nor the old one | **Apply again**, **Restore anyway** |
| Kestrel's plan isn't active | Power plan only: Kestrel's plan exists but another plan is active | **Use it again**, **Remove it** |
| Couldn't read | The read failed; a failed read is never treated as "absent" | None |

### Preview

Lists each ticked change: what it is in plain words, the technical location (registry value or plan setting), the
value now and the value after, and when it takes effect ("Now", "Next time VALORANT opens", "Next sign-in"). The power
plan shows the current plan's value beside Kestrel's for each setting Kestrel sets, and says: "Switching plans also
replaces everything else in [current plan] with the settings of [base plan]. Undo returns to your plan as it was."
**Apply** and **Cancel**. A Home fix button opens this Preview with only that tweak.

### Apply

1. Refused while VALORANT is running or a benchmark session is armed. The same goes for Undo inside the app.
2. **Restore point** ("Creating a restore point… this can take a minute"). See "Restore point".
3. Each change: record the old value, write, read back. One failure doesn't stop the others; each item reports its own
   result.
4. Kestrel scans again, so Health, Home and the next benchmark's "PC at the time" are current. Then the result: what
   changed, "Restart VALORANT" if any change waits for the next launch, and a button to run a benchmark. The copy says
   "Measure it on your PC", never a promised number.

### Undo

**Undo** on a row undoes that tweak. **Revert everything** lists what will be undone, asks once, and undoes in reverse
order. System Protection stays on: turning it off deletes that drive's restore points, so it has its own row with that
warning and is never part of Revert everything or the uninstaller's undo.

If a value holds something else now, Undo leaves it alone and the row says **Changed by something else**. **Restore
anyway** asks first, then writes the old value. Revert everything leaves those rows as they are and says so.

If a value went back to what it was before Kestrel, by hand or through another tool, there's nothing to undo: Kestrel
closes the record without touching Windows, the report notes "went back outside Kestrel", and the row returns to its
normal state.

### Kestrel report

A panel on the tab, newest first, with **Copy report**: every applied, undone and re-applied change with its time, the
value before and after and whether it's still active, plus each restore point's result. The same report goes into Copy
diagnostics. Plain text:

```
Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20
Active
  Kestrel power plan: EXM Free Power Plan V8 → Kestrel (2026-11-02 18:12)
History
  2026-11-02 18:15 Undid Game Mode: back to off
  2026-11-02 18:12 Applied Game Mode: off → on
  2026-11-02 18:12 Applied Kestrel power plan: EXM Free Power Plan V8 → Kestrel
  2026-11-02 18:12 Restore point created (#57)
```

### Home and Health

A new `Fix::Tweak { tweak }` replaces `Fix::LaterStage { stage: TWEAKS_STAGE }` on `power-plan` (the Balanced and Power
saver case), `game-mode` and `background-recording`, and `Fix::Steps` on `valorant-gpu`. Home and Health show a **Fix**
button that opens the Tweaks tab with that tweak's Preview. The check texts stop saying "arrives in stage 5". The Info
text for another tool's plan says Kestrel's can be tried in the Tweaks tab. On a dual-CCD X3D, Balanced passes the check,
since it's what AMD recommends there and Kestrel doesn't offer its plan. **Apply plan** on Home keeps leading to the
tab. `TWEAKS_STAGE` and `STAGE.tweaks` go away; the tests that pin them follow.

### First run (decision 22)

New users only; the owner's first run is done. The result step's **Not now** goes to the Tweaks tab instead of Home.
The first saved benchmark's result gets a **See your plan** button. The tab shows a one-time note with **Not now**,
which goes to Home; a flag next to the first-run flag remembers it.

### Uninstaller (decision 88)

When Kestrel's changes are active, the uninstaller asks: "Undo Kestrel's changes before removing it?" Yes is the
default. Yes runs Kestrel's own undo; if something can't be undone it says so, and the uninstall continues. A silent
uninstall leaves changes in place. Ticking "Delete the application data" also removes `%ProgramData%\Kestrel`, after
this question.

Installing a newer version is not removing Kestrel: it doesn't ask and the changes stay. That covers `/UPDATE` and also
running a newer setup by hand, the only update route until stage 8, where the installer runs the old uninstaller itself.

### Benchmark

- Each saved benchmark records the active tweak ids: `Context.applied_tweaks: Option<Vec<String>>`, strings rather than
  an enum so an old build can still load a newer file. None means it predates stage 5.
- A comparison lists the tweaks added or removed between the two results, and a changed power plan. They show next to
  the existing settings warning and in Copy results.
- A result whose average-FPS spread is above 10% says so, in the app and in Copy results: "These runs vary by 24.9%, so
  only big changes will count as real. Close background apps and run again."

## The Essential tier

Per-user values are written under `HKEY_USERS\<SID>` (see "Which account gets the changes"); `HKCU` below is shorthand
for that. Impact tags replace the guesses in decision 30 where the research contradicts them.

| Tweak (id) | What changes | Offered when | Ticked by default | Takes effect | Impact tag |
| --- | --- | --- | --- | --- | --- |
| Game Mode (`game-mode`) | `HKCU\Software\Microsoft\GameBar`, `AutoGameModeEnabled` = 1 (DWORD). Absent means on | The value is 0. If `HKLM\Software\Policies\Microsoft\Windows\GameDVR`, `AllowAutoGameMode` is 0: Blocked by policy | Yes | Now | Medium |
| Background recording off (`background-recording`) | `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR`, `HistoricalCaptureEnabled` = 0 (DWORD). Never `AppCaptureEnabled`, `GameDVR_Enabled` or `AllowGameDVR` | The value is 1 and `AppCaptureEnabled` isn't 0 and the `AllowGameDVR` policy isn't 0. Absent means off | Yes | Now | Medium |
| Optimizations for windowed games (`windowed-optimizations`) | `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`, `DirectXUserGlobalSettings` (string of `Name=Value;` tokens), token `SwapEffectUpgradeEnable=1`, leaving every other token alone | Windows build 22621 or later; hidden below it | Only when VALORANT's `FullscreenMode` is 1 or 2 (windowed fullscreen, windowed). In exclusive fullscreen or when unknown: unticked, "Only matters in windowed modes" | Next time VALORANT opens | Situational |
| GPU preference (`gpu-preference`) | Same key. Value named by VALORANT's exe path **with backslashes** (`valorant.rs` returns forward slashes), token `GpuPreference=2` | Two or more hardware GPUs (`gpu::is_hybrid` or stage 3's adapter list; virtual adapters don't count) and VALORANT found. Otherwise Not needed. Token already 2: Already set | Yes when the token is absent or 0. Token 1: unticked, "You picked Power saving" | Next time VALORANT opens | High on hybrid PCs |
| Enhance pointer precision off (`pointer-precision`) | `SystemParametersInfoW(SPI_SETMOUSE, 0, [0,0,0], SPIF_UPDATEINIFILE)`, then `SendNotifyMessageW` to tell other windows, because `SPIF_SENDCHANGE` can block on one hung window; on disk, `HKCU\Control Panel\Mouse`, strings `MouseSpeed`, `MouseThreshold1`, `MouseThreshold2` = "0" | Acceleration is on | **No.** Its row says: "Doesn't change aim or FPS in VALORANT, which reads the mouse directly. Only the cursor on the desktop changes." | Now | None in game |
| Kestrel power plan (`power-plan`) | See below | See below | See below | Now | Low |

### Kestrel power plan

A new plan named "Kestrel" with a description saying it came from Kestrel, always under one fixed GUID. It's created
with `PowerDuplicateScheme` from a stock plan, its values are set with `PowerWriteACValueIndex`, and it's activated with `PowerSetActiveScheme`.
Values go through `PowerReadValueMin`/`Max` and are clamped to the machine's range, and the Preview shows the clamped value. Only AC
values change, so battery behaviour is the base plan's.

| Setting | GUID (subgroup / setting) | Value |
| --- | --- | --- |
| Core parking minimum cores | `54533251-82be-4824-96c1-47b60b740d00` / `0cc5b647-c1df-4637-891a-dec35c318583` | 100 (parking off) |
| USB selective suspend | `2a737441-1930-4402-8d77-b2bebba308a3` / `48e6b7a6-50f5-4782-a5d4-53bb8f07e226` | 0 |
| PCIe link-state power management | `501a4d13-42af-4429-9fd1-a8218c268e20` / `ee12f906-d277-404b-b6da-e5fa1a576df5` | 0 |

| PC | Base plan | Settings written |
| --- | --- | --- |
| Desktop, one core type | High performance (`8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c`) | All three |
| Laptop, Modern Standby (`GetPwrCapabilities`, `AoAc`) or hybrid CPU | Balanced (`381b4222-f694-41f0-9685-ff5bb260df2e`) | USB and PCIe only |
| Dual-CCD X3D (7900X3D, 7950X3D, 9900X3D, 9950X3D and their variants) | Not offered: "AMD's guidance for this chip is Balanced with Game Mode on" | None |

| Active plan now | State |
| --- | --- |
| Built-in Balanced or Power saver | Available, ticked |
| Built-in High performance or Ultimate Performance | Available, unticked: "You're already on a fast plan. Kestrel's also turns off USB and PCIe power saving." |
| Any other plan (the owner's EXM plan, decision 87) | Available, unticked: "You're on [name]. Measure before and after to see which is better." |
| Kestrel's, created by this journal | Applied |

Stock plans may be missing: some optimisers delete them. Enumerate with `PowerEnumerate`, try to duplicate High
performance anyway, fall back to Balanced, and otherwise fail with a clear message. Never base the plan on another
tool's. A policy-set plan (`HKLM\SOFTWARE\Policies\Microsoft\Power\PowerSettings\ActivePowerScheme`) gives Blocked by
policy.

**Undo:** activate the previous plan by GUID (if it's gone, Balanced, and say so), then delete Kestrel's plan.

Kestrel's plan stays on disk when another plan becomes active, so this record stays open for as long as the plan exists,
even if the plan now active is the previous one:

- Another plan active: the row says **Kestrel's plan isn't active**. **Use it again** activates it. **Remove it** keeps the
  plan now active and deletes Kestrel's. Revert everything and the uninstaller's undo do the same as Remove it.
- Kestrel's plan deleted outside Kestrel: the record closes and the report says so.
- A crash after the plan was created and before it was activated lands in the same state and needs no special handling.

Kestrel only ever deletes that one GUID.

## How changes are recorded

### The journal

- An append-only JSON-lines file, `%ProgramData%\Kestrel\journal.jsonl`. The folder is set up so only Administrators and
  SYSTEM can write to it; a folder that already exists with another owner is not trusted. An elevated app must not act
  on a file any user can edit.
- **Treat it as untrusted anyway.** Undo accepts only known tweak ids and fixed targets; it never takes a registry path
  from the file. A SID must look like a SID, a saved plan must be a GUID, an exe path must end in
  `\VALORANT-Win64-Shipping.exe` and is only ever used as a value name in the one key above.
- Each change: read the prior state, append and flush an **intent** (batch, tweak id, target, prior, planned value),
  write to Windows, read back, append and flush the **outcome**. Writes are `FlushFileBuffers`-durable. A truncated last
  line is ignored.
- **Prior states are distinct:** absent, value, read failed. A failed read aborts that item, because undo would
  otherwise delete a real value.
- **Already at target writes nothing.** Kestrel never undoes what it didn't change.
- **The first prior is kept.** Apply again doesn't replace it.
- **Token-level records** for the two DirectX strings, so an undo never restores a whole string over a later Auto HDR
  or VRR change. Merge rule: split on `;`, split each on the first `=`, compare names ignoring case, keep unknown tokens
  exactly, write `k=v;`. Undo restores or removes only Kestrel's token; if the string ends up empty and the value didn't
  exist before, delete the value; if Kestrel created the key and it's empty, delete the key.
- If the journal can't be written, Apply refuses before any change and says why. Unreadable lines are shown as a warning
  in the report and never acted on.

### Undo and crash recovery

- **Every open record is judged the same way,** comparing the current value with the old one and the one Kestrel wrote,
  by what they mean (an absent value counts as that setting's default):
  - Equal to what Kestrel wrote: **Applied**.
  - Equal to the old value: the record closes, with no write to Windows, and the report notes it (see "Undo"). Undo
    finding this state does the same.
  - Anything else: **Changed by something else**, and nothing is written.

  The power plan's rules are in "Kestrel power plan".
- **This runs at start and whenever the plan is read.** Windows updates can reset settings, so it catches those. Kestrel
  writes to its journal here but never to Windows, and it never re-applies by itself.
- **After a crash,** an intent with no outcome goes through the same check, with "equal to the old value" meaning it was
  never applied. The tab says Kestrel was interrupted and what happened to each item.
- **What counts as active:** Applied rows only, for `applied_tweaks` and the report's Active list. Revert everything also
  removes an inactive Kestrel plan. The uninstaller's check counts every open record.
- A failed undo keeps the record, so it can be tried again.
- One process at a time may write the journal, including the uninstaller's undo.

### Which account gets the changes

Kestrel's manifest requires elevation, and a standard user who types an admin's password runs Kestrel **as that admin**,
so `HKCU` is the wrong hive: a tweak would pass its check and leave the real user unchanged.

- Find the signed-in account: `GetShellWindow`, its process's token, `TokenUser`. Fallback: the session's user through
  `WTSQuerySessionInformationW` and `LookupAccountNameW`. If neither works, per-user tweaks say "Couldn't tell which
  account is signed in" and the power plan still works.
- **Always** read and write per-user values under `HKEY_USERS\<SID>`, even when the accounts match, so there's one code
  path.
- A per-user record is keyed on tweak and SID. Rows, Revert everything and `applied_tweaks` cover the signed-in account plus
  machine-wide changes. If another account has open records, one line says so; those wait until that account is signed
  in. The uninstaller's undo does what it can and says what's left.
- `registry_probe.rs`'s `hkcu()` reads the same way, so Health sees the account the changes go to.
- Pointer precision when the accounts differ: write the registry only and say "Next sign-in", because
  `SPIF_UPDATEINIFILE` writes the caller's profile.
- Copy diagnostics says whether the accounts differ. Benchmark data stays under the elevating account's app data, a
  known limit.

### Restore point

The journal is the real undo. The restore point is a bonus that Windows may later remove, so the app never promises it.

- Each Apply asks Windows for a point, in a short-lived copy of Kestrel (`Kestrel.exe` with a flag, already elevated, no
  extra prompt). Microsoft's sample needs a process-wide `CoInitializeSecurity`, which may clash with WebView2.
- That process loads `SrClient.dll` from System32 at run time (`windows-sys` binds the function to `sfc.dll`; don't use
  its binding), and calls `SRSetRestorePointW` with `BEGIN_SYSTEM_CHANGE` and `MODIFY_SETTINGS`, then `END_SYSTEM_CHANGE`
  with the returned sequence if a new point was opened.
- **Windows may return success for an old point.** Inside its 24-hour window it reports success with the earlier
  sequence number. Read the highest sequence from WMI `root\default`, `SystemRestore`, before and after, polling for up
  to 30 seconds. Only a higher sequence is "created". Otherwise the result is "reused", with the earlier point's time.
  `SystemRestorePointCreationFrequency` is left alone.
- **System Protection off (decision 89):** detected from
  `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SPP\Clients`, value `{09F7EDC5-294E-4180-AF6A-FB0E6A0E9513}`,
  containing the Windows drive's volume GUID, or from `ERROR_SERVICE_DISABLED`. The dialog: "System Protection is off on
  your Windows drive. Turn it on and create a restore point? Windows will use some disk space for restore points."
  **Turn on and create**, **Continue
  without**, **Cancel**. Turning on uses WMI `SystemRestore.Enable` with that drive and is a journal entry with its own row. If
  `DisableSR` or `DisableConfig` policies are set, only **Continue without** and **Cancel** are offered. Kestrel never
  runs `vssadmin resize` and never deletes those policies.
- **Failure or timeout (3 minutes):** "Couldn't create a restore point: [reason]. Continue with Kestrel's undo only?" The
  result (created, reused, skipped, failed) goes in the journal and the report.

## Architecture

- **`crates/kestrel-tweaks`** (new). Pure, tested on macOS: the catalog (ids, copy, impact, timing), the plan rules, the
  journal format and its parser, the undo and recovery decisions, token merging, the report text. A `System` trait stands
  between it and Windows, with a fake for tests, like `Platform` in the benchmark. The Windows implementation sits behind
  `cfg(windows)` in the same crate. `kestrel-probe` keeps its read-only guarantee and never gains a write.
- **`app/src-tauri/src/tweaks/`**: commands and state, following `bench/`: `tweaks_plan`, `tweaks_preview`, `tweaks_apply`,
  `tweaks_undo`, `tweaks_revert_all`, `tweaks_report`, and the ones the rows' other buttons need. Commands return what
  changed and the page re-reads the view, since nothing changes in the background. One apply at a time; no platform call
  under a lock; `spawn_blocking` for the slow parts. `get_diagnostics` adds the report and the account check.
- **`main.rs`** parses flags before starting Tauri: the restore-point helper, and the uninstaller's check and undo. It
  also calls `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR)` at start-up,
  from the stage-1 backlog, since Kestrel now writes to the system.
- **UI:** `app/src/pages/Tweaks.tsx` and `app/src/components/tweaks/`; browser-mode fakes in `client.ts`. The page's own
  labels live in `strings.ts`; per-tweak text comes from Rust, as Findings' text does.
- **Installer:** `installerHooks` in `tauri.conf.json` with `NSIS_HOOK_PREUNINSTALL`. It asks only when `$UpdateMode` isn't
  1, the uninstaller isn't silent, and `$EXEDIR` differs from `$INSTDIR`. Tauri's installer template runs the old
  uninstaller in place with `_?=` during an upgrade, while NSIS copies a user-started uninstaller to a temp folder first, so
  those two folders match only during an upgrade. Checked against the template in CLI 2.12.1; confirm it on the owner's PC.
- **Release:** the workspace goes to 0.5.0. `release.yml`'s body stops saying the build changes nothing: "Test build. It
  changes your PC only when you press Apply, and every change can be undone."

## Errors

| Situation | What happens |
| --- | --- |
| VALORANT running, or a benchmark armed | Apply and Undo are off and say why |
| A read fails | The row says Couldn't read; nothing is written for it |
| A write fails, or the read-back differs | That item fails with the reason; if the value changed, Kestrel puts the prior back and records it; the others go on |
| The journal can't be written | Apply refuses before any change |
| Interrupted mid-apply | Settled on the next start (see "Undo and crash recovery") |
| A stock plan is missing | Falls back as in "Kestrel power plan" |
| The previous plan is gone at undo | Balanced is activated, and the report says so |
| VALORANT isn't found | GPU preference says "VALORANT not found"; the rest still works |
| An organisation policy sets a value | Blocked by policy; Kestrel never edits policies |
| Windows 10 22H2 | Windowed optimizations is hidden; a banner already warns that it's untested |

## Testing

- **`kestrel-tweaks` unit tests with the fake system:** each row state from each input; the power plan's variants and
  active-plan cases; token merge and undo with unknown tokens, duplicates and mixed case; every prior state; the
  three-way undo; crash recovery at each step (no outcome, truncated line); capture-once on Apply again; untrusted journal
  input (unknown id, odd path, odd SID) ignored; the report text.
- **Round trip on GitHub's Windows machine.** A new binary, not `cargo test`, because it changes the machine it runs on;
  it refuses to run unless told to with an explicit flag. It reads the raw values, applies every tweak, reads back and
  checks the Health findings, undoes, and asserts the raw values equal the first read. Runs in `ci.yml` and in
  `release.yml` before the build, so a failing round trip stops the release. This keeps the tracker's promise. Where the
  machine can't exercise something (no VALORANT, no second GPU, maybe no System Restore), a stub stands in and the job says
  so. The restore point and the uninstaller prompt are checked on the owner's PC.
- **UI tests:** the row states, Preview, Revert everything, the report panel, the measure-first note, in browser mode.
- **Windows-only code** is linted with `cargo clippy --target x86_64-pc-windows-msvc` from the Mac and built on CI.

## What to check on the owner's PC

These can only be settled on a real PC:

- Whether stock plans exist after EXM, and how the three settings compare (the Preview shows it).
- Whether the restore-point helper creates a point, how long it takes, and whether `SPP\Clients` matches Windows'
  System Protection page.
- The VALORANT exe value name's letter case for GPU preference, if the PC has its integrated graphics enabled.
- What "Optimizations for windowed games" reads in Settings when the token is absent.

## What to test in stage 5 (for the tracker)

- Install v0.5.0 over v0.3.0; the results and the starting point are still there.
- Before applying: run a benchmark with no screen share and press **Use as baseline**.
- The Tweaks tab lists the Essential tier. Game Mode and background recording probably say Already set; the power plan is
  unticked and names your EXM plan.
- Tick Kestrel's plan and open Preview: it shows your plan's values beside Kestrel's. Nothing has changed yet.
- Apply: a restore point is created or reused, or Kestrel asks about System Protection. Control Panel → Power Options shows
  Kestrel active.
- Run a benchmark; the comparison lists the tweak and the plan change.
- Undo: your EXM plan is active again and the Kestrel plan is gone from Power Options.
- Tick everything available, Apply, then Revert everything: every setting is back. Close and reopen Kestrel between steps
  once: the rows still show the right state.
- Apply Kestrel's plan, then pick Balanced by hand in Power Options: the row says Kestrel's plan isn't active. Remove it:
  Balanced stays and Kestrel's plan is gone. Put your EXM plan back afterwards.
- If Optimizations for windowed games was applied, turn it off in Settings: the row says Changed by something else, and Undo
  leaves it alone.
- Copy report and Copy diagnostics, then paste both in chat.
- Uninstall with a change active: it asks whether to undo. Then reinstall.
- On the first fix build after this one: install it over v0.5.x with a change active. It doesn't ask, and the change stays.

## Left to the builder

Crate internals, the journal's exact fields, flag names, how the rows lay out in the current look, poll intervals, and how
the round-trip binary is structured. Where Kestrel made a product call beyond the owner's answers, it's listed below.

## Calls Kestrel made (approved with this spec)

- Decision 90: the NVIDIA profile moves to stage 6.
- Pointer precision comes unticked, because it changes the desktop's mouse feel with no benefit in the game.
- The power plan's impact tag drops from High to Low, on the research.
- A built-in fast plan is offered unticked too.
- The spread warning's threshold is 10%. The owner's clean benchmark can calibrate it.
- Apply and Undo are refused with VALORANT open; the uninstaller's undo isn't, since it has no window to ask from.
- Restore points run in a helper process from the start, instead of trying the main process first.
- The journal lives in `%ProgramData%\Kestrel`, which also keeps it out of the uninstaller's "Delete the application data"
  unless that box is ticked.
- Version 0.5.0, matching the stage number.
- Installing a newer version by hand counts as an update: no question, and the changes stay.
- A value that goes back to what it was closes its record, with a note in the report, rather than staying flagged.
- The power plan row has its own state when Kestrel's plan exists but isn't active, with **Remove it** keeping the plan you
  chose.
- The Preview compares the three settings Kestrel sets and says in one sentence that the rest comes from the base plan. It
  doesn't list every setting of a plan like EXM's.
- With several accounts on one PC, each account's changes are tracked apart.
