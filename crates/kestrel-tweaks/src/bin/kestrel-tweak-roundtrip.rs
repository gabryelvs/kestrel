//! Applies every Essential tweak on this machine, checks what Windows holds, undoes them, checks that all of it is
//! back, and leaves the machine as it found it. It runs on GitHub's Windows machine, in CI and before a release,
//! because that is where Kestrel's Windows code first meets Windows. It isn't a `cargo test` because it changes the
//! machine it runs on, so it does nothing without its flag.
//!
//! Each step says what it did, plainly enough to tell what went wrong from a CI log alone. Every mismatch is noted and
//! the steps go on where they can, so one log says as much as possible. What a machine can't exercise is left out with
//! a warning, and only a tweak that is offered and doesn't work fails the run. Putting everything back happens whatever
//! the steps came to, a panic included, and the exit code is non-zero if anything differed.
//!
//! The values are read and restored with `winreg` directly, as their raw type and bytes, not through `System`: that is
//! what is being checked, and a REG_DWORD has to stay a REG_DWORD. Only the hive under `HKEY_USERS` is ever touched.

#[cfg(not(windows))]
fn main() {
    println!("Windows only");
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    roundtrip::main()
}

#[cfg(windows)]
mod roundtrip {
    use std::fmt;
    use std::fs;
    use std::io;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::process::{Command, ExitCode};
    use std::sync::Arc;

    use kestrel_core::health::Status;
    use kestrel_core::snapshot::FormFactor;
    use kestrel_core::tweak_id::TweakId;
    use kestrel_probe::account::account;
    use kestrel_probe::parse::user_key_path;
    use kestrel_tweaks::folder::journal_dir;
    use kestrel_tweaks::journal::JournalFile;
    use kestrel_tweaks::parse;
    use kestrel_tweaks::power::{variant, Variant, KESTREL_PLAN_GUID, KESTREL_PLAN_NAME};
    use kestrel_tweaks::targets::{
        ALLOW_AUTO_GAME_MODE, APP_CAPTURE, AUTO_GAME_MODE, DIRECTX_KEY, DIRECTX_SETTINGS,
        GAME_BAR_KEY, GAME_DVR_KEY, HISTORICAL_CAPTURE, MOUSE_KEY, MOUSE_SPEED, MOUSE_THRESHOLD_1,
        MOUSE_THRESHOLD_2,
    };
    use kestrel_tweaks::win::{AdminFolder, WinSystem};
    use kestrel_tweaks::{journal_file, Engine, Gate, PlanInput, Read, RowState, System};
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_REQUIRES_INTERACTIVE_WINDOWSTATION,
    };
    use winreg::enums::{HKEY_USERS, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_DWORD, REG_SZ};
    use winreg::{RegKey, RegValue};

    const FLAG: &str = "--yes-change-this-machine";

    const WOULD_DO: &str = "\
This program tests Kestrel's Windows code by changing this PC's settings, so it only runs when told to.
Nothing has been changed. With --yes-change-this-machine it would:

  1. Save Game Mode, background recording, the DirectX settings, the mouse and the power plan as they are now. It stops
     before changing anything if this PC has used Kestrel, because it would delete Kestrel's record of its changes.
  2. Set them to a known state: Game Mode off, background recording on, pointer precision on.
  3. Ask Kestrel which tweaks it offers for a desktop PC with two graphics chips.
  4. Apply them all, and read back what Windows holds. That includes Kestrel's power plan, which becomes the plan in use.
  5. Check that Kestrel's Health Check agrees.
  6. Undo them all, and read back that everything is as it was in step 2.
  7. Check that a value of the wrong type is refused and left alone, and so is a journal folder someone else owns.
  8. Put everything back as step 1 found it, whatever happened before, and delete Kestrel's folder in ProgramData.

It is meant for a build machine that gets thrown away.
";

    /// Nothing is at this path. It is only ever the name of a registry value, the way VALORANT's real path would be.
    const GAME_EXE: &str = r"C:\KestrelRoundTrip\VALORANT-Win64-Shipping.exe";
    /// The DirectX settings before Kestrel: someone else's token, which has to survive.
    const OTHER_TOKEN: &str = "VRROptimizeEnable=1;";
    /// Windows' default mouse, acceleration on, as [threshold 1, threshold 2, acceleration].
    const MOUSE_ON: [i32; 3] = [6, 10, 1];
    const MOUSE_OFF: [i32; 3] = [0, 0, 0];
    /// What `SPI_SETMOUSE` answers when this session isn't allowed to change the mouse, which says nothing about
    /// Kestrel.
    const SESSION_REFUSED: [i32; 2] = [
        ERROR_ACCESS_DENIED as i32,
        ERROR_REQUIRES_INTERACTIVE_WINDOWSTATION as i32,
    ];

    /// One registry value in the hive being used.
    #[derive(Clone, Copy)]
    struct Spot {
        key: &'static str,
        name: &'static str,
    }

    impl fmt::Display for Spot {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, r"{}\{}", self.key, self.name)
        }
    }

    const fn spot(key: &'static str, name: &'static str) -> Spot {
        Spot { key, name }
    }

    const GAME_MODE: Spot = spot(GAME_BAR_KEY, AUTO_GAME_MODE);
    const RECORDING: Spot = spot(GAME_DVR_KEY, HISTORICAL_CAPTURE);
    const DIRECTX: Spot = spot(DIRECTX_KEY, DIRECTX_SETTINGS);
    const GAME_VALUE: Spot = spot(DIRECTX_KEY, GAME_EXE);
    const SPEED: Spot = spot(MOUSE_KEY, MOUSE_SPEED);
    const THRESHOLD_1: Spot = spot(MOUSE_KEY, MOUSE_THRESHOLD_1);
    const THRESHOLD_2: Spot = spot(MOUSE_KEY, MOUSE_THRESHOLD_2);

    /// Everything this program or Kestrel changes, saved in step 1 and restored in step 8.
    const CHANGED: [Spot; 7] = [
        GAME_MODE,
        RECORDING,
        DIRECTX,
        GAME_VALUE,
        SPEED,
        THRESHOLD_1,
        THRESHOLD_2,
    ];
    /// What Kestrel must never write. These are only read, at both ends, to show they didn't change.
    const NEVER_WRITTEN: [Spot; 3] = [
        spot(GAME_DVR_KEY, APP_CAPTURE),
        spot(GAME_BAR_KEY, ALLOW_AUTO_GAME_MODE),
        spot(r"System\GameConfigStore", "GameDVR_Enabled"),
    ];
    /// Keys the changes above can create, deepest first. Any that weren't there in step 1 go again in step 8.
    const KEYS: [&str; 4] = [
        DIRECTX_KEY,
        r"Software\Microsoft\DirectX",
        GAME_DVR_KEY,
        GAME_BAR_KEY,
    ];

    /// One setting of Kestrel's plan: what to call it, its subgroup and GUID, and the value the spec gives it.
    type PlanSetting = (&'static str, &'static str, &'static str, u32);

    const PARKING: PlanSetting = (
        "Core parking minimum cores",
        "54533251-82be-4824-96c1-47b60b740d00",
        "0cc5b647-c1df-4637-891a-dec35c318583",
        100,
    );
    const USB_SUSPEND: PlanSetting = (
        "USB selective suspend",
        "2a737441-1930-4402-8d77-b2bebba308a3",
        "48e6b7a6-50f5-4782-a5d4-53bb8f07e226",
        0,
    );
    const LINK_STATE: PlanSetting = (
        "PCIe link-state power management",
        "501a4d13-42af-4429-9fd1-a8218c268e20",
        "ee12f906-d277-404b-b6da-e5fa1a576df5",
        0,
    );

    /// A value exactly as the registry holds it: its type and bytes, or None when it isn't there.
    type Raw = Option<RegValue<'static>>;

    /// What the run has found wrong, and what it couldn't exercise on this machine.
    #[derive(Default)]
    struct Checks {
        wrong: Vec<String>,
        left_out: Vec<String>,
    }

    impl Checks {
        fn check(&mut self, passed: bool, what: impl Into<String>) {
            let what = what.into();
            if passed {
                println!("  ok: {what}");
            } else {
                println!("  WRONG: {what}");
                self.wrong.push(what);
            }
        }

        fn wrong(&mut self, what: impl Into<String>) {
            self.check(false, what);
        }

        fn leave_out(&mut self, what: impl Into<String>) {
            let what = what.into();
            println!("::warning::LEFT OUT: {what}");
            self.left_out.push(what);
        }
    }

    /// What step 1 saved, and what every step after it works with.
    struct Run {
        system: Arc<WinSystem>,
        /// The hive being used: the signed-in account's, or this process's when Windows names nobody.
        sid: String,
        differs: bool,
        values: Vec<(Spot, Raw)>,
        never_written: Vec<(Spot, Raw)>,
        missing_keys: Vec<&'static str>,
        /// None when Windows wouldn't say.
        mouse: Option<[i32; 3]>,
        active: String,
        plans: Vec<String>,
    }

    /// The state step 2 arranged, which an undo has to come back to.
    struct Arranged {
        values: Vec<(Spot, Raw)>,
        mouse: Option<[i32; 3]>,
    }

    pub fn main() -> ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args != [FLAG] {
            print!("{WOULD_DO}");
            return ExitCode::from(2);
        }
        let run = match step_1() {
            Ok(run) => run,
            Err(why) => {
                println!("Stopped before changing anything: {why}");
                return ExitCode::FAILURE;
            }
        };
        let mut checks = Checks::default();
        match catch_unwind(AssertUnwindSafe(|| steps(&run, &mut checks))) {
            Ok(Ok(())) => {}
            Ok(Err(why)) => checks.wrong(format!("The steps stopped early: {why}")),
            Err(_) => checks.wrong("A step panicked. Its message is above."),
        }
        step_8(&run, &mut checks);
        finish(&checks)
    }

    fn heading(text: &str) {
        println!("\n== {text}");
    }

    /// Steps 2 to 7, which change the machine. An error means there is no point going on; step 8 still runs.
    fn steps(run: &Run, checks: &mut Checks) -> Result<(), String> {
        let (arranged, mouse_left_out) = step_2(run, checks)?;
        heading("Step 3: what Kestrel offers on this machine");
        let input = plan_input();
        let engine = Engine::new(run.system.clone(), journal_file());
        let offered = what_is_offered(&engine, &input, mouse_left_out, checks)?;
        step_4(run, &engine, &input, &arranged, &offered, checks)?;
        step_5(&offered, checks);
        step_6(run, &engine, &input, &arranged, checks)?;
        step_7(run, &engine, &input, checks)?;
        someone_elses_folder(run, &input, checks);
        Ok(())
    }

    // Reading and writing the registry directly.

    fn is_missing(error: &io::Error) -> bool {
        error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32)
    }

    fn raw(sid: &str, at: Spot) -> Result<Raw, String> {
        let path = user_key_path(sid, at.key);
        let key = match RegKey::predef(HKEY_USERS).open_subkey_with_flags(path, KEY_READ) {
            Ok(key) => key,
            Err(error) if is_missing(&error) => return Ok(None),
            Err(error) => return Err(format!("Couldn't open {}: {error}", at.key)),
        };
        match key.get_raw_value(at.name) {
            Ok(value) => Ok(Some(value)),
            Err(error) if is_missing(&error) => Ok(None),
            Err(error) => Err(format!("Couldn't read {at}: {error}")),
        }
    }

    fn read_all(sid: &str, spots: &[Spot]) -> Result<Vec<(Spot, Raw)>, String> {
        spots.iter().map(|&at| Ok((at, raw(sid, at)?))).collect()
    }

    fn key_exists(sid: &str, key: &str) -> Result<bool, String> {
        match RegKey::predef(HKEY_USERS).open_subkey_with_flags(user_key_path(sid, key), KEY_READ) {
            Ok(_) => Ok(true),
            Err(error) if is_missing(&error) => Ok(false),
            Err(error) => Err(format!("Couldn't open {key}: {error}")),
        }
    }

    fn put_raw(sid: &str, at: Spot, value: &RegValue) -> io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_USERS)
            .create_subkey_with_flags(user_key_path(sid, at.key), KEY_SET_VALUE)?;
        key.set_raw_value(at.name, value)
    }

    fn remove_raw(sid: &str, at: Spot) -> io::Result<()> {
        let removed = RegKey::predef(HKEY_USERS)
            .open_subkey_with_flags(user_key_path(sid, at.key), KEY_SET_VALUE)
            .and_then(|key| key.delete_value(at.name));
        match removed {
            Err(error) if is_missing(&error) => Ok(()),
            other => other,
        }
    }

    fn dword(value: u32) -> Raw {
        Some(RegValue {
            vtype: REG_DWORD,
            bytes: value.to_le_bytes().to_vec().into(),
        })
    }

    fn text(value: &str) -> Raw {
        Some(RegValue {
            vtype: REG_SZ,
            bytes: parse::wide_bytes(value).into(),
        })
    }

    /// A raw value for the log, whatever it holds. `RegValue`'s own formatting gives up on a malformed one.
    fn shown(raw: &Raw) -> String {
        let Some(value) = raw else {
            return "absent".to_string();
        };
        let bytes = &value.bytes[..];
        let spelled = match value.vtype {
            REG_SZ => parse::text_from_wide_bytes(bytes).map(|text| format!("\"{text}\"")),
            REG_DWORD => <[u8; 4]>::try_from(bytes)
                .ok()
                .map(|bytes| u32::from_le_bytes(bytes).to_string()),
            _ => None,
        };
        format!(
            "{:?} {}",
            value.vtype,
            spelled.unwrap_or_else(|| format!("{bytes:?}"))
        )
    }

    // Comparing what is there.

    fn expect_raw(run: &Run, at: Spot, wanted: &Raw, checks: &mut Checks) {
        match raw(&run.sid, at) {
            Ok(found) if found == *wanted => {
                checks.check(true, format!("{at} is {}", shown(wanted)));
            }
            Ok(found) => checks.wrong(format!(
                "{at} is {}, and should be {}",
                shown(&found),
                shown(wanted)
            )),
            Err(why) => checks.wrong(why),
        }
    }

    /// The mouse in force, which has to read exactly what it was given: an undo counts only if it does.
    fn expect_mouse(run: &Run, wanted: [i32; 3], checks: &mut Checks) {
        match run.system.mouse_live() {
            Read::Value(found) => checks.check(
                found == wanted,
                format!("SPI_GETMOUSE reads {found:?}, and should read exactly {wanted:?}"),
            ),
            other => checks.wrong(format!("SPI_GETMOUSE: {other:?}")),
        }
    }

    /// `MouseSpeed`, `MouseThreshold1` and `MouseThreshold2`, which are strings, in that order.
    fn expect_mouse_strings(run: &Run, wanted: [&str; 3], checks: &mut Checks) {
        for (at, wanted) in [SPEED, THRESHOLD_1, THRESHOLD_2].into_iter().zip(wanted) {
            expect_raw(run, at, &text(wanted), checks);
        }
    }

    fn expect_plans_as_found(run: &Run, checks: &mut Checks) {
        let active = run.system.active_plan();
        checks.check(
            active == Read::Value(run.active.clone()),
            format!(
                "the plan in use is {active:?}, and should be {}",
                run.active
            ),
        );
        match sorted_plans(&run.system) {
            Ok(plans) => checks.check(
                plans == run.plans,
                format!(
                    "the plans on this machine are {plans:?}, and should be {:?}",
                    run.plans
                ),
            ),
            Err(why) => checks.wrong(why),
        }
    }

    fn sorted_plans(system: &WinSystem) -> Result<Vec<String>, String> {
        let mut plans = system
            .plans()
            .map_err(|why| format!("Windows wouldn't list the power plans: {why}"))?;
        plans.sort();
        Ok(plans)
    }

    // The steps.

    fn step_1() -> Result<Run, String> {
        heading("Step 1: who this is for, and what is on this machine");
        let who = account();
        println!(
            "The signed-in account, as Windows names it: {}",
            who.signed_in.as_deref().unwrap_or("nobody")
        );
        println!(
            "This process's account: {}",
            who.this_process.as_deref().unwrap_or("nobody")
        );
        let me = who
            .this_process
            .ok_or("Windows wouldn't name the account this process runs as.")?;
        let (system, sid) = match who.signed_in {
            Some(sid) => {
                println!("Using the signed-in account's hive, as Kestrel does.");
                (WinSystem::new(), sid)
            }
            None => {
                println!("Using this process's own hive, through WinSystem::with_signed_in: Kestrel itself would say it can't tell whose settings to change.");
                (WinSystem::with_signed_in(me.clone()), me)
            }
        };
        let system = Arc::new(system);
        let differs = system.account().differs();
        println!("The two accounts differ: {differs}");
        let plans = sorted_plans(&system)?;
        // Step 8 deletes the journal's folder and Kestrel's plan, so on a PC that has used Kestrel it would lose the
        // record of real changes.
        let dir = journal_dir().ok_or("Windows wouldn't say where the ProgramData folder is.")?;
        let folder_is_free =
            matches!(fs::symlink_metadata(&dir), Err(why) if why.kind() == io::ErrorKind::NotFound);
        if !folder_is_free {
            return Err(format!("{} is already here, so this PC has used Kestrel, and this program would delete Kestrel's record of its changes.", dir.display()));
        }
        if plans.iter().any(|plan| plan == KESTREL_PLAN_GUID) {
            return Err("Kestrel's power plan is already here, so this PC has used Kestrel, and this program would delete it.".to_string());
        }
        let active = match system.active_plan() {
            Read::Value(active) => active,
            other => {
                return Err(format!(
                    "Windows wouldn't say which plan is in use: {other:?}"
                ))
            }
        };
        println!(
            "The plan in use: {active} ({})",
            system.plan_name(&active).as_deref().unwrap_or("no name")
        );
        println!("The plans on this machine: {plans:?}");
        println!("Modern Standby: {:?}", system.modern_standby());
        let mouse = match system.mouse_live() {
            Read::Value(mouse) => Some(mouse),
            other => {
                println!("The mouse in force can't be read here: {other:?}");
                None
            }
        };
        println!("The mouse in force: {mouse:?}");
        let values = read_all(&sid, &CHANGED)?;
        let never_written = read_all(&sid, &NEVER_WRITTEN)?;
        for (at, value) in values.iter().chain(&never_written) {
            println!("  {at} is {}", shown(value));
        }
        let mut missing_keys = Vec::new();
        for key in KEYS {
            if !key_exists(&sid, key)? {
                println!("  {key} isn't there yet.");
                missing_keys.push(key);
            }
        }
        Ok(Run {
            system,
            sid,
            differs,
            values,
            never_written,
            missing_keys,
            mouse,
            active,
            plans,
        })
    }

    /// Returns the state to come back to, and why the mouse has to be left out, if it does.
    fn step_2(run: &Run, checks: &mut Checks) -> Result<(Arranged, Option<String>), String> {
        heading("Step 2: arrange a known state");
        let (sys, sid) = (&run.system, run.sid.as_str());
        let refused = |what: &str, why: io::Error| format!("Couldn't arrange {what}: {why}");
        sys.write_dword(sid, GAME_MODE.key, GAME_MODE.name, 0)
            .map_err(|why| refused("Game Mode", why))?;
        sys.write_dword(sid, RECORDING.key, RECORDING.name, 1)
            .map_err(|why| refused("background recording", why))?;
        sys.write_string(sid, DIRECTX.key, DIRECTX.name, OTHER_TOKEN)
            .map_err(|why| refused("the DirectX settings", why))?;
        remove_raw(sid, GAME_VALUE)
            .map_err(|why| refused("the game's graphics preference", why))?;
        let mouse_left_out = if run.differs {
            // Kestrel only writes the strings when the accounts differ, and the mouse in force is this session's.
            for (name, value) in parse::mouse_strings(MOUSE_ON) {
                sys.write_string(sid, MOUSE_KEY, name, &value)
                    .map_err(|why| refused("the mouse strings", why))?;
            }
            expect_mouse_strings(run, ["1", "6", "10"], checks);
            None
        } else {
            match sys.set_mouse_live(MOUSE_ON, true) {
                Ok(()) => {
                    expect_mouse(run, MOUSE_ON, checks);
                    expect_mouse_strings(run, ["1", "6", "10"], checks);
                    None
                }
                Err(why)
                    if why
                        .raw_os_error()
                        .is_some_and(|code| SESSION_REFUSED.contains(&code)) =>
                {
                    Some(format!(
                        "this session isn't allowed to change the mouse: {why}"
                    ))
                }
                Err(why) => return Err(refused("the mouse", why)),
            }
        };
        let values = read_all(sid, &CHANGED)?;
        for (at, value) in &values {
            println!("  {at} is {}", shown(value));
        }
        let mouse = match sys.mouse_live() {
            Read::Value(mouse) => Some(mouse),
            _ => None,
        };
        println!("  the mouse in force is {mouse:?}");
        Ok((Arranged { values, mouse }, mouse_left_out))
    }

    /// A desktop with one kind of core, two graphics chips and VALORANT in windowed fullscreen, on this machine's own
    /// build and CPU, so that everything that can be offered is.
    fn plan_input() -> PlanInput {
        let snapshot = kestrel_probe::collect_snapshot();
        println!(
            "This machine: Windows build {}, {}",
            snapshot.os.build, snapshot.cpu.name
        );
        println!("VALORANT and a second graphics chip aren't needed: a made-up path and a pretend choice of chips stand in.");
        for problem in &snapshot.probe_errors {
            println!(
                "  the probe says, about {}: {}",
                problem.probe, problem.message
            );
        }
        PlanInput {
            build: snapshot.os.build,
            form_factor: FormFactor::Desktop,
            hybrid_cpu: false,
            cpu_name: snapshot.cpu.name,
            gpu_choice: true,
            valorant_exe: Some(GAME_EXE.to_string()),
            fullscreen_mode: Some(1),
        }
    }

    /// The tweaks Kestrel offers here. Anything it doesn't is left out and said so, and the rest are what gets
    /// asserted.
    fn what_is_offered(
        engine: &Engine,
        input: &PlanInput,
        mouse_left_out: Option<String>,
        checks: &mut Checks,
    ) -> Result<Vec<TweakId>, String> {
        let view = engine
            .view(input, Gate::Open)
            .map_err(|why| format!("Kestrel couldn't show its rows: {why}"))?;
        let mut offered = Vec::new();
        for id in TweakId::ESSENTIAL {
            let name = id.as_str();
            match view.rows.iter().find(|row| row.id == id) {
                None => checks.leave_out(format!("{name}: it has no row on build {}", input.build)),
                Some(_) if id == TweakId::PointerPrecision && mouse_left_out.is_some() => {
                    let why = mouse_left_out.as_deref().unwrap_or_default();
                    checks.leave_out(format!("{name}: {why}"));
                }
                Some(row) if row.state != RowState::Available => {
                    checks.leave_out(format!("{name}: its row says {:?}: {}", row.state, row.why));
                }
                Some(row) => {
                    println!("  offered: {name}: {}", row.why);
                    offered.push(id);
                }
            }
        }
        if offered.is_empty() {
            return Err("Kestrel offers nothing here, so there is nothing to test.".to_string());
        }
        Ok(offered)
    }

    fn step_4(
        run: &Run,
        engine: &Engine,
        input: &PlanInput,
        arranged: &Arranged,
        offered: &[TweakId],
        checks: &mut Checks,
    ) -> Result<(), String> {
        heading("Step 4: apply what is offered, and read back what Windows holds");
        // This is also the journal's first real run: its folder, its lock, and an append flushed to disk.
        let items = engine
            .apply(input, offered)
            .map_err(|why| format!("Apply refused, and changed nothing: {why}"))?;
        for item in &items {
            checks.check(item.ok, format!("{}: {}", item.id.as_str(), item.message));
        }
        for id in offered {
            match id {
                TweakId::GameMode => expect_raw(run, GAME_MODE, &dword(1), checks),
                TweakId::BackgroundRecording => expect_raw(run, RECORDING, &dword(0), checks),
                TweakId::WindowedOptimizations => expect_raw(
                    run,
                    DIRECTX,
                    &text("VRROptimizeEnable=1;SwapEffectUpgradeEnable=1;"),
                    checks,
                ),
                TweakId::GpuPreference => {
                    expect_raw(run, GAME_VALUE, &text("GpuPreference=2;"), checks);
                }
                TweakId::PointerPrecision => {
                    expect_mouse_strings(run, ["0", "0", "0"], checks);
                    let wanted = if run.differs {
                        arranged.mouse
                    } else {
                        Some(MOUSE_OFF)
                    };
                    if let Some(wanted) = wanted {
                        expect_mouse(run, wanted, checks);
                    }
                }
                TweakId::PowerPlan => expect_kestrels_plan(run, input, checks),
                TweakId::SystemProtection => {}
            }
        }
        // The rows are what a person sees, and they judge all of this for themselves.
        let view = engine
            .view(input, Gate::Open)
            .map_err(|why| format!("Kestrel couldn't show its rows: {why}"))?;
        for row in view.rows.iter().filter(|row| offered.contains(&row.id)) {
            checks.check(
                row.state == RowState::Applied,
                format!(
                    "{}'s row says {:?}: {}",
                    row.id.as_str(),
                    row.state,
                    row.why
                ),
            );
        }
        checks.check(
            view.unreadable_lines == 0,
            format!(
                "the journal has {} lines it doesn't believe",
                view.unreadable_lines
            ),
        );
        Ok(())
    }

    fn expect_kestrels_plan(run: &Run, input: &PlanInput, checks: &mut Checks) {
        let sys = &run.system;
        let active = sys.active_plan();
        checks.check(
            active == Read::Value(KESTREL_PLAN_GUID.to_string()),
            format!("the plan in use is {active:?}, and should be Kestrel's, {KESTREL_PLAN_GUID}"),
        );
        let name = sys.plan_name(KESTREL_PLAN_GUID);
        checks.check(
            name.as_deref() == Some(KESTREL_PLAN_NAME),
            format!("Kestrel's plan is named {name:?}"),
        );
        let settings: &[PlanSetting] = match variant(input, sys.modern_standby()) {
            Variant::Full => &[PARKING, USB_SUSPEND, LINK_STATE],
            Variant::Light => {
                println!("  This machine has Modern Standby, so Kestrel's plan leaves core parking alone.");
                &[USB_SUSPEND, LINK_STATE]
            }
            Variant::NotOffered => &[],
        };
        for &(label, subgroup, setting, value) in settings {
            // Windows may accept less than the spec's value, and then the plan holds the nearest it accepts.
            let range = sys.plan_value_range(subgroup, setting);
            let wanted = range.map_or(value, |(min, max)| value.max(min).min(max));
            let found = sys.read_plan_value(KESTREL_PLAN_GUID, subgroup, setting);
            checks.check(
                found == Read::Value(wanted),
                format!(
                    "{label} reads {found:?}, and should read {wanted} (Windows' range: {range:?})"
                ),
            );
        }
    }

    fn step_5(offered: &[TweakId], checks: &mut Checks) {
        heading("Step 5: what the Health Check says");
        // With nobody signed in, the probe reads HKEY_CURRENT_USER. For this process that is the same hive as the
        // HKEY_USERS\<this process's SID> everything above went to, so what it sees is what was written.
        println!("  The probe reads the signed-in account's hive, or with nobody signed in this process's own: the hive used above.");
        let report = kestrel_probe::collect_report(env!("CARGO_PKG_VERSION"));
        for tweak in [
            TweakId::GameMode,
            TweakId::BackgroundRecording,
            TweakId::PowerPlan,
        ] {
            let name = tweak.as_str();
            if !offered.contains(&tweak) {
                println!("  {name} was left out above, so its finding isn't judged.");
                continue;
            }
            match report.findings.iter().find(|finding| finding.id == name) {
                Some(finding) => checks.check(
                    finding.status == Status::Pass,
                    format!("{name}: {:?}: {}", finding.status, finding.title),
                ),
                None => checks.wrong(format!("{name}: the Health Check has no such finding")),
            }
        }
        if offered.contains(&TweakId::PowerPlan) {
            // High performance passes too, and a runner may have started on it, so the status alone doesn't show this.
            let plan = report.snapshot.windows.power_plan.as_ref();
            let seen = plan.map(|plan| plan.guid.as_str());
            checks.check(
                seen == Some(KESTREL_PLAN_GUID),
                format!("the probe sees the plan in use as {seen:?}"),
            );
        }
    }

    fn step_6(
        run: &Run,
        engine: &Engine,
        input: &PlanInput,
        arranged: &Arranged,
        checks: &mut Checks,
    ) -> Result<(), String> {
        heading("Step 6: undo everything, and read back what Windows holds");
        let items = engine
            .revert_all()
            .map_err(|why| format!("Revert everything failed: {why}"))?;
        for item in &items {
            checks.check(item.ok, format!("{}: {}", item.id.as_str(), item.message));
        }
        for (at, value) in &arranged.values {
            expect_raw(run, *at, value, checks);
        }
        if let Some(mouse) = arranged.mouse {
            expect_mouse(run, mouse, checks);
        }
        expect_plans_as_found(run, checks);
        checks.check(
            !engine.has_open_records(),
            "the journal has no change left open",
        );
        println!("Kestrel's report now says:");
        let now = chrono::Local::now().to_rfc3339();
        println!(
            "{}",
            engine.report(env!("CARGO_PKG_VERSION"), input.build, &now)
        );
        Ok(())
    }

    fn step_7(
        run: &Run,
        engine: &Engine,
        input: &PlanInput,
        checks: &mut Checks,
    ) -> Result<(), String> {
        heading("Step 7: a value of the wrong type is refused and left alone");
        let (sys, sid) = (&run.system, run.sid.as_str());
        sys.write_string(sid, GAME_MODE.key, GAME_MODE.name, "not a number")
            .map_err(|why| format!("Couldn't write the wrong type: {why}"))?;
        let read = sys.read_dword(sid, GAME_MODE.key, GAME_MODE.name);
        checks.check(
            matches!(read, Read::Failed(_)),
            format!("{GAME_MODE} as a DWORD reads {read:?}"),
        );
        wrong_type_is_left_alone(run, engine, input, TweakId::GameMode, GAME_MODE, checks)?;

        let binary = RegValue {
            vtype: REG_BINARY,
            bytes: vec![1u8, 2, 3, 4].into(),
        };
        put_raw(sid, DIRECTX, &binary)
            .map_err(|why| format!("Couldn't write the wrong type: {why}"))?;
        let read = sys.read_string(sid, DIRECTX.key, DIRECTX.name);
        checks.check(
            matches!(read, Read::Failed(_)),
            format!("{DIRECTX} as a string reads {read:?}"),
        );
        wrong_type_is_left_alone(
            run,
            engine,
            input,
            TweakId::WindowedOptimizations,
            DIRECTX,
            checks,
        )
    }

    /// With the value at `at` of the wrong type, the row says Couldn't read and Apply leaves the value exactly as it
    /// is.
    fn wrong_type_is_left_alone(
        run: &Run,
        engine: &Engine,
        input: &PlanInput,
        id: TweakId,
        at: Spot,
        checks: &mut Checks,
    ) -> Result<(), String> {
        let name = id.as_str();
        let stored = raw(&run.sid, at)?;
        let view = engine
            .view(input, Gate::Open)
            .map_err(|why| format!("Kestrel couldn't show its rows: {why}"))?;
        match view.rows.iter().find(|row| row.id == id) {
            Some(row) => checks.check(
                row.state == RowState::CouldntRead,
                format!("{name}'s row says {:?}: {}", row.state, row.why),
            ),
            None => println!("  {name} has no row on build {}.", input.build),
        }
        let items = engine
            .apply(input, &[id])
            .map_err(|why| format!("Apply refused: {why}"))?;
        for item in &items {
            checks.check(
                !item.ok,
                format!("Apply on {name} says ok={}: {}", item.ok, item.message),
            );
        }
        expect_raw(run, at, &stored, checks);
        checks.check(
            !engine.has_open_records(),
            "the journal has no change left open",
        );
        Ok(())
    }

    /// A journal folder someone else owns: Apply refuses before anything happens, and says why. This uses a folder of
    /// its own, so that the real one's path isn't left owned by someone else if this goes wrong.
    fn someone_elses_folder(run: &Run, input: &PlanInput, checks: &mut Checks) {
        heading("Also: a journal folder that belongs to another account is refused");
        let dir =
            std::env::temp_dir().join(format!("kestrel-roundtrip-not-mine-{}", std::process::id()));
        if let Err(why) = fs::create_dir(&dir) {
            checks.wrong(format!("Couldn't make {}: {why}", dir.display()));
            return;
        }
        // LOCAL SERVICE. Giving a folder away takes elevation, which `icacls` turns on for itself.
        let given = Command::new("icacls")
            .arg(&dir)
            .args(["/setowner", "*S-1-5-19"])
            .output();
        match given {
            Ok(output) if output.status.success() => {
                let engine = Engine::new(
                    run.system.clone(),
                    JournalFile::new(&dir, Box::new(AdminFolder)),
                );
                match engine.apply(input, &[TweakId::GameMode]) {
                    Err(why) if why.kind() == io::ErrorKind::PermissionDenied => checks.check(
                        why.to_string().ends_with("belongs to another account. Delete it, then try again."),
                        format!("Apply says: {why}"),
                    ),
                    other => checks.wrong(format!("Apply should have refused, and says: {other:?}")),
                }
            }
            other => checks.leave_out(format!(
                "a folder that belongs to another account: this machine wouldn't give one away: {other:?}"
            )),
        }
        if let Err(why) = fs::remove_dir_all(&dir) {
            println!("  Couldn't remove {}: {why}", dir.display());
        }
    }

    /// Puts everything back, whatever the steps came to, and checks that it is back.
    fn step_8(run: &Run, checks: &mut Checks) {
        heading("Step 8: put this machine back as step 1 found it");
        let (sys, sid) = (&run.system, run.sid.as_str());
        // In this order: Windows won't delete the plan in use.
        if sys.active_plan() != Read::Value(run.active.clone()) {
            if let Err(why) = sys.activate_plan(&run.active) {
                checks.wrong(format!("Couldn't switch back to {}: {why}", run.active));
            }
        }
        if sys.plan_exists(KESTREL_PLAN_GUID) {
            match sys.delete_plan(KESTREL_PLAN_GUID) {
                Ok(()) => println!("  Deleted Kestrel's plan, which was still here."),
                Err(why) => checks.wrong(format!("Couldn't delete Kestrel's plan: {why}")),
            }
        }
        for (at, was) in &run.values {
            let put_back = match was {
                Some(value) => put_raw(sid, *at, value),
                None => remove_raw(sid, *at),
            };
            if let Err(why) = put_back {
                checks.wrong(format!("Couldn't put {at} back: {why}"));
            }
        }
        for key in &run.missing_keys {
            if let Err(why) = sys.delete_key_if_empty(sid, key) {
                checks.wrong(format!("Couldn't remove {key}: {why}"));
            }
        }
        if let Some(mouse) = run.mouse {
            if sys.mouse_live() != Read::Value(mouse) {
                if let Err(why) = sys.set_mouse_live(mouse, false) {
                    checks.wrong(format!("Couldn't put the mouse back: {why}"));
                }
            }
        }
        match journal_dir() {
            Some(dir) => match fs::remove_dir_all(&dir) {
                Err(why) if why.kind() != io::ErrorKind::NotFound => {
                    checks.wrong(format!("Couldn't remove {}: {why}", dir.display()));
                }
                _ => checks.check(!dir.exists(), format!("{} is gone", dir.display())),
            },
            None => checks.wrong("Windows wouldn't say where the journal's folder is."),
        }

        println!("Reading it all back:");
        for (at, was) in run.values.iter().chain(&run.never_written) {
            expect_raw(run, *at, was, checks);
        }
        for key in &run.missing_keys {
            if matches!(key_exists(sid, key), Ok(true)) {
                println!("  {key} wasn't here at the start, and holds something now, so it stays.");
            }
        }
        if let Some(mouse) = run.mouse {
            expect_mouse(run, mouse, checks);
        }
        expect_plans_as_found(run, checks);
    }

    fn finish(checks: &Checks) -> ExitCode {
        heading("What this run doesn't cover");
        println!("The restore point and the uninstaller aren't exercised here, and System Protection isn't touched. They are checked on the owner's PC.");
        println!("VALORANT's graphics preference is checked as a registry value, not by what it does: this machine has no VALORANT and no second graphics chip.");
        for what in &checks.left_out {
            println!("Left out on this machine: {what}");
        }
        heading("The result");
        if checks.wrong.is_empty() {
            println!("Everything Kestrel offered here applied, read back, undid and read back as it was.");
            return ExitCode::SUCCESS;
        }
        println!("What differed:");
        for what in &checks.wrong {
            println!("  {what}");
        }
        ExitCode::FAILURE
    }
}
