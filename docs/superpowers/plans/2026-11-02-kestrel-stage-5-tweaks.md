# Kestrel Stage 5: Tweaks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Tweaks tab, the engine behind it (journal, undo, crash recovery, restore point, report), the Essential tier, the uninstaller's question and the benchmark's memory of active tweaks, and ship it as v0.5.0.

**Architecture:** A new crate, `crates/kestrel-tweaks`, holds the catalog, plan rules, journal and undo decisions as pure Rust behind a `System` trait, with a Windows implementation behind `cfg(windows)` and a fake for tests on macOS. `app/src-tauri/src/tweaks/` exposes it as request/response commands, and `app/src/pages/Tweaks.tsx` is the page. `kestrel-probe` stays read-only and only gains a way to name the signed-in account.

**Tech Stack:** Rust 1.99 (`File::try_lock`), `windows-sys` 0.61.2, `winreg` 0.56, `wmi` 0.18.4, Tauri 2, NSIS hooks, React 19, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-02-kestrel-stage-5-tweaks-design.md`. Exact Windows facts, ids and pitfalls: `docs/superpowers/notes/2026-11-02-stage-5-tweak-research.md`. Read the sections a task names.

## Global Constraints

- Nothing on the PC changes before the person presses Apply or an undo-family button. Reading a view or a Preview writes only to Kestrel's journal.
- Never write a `Policies` key. Never write `AppCaptureEnabled`, `GameDVR_Enabled`, `AllowGameDVR`, `AllowAutoGameMode` or `SystemRestorePointCreationFrequency`. Never run `powercfg /restoredefaultschemes` or `vssadmin`.
- Never stop or disable a Windows service, clean traces, touch Vanguard, or open a handle to the game. The game is found by name: `kestrel_probe::process::game_running`.
- Kestrel never re-applies by itself and never overwrites a value it didn't write without the person pressing **Restore anyway**.
- The only plan Kestrel ever deletes is `KESTREL_PLAN_GUID`. Nothing from EXM is copied.
- A failed read is never "absent". Only `ERROR_FILE_NOT_FOUND` maps to `Read::Absent`; a wrong registry type or any other error is `Read::Failed`.
- `kestrel-probe` gains no write of any kind.
- No network calls, no telemetry, no kernel driver, no injection.
- The copy never promises a gain. It says "Measure it on your PC".
- Per-tweak text (titles, reasons, report lines) lives in Rust, as Findings' text does. The page's own labels live in `strings.ts`.
- `unsafe` calls follow `crates/kestrel-probe/src/process.rs`: `windows-sys`, one `// SAFETY:` per call, RAII for handles. Pure logic is tested on macOS.
- Every commit passes `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and for tasks touching Windows code `cargo clippy --target x86_64-pc-windows-msvc -p kestrel-probe -p kestrel-tweaks --all-targets -- -D warnings`. UI tasks pass `npm test` and `npx tsc --noEmit` in `app/`.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Rulings on the spec

These refine the spec and were applied to it in the same commit as this plan:

- **A fixed plan GUID.** `KESTREL_PLAN_GUID` = `4b455354-524c-4e50-a1b2-c3d4e5f60705`, passed to `PowerDuplicateScheme` through a non-null pointer. A crash between creating the plan and recording it can't leave an orphan, and undo never takes a plan GUID from the journal. The round trip on GitHub proves it works; if it doesn't, fall back to the allocated GUID and journal it.
- **No `tweaks-state` event.** Nothing changes the state in the background, so commands return what changed and the page re-reads the view.
- **Health agrees with the plan rule on dual-CCD X3D.** Balanced passes there, or Health would flag what Kestrel refuses to fix.

## Review Focus

The input classes most likely to bite a person that the spec implies and ordinary tests would miss. Each has its test in the named task.

- A registry value of the wrong type (`AutoGameModeEnabled` as a string, `DirectXUserGlobalSettings` as binary): the row says Couldn't read and nothing is written, because treating it as absent would delete it on undo. Task 5 with the fake, and the round trip's wrong-type step in Task 9.
- A `DirectXUserGlobalSettings` string with odd contents: no trailing `;`, empty tokens, duplicates in mixed case, a token with no `=`, a value containing `=`: every unknown part survives apply and undo. Task 3.
- `C:\ProgramData\Kestrel` created beforehand by a standard user: Apply refuses and says why. Task 9.
- Windows refuses to activate the plan after it was created: the plan is deleted again, the old plan stays active, and the item fails with the reason. Task 6.
- VALORANT's GPU-preference value already exists with different letter case or slashes: Kestrel updates that value rather than adding a second one. Task 5.

---

## Shared contract

Names every task may rely on. Field-level details not listed are the implementer's.

```rust
// crates/kestrel-core/src/tweak_id.rs (Task 1)
#[serde(rename_all = "kebab-case")]
pub enum TweakId { PowerPlan, GameMode, BackgroundRecording, WindowedOptimizations, GpuPreference, PointerPrecision, SystemProtection }
impl TweakId {
    /// The Essential rows, in display order: everything but SystemProtection.
    pub const ESSENTIAL: [TweakId; 6];
    pub fn as_str(self) -> &'static str;          // "power-plan", ...
    pub fn parse(text: &str) -> Option<TweakId>;
}

// crates/kestrel-probe/src/account.rs (Task 2)
pub struct Account { pub signed_in: Option<String>, pub this_process: Option<String> } // SIDs as strings
impl Account { pub fn differs(&self) -> bool; }
pub fn account() -> Account;

// crates/kestrel-tweaks/src/system.rs (Task 3)
pub enum Read<T> { Absent, Value(T), Failed(String) }
pub trait System: Send + Sync {
    fn account(&self) -> Account;
    // Per-user values, always under HKEY_USERS\<sid>\<key>.
    fn read_dword(&self, sid: &str, key: &str, name: &str) -> Read<u32>;
    fn write_dword(&self, sid: &str, key: &str, name: &str, value: u32) -> io::Result<()>;
    fn read_string(&self, sid: &str, key: &str, name: &str) -> Read<String>;
    fn write_string(&self, sid: &str, key: &str, name: &str, value: &str) -> io::Result<()>;
    fn delete_value(&self, sid: &str, key: &str, name: &str) -> io::Result<()>;
    fn value_names(&self, sid: &str, key: &str) -> Read<Vec<String>>; // Absent when the key doesn't exist
    fn delete_key_if_empty(&self, sid: &str, key: &str) -> io::Result<()>;
    // Machine-wide policy reads. There is deliberately no machine-wide registry write.
    fn read_policy_dword(&self, key: &str, name: &str) -> Read<u32>;
    fn read_policy_string(&self, key: &str, name: &str) -> Read<String>;
    // Mouse: SPI_GETMOUSE / SPI_SETMOUSE. `persist` adds SPIF_UPDATEINIFILE.
    fn mouse_live(&self) -> Read<[i32; 3]>;
    fn set_mouse_live(&self, values: [i32; 3], persist: bool) -> io::Result<()>;
    // Power: GUIDs as lower-case strings.
    fn active_plan(&self) -> Read<String>;
    fn plan_exists(&self, guid: &str) -> bool;
    fn plan_name(&self, guid: &str) -> Option<String>;
    fn duplicate_plan(&self, base: &str, new: &str) -> io::Result<()>;
    fn describe_plan(&self, guid: &str, name: &str, description: &str) -> io::Result<()>;
    fn read_plan_value(&self, guid: &str, subgroup: &str, setting: &str) -> Read<u32>;
    fn plan_value_range(&self, subgroup: &str, setting: &str) -> Option<(u32, u32)>;
    fn write_plan_value(&self, guid: &str, subgroup: &str, setting: &str, value: u32) -> io::Result<()>; // AC only
    fn activate_plan(&self, guid: &str) -> io::Result<()>;
    fn delete_plan(&self, guid: &str) -> io::Result<()>;
    fn modern_standby(&self) -> Option<bool>;
    // System Protection on the system drive.
    fn protection(&self) -> Readiness;
    fn turn_protection_on(&self) -> io::Result<()>;
    fn turn_protection_off(&self) -> io::Result<()>;
}
#[serde(rename_all = "kebab-case")] pub enum Readiness { Ready, ProtectionOff, BlockedByPolicy, Unknown }

// crates/kestrel-tweaks/src/catalog.rs (Task 3)
#[serde(rename_all = "kebab-case")] pub enum Impact { High, Medium, Low, Situational, NoneInGame }
#[serde(rename_all = "kebab-case")] pub enum Timing { Now, NextLaunch, NextSignIn }
pub struct Info { pub id: TweakId, pub title: &'static str, pub impact: Impact, pub timing: Timing }
pub fn info(id: TweakId) -> Info;

// crates/kestrel-tweaks/src/journal.rs (Task 4)
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Saved {
    Dword { value: Option<u32> },
    Token { token: Option<String>, value_existed: bool, key_existed: bool },
    Mouse { speed: Option<String>, threshold1: Option<String>, threshold2: Option<String>, live: [i32; 3] },
    Plan { guid: String, name: String },
    Protection { on: bool },
}
pub struct OpenRecord { pub tweak: TweakId, pub sid: Option<String>, pub value_name: Option<String>, pub prior: Saved, pub planned: Saved, pub at: String, pub settled: bool }
pub struct Replay { pub open: Vec<OpenRecord>, pub history: Vec<Entry>, pub unreadable: u32 } // open: oldest first
pub fn replay(text: &str) -> Replay;
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum RestoreOutcome { Created { sequence: u32 }, Reused { sequence: u32, time: String }, Skipped, Failed { reason: String } }
/// Where the journal may live: `check` refuses a folder that exists with an untrusted owner, `prepare` creates and locks it down.
pub trait Folder: Send + Sync { fn check(&self, dir: &Path) -> io::Result<()>; fn prepare(&self, dir: &Path) -> io::Result<()>; }
pub struct PlainFolder; // tests and off Windows: check is Ok, prepare is create_dir_all
pub struct JournalFile; // new(dir, Box<dyn Folder>), lock() -> io::Result<JournalLock>, append(&Entry) -> io::Result<()>, read() -> io::Result<Replay>

// crates/kestrel-tweaks/src/plan.rs (Task 5)
pub struct PlanInput { pub build: u32, pub form_factor: FormFactor, pub hybrid_cpu: bool, pub cpu_name: String, pub gpu_choice: bool, pub valorant_exe: Option<String>, pub fullscreen_mode: Option<u8> }
impl PlanInput { pub fn from_snapshot(s: &SystemSnapshot, fullscreen_mode: Option<u8>) -> Self; }
#[serde(rename_all = "kebab-case")] pub enum RowState { Available, AlreadySet, NotNeeded, BlockedByPolicy, Applied, ChangedElsewhere, PlanInactive, CouldntRead }
#[serde(rename_all = "camelCase")] pub struct Row { pub id: TweakId, pub title: String, pub state: RowState, pub ticked: bool, pub why: String, pub impact: Impact, pub timing: Timing }
#[serde(rename_all = "camelCase")] pub struct ValueChange { pub label: String, pub now: String, pub after: String }
#[serde(rename_all = "camelCase")] pub struct PreviewItem { pub id: TweakId, pub title: String, pub location: String, pub changes: Vec<ValueChange>, pub timing: Timing, pub note: Option<String> }

// crates/kestrel-tweaks/src/engine.rs (Task 7)
#[serde(rename_all = "kebab-case")] pub enum Gate { Open, GameRunning, BenchmarkActive }
#[serde(rename_all = "camelCase")] pub struct TweaksView { pub rows: Vec<Row>, pub gate: Gate, pub other_account: bool, pub pending_other_accounts: u32, pub interrupted: Vec<String>, pub unreadable_lines: u32, pub any_active: bool }
#[serde(rename_all = "camelCase")] pub struct PreviewView { pub items: Vec<PreviewItem>, pub restore: Readiness }
#[serde(rename_all = "camelCase")] pub struct ItemOutcome { pub id: TweakId, pub title: String, pub ok: bool, pub message: String }
pub struct Engine;
impl Engine {
    pub fn new(system: Arc<dyn System>, journal: JournalFile) -> Self;
    pub fn view(&self, input: &PlanInput, gate: Gate) -> io::Result<TweaksView>;
    pub fn preview(&self, input: &PlanInput, ids: &[TweakId]) -> PreviewView;
    pub fn apply(&self, input: &PlanInput, ids: &[TweakId]) -> io::Result<Vec<ItemOutcome>>;
    pub fn undo(&self, id: TweakId) -> io::Result<ItemOutcome>;
    pub fn revert_all(&self) -> io::Result<Vec<ItemOutcome>>;
    pub fn apply_again(&self, input: &PlanInput, id: TweakId) -> io::Result<ItemOutcome>;
    pub fn restore_anyway(&self, id: TweakId) -> io::Result<ItemOutcome>;
    pub fn use_plan_again(&self) -> io::Result<ItemOutcome>;
    pub fn remove_plan(&self) -> io::Result<ItemOutcome>;
    pub fn turn_protection_on(&self) -> io::Result<()>; // journals SystemProtection, then calls the system
    pub fn record_restore(&self, outcome: &RestoreOutcome) -> io::Result<()>;
    pub fn applied_ids(&self) -> Vec<String>;     // the signed-in account's and machine-wide Applied rows
    pub fn has_open_records(&self) -> bool;       // any account
    pub fn report(&self, kestrel_version: &str, build: u32, now: &str) -> String;
}

// crates/kestrel-tweaks/src/restore.rs (Task 10)
#[serde(rename_all = "kebab-case")] pub enum RestoreChoice { Create, TurnOnAndCreate, Skip }
```

Time strings in the journal are `chrono::Local::now().to_rfc3339()`. The report shows their first 16 characters with `T` replaced by a space.

---

### Task 1: The `Fix::Tweak` link from Health and Home

**Files:**
- Create: `crates/kestrel-core/src/tweak_id.rs`
- Modify: `crates/kestrel-core/src/lib.rs`, `crates/kestrel-core/src/health.rs`, `crates/kestrel-core/src/cpu.rs`
- Modify: `app/src/api/types.ts`, `app/src/components/FindingRow.tsx`, `app/src/components/ScoreCard.tsx`, `app/src/pages/Home.tsx`, `app/src/pages/Health.tsx`, `app/src/App.tsx`, `app/src/strings.ts`, `app/src/api/sample-report.test.ts`, `app/src/api/mock-report.json`

**Interfaces:**
- Produces: `kestrel_core::tweak_id::TweakId` (shared contract); `Fix::Tweak { tweak: TweakId }`, serialized `{"kind":"tweak","tweak":"game-mode"}`; `kestrel_core::cpu::is_dual_ccd_x3d(name: &str) -> bool`; on the UI side `onTweak: (tweak: TweakId) => void` on `Home`, `ScoreCard`, `FindingRow`, `Health`, and App state `tweakFocus: TweakId | null` that Task 13 reads.

- [ ] **Step 1: Write the failing tests in `health.rs`, replacing `later_stage_fixes_follow_the_current_stage_order`**

```rust
#[test]
fn tweak_fixes_point_at_their_tweak() {
    let mut s = owner_pc_fixture(); // has Balanced
    s.windows.game_mode = Some(false);
    s.windows.background_recording = Some(true);
    let f = run_health_check(&s, today());
    for (id, tweak) in [("power-plan", TweakId::PowerPlan), ("game-mode", TweakId::GameMode), ("background-recording", TweakId::BackgroundRecording)] {
        assert_eq!((get(&f, id).status, get(&f, id).fix.clone()), (Status::Problem, Fix::Tweak { tweak }), "{id}");
    }
    assert!(!get(&f, "power-plan").detail.contains("stage"));
    assert_eq!(get(&f, "power-plan").detail, "This plan lets the CPU slow down between frames, which hurts 1% lows. Kestrel's plan is in the Tweaks tab.");
}

#[test]
fn competitive_fixes_keep_their_stage() { /* startup-apps, adapter-power, delivery-optimization stay Fix::LaterStage { stage: COMPETITIVE_STAGE }, with the same detail assertions the old test had */ }

#[test]
fn a_hybrid_pc_without_a_gpu_preference_gets_the_tweak() { /* hybrid laptop fixture: valorant-gpu is Problem with Fix::Tweak { tweak: TweakId::GpuPreference } */ }

#[test]
fn balanced_passes_on_a_dual_ccd_x3d() {
    let mut s = owner_pc_fixture();
    s.cpu.name = "AMD Ryzen 9 7950X3D 16-Core Processor".into();
    let f = run_health_check(&s, today());
    assert_eq!(get(&f, "power-plan").status, Status::Pass);
}

#[test]
fn another_tools_plan_points_at_the_tweaks_tab() { /* EXM plan: Info; detail == "Kestrel can't tell how this plan is tuned, so it isn't counted as a problem. You can try Kestrel's plan in the Tweaks tab and measure before and after." */ }
```

In `cpu.rs`: `dual_ccd_x3d_names`: true for "AMD Ryzen 9 7900X3D 12-Core Processor", 7950X3D, 9900X3D, 9950X3D, 9950X3D2; false for "AMD Ryzen 7 7800X3D", "AMD Ryzen 7 9800X3D", "Intel Core i9-11900K". In `tweak_id.rs`: every `ESSENTIAL` id round-trips through `as_str` and `parse`, `SystemProtection` isn't in `ESSENTIAL`, and `parse("nope")` is None.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-core`
Expected: compile errors on `TweakId` and `Fix::Tweak`.

- [ ] **Step 3: Implement**

Remove `TWEAKS_STAGE`. `power_plan` returns Pass with title "Power plan: Balanced" when the plan is Balanced and `is_dual_ccd_x3d`. `game_mode`, `background_recording` and the hybrid branch of `valorant_gpu` use `Fix::Tweak`; keep their titles and details. Keep the "monitor plugged into the motherboard" steps.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p kestrel-core`
Expected: PASS.

- [ ] **Step 5: Regenerate the sample and update the UI**

Run in `app/`: `npm run mock`. In `types.ts` add `TweakId` and `{ kind: 'tweak'; tweak: TweakId }`. `FindingRow` and `Health`'s `CheckRow` render `<button className="link">` labelled `strings.health.fix` ("Fix") calling `onTweak(fix.tweak)`. `App.tsx` adds `openTweak(id)`, which sets `tweakFocus` and switches to the Tweaks tab, mirroring `openGuide`. The Tweaks tab still shows ComingSoon until Task 13. Update `sample-report.test.ts`: the sample's `power-plan` fix is `{ kind: 'tweak', tweak: 'power-plan' }`, and `adapter-power` stays `STAGE.competitive`.

- [ ] **Step 6: Run the UI checks**

Run in `app/`: `npm test && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/kestrel-core app/src
git commit -m "feat(core): Health and Home fixes point at their tweak"
```

### Task 2: Name the signed-in account and read its settings

**Files:**
- Create: `crates/kestrel-probe/src/account.rs`, `crates/kestrel-probe/src/win/account.rs`
- Modify: `crates/kestrel-probe/src/lib.rs`, `crates/kestrel-probe/src/win/mod.rs`, `crates/kestrel-probe/src/win/registry_probe.rs`, `crates/kestrel-probe/src/parse.rs`, `crates/kestrel-probe/Cargo.toml`

**Interfaces:**
- Produces: `kestrel_probe::account::{Account, account}` (shared contract), `kestrel_probe::parse::user_key_path(sid: &str, path: &str) -> String`.

Research section 9 has the API route. This reads only.

- [ ] **Step 1: Write the failing tests**

```rust
// account.rs
#[test]
fn differs_only_when_both_are_known_and_unequal() {
    let a = |s: Option<&str>, p: Option<&str>| Account { signed_in: s.map(Into::into), this_process: p.map(Into::into) };
    assert!(a(Some("S-1-5-21-1-1001"), Some("S-1-5-21-1-1002")).differs());
    assert!(!a(Some("S-1-5-21-1-1001"), Some("S-1-5-21-1-1001")).differs());
    assert!(!a(None, Some("S-1-5-21-1-1002")).differs());
}
// parse.rs
#[test]
fn user_key_path_joins_sid_and_key() {
    assert_eq!(user_key_path("S-1-5-21-1-1001", r"Software\Microsoft\GameBar"), r"S-1-5-21-1-1001\Software\Microsoft\GameBar");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-probe`
Expected: compile errors.

- [ ] **Step 3: Implement**

On Windows: `GetShellWindow` → `GetWindowThreadProcessId` → `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` → `OpenProcessToken(TOKEN_QUERY)` → `GetTokenInformation(TokenUser)` → `ConvertSidToStringSidW`; fallback `ProcessIdToSessionId(GetCurrentProcessId())` → `WTSQuerySessionInformationW` for user and domain → `LookupAccountNameW`. `this_process` reads the current process's token. Cache a known `signed_in` in a `OnceLock`. Off Windows both are `S-1-5-21-1-2-3-1001`. Add `windows-sys` features `Win32_UI_WindowsAndMessaging`, `Win32_System_RemoteDesktop`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_System_Threading`.

In `registry_probe.rs`, `hkcu(path)` opens `HKEY_USERS\<user_key_path>` when `account().signed_in` is known, else `HKEY_CURRENT_USER`. The per-user `Run` key in `startup_app_count` goes the same way.

- [ ] **Step 4: Run the checks**

Run: `cargo test -p kestrel-probe && cargo clippy --target x86_64-pc-windows-msvc -p kestrel-probe --all-targets -- -D warnings`
Expected: PASS and no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-probe Cargo.lock
git commit -m "feat(probe): name the signed-in account and read its settings"
```

### Task 3: The `kestrel-tweaks` crate: catalog, `System`, fake and token merging

**Files:**
- Create: `crates/kestrel-tweaks/Cargo.toml`, `src/lib.rs`, `src/catalog.rs`, `src/system.rs`, `src/fake.rs`, `src/tokens.rs`
- Modify: root `Cargo.toml` (workspace member)

**Interfaces:**
- Consumes: `TweakId`, `Account`.
- Produces: `Read<T>`, `System`, `Impact`, `Timing`, `Info`, `info` (shared contract). `tokens::get(string: &str, name: &str) -> Option<String>`, `tokens::set(string: &str, name: &str, value: &str) -> String`, `tokens::remove(string: &str, name: &str) -> String`. `fake::FakeSystem`: `new()` (signed-in and process SID `S-1-5-21-1-2-3-1001`, plans Balanced, High performance, Power saver, active Balanced, Modern Standby false, protection Ready, mouse live `[6, 10, 1]`), `set_dword`/`dword`, `set_string`/`string`, `add_plan(guid, name)`, `set_active`, `plans()`, `set_account`, `fail_write(name)`, `fail_read(name)`, `fail_duplicate(guid)`, `fail_activate(guid)`, `lie_on_readback(name)`, `set_plan_range`, `dump()` returning a comparable snapshot of everything.

Dependencies: `chrono` (`std`, `clock`), `kestrel-core`, `kestrel-probe`, `serde`, `serde_json`; dev `tempfile`; Windows-only `windows-sys`, `winreg`, `wmi` added in Tasks 9 and 10. Catalog titles: "Kestrel power plan", "Game Mode", "Background recording off", "Optimizations for windowed games", "GPU preference", "Enhance pointer precision off", "System Protection". Impact and timing per the spec's Essential table; GPU preference is `High`; `PowerPlan` is `Low`; `PointerPrecision` is `NoneInGame`; `WindowedOptimizations` is `Situational`; `GameMode` and `BackgroundRecording` are `Medium`.

- [ ] **Step 1: Write the failing tests in `tokens.rs`**

```rust
const N: &str = "SwapEffectUpgradeEnable";
#[test] fn set_on_empty() { assert_eq!(set("", N, "1"), "SwapEffectUpgradeEnable=1;"); }
#[test] fn set_keeps_other_tokens_and_their_order() {
    assert_eq!(set("VRROptimizeEnable=0;AutoHDREnable=2097;", N, "1"), "VRROptimizeEnable=0;AutoHDREnable=2097;SwapEffectUpgradeEnable=1;");
}
#[test] fn set_replaces_in_place_keeping_the_spelling() { assert_eq!(set("a=1;swapeffectupgradeenable=0;b=2;", N, "1"), "a=1;swapeffectupgradeenable=1;b=2;"); }
#[test] fn duplicates_collapse_last_wins() { assert_eq!(get("SwapEffectUpgradeEnable=0;swapeffectupgradeenable=1;", N).as_deref(), Some("1")); assert_eq!(set("SwapEffectUpgradeEnable=0;swapeffectupgradeenable=0;x=1", N, "1"), "SwapEffectUpgradeEnable=1;x=1;"); }
#[test] fn odd_contents_survive() {
    assert_eq!(set("Foo;;A=b=c;Bare", N, "1"), "Foo;A=b=c;Bare;SwapEffectUpgradeEnable=1;");
    assert_eq!(get("A=b=c;", "A").as_deref(), Some("b=c"));
    assert_eq!(remove("Foo;A=b=c;SwapEffectUpgradeEnable=1;", N), "Foo;A=b=c;");
}
#[test] fn remove_the_only_token_leaves_an_empty_string() { assert_eq!(remove("swapeffectupgradeenable=1;", N), ""); }
#[test] fn get_absent_is_none() { assert_eq!(get("A=1;", N), None); assert_eq!(get("", N), None); }
```

Plus in `fake.rs`: `failed_reads_are_failed_not_absent` and `dump_changes_when_a_value_changes`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-tweaks`
Expected: compile errors.

- [ ] **Step 3: Implement**

The `System` trait and `Read` as in the shared contract. `fake.rs` is `pub`, always compiled: the app's macOS build and tests use it.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p kestrel-tweaks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/kestrel-tweaks
git commit -m "feat(tweaks): crate with the catalog, the System trait, a fake and token merging"
```

### Task 4: The journal

**Files:**
- Create: `crates/kestrel-tweaks/src/journal.rs`

**Interfaces:**
- Produces: `Saved`, `OpenRecord`, `Replay`, `replay`, `JournalFile`, `JournalLock` and these entries:

```rust
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Entry {
    Intent { at: String, batch: String, tweak: String, sid: Option<String>, value_name: Option<String>, prior: Saved, planned: Saved },
    Outcome { at: String, tweak: String, sid: Option<String>, applied: bool, detail: Option<String>, interrupted: bool },
    Reapplied { at: String, tweak: String, sid: Option<String> },
    Undone { at: String, tweak: String, sid: Option<String>, detail: Option<String> },
    Closed { at: String, tweak: String, sid: Option<String>, reason: CloseReason },
    RestorePoint { at: String, outcome: RestoreOutcome },
}
#[serde(rename_all = "kebab-case")] pub enum CloseReason { WentBack, PlanRemovedElsewhere, NotApplied }
```

Each line is `{"v":1, ...entry}`. The record's key is `(tweak, sid)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn intent_and_applied_outcome_open_a_settled_record() { /* one Intent + Outcome{applied:true}: open.len()==1, settled, prior and planned as written */ }
#[test] fn intent_without_outcome_is_unsettled() { /* settled == false */ }
#[test] fn failed_outcome_leaves_no_open_record() { /* Outcome{applied:false}: open is empty, history keeps both */ }
#[test] fn a_second_intent_keeps_the_first_prior() { /* same (tweak,sid) twice: open[0].prior is the first */ }
#[test] fn undone_and_closed_end_the_record() { /* each removes it from open; history keeps it */ }
#[test] fn records_are_per_tweak_and_account() { /* same tweak, two SIDs: two open records */ }
#[test] fn a_truncated_last_line_is_ignored_silently() { /* valid line + `{"v":1,"kind":"inte`: unreadable == 0 */ }
#[test] fn an_unreadable_middle_line_is_counted() { /* garbage between valid lines: unreadable == 1, others read */ }
#[test] fn an_unknown_tweak_is_refused() { /* tweak "format-c": unreadable == 1, not in open */ }
#[test] fn an_odd_sid_is_refused() { /* sid `S-1-5-21-1\..\Software`: refused. Only `S-` then digits and dashes passes */ }
#[test] fn a_gpu_value_name_must_be_the_game() { /* value_name `C:\evil.exe`: refused; `D:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe`: accepted; any other tweak with a value_name: refused */ }
#[test] fn a_saved_plan_guid_must_be_a_guid() { /* Saved::Plan{guid:"..\\x"}: refused */ }
#[test] fn append_then_read_round_trips() { /* JournalFile in a tempdir */ }
#[test] fn a_second_lock_is_refused() { /* two JournalFile on one dir: first lock() Ok, second Err with ErrorKind::WouldBlock until the first guard drops */ }
#[test] fn reading_a_missing_journal_creates_nothing() { /* new + read on a dir that doesn't exist: empty Replay, the dir still doesn't exist */ }
#[test] fn an_untrusted_folder_refuses_reads_and_locks() { /* a test Folder whose check fails: read and lock return that error */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-tweaks journal`
Expected: compile errors.

- [ ] **Step 3: Implement**

`JournalFile::new` touches nothing, so a PC that never pressed Apply has no folder. `lock()` runs `folder.prepare` once, then `File::try_lock` on `journal.lock`, returning `ErrorKind::WouldBlock` with "Kestrel is already changing settings in another window." `append` opens in append mode, writes one line, and calls `sync_data` before returning. `read` runs `folder.check` and returns an empty `Replay` when the file doesn't exist. Reads take no lock.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p kestrel-tweaks journal`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-tweaks
git commit -m "feat(tweaks): the journal, its replay and its distrust of its own file"
```

### Task 5: Settings targets and the plan rules for the Windows settings

**Files:**
- Create: `crates/kestrel-tweaks/src/targets.rs`, `crates/kestrel-tweaks/src/plan.rs`
- Modify: `crates/kestrel-core/src/gpu.rs`

**Interfaces:**
- Consumes: `System`, `Saved`, `tokens`, `info`, `kestrel_core::gpu`.
- Produces: `PlanInput`, `RowState`, `Row`, `ValueChange`, `PreviewItem` (shared contract);

```rust
// targets.rs: the five non-plan settings
pub fn current(sys: &dyn System, tweak: TweakId, sid: &str, value_name: Option<&str>) -> Read<Saved>;
pub fn target(tweak: TweakId, prior: &Saved) -> Saved;     // what Kestrel writes, keeping value_existed/key_existed from prior
pub fn write(sys: &dyn System, tweak: TweakId, sid: &str, value_name: Option<&str>, value: &Saved, accounts_differ: bool) -> io::Result<()>;
pub fn restore(sys: &dyn System, tweak: TweakId, sid: &str, value_name: Option<&str>, prior: &Saved, accounts_differ: bool) -> io::Result<()>;
pub fn same_meaning(tweak: TweakId, a: &Saved, b: &Saved) -> bool;
pub fn describe(tweak: TweakId, value: &Saved) -> String;  // "on", "off", "High performance", "Windows decides", "Power saving", "not set"
pub fn gpu_value_name(sys: &dyn System, sid: &str, exe: &str) -> Read<String>; // the existing value's own spelling, else `exe`
// plan.rs
pub fn setting_row(sys: &dyn System, input: &PlanInput, id: TweakId, sid: Option<&str>, open: Option<&OpenRecord>) -> Option<Row>; // None: hidden (windowed below 22621)
```

Keys and values come from the spec's Essential table. `PlanInput::from_snapshot`: `hybrid_cpu` is `p_cores.is_some()`; `gpu_choice` is true with `gpu::is_hybrid` or two or more non-virtual GPUs (add `pub fn hardware_gpu_count(gpus: &[GpuInfo]) -> usize` in `gpu.rs`); `valorant_exe` is `valorant::game_exe_path(&v.path)` with `/` replaced by `\`. `same_meaning` treats an absent `AutoGameModeEnabled` as 1, an absent `HistoricalCaptureEnabled` as 0, an absent `GpuPreference` token as "0", absent mouse strings as `1/6/10`, and ignores `live`, `value_existed` and `key_existed`. A windowed token has no default: absent equals only absent.

Row text, exact (`why`):

| Tweak, case | State, ticked | why |
| --- | --- | --- |
| Game Mode 0 | Available, yes | Tells Windows to favour the game you're playing. Ryzen X3D chips also need it to pick the right cores. |
| Game Mode absent or 1 | AlreadySet | Game Mode is already on. |
| Game Mode policy 0 | BlockedByPolicy | An organisation policy turns Game Mode off. |
| Recording 1, capture on, policy not 0 | Available, yes | Keeps recording gameplay in the background, which keeps the video encoder busy. Manual recording and screenshots keep working. |
| Recording otherwise | AlreadySet | Background recording is already off. |
| Windowed, token "1" | AlreadySet | Already on. |
| Windowed, `fullscreen_mode` 1 or 2 | Available, yes | Lets windowed modes present frames the faster way. Your VALORANT runs in a windowed mode. |
| Windowed, otherwise | Available, no | Only matters in windowed modes. |
| GPU, no `gpu_choice` | NotNeeded | One graphics card: nothing to choose. |
| GPU, no `valorant_exe` | NotNeeded | VALORANT not found. |
| GPU, token "2" | AlreadySet | VALORANT is already set to High performance. |
| GPU, token "1" | Available, no | You picked Power saving. |
| GPU, token absent or "0" | Available, yes | This PC has two graphics chips, and Windows can pick the weaker one. |
| Pointer, acceleration on | Available, no | Doesn't change aim or FPS in VALORANT, which reads the mouse directly. Only the cursor on the desktop changes. |
| Pointer, off | AlreadySet | Already off. |
| Any row, read failed | CouldntRead | Couldn't read this setting. |
| Any per-user row, `sid` None | CouldntRead | Couldn't tell which account is signed in. |
| Open record, current = planned | Applied, no | Kestrel changed this. |
| Open record, otherwise | ChangedElsewhere, no | Something else changed this after Kestrel did. |

- [ ] **Step 1: Write the failing tests**

One per row of the table above, named `game_mode_zero_is_available_and_ticked`, `game_mode_absent_is_already_set`, and so on, asserting `(state, ticked, why)`. Also:

```rust
#[test] fn windowed_is_hidden_below_22621() { /* build 22000: setting_row returns None; 22621: Some */ }
#[test] fn a_wrong_type_is_couldnt_read() { /* fake.fail_read("AutoGameModeEnabled"): CouldntRead, and current() is Read::Failed */ }
#[test] fn gpu_value_name_reuses_the_existing_spelling() {
    // A value `c:\riot games\valorant\live\shootergame\binaries\win64\valorant-win64-shipping.exe` exists:
    // gpu_value_name returns that spelling for the proper-case exe, and write() leaves exactly one value in the key.
}
#[test] fn gpu_exe_uses_backslashes() { /* from_snapshot with path "C:/Riot Games/VALORANT/live": valorant_exe ends in r"ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe" */ }
#[test] fn absent_means_the_default_when_comparing() { /* same_meaning(GameMode, Dword None, Dword Some(1)); same_meaning(BackgroundRecording, Dword None, Dword Some(0)); same_meaning(GpuPreference, Token None, Token Some("0")); !same_meaning(WindowedOptimizations, Token None, Token Some("0")) */ }
#[test] fn token_write_and_restore_leave_other_tokens() { /* string "VRROptimizeEnable=1;": write then restore with prior Token None: string is back, value still exists */ }
#[test] fn restore_removes_what_kestrel_created() { /* prior value_existed false, key_existed false: value and empty key are gone */ }
#[test] fn gpu_restore_deletes_rather_than_writing_zero() { /* no `GpuPreference=0;` left */ }
#[test] fn mouse_write_is_registry_only_when_accounts_differ() { /* accounts_differ: strings under the SID are 0/0/0 and set_mouse_live wasn't called with persist; same account: set_mouse_live([0,0,0], true) */ }
#[test] fn mouse_restore_puts_strings_and_live_back() { /* prior 1/6/10, live [6,10,1] */ }
```

Pointer's `timing` is `Now`, but its Preview says `NextSignIn` when accounts differ (Task 7).

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-tweaks`
Expected: compile errors.

- [ ] **Step 3: Implement `targets.rs` and `plan.rs`**

Reading "acceleration on" for the row: `mouse_live`'s third value is non-zero when the accounts match; when they differ, `MouseSpeed` under the SID is non-"0" (absent counts as on).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p kestrel-tweaks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-tweaks crates/kestrel-core/src/gpu.rs
git commit -m "feat(tweaks): the Windows settings' targets and plan rules"
```

### Task 6: The Kestrel power plan

**Files:**
- Create: `crates/kestrel-tweaks/src/power.rs`

**Interfaces:**
- Consumes: `System`, `PlanInput`, `Row`, `Saved::Plan`, `kestrel_core::cpu::is_dual_ccd_x3d`.
- Produces:

```rust
pub const KESTREL_PLAN_GUID: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";
pub enum Variant { Full, Light, NotOffered }
pub fn variant(input: &PlanInput, modern_standby: Option<bool>) -> Variant;
pub fn row(sys: &dyn System, input: &PlanInput, open: Option<&OpenRecord>) -> Row;
pub fn create_and_activate(sys: &dyn System, input: &PlanInput) -> Result<Saved, String>;   // returns the prior as Saved::Plan; rolls back on failure
pub fn preview(sys: &dyn System, input: &PlanInput) -> PreviewItem;
pub fn undo(sys: &dyn System, prior: &Saved) -> Result<String, String>;                    // Ok(message); Balanced fallback is named in it
pub fn remove(sys: &dyn System) -> Result<(), String>;                                     // deletes Kestrel's plan only when it isn't active
pub fn use_again(sys: &dyn System) -> Result<(), String>;
```

GUIDs and values: the spec's "Kestrel power plan" tables. `Full`: desktop (`FormFactor::Desktop`), `hybrid_cpu` false, `modern_standby` not `Some(true)`. `NotOffered`: `is_dual_ccd_x3d`. Everything else is `Light`, including an unknown form factor. The plan's name is "Kestrel"; its description is "Created by Kestrel. Undo it from Kestrel's Tweaks tab."

Row text (`why`):

| Case | State, ticked | why |
| --- | --- | --- |
| `NotOffered` | NotNeeded | AMD's guidance for this chip is Balanced with Game Mode on. |
| Policy sets the plan | BlockedByPolicy | An organisation policy picks the power plan. |
| Kestrel's plan active, record open | Applied | Kestrel's plan is active. |
| Kestrel's plan exists, inactive, record open | PlanInactive | Another plan is active: [name]. |
| Kestrel's plan active, no record | AlreadySet | Kestrel's plan is already active. |
| Built-in Balanced or Power saver | Available, yes | [name] lets the CPU slow down between frames. Expect a small gain at most, and measure it on your PC. |
| Built-in High performance or Ultimate Performance | Available, no | You're already on a fast plan. Kestrel's also turns off USB and PCIe power saving. |
| Any other plan | Available, no | You're on [name]. Measure before and after to see which is better. |
| Active plan unreadable | CouldntRead | Couldn't read your power plan. |

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn variant_by_pc() { /* desktop one core type: Full. Laptop, Unknown form factor, hybrid_cpu, modern_standby Some(true): Light. "AMD Ryzen 9 9950X3D": NotOffered. "AMD Ryzen 7 9800X3D" desktop: Full */ }
// One test per row of the table above, asserting (state, ticked, why), with the plan names "Balanced" and "EXM Free Power Plan V8".
#[test] fn full_writes_all_three_on_a_high_performance_copy() {
    // fake: active EXM. create_and_activate: duplicate_plan(HIGH_PERFORMANCE, KESTREL); values 100, 0, 0; name "Kestrel"; active == KESTREL;
    // the returned prior is Plan { guid of EXM, name "EXM Free Power Plan V8" }; EXM's plan still exists.
}
#[test] fn light_leaves_parking_alone_on_a_balanced_copy() { /* base Balanced; USB and ASPM written; CPMINCORES not written */ }
#[test] fn values_are_clamped_to_the_machines_range() { /* plan_value_range(parking) = (0, 50): writes 50 */ }
#[test] fn a_missing_high_performance_falls_back_to_balanced() { /* fail_duplicate(HP): Full still creates from Balanced */ }
#[test] fn no_stock_plan_fails_and_leaves_nothing() { /* both fail: Err, plans() unchanged, active unchanged */ }
#[test] fn a_failed_activation_deletes_the_plan_again() { /* fail_activate(KESTREL): Err containing the reason; plan gone; prior still active */ }
#[test] fn a_leftover_inactive_plan_is_replaced() { /* Kestrel's plan exists inactive, no record: create deletes it first, ends active */ }
#[test] fn undo_returns_to_the_old_plan_and_deletes_kestrels() {}
#[test] fn undo_falls_back_to_balanced_when_the_old_plan_is_gone() { /* active Balanced; message == "Your previous plan is gone, so Kestrel switched to Balanced." */ }
#[test] fn remove_keeps_the_plan_now_active() { /* active Balanced after a manual switch: Kestrel's gone, Balanced still active, EXM's still there */ }
#[test] fn nothing_but_kestrels_plan_is_ever_deleted() { /* after every path above, every other plan still exists */ }
#[test] fn preview_compares_with_the_current_plan_and_says_the_rest_comes_from_the_base() {
    // changes: label "Core parking minimum cores", now from the active plan, after "100%"; "USB selective suspend"; "PCIe link-state power management".
    // note == "Switching plans also replaces everything else in EXM Free Power Plan V8 with the settings of High performance. Undo returns to your plan as it was."
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-tweaks power`
Expected: compile errors.

- [ ] **Step 3: Implement `power.rs`**

Built-in plans are recognised by GUID, including Ultimate Performance `e9a42b02-d5df-448d-aa00-03f14749eb61`. Policy: `read_policy_string(r"SOFTWARE\Policies\Microsoft\Power\PowerSettings", "ActivePowerScheme")` returning a value.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p kestrel-tweaks power`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-tweaks
git commit -m "feat(tweaks): the Kestrel power plan, its variants and its undo"
```

### Task 7: The engine: apply, undo, reconcile, recover

**Files:**
- Create: `crates/kestrel-tweaks/src/engine.rs`

**Interfaces:**
- Consumes: Tasks 3–6.
- Produces: `Gate`, `TweaksView`, `PreviewView`, `ItemOutcome`, `Engine` (shared contract).

Behaviour, from the spec's "How changes are recorded":

- **Apply, per item:** under the journal lock, read the current value; if the row isn't Available or ChangedElsewhere, skip it with `ok: false`, "Already set." A failed read gives `ok: false` and writes nothing. Append and flush the `Intent`; write; read back; if the read-back's meaning differs from the target, restore the prior when the value changed, append `Outcome{applied:false, detail}`, and report the reason. Otherwise append `Outcome{applied:true}`. One failure doesn't stop the others. `PowerPlan` uses `power::create_and_activate`, whose intent carries the prior `Saved::Plan` and planned `Saved::Plan { guid: KESTREL, name: "Kestrel" }`.
- **Reconcile**, at the start of `view`, `applied_ids` and `has_open_records`: for each open record of the signed-in account or machine-wide, compare the current value. Settled: current = prior closes with `Closed{WentBack}`. Unsettled (crash): current = planned appends `Outcome{applied:true, interrupted:true}`; current = prior appends `Closed{NotApplied}`; otherwise `Outcome{applied:true, interrupted:true}` so the row flags it. Each unsettled record adds a line to `interrupted`, such as "Kestrel was interrupted while changing Game Mode. It finished." or "... It didn't change." These stay in memory until the app closes. A journal write failing during reconcile is ignored: the next read tries again. A plan record stays open while Kestrel's plan exists; if the plan is gone it closes with `PlanRemovedElsewhere`.
- **Undo:** the three-way check. Current = planned: restore, then `Undone`. Current = prior: `Closed{WentBack}`, `ok: true`. Otherwise: `ok: false`, "Something else changed this after Kestrel did. Kestrel left it alone." `PowerPlan` with Kestrel's plan active uses `power::undo`, and with another plan active it's `remove_plan`. `SystemProtection` calls `turn_protection_off`.
- **Revert everything:** open records of this account and machine-wide, newest first, skipping `SystemProtection` and ChangedElsewhere rows; an inactive Kestrel plan is removed. Rows left alone come back with `ok: false`.
- **Apply again:** writes the target, appends `Reapplied`, keeps the first prior. **Restore anyway:** writes the prior whatever holds now, appends `Undone`.
- **Accounts:** `view` uses `account().signed_in`. `other_account` is `account().differs()`. Records of another SID count in `pending_other_accounts`, are never touched, and `has_open_records` counts them. In Preview, pointer precision's timing is `NextSignIn` when accounts differ.
- **`any_active`:** any open record of this account or machine-wide, except `SystemProtection`. The `SystemProtection` row appears in `rows` only while its record is open, with why "System Protection is on because Kestrel turned it on. Turning it off deletes this drive's restore points." and state `Applied`.
- **Preview locations:** plain registry paths with `HKCU\` as the shorthand, such as `HKCU\Software\Microsoft\GameBar, AutoGameModeEnabled`; the plan's is "Power plan".
- If the journal can't be locked or appended to, `apply` returns the error before any change.

- [ ] **Step 1: Write the failing tests**

All use `FakeSystem` and a tempdir journal.

```rust
#[test] fn apply_then_undo_restores_everything_for_each_tweak() {
    // For each Essential id with the fake arranged so its row is Available (values present, values absent, key absent):
    // let before = fake.dump(); apply; assert changed; undo; assert_eq!(fake.dump(), before); the view's row is back to Available.
}
#[test] fn the_intent_is_on_disk_before_the_write() { /* fake.fail_write: journal holds Intent then Outcome{applied:false} */ }
#[test] fn a_lying_readback_puts_the_prior_back() { /* lie_on_readback: ok false; value == prior */ }
#[test] fn one_failure_doesnt_stop_the_others() { /* two ids, first fails: second applied */ }
#[test] fn already_set_writes_nothing_and_records_nothing() { /* game mode absent: ok false, no journal entries, dump unchanged */ }
#[test] fn a_failed_read_writes_nothing() {}
#[test] fn a_refused_journal_means_no_change() { /* a second lock held: apply returns Err, dump unchanged */ }
#[test] fn a_value_back_at_the_prior_closes_quietly() { /* apply, then fake puts the prior back, view: row Available; dump unchanged by the view; history has Closed{WentBack} */ }
#[test] fn a_third_value_flags_the_row_and_undo_leaves_it() { /* windowed: fake sets token "0" (prior absent): ChangedElsewhere; undo ok false; string unchanged */ }
#[test] fn restore_anyway_writes_the_prior() {}
#[test] fn apply_again_keeps_the_first_prior() { /* after flag: apply_again; undo returns to the first prior */ }
#[test] fn later_token_changes_survive_undo() { /* apply windowed; fake adds VRROptimizeEnable=1; undo: VRR still there */ }
#[test] fn crash_after_the_write_is_settled_as_applied() { /* hand-written journal with Intent only, fake holding planned: view Applied, interrupted has one line */ }
#[test] fn crash_before_the_write_closes_as_not_applied() { /* fake holding prior: row Available, Closed{NotApplied} */ }
#[test] fn crash_with_a_third_value_is_flagged() {}
#[test] fn revert_everything_goes_newest_first_and_leaves_flagged_rows() { /* apply A then B: fake's write log shows B restored before A; a flagged row is left and ok false */ }
#[test] fn revert_everything_leaves_system_protection_on() {}
#[test] fn inactive_kestrel_plan_row_and_remove() { /* apply plan; fake switches to Balanced: PlanInactive; remove_plan: Balanced active, Kestrel gone, record closed; use_plan_again path: Kestrel active, row Applied */ }
#[test] fn another_accounts_records_wait() { /* journal record with other SID: row unaffected, pending_other_accounts 1, revert_all leaves it, has_open_records true */ }
#[test] fn no_signed_in_account_still_allows_the_plan() { /* set_account(None): per-user rows CouldntRead; plan apply works */ }
#[test] fn applied_ids_lists_only_applied_rows() { /* flagged rows and an inactive plan aren't listed; ids are as_str() */ }
#[test] fn accounts_differ_preview_says_next_sign_in_for_the_pointer() {}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-tweaks engine`
Expected: compile errors.

- [ ] **Step 3: Implement `engine.rs`**

Keep `Engine` free of OS calls beyond `System`. `batch` is the apply's start time string.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p kestrel-tweaks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-tweaks
git commit -m "feat(tweaks): the engine: apply, undo, reconcile and crash recovery"
```

### Task 8: The Kestrel report

**Files:**
- Create: `crates/kestrel-tweaks/src/report.rs`
- Modify: `crates/kestrel-tweaks/src/engine.rs` (`Engine::report`)

**Interfaces:**
- Consumes: `Replay`, `Entry`, `targets::describe`, `info`.
- Produces: `report::text(replay: &Replay, kestrel_version: &str, build: u32, now: &str, unreadable_note: bool) -> String`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_report_matches_the_spec() {
    let text = report::text(&journal_with_the_specs_example(), "0.5.0", 26200, "2026-11-02T18:20:00+00:00", false);
    assert_eq!(text, "\
Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20
Active
  Kestrel power plan: EXM Free Power Plan V8 → Kestrel (2026-11-02 18:12)
History
  2026-11-02 18:15 Undid Game Mode: back to off
  2026-11-02 18:12 Applied Game Mode: off → on
  2026-11-02 18:12 Applied Kestrel power plan: EXM Free Power Plan V8 → Kestrel
  2026-11-02 18:12 Restore point created (#57)");
}
#[test] fn no_changes_says_so() { /* header + "No changes yet." */ }
#[test] fn every_entry_kind_has_a_line() {
    // Reused: "Restore point reused from 2026-11-02 09:00 (#56)". Skipped: "Restore point skipped". Failed: "Restore point failed: [reason]".
    // Reapplied: "Applied Game Mode again". WentBack: "Game Mode went back outside Kestrel". PlanRemovedElsewhere: "Kestrel power plan was removed outside Kestrel".
    // Failed outcome: "Couldn't apply Game Mode: [detail]". SystemProtection: "Applied System Protection: off → on".
    // NotApplied: no line. Unsettled intents: no line.
}
#[test] fn unreadable_lines_are_warned_about() { /* a trailing line "Part of Kestrel's history couldn't be read." */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-tweaks report`
Expected: compile errors.

- [ ] **Step 3: Implement `report.rs` and `Engine::report`**

- [ ] **Step 4: Run the tests**

Run: `cargo test -p kestrel-tweaks report`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-tweaks
git commit -m "feat(tweaks): the Kestrel report"
```

### Task 9: Windows implementation, and the round trip on GitHub

**Files:**
- Create: `crates/kestrel-tweaks/src/win/mod.rs`, `win/registry.rs`, `win/power.rs`, `win/mouse.rs`, `win/folder.rs`, `crates/kestrel-tweaks/src/folder.rs`, `crates/kestrel-tweaks/src/bin/kestrel-tweak-roundtrip.rs`
- Modify: `crates/kestrel-tweaks/Cargo.toml`, `src/lib.rs`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `System`, `Engine`, `PlanInput`, `kestrel_probe::collect_report`.
- Produces: `kestrel_tweaks::this_pc() -> Arc<dyn System>` (`WinSystem` on Windows; a `FakeSystem` with a stock Balanced PC elsewhere); `kestrel_tweaks::journal_file() -> JournalFile` (`AdminFolder` on Windows, `PlainFolder` elsewhere); `folder::owner_is_trusted(owner: &str, me: &str) -> bool`.

Research sections 1–6 and 9 have the mechanisms. `protection`, `turn_protection_on` and `turn_protection_off` return `Readiness::Unknown` and an `Unsupported` error until Task 10. The journal's folder is `FOLDERID_ProgramData` + `Kestrel` through `SHGetKnownFolderPath`, not an environment variable; off Windows, `std::env::temp_dir()/kestrel-dev`.

`AdminFolder::check`: a missing folder is fine; otherwise read its owner with `GetNamedSecurityInfoW`, and if `owner_is_trusted` is false, fail with `ErrorKind::PermissionDenied`, "C:\ProgramData\Kestrel belongs to another account. Delete it, then try again." `AdminFolder::prepare`: `check`, create the folder if absent, then apply `SetNamedSecurityInfoW` with the owner and protected DACL from SDDL `O:BAD:PAI(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)`.

Registry: only `ERROR_FILE_NOT_FOUND` maps to `Read::Absent`. A value of the wrong type is `Read::Failed`; check the raw type with `get_raw_value`.

Round trip, `kestrel-tweak-roundtrip --yes-change-this-machine` (without the flag it prints what it would do and exits 2; off Windows it prints "Windows only" and exits 0):

1. Use `account().signed_in`, falling back to `this_process`, and print which. Save raw copies of every value below, the active plan and the plan list.
2. Arrange: `AutoGameModeEnabled`=0, `HistoricalCaptureEnabled`=1, mouse acceleration on with strings `1/6/10`, no `SwapEffectUpgradeEnable` token, and `DirectXUserGlobalSettings` holding `VRROptimizeEnable=1;`. Take the arranged state as `before`.
3. `PlanInput`: desktop, one core type, `gpu_choice: true`, `valorant_exe: Some(r"C:\KestrelRoundTrip\VALORANT-Win64-Shipping.exe")`, `fullscreen_mode: Some(1)`, this machine's build.
4. `Engine::apply` all six; assert every `ok`. Read the raw values and assert each target. Assert the active plan is `KESTREL_PLAN_GUID` with the three values read back.
5. `kestrel_probe::collect_report`: assert `game-mode`, `background-recording`, and `power-plan` are `Status::Pass`.
6. `Engine::revert_all`; assert every `ok`, raw values equal `before` (`VRROptimizeEnable=1;` still there, no GPU value), and Kestrel's plan gone with the old plan active.
7. Wrong-type check: write `AutoGameModeEnabled` as a string; assert `read_dword` is `Read::Failed` and the row is CouldntRead, and that `Engine::apply` leaves the string in place.
8. Restore the raw copies from step 1 and remove the journal folder. Exit non-zero on any mismatch, printing what differed. Restoring happens on failure too.

The restore point and the uninstaller aren't exercised here; the job's output says so.

- [ ] **Step 1: Write the failing test for the pure part**

```rust
// folder.rs
#[test]
fn only_admins_system_or_this_process_own_the_folder() {
    assert!(owner_is_trusted("S-1-5-32-544", "S-1-5-21-1-1001"));
    assert!(owner_is_trusted("S-1-5-18", "S-1-5-21-1-1001"));
    assert!(owner_is_trusted("S-1-5-21-1-1001", "S-1-5-21-1-1001"));
    assert!(!owner_is_trusted("S-1-5-21-1-1002", "S-1-5-21-1-1001"));
    assert!(!owner_is_trusted("", "S-1-5-21-1-1001"));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p kestrel-tweaks folder`
Expected: compile error.

- [ ] **Step 3: Implement the Windows code**

`windows-sys` features: `Win32_Foundation`, `Win32_System_Power`, `Win32_System_SystemServices`, `Win32_System_Registry`, `Win32_UI_WindowsAndMessaging`, `Win32_UI_Shell`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_System_Com`, `Win32_Storage_FileSystem`, `Win32_System_Memory` (`LocalFree`). `winreg` 0.56. `PowerEnumerate` lists plans; `PowerReadFriendlyName` names them. `PowerDuplicateScheme` takes a pointer to a GUID pointer: point it at a `GUID` parsed from `new`. `AoAc` from `GetPwrCapabilities`. Write AC values only. After writing a value on the active scheme, call `PowerSetActiveScheme` again.

- [ ] **Step 4: Lint for Windows from the Mac**

Run: `cargo clippy --target x86_64-pc-windows-msvc -p kestrel-tweaks --all-targets -- -D warnings && cargo test -p kestrel-tweaks`
Expected: no warnings; PASS.

- [ ] **Step 5: Wire CI and release**

In `ci.yml`'s Windows job: add `-p kestrel-tweaks` to the clippy and test lines, and after "Probe this runner" add a step named "Apply, check and undo every tweak on this machine" running `cargo run --locked -q -p kestrel-tweaks --bin kestrel-tweak-roundtrip -- --yes-change-this-machine`. In `release.yml`: the same step before the Tauri action, so a failure stops the release. Add `-p kestrel-tweaks` to the Linux and macOS test jobs that list crates.

- [ ] **Step 6: Commit**

```bash
git add crates/kestrel-tweaks .github/workflows Cargo.lock
git commit -m "feat(tweaks): the Windows implementation and an apply-check-undo round trip on GitHub"
```

- [ ] **Step 7: Prove it on Windows**

The controller, not the implementer, pushes `stage-5` and reads the Windows job's result. A round-trip failure goes back to this task as a fix round before Task 10 starts.

### Task 10: The restore point

**Files:**
- Create: `crates/kestrel-tweaks/src/restore.rs`, `crates/kestrel-tweaks/src/win/restore.rs`
- Modify: `crates/kestrel-tweaks/src/win/mod.rs`, `crates/kestrel-tweaks/src/lib.rs`, `crates/kestrel-tweaks/src/engine.rs`, `crates/kestrel-tweaks/Cargo.toml`

**Interfaces:**
- Consumes: `System`, `Engine`, `Saved::Protection`.
- Produces: `Readiness`, `RestoreChoice`, `RestoreOutcome` (shared contract), and:

```rust
pub struct Point { pub sequence: u32, pub time: String }
pub enum CreateError { ProtectionOff, Other(String) }
pub trait RestorePoints { fn newest(&self) -> Result<Option<Point>, String>; fn create(&self, description: &str) -> Result<u32, CreateError>; }
pub fn make_point(points: &dyn RestorePoints, description: &str, wait: &mut dyn FnMut()) -> RestoreOutcome;
pub const DESCRIPTION: &str = "Kestrel: before applying tweaks";
pub const HELPER_FLAG: &str = "--restore-point";
pub const HELPER_TIMEOUT: Duration = Duration::from_secs(180);
/// Runs in the short-lived helper process, on Windows: prints one line of RestoreOutcome JSON to stdout. Exit code 0.
pub fn helper_main() -> i32;
/// Runs in the app: spawns `exe --restore-point`, waits up to HELPER_TIMEOUT, kills it past that, parses its line.
pub fn run_helper(exe: &Path) -> RestoreOutcome;
```

`make_point`: read `newest` as `before`; `create`; on `Ok(seq)`, if `before` has that same sequence, it's `Reused` with `before`'s time. Otherwise poll `newest`, calling `wait` between polls, up to 30 times; a sequence above `before`'s is `Created`. If none shows up: `Failed { reason: "Windows didn't list the new restore point." }`. `CreateError::ProtectionOff` is `Failed { reason: "System Protection is off." }`.

Windows helper: `CoInitializeEx` and Microsoft's `CoInitializeSecurity` sample (research section 8) in the helper only, `LoadLibraryExW("SrClient.dll", LOAD_LIBRARY_SEARCH_SYSTEM32)` and `GetProcAddress("SRSetRestorePointW")` (never `windows-sys`'s binding), `BEGIN_SYSTEM_CHANGE` + `MODIFY_SETTINGS`, then `END_SYSTEM_CHANGE` with the returned sequence only when a new point opened. `ERROR_SERVICE_DISABLED` is `ProtectionOff`. `newest` queries `root\default`, `SELECT SequenceNumber, CreationTime FROM SystemRestore` through `wmi`.

`WinSystem::protection`: policy `DisableSR` or `DisableConfig` under `SOFTWARE\Policies\Microsoft\Windows NT\SystemRestore` non-zero is `BlockedByPolicy`; `SPP\Clients` value `{09F7EDC5-294E-4180-AF6A-FB0E6A0E9513}` containing the system drive's volume GUID (`GetVolumeNameForVolumeMountPointW`) is `Ready`; a readable value without it is `ProtectionOff`; otherwise `Unknown`. `turn_protection_on`/`off` call WMI `SystemRestore.Enable` and `Disable` with the system drive through `exec_class_method`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_higher_sequence_is_created() { /* before 56, create Ok(57), newest 57 on the second poll: Created{57} */ }
#[test] fn the_same_sequence_is_reused_with_its_time() { /* before {56, t}, create Ok(56): Reused{56, t}, no polling */ }
#[test] fn success_without_a_listing_fails_after_thirty_polls() { /* wait called 30 times */ }
#[test] fn protection_off_is_reported() {}
#[test] fn no_earlier_point_still_works() { /* before None, create Ok(1), newest 1: Created{1} */ }
#[test] fn run_helper_fails_cleanly_on_garbage() { /* parse helper: outcome_from_line("not json") is Failed */ }
// engine.rs
#[test] fn turning_protection_on_is_recorded_and_undoable() { /* turn_protection_on: SystemProtection row Applied; undo calls turn_protection_off; revert_all doesn't */ }
#[test] fn the_restore_outcome_is_in_the_report() {}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-tweaks restore`
Expected: compile errors.

- [ ] **Step 3: Implement**

- [ ] **Step 4: Run the checks**

Run: `cargo test -p kestrel-tweaks && cargo clippy --target x86_64-pc-windows-msvc -p kestrel-tweaks --all-targets -- -D warnings`
Expected: PASS and no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-tweaks Cargo.lock
git commit -m "feat(tweaks): the restore point, in a helper process, reported truthfully"
```

### Task 11: Benchmarks remember the tweaks

**Files:**
- Modify: `crates/kestrel-bench/src/model.rs`, `src/result.rs`, `src/text.rs`, `src/store.rs` (test only)
- Modify: `app/src/api/types.ts`, `app/src/api/sample-bench.ts`, `app/src/components/bench/ResultDetail.tsx`, `app/src/strings.ts`, `app/src/styles/bench.css` if needed

**Interfaces:**
- Produces: `Context.applied_tweaks: Option<Vec<String>>`; `Comparison.tweaks_added: Vec<String>`, `tweaks_removed: Vec<String>`, `power_plan_change: Option<[String; 2]>`; `result::varies_too_much(spread: f64) -> bool` (true above 0.10). `strings.tweaks.names: Record<TweakId, string>` for the titles, matching the catalog.

- [ ] **Step 1: Write the failing tests**

```rust
// store.rs
#[test] fn a_benchmark_from_before_stage_5_loads_with_no_tweak_list() { /* existing old-JSON fixture: context.applied_tweaks == None */ }
// result.rs
#[test] fn the_comparison_lists_tweak_changes() { /* base Some(["game-mode"]), now Some(["game-mode","power-plan"]): added ["power-plan"], removed [] */ }
#[test] fn a_side_with_no_list_compares_nothing() { /* base None: both lists empty */ }
#[test] fn a_changed_plan_is_named() { /* "EXM Free Power Plan V8" → "Kestrel": Some(["EXM Free Power Plan V8","Kestrel"]); same name or a missing side: None */ }
#[test] fn runs_varying_over_ten_percent_are_flagged() { assert!(varies_too_much(0.249)); assert!(!varies_too_much(0.10)); }
// text.rs
#[test] fn copy_results_says_what_changed_and_warns_about_spread() {
    // Lines, each exact:
    // "These runs vary by 24.9%, so only big changes will count as real. Close background apps and run again."
    // "Tweaks since Starting point: added power-plan; removed game-mode"
    // "Power plan since Starting point: EXM Free Power Plan V8 → Kestrel"
    // A spread of 5% has no warning line; no tweak changes, no tweaks line.
}
```

UI: `ResultDetail.test.tsx` with `renderToStaticMarkup`, following `HwPanel.test.tsx`: the warning shows at spread 0.249 and not at 0.05; a comparison with `tweaksAdded: ['power-plan']` shows "Kestrel power plan".

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel-bench` and, in `app/`, `npm test`
Expected: FAIL.

- [ ] **Step 3: Implement**

`applied_tweaks` is additive with `serde(default)`, so old files load. The warning sits next to the spread in `ResultDetail`; the tweak and plan lines sit next to the existing `settingsDiffer` caution. Add `appliedTweaks`, `tweaksAdded`, `tweaksRemoved`, `powerPlanChange` to `types.ts` and the sample.

- [ ] **Step 4: Run the checks**

Run: `cargo test -p kestrel-bench`, and in `app/`, `npm test && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kestrel-bench app/src
git commit -m "feat(bench): results remember the active tweaks and warn about wide spreads"
```

### Task 12: The app layer: commands, gate, flags

**Files:**
- Create: `app/src-tauri/src/tweaks/mod.rs`, `app/src-tauri/src/tweaks/commands.rs`, `app/src-tauri/src/cli.rs`
- Modify: `app/src-tauri/src/lib.rs`, `app/src-tauri/src/main.rs`, `app/src-tauri/Cargo.toml`, `app/src-tauri/src/bench/commands.rs`, `app/src-tauri/src/bench/controller.rs`

**Interfaces:**
- Consumes: `Engine`, `this_pc`, `journal_file`, `run_helper`, `helper_main`, `PlanInput`.
- Produces, all `#[tauri::command(async)]` returning `Result<_, String>`:

| Command | Arguments | Returns |
| --- | --- | --- |
| `tweaks_view` | | `TweaksView` |
| `tweaks_preview` | `ids: Vec<TweakId>` | `PreviewView` |
| `tweaks_restore_point` | `choice: RestoreChoice` | `RestoreOutcome` |
| `tweaks_apply` | `ids: Vec<TweakId>` | `Vec<ItemOutcome>` |
| `tweaks_undo`, `tweaks_apply_again`, `tweaks_restore_anyway` | `id: TweakId` | `ItemOutcome` |
| `tweaks_revert_all` | | `Vec<ItemOutcome>` |
| `tweaks_use_plan_again`, `tweaks_remove_plan` | | `ItemOutcome` |
| `tweaks_report` | | `String` |

- `Controller::session_active(&self) -> bool`: true while a session is `Waiting`, `Capturing` or in `Review`, or `Done` with no result yet.
- `pub fn gate(game_running: bool, session_active: bool) -> Gate`: session first, then game. Every mutating command returns the gate's message when it's closed: "Close VALORANT first." or "Finish the benchmark first."
- The engine is an `Arc<Engine>` managed from `setup`; building it touches nothing, and a refused journal folder surfaces as the command's error.
- `tweaks_restore_point`: `Skip` records `Skipped`; `TurnOnAndCreate` calls `engine.turn_protection_on()` first; both creating choices call `run_helper(current_exe)` off the async runtime's thread and record the outcome.
- `PlanInput` comes from `latest_report` and `controller.game_settings().and_then(|s| s.fullscreen_mode)`.
- `bench_start` sets `context.applied_tweaks = Some(engine.applied_ids())`.
- `get_diagnostics` adds `"tweaks": { "report": ..., "accountsDiffer": ... }`.
- `cli::handle(args: &[String], journal: impl Fn() -> JournalFile) -> Option<i32>`: `--restore-point` runs `helper_main`; `--tweaks-has-changes` returns 0 when any record is open, 1 when none (a missing folder is none, and it stays missing), 2 on error; `--tweaks-revert-all` returns 0 when everything was undone, 3 when something was left; anything else returns None. `main.rs` first calls `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR)` on Windows, then `cli::handle`, then `run()`.
- Off Windows, `this_pc()` gives the fake, so the app runs on the Mac.

- [ ] **Step 1: Write the failing tests**

```rust
// tweaks/mod.rs
#[test] fn the_gate_names_the_benchmark_first() {
    assert_eq!(gate(true, true), Gate::BenchmarkActive);
    assert_eq!(gate(true, false), Gate::GameRunning);
    assert_eq!(gate(false, false), Gate::Open);
}
// cli.rs: tempdir journal
#[test] fn has_changes_reads_the_journal() { /* empty journal: Some(1); one open record: Some(0) */ }
#[test] fn an_unrelated_flag_starts_the_app() { assert_eq!(handle(&["--whatever".into()], ...), None); assert_eq!(handle(&[], ...), None); }
// controller.rs, in its existing fake-platform tests
#[test] fn a_session_is_active_until_its_result_is_ready() { /* none: false; Waiting: true; Failed and Cancelled: false */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p kestrel`
Expected: compile errors.

- [ ] **Step 3: Implement**

Register the commands in `lib.rs`. `lib.rs`'s top comment no longer says Kestrel writes only inside its own folder: say it writes to Windows only through `kestrel-tweaks`, when the person presses Apply.

- [ ] **Step 4: Run the checks**

Run: `cargo test -p kestrel && cargo clippy -p kestrel --all-targets -- -D warnings`
Expected: PASS and no warnings. This needs `app/dist` built, as in CI's `app-macos` job.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri Cargo.lock
git commit -m "feat(app): tweak commands, the apply gate and command-line flags"
```

### Task 13: The Tweaks page

**Files:**
- Create: `app/src/pages/Tweaks.tsx`, `app/src/components/tweaks/TweakRow.tsx`, `PreviewPanel.tsx`, `RestoreStep.tsx`, `ReportPanel.tsx`, `app/src/api/tweaks.ts`, `app/src/api/tweaks.test.ts`, `app/src/api/sample-tweaks.ts`, `app/src/api/sample-tweaks.test.ts`, `app/src/pages/Tweaks.test.tsx`, `app/src/styles/tweaks.css`
- Modify: `app/src/api/client.ts`, `app/src/api/types.ts`, `app/src/App.tsx`, `app/src/strings.ts`, `app/src/strings.test.ts`, `app/src/main.tsx` (stylesheet)

**Interfaces:**
- Consumes: the commands in Task 12, `tweakFocus` from Task 1.
- Produces: `Tweaks` page with props `{ focus: TweakId | null; onFocusUsed: () => void; onRescan: () => void; onTab: (tab: Tab) => void }`; `measureNote(list: BenchmarkSummary[]): MeasureNote | null` in `api/tweaks.ts`, where `MeasureNote` is `{ kind: 'none' } | { kind: 'spread'; pct: string }`.

Layout and copy follow the spec's "What a person sees": a scrolling page of panels in the current look, built from the Hardware tab's classes. Labels in `strings.tweaks`:

- `intro`: "Kestrel picked these for your PC. Nothing changes until you press Apply."
- `preview(n)`: `` `Preview ${n} change${n === 1 ? '' : 's'}` ``
- `measureNone`: "Run a benchmark first, so you can compare afterwards."
- `measureSpread(pct)`: `` `Your starting point's runs vary by ${pct}, so small changes won't show. Run a clean one before applying.` ``
- `otherAccount`: "Kestrel is running as another account. Changes go to the account signed in now."
- `otherPending(n)`: `` `Another account has ${n} change(s) from Kestrel. Those wait until that account is signed in.` ``
- `gate`: `{ 'game-running': 'Close VALORANT first.', 'benchmark-active': 'Finish the benchmark first.' }`
- `states`, `impact` and `timing` labels per the spec's tables: "Next time VALORANT opens", "Next sign-in", "Now"; impact "None in game".
- `restoring`: "Creating a restore point… this can take a minute."
- `protectionOff`: "System Protection is off on your Windows drive. Turn it on and create a restore point? Windows will use some disk space for restore points."
- `restoreFailed(reason)`: `` `Couldn't create a restore point: ${reason}. Continue with Kestrel's undo only?` ``
- `done`: "Measure it on your PC: run a benchmark and compare it with your starting point."
- `restartValorant`: "Restart VALORANT for these to take effect."
- `revertConfirm`: "Undo every change on this list?"
- `protectionWarning`: "Turning System Protection off deletes this drive's restore points."
- `laterTiers`: Competitive and Extreme, each with `strings.comingSoon.arrives(STAGE.competitive)`.

Flow on **Apply** (in `PreviewPanel` and `RestoreStep`):

1. `readiness` from the preview: `ready` or `unknown` calls `tweaksRestorePoint('create')`. `protection-off` shows the dialog with **Turn on and create**, **Continue without**, **Cancel**. `blocked-by-policy` shows **Continue without** and **Cancel**, naming the policy.
2. A `failed` outcome shows `restoreFailed` with **Continue** and **Cancel**.
3. `tweaksApply(ids)`, then `onRescan()` and re-read the view, then show each `ItemOutcome`, `restartValorant` if any applied item's timing is `next-launch`, `done`, and a button to the Benchmark tab.

`focus` opens the Preview with only that tweak, then calls `onFocusUsed`. Row controls per the spec's state table: **Undo** on Applied; **Apply again** and **Restore anyway** on ChangedElsewhere (Restore anyway asks first); **Use it again** and **Remove it** on PlanInactive; `protectionWarning` beside the System Protection row's Undo. **Revert everything** shows when `anyActive`, and lists the rows first. **Copy report** uses `CopyButton`.

`sample-tweaks.ts` is a stateful browser-preview fake of the EXM owner's PC: plan row unticked and naming "EXM Free Power Plan V8", Game Mode and recording Already set, pointer Available, windowed Available, GPU Not needed; apply and undo change its state and its report.

- [ ] **Step 1: Write the failing tests**

```ts
// tweaks.test.ts
it('asks for a benchmark when there is no baseline', () => expect(measureNote([])).toEqual({ kind: 'none' }));
it('flags a baseline whose runs vary over 10%', () => expect(measureNote([summary({ isBaseline: true, spread: 0.249 })])).toEqual({ kind: 'spread', pct: '24.9%' }));
it('stays quiet for a clean baseline', () => expect(measureNote([summary({ isBaseline: true, spread: 0.05 })])).toBeNull());
// sample-tweaks.test.ts
it('walks apply and undo back to where it started', async () => { /* view, apply power-plan: Applied and in the report; undo: Available */ });
// Tweaks.test.tsx with renderToStaticMarkup of the row and panels
it('shows an Undo on an applied row and no tick box', ...);
it('shows Apply again and Restore anyway on a changed row', ...);
it('shows Use it again and Remove it on an inactive plan', ...);
it('shows the gate reason instead of an enabled Preview when VALORANT is open', ...);
it('never shows a tick box on Already set or Not needed', ...);
it('lists each change in the Preview: now, after, timing and the plan note', ...);
it('asks once before Revert everything, listing the rows', ...);
it('shows the report text with Copy report', ...);
// strings.test.ts: remove comingSoon.tweaks; Lock In's assertion stays; STAGE no longer has tweaks
```

- [ ] **Step 2: Run them to verify they fail**

Run in `app/`: `npm test`
Expected: FAIL.

- [ ] **Step 3: Implement**

Remove `STAGE.tweaks`, `strings.comingSoon.tweaks`, and the ComingSoon branch for Tweaks in `App.tsx`. Add the browser-mode branch of each `tweaks*` function in `client.ts`, as the `bench*` functions do.

- [ ] **Step 4: Run the checks, then look at it**

Run in `app/`: `npm test && npx tsc --noEmit`
Expected: PASS.

Run `npm run dev` and walk the browser preview through tick, Preview, Apply, Undo and Revert everything at 1180×760 and 1024×680; confirm no horizontal scroll and that a long plan name wraps.

- [ ] **Step 5: Commit**

```bash
git add app/src
git commit -m "feat(app): the Tweaks tab with Preview, Apply, Undo and the Kestrel report"
```

### Task 14: First run and "See your plan"

**Files:**
- Modify: `app/src/first-run/FirstRun.tsx`, `app/src/first-run/flag.ts`, `app/src/App.tsx`, `app/src/pages/Benchmark.tsx`, `app/src/components/bench/ResultDetail.tsx`, `app/src/pages/Tweaks.tsx`, `app/src/strings.ts`
- Test: `app/src/first-run/flag.test.ts`, `app/src/pages/Tweaks.test.tsx`

**Interfaces:**
- Produces: `planNoteWanted(): boolean`, `askForPlanNote(): void`, `planNoteSeen(): void` in `flag.ts`, stored under `kestrel.planNote`.

Decision 22: the result step's **Not now** calls `onDone(report, 'tweaks')` and `askForPlanNote()`. **Measure now** also calls `askForPlanNote()` and goes to the Benchmark tab. `App.finishFirstRun` takes `'home' | 'benchmark' | 'tweaks'`. The first saved benchmark's result (`items.length === 1`, `planNoteWanted()` true) shows **See your plan**, which goes to Tweaks. The Tweaks page shows `strings.tweaks.firstRunNote` ("Here's the plan Kestrel picked for your PC. Nothing changes until you press Apply.") with **Not now**, which calls `planNoteSeen()` and goes Home, while `planNoteWanted()`. Applying also calls `planNoteSeen()`. People whose first run is already done never see it.

- [ ] **Step 1: Write the failing tests**

```ts
it('wants the note only after askForPlanNote, until it is seen', () => {
  expect(planNoteWanted()).toBe(false);
  askForPlanNote();
  expect(planNoteWanted()).toBe(true);
  planNoteSeen();
  expect(planNoteWanted()).toBe(false);
});
it('shows the first-run note and its Not now on the Tweaks page while wanted', ...);
```

- [ ] **Step 2: Run them to verify they fail**

Run in `app/`: `npm test`
Expected: FAIL.

- [ ] **Step 3: Implement**

- [ ] **Step 4: Run the checks**

Run in `app/`: `npm test && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src
git commit -m "feat(app): the first run ends at the plan"
```

### Task 15: The uninstaller, the launch check and v0.5.0

**Files:**
- Create: `app/src-tauri/windows/hooks.nsh`
- Modify: `app/src-tauri/tauri.conf.json`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`, `Cargo.toml`, `Cargo.lock`, `crates/kestrel-probe/Cargo.toml`, `crates/kestrel-tweaks/Cargo.toml`, `app/package.json`, `app/package-lock.json`, `app/src/api/mock-report.json`

**Interfaces:**
- Consumes: `--tweaks-has-changes` and `--tweaks-revert-all` from Task 12.

`hooks.nsh`:

```nsh
!macro NSIS_HOOK_PREUNINSTALL
  ; Tauri's installer runs this uninstaller in place with _?= during an upgrade, so $EXEDIR is the install folder;
  ; a person's uninstall runs from the copy NSIS makes in a temp folder. Only a real uninstall asks.
  ${If} $UpdateMode <> 1
  ${AndIf} $EXEDIR != $INSTDIR
  ${AndIfNot} ${Silent}
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --tweaks-has-changes' $0
    ${If} $0 = 0
      MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON1 "Undo Kestrel's changes before removing it?" IDNO kestrel_keep
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --tweaks-revert-all' $0
      ${If} $0 <> 0
        MessageBox MB_OK|MB_ICONEXCLAMATION "Kestrel couldn't undo everything. Check those settings in Windows."
      ${EndIf}
      kestrel_keep:
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; "Delete the application data" also removes the undo history, after the question above.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ExpandEnvStrings $0 "%ProgramData%\Kestrel"
    RMDir /r "$0"
  ${EndIf}
!macroend
```

`tauri.conf.json`: `"nsis": { "installMode": "perMachine", "installerHooks": "windows/hooks.nsh" }`. The Kestrel window may still be open when the hook runs: the template's own check closes it right after, and the journal lock keeps the two from writing at once, so a refused undo lands in the "couldn't undo everything" message.

- [ ] **Step 1: Add the CI checks**

In `ci.yml`'s Windows job, after the installer check, a step named "Kestrel starts, and its flags answer":

```pwsh
& "target/release/Kestrel.exe" --tweaks-has-changes | Out-Null
if ($LASTEXITCODE -ne 1) { throw "--tweaks-has-changes should exit 1 with no changes, got $LASTEXITCODE" }
$p = Start-Process "target/release/Kestrel.exe" -PassThru
Start-Sleep -Seconds 15
if ($p.HasExited) { throw "Kestrel.exe exited on its own with code $($p.ExitCode)" }
Stop-Process -Id $p.Id -Force
```

This also proves `SetDefaultDllDirectories` doesn't stop the window from opening. If the runner's own session can't host the window (the process exits with a WebView2 error unrelated to the code), downgrade the second half to a warning and say so in the report.

- [ ] **Step 2: Update the release text and README**

`release.yml` body: "Test build. It changes your PC only when you press Apply, and every change can be undone. Before applying, run a benchmark so you can compare afterwards." README: add above Stage 3:

```markdown
Stage 5 (v0.5.0): the Tweaks tab. Essential tweaks picked for your PC, each with why it was picked, a Preview of every
change, Undo one at a time or for everything, a restore point, and a Kestrel report listing every change. Kestrel
changes your PC only when you press Apply. Stage 4, the other games, waits until someone can test them.
```

and rewrite "Next, in this order" so it lists stage 6 first, then 7, 8, 9, and then "the other games (FiveM, Rainbow Six Siege, Warzone), once someone can test them". Keep the older paragraphs.

- [ ] **Step 3: Bump to 0.5.0**

As commit `791f11e` did: the workspace `version`, `app/package.json`, `app/package-lock.json`, `tauri.conf.json`, the `kestrel-core` version in the crates' `Cargo.toml`, then `cargo update --workspace` and `npm run mock` in `app/` so `appVersion` follows.

- [ ] **Step 4: Run everything**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`, and in `app/`: `npm test && npx tsc --noEmit && npm run build`
Expected: PASS and no warnings. NSIS isn't on the Mac: CI's `tauri build` compiles the hooks.

- [ ] **Step 5: Commit**

```bash
git add -A app .github README.md Cargo.toml Cargo.lock crates
git commit -m "feat(installer): the uninstaller asks before undoing; version 0.5.0"
```

---

## Not in this plan

- The tracker's "What to test in stage 5" list and the release itself: the controller does those after the final review (decision 73).
- The owner's REAL PC checks in the spec's "What to check on the owner's PC": they're in the test list, and the app shows what they need.
- A brief for Claude Design: on request.
