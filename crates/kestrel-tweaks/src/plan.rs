//! The plan rules for the five Windows settings: whether each is on offer, whether it starts ticked, and what its row
//! says. (The power plan, the sixth Essential row, has its rules in `power`.) A row is a verdict on one setting for one
//! account, from what the PC holds now, what the scan knows about the PC (`PlanInput`), and the change Kestrel has
//! open for it, if any. Making a row only reads: nothing here writes to Windows.
//!
//! The words of a row are in Rust, like Findings' text; the page's own labels live in `strings.ts`.
use kestrel_core::snapshot::{FormFactor, SystemSnapshot};
use kestrel_core::tweak_id::TweakId;
use kestrel_core::{gpu, valorant};
use serde::{Deserialize, Serialize};

use crate::catalog::{info, Impact, Timing};
use crate::journal::{OpenRecord, Saved};
use crate::system::System;
use crate::targets::{self, known};

/// The first Windows build that has Optimizations for windowed games (Windows 11 22H2).
const WINDOWED_BUILD: u32 = 22621;

/// What the rules need to know about the PC besides what is in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanInput {
    /// The Windows build number; 0 when the scan couldn't tell.
    pub build: u32,
    pub form_factor: FormFactor,
    /// The CPU has more than one kind of core, like Intel's performance and efficiency cores.
    pub hybrid_cpu: bool,
    pub cpu_name: String,
    /// Windows has more than one graphics chip to choose between.
    pub gpu_choice: bool,
    /// The full path of VALORANT's executable with backslashes, which is how the graphics preference names it. None when
    /// VALORANT wasn't found.
    pub valorant_exe: Option<String>,
    /// VALORANT's `FullscreenMode`: 0 fullscreen, 1 windowed fullscreen, 2 windowed. None when it couldn't be read.
    pub fullscreen_mode: Option<u8>,
}

impl PlanInput {
    /// The rules' view of a scan. `fullscreen_mode` comes from VALORANT's own settings file, not from the scan.
    pub fn from_snapshot(s: &SystemSnapshot, fullscreen_mode: Option<u8>) -> Self {
        PlanInput {
            build: s.os.build,
            form_factor: s.machine.form_factor,
            hybrid_cpu: s.cpu.p_cores.is_some(),
            cpu_name: s.cpu.name.clone(),
            // A hybrid PC, or two real graphics chips; the software adapters (Parsec, remote desktop) don't count.
            gpu_choice: gpu::is_hybrid(&s.gpus) || gpu::hardware_gpu_count(&s.gpus) >= 2,
            // `valorant.rs` works in forward slashes, and the registry value is named with backslashes.
            valorant_exe: s
                .storage
                .valorant
                .as_ref()
                .map(|v| valorant::game_exe_path(&v.path).replace('/', "\\")),
            fullscreen_mode,
        }
    }
}

/// What a row says about its setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RowState {
    /// Kestrel can change it.
    Available,
    /// It is already as Kestrel would leave it.
    AlreadySet,
    /// There is nothing here for it to do.
    NotNeeded,
    /// An organisation policy decides it.
    BlockedByPolicy,
    /// Kestrel changed it, and it is still as Kestrel left it.
    Applied,
    /// Kestrel changed it, and something else has changed it since.
    ChangedElsewhere,
    /// Kestrel's power plan exists but another plan is in use. Only the power plan is ever this.
    PlanInactive,
    /// A read failed, or Kestrel doesn't know whose settings to read.
    CouldntRead,
}

/// One row of the Tweaks tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: TweakId,
    pub title: String,
    pub state: RowState,
    /// Whether its box starts ticked. Only a row that is `Available` can be.
    pub ticked: bool,
    /// One sentence that says why the row is as it is.
    pub why: String,
    pub impact: Impact,
    pub timing: Timing,
}

/// One value in a Preview: what it holds now, and what Apply will make it hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueChange {
    pub label: String,
    pub now: String,
    pub after: String,
}

/// What Apply would do for one tweak, as the Preview shows it before anything is changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub id: TweakId,
    pub title: String,
    /// Where the change is made, as a person could look it up.
    pub location: String,
    pub changes: Vec<ValueChange>,
    pub timing: Timing,
    /// Anything the person should know that the values don't say.
    pub note: Option<String>,
}

/// What a row says: its state, whether it is ticked, and why. A rule that can't read what it needs answers with what
/// stopped it instead, and the row says Couldn't read with that reason: on a real PC it is all there is to go on.
type Verdict = (RowState, bool, &'static str);

/// `targets::current` gives each setting its own kind of state, so this isn't expected.
const NOT_ITS_KIND: &str = "It didn't read as the kind of value it should be.";
const NO_ACCOUNT: Verdict = (
    RowState::CouldntRead,
    false,
    "Couldn't tell which account is signed in.",
);
const APPLIED: Verdict = (RowState::Applied, false, "Kestrel changed this.");
const CHANGED_ELSEWHERE: Verdict = (
    RowState::ChangedElsewhere,
    false,
    "Something else changed this after Kestrel did.",
);

/// The row for one of the five Windows settings, for the account `sid`; `open` is the change Kestrel has open for this
/// tweak and account, if any. None when there is no row to show: for the power plan and System Protection, which have
/// rows of their own, and for Optimizations for windowed games on a Windows build that doesn't have it. A row is shown
/// for a change Kestrel already made even then, so that it can be undone.
///
/// An open record decides first, by comparing the setting now with what Kestrel wrote: the same is Applied, anything
/// else is ChangedElsewhere. (The engine closes a record whose setting went back to the old value before it asks for
/// a row; one that gets here anyway is not what Kestrel left, so it is ChangedElsewhere.) Without a record, a policy
/// comes next and then the value itself. Any read that fails makes the row CouldntRead, with what stopped it, and so
/// does not knowing the account, since all five settings are per account.
pub fn setting_row(
    sys: &dyn System,
    input: &PlanInput,
    id: TweakId,
    sid: Option<&str>,
    open: Option<&OpenRecord>,
) -> Option<Row> {
    if id == TweakId::WindowedOptimizations && input.build < WINDOWED_BUILD && open.is_none() {
        return None;
    }
    let (state, ticked, why) = match verdict(sys, input, id, sid, open)? {
        Ok((state, ticked, why)) => (state, ticked, why.to_string()),
        // The same words as the Apply it stops.
        Err(why) => (
            RowState::CouldntRead,
            false,
            format!("Couldn't read this setting: {why}"),
        ),
    };
    let tags = info(id);
    Some(Row {
        id,
        title: tags.title.to_string(),
        state,
        ticked,
        why,
        // The catalog's tag is for a PC with a choice to make: with one graphics card there is nothing to gain.
        impact: if id == TweakId::GpuPreference && !input.gpu_choice {
            Impact::NoneInGame
        } else {
            tags.impact
        },
        timing: tags.timing,
    })
}

fn verdict(
    sys: &dyn System,
    input: &PlanInput,
    id: TweakId,
    sid: Option<&str>,
    open: Option<&OpenRecord>,
) -> Option<Result<Verdict, String>> {
    if matches!(id, TweakId::PowerPlan | TweakId::SystemProtection) {
        return None;
    }
    let Some(sid) = sid else {
        return Some(Ok(NO_ACCOUNT));
    };
    if let Some(record) = open {
        return Some(recorded(sys, id, sid, record));
    }
    Some(match id {
        TweakId::GameMode => game_mode(sys, sid),
        TweakId::BackgroundRecording => background_recording(sys, sid),
        TweakId::WindowedOptimizations => windowed(sys, input, sid),
        TweakId::GpuPreference => gpu_preference(sys, input, sid),
        TweakId::PointerPrecision => pointer(sys, sid),
        TweakId::PowerPlan | TweakId::SystemProtection => return None,
    })
}

/// A setting Kestrel has changed: it is Applied while it still holds what Kestrel wrote. The record names the value a
/// graphics preference lives in, so that is the one read, not one worked out again.
fn recorded(
    sys: &dyn System,
    id: TweakId,
    sid: &str,
    record: &OpenRecord,
) -> Result<Verdict, String> {
    let now = known(targets::current(sys, id, sid, record.value_name.as_deref()))?;
    Ok(if targets::same_meaning(id, &now, &record.planned) {
        APPLIED
    } else {
        CHANGED_ELSEWHERE
    })
}

fn game_mode(sys: &dyn System, sid: &str) -> Result<Verdict, String> {
    // A policy that turns Game Mode off decides it, whatever the person's own value says.
    let policy = targets::found(
        sys.read_policy_dword(targets::GAME_DVR_POLICY_KEY, targets::ALLOW_AUTO_GAME_MODE),
    )?;
    let Saved::Dword { value } = known(targets::current(sys, TweakId::GameMode, sid, None))? else {
        return Err(NOT_ITS_KIND.to_string());
    };
    if policy == Some(0) {
        return Ok((
            RowState::BlockedByPolicy,
            false,
            "An organisation policy turns Game Mode off.",
        ));
    }
    // Only a 0 is off; absent is on, as is anything else.
    Ok(if value == Some(0) {
        (
            RowState::Available,
            true,
            "Tells Windows to favour the game you're playing. Ryzen X3D chips also need it to pick the right cores.",
        )
    } else {
        (RowState::AlreadySet, false, "Game Mode is already on.")
    })
}

fn background_recording(sys: &dyn System, sid: &str) -> Result<Verdict, String> {
    // Recording in the background is on only if capture as a whole is on and no policy forbids it. The two that say
    // otherwise are read here and never written.
    let policy = targets::found(
        sys.read_policy_dword(targets::GAME_DVR_POLICY_KEY, targets::ALLOW_GAME_DVR),
    )?;
    let capture = targets::found(sys.read_dword(sid, targets::GAME_DVR_KEY, targets::APP_CAPTURE))?;
    let Saved::Dword { value } = known(targets::current(
        sys,
        TweakId::BackgroundRecording,
        sid,
        None,
    ))?
    else {
        return Err(NOT_ITS_KIND.to_string());
    };
    Ok(
        if value == Some(1) && policy != Some(0) && capture != Some(0) {
            (
            RowState::Available,
            true,
            "Keeps recording gameplay in the background, which keeps the video encoder busy. Manual recording and screenshots keep working.",
        )
        } else {
            (
                RowState::AlreadySet,
                false,
                "Background recording is already off.",
            )
        },
    )
}

fn windowed(sys: &dyn System, input: &PlanInput, sid: &str) -> Result<Verdict, String> {
    let Saved::Token { token, .. } = known(targets::current(
        sys,
        TweakId::WindowedOptimizations,
        sid,
        None,
    ))?
    else {
        return Err(NOT_ITS_KIND.to_string());
    };
    Ok(if token.as_deref() == Some("1") {
        (RowState::AlreadySet, false, "Already on.")
    } else if matches!(input.fullscreen_mode, Some(1 | 2)) {
        (
            RowState::Available,
            true,
            "Lets windowed modes present frames with the newer flip model. Your VALORANT runs in a windowed mode.",
        )
    } else {
        (
            RowState::Available,
            false,
            "Only matters in windowed modes.",
        )
    })
}

fn gpu_preference(sys: &dyn System, input: &PlanInput, sid: &str) -> Result<Verdict, String> {
    if !input.gpu_choice {
        return Ok((
            RowState::NotNeeded,
            false,
            "One graphics card: nothing to choose.",
        ));
    }
    let Some(exe) = input.valorant_exe.as_deref() else {
        return Ok((RowState::NotNeeded, false, "VALORANT not found."));
    };
    // The value may already be there under another spelling of the path; that one is read, and later written.
    let name = known(targets::gpu_value_name(sys, sid, exe))?;
    let Saved::Token { token, .. } = known(targets::current(
        sys,
        TweakId::GpuPreference,
        sid,
        Some(&name),
    ))?
    else {
        return Err(NOT_ITS_KIND.to_string());
    };
    Ok(match token.as_deref() {
        Some("2") => (
            RowState::AlreadySet,
            false,
            "VALORANT is already set to High performance.",
        ),
        // The person chose it. Kestrel offers to change it, but doesn't tick it for them.
        Some("1") => (RowState::Available, false, "You picked Power saving."),
        None | Some("0") => (
            RowState::Available,
            true,
            "This PC has two graphics chips, and Windows can pick the weaker one.",
        ),
        // Windows doesn't define any other value, so this isn't one Kestrel understands well enough to replace.
        Some(other) => {
            return Err(format!(
                "VALORANT's graphics preference is {other}, which Windows doesn't define."
            ))
        }
    })
}

fn pointer(sys: &dyn System, sid: &str) -> Result<Verdict, String> {
    let Saved::Mouse { speed, live, .. } =
        known(targets::current(sys, TweakId::PointerPrecision, sid, None))?
    else {
        return Err(NOT_ITS_KIND.to_string());
    };
    // The mouse in force is the person's own when Kestrel runs as them. Otherwise Kestrel only ever changes the
    // strings, so those are what say whether it has.
    let on = if sys.account().differs() {
        targets::acceleration_on(speed.as_deref())
    } else {
        live[2] != 0
    };
    Ok(if on {
        (
            RowState::Available,
            false,
            "Doesn't change aim or FPS in VALORANT, which reads the mouse directly. Only the cursor on the desktop changes.",
        )
    } else {
        (RowState::AlreadySet, false, "Already off.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeSystem, MOUSE_LIVE, SID};
    use crate::system::Read;
    use crate::targets::{
        ALLOW_AUTO_GAME_MODE, ALLOW_GAME_DVR, APP_CAPTURE, AUTO_GAME_MODE, DIRECTX_KEY,
        DIRECTX_SETTINGS, GAME_BAR_KEY, GAME_DVR_KEY, GAME_DVR_POLICY_KEY, HISTORICAL_CAPTURE,
        MOUSE_KEY, MOUSE_SPEED, MOUSE_THRESHOLD_1, MOUSE_THRESHOLD_2,
    };
    use kestrel_core::snapshot::{owner_pc_fixture, GpuInfo, GpuVendor};
    use kestrel_probe::account::Account;
    use RowState::{
        AlreadySet, Applied, Available, BlockedByPolicy, ChangedElsewhere, CouldntRead, NotNeeded,
    };
    use TweakId::{
        BackgroundRecording, GameMode, GpuPreference, PointerPrecision, PowerPlan,
        SystemProtection, WindowedOptimizations,
    };

    const OTHER_SID: &str = "S-1-5-21-1-2-3-1002";
    const EXE: &str =
        r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe";
    const EXE_LOWER: &str =
        r"c:\riot games\valorant\live\shootergame\binaries\win64\valorant-win64-shipping.exe";

    /// The five settings, in the order of the Tweaks tab.
    const SETTINGS: [TweakId; 5] = [
        GameMode,
        BackgroundRecording,
        WindowedOptimizations,
        GpuPreference,
        PointerPrecision,
    ];

    // The words of the rows, as the table gives them.
    const GAME_MODE_OFF: &str = "Tells Windows to favour the game you're playing. Ryzen X3D chips also need it to pick the right cores.";
    const GAME_MODE_ON: &str = "Game Mode is already on.";
    const GAME_MODE_POLICY: &str = "An organisation policy turns Game Mode off.";
    const RECORDING_ON: &str = "Keeps recording gameplay in the background, which keeps the video encoder busy. Manual recording and screenshots keep working.";
    const RECORDING_OFF: &str = "Background recording is already off.";
    const WINDOWED_ON: &str = "Already on.";
    const WINDOWED_MODE: &str =
        "Lets windowed modes present frames with the newer flip model. Your VALORANT runs in a windowed mode.";
    const WINDOWED_NOT_MODE: &str = "Only matters in windowed modes.";
    const ONE_GPU: &str = "One graphics card: nothing to choose.";
    const NO_VALORANT: &str = "VALORANT not found.";
    const GPU_HIGH: &str = "VALORANT is already set to High performance.";
    const GPU_POWER_SAVING: &str = "You picked Power saving.";
    const GPU_WEAKER: &str = "This PC has two graphics chips, and Windows can pick the weaker one.";
    const POINTER_ON: &str = "Doesn't change aim or FPS in VALORANT, which reads the mouse directly. Only the cursor on the desktop changes.";
    const POINTER_OFF: &str = "Already off.";
    const NO_ACCOUNT: &str = "Couldn't tell which account is signed in.";
    const KESTREL_DID: &str = "Kestrel changed this.";
    const SOMETHING_ELSE: &str = "Something else changed this after Kestrel did.";

    /// A desktop on Windows 11 25H2 with two graphics chips and VALORANT where Riot puts it.
    fn input() -> PlanInput {
        PlanInput {
            build: 26200,
            form_factor: FormFactor::Desktop,
            hybrid_cpu: false,
            cpu_name: "Intel(R) Core(TM) i9-11900K".to_string(),
            gpu_choice: true,
            valorant_exe: Some(EXE.to_string()),
            fullscreen_mode: None,
        }
    }

    /// What a row says, which is what the table gives: the state, whether it is ticked, and why.
    type Says = (RowState, bool, String);

    fn says(state: RowState, ticked: bool, why: &str) -> Says {
        (state, ticked, why.to_string())
    }

    /// A row that couldn't be read, and what stopped it.
    fn couldnt(reason: &str) -> Says {
        says(
            CouldntRead,
            false,
            &format!("Couldn't read this setting: {reason}"),
        )
    }

    fn row_with(pc: &FakeSystem, input: &PlanInput, id: TweakId) -> Says {
        let row = setting_row(pc, input, id, Some(SID), None).expect("a row");
        (row.state, row.ticked, row.why)
    }

    fn row(pc: &FakeSystem, id: TweakId) -> Says {
        row_with(pc, &input(), id)
    }

    fn name_of(id: TweakId) -> Option<&'static str> {
        (id == GpuPreference).then_some(EXE)
    }

    /// A PC where each setting has something to change: Game Mode is off, background recording is on, the windowed
    /// token isn't set (the settings value holds another token only), the game's graphics preference is left to
    /// Windows, and the mouse is as it ships.
    fn arranged() -> FakeSystem {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1);
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=0;");
        pc
    }

    fn differing() -> FakeSystem {
        let pc = arranged();
        pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: Some(OTHER_SID.to_string()),
        });
        pc
    }

    fn open(tweak: TweakId, value_name: Option<&str>, prior: Saved, planned: Saved) -> OpenRecord {
        OpenRecord {
            tweak,
            sid: Some(SID.to_string()),
            value_name: value_name.map(str::to_string),
            prior,
            planned,
            at: "2026-11-02T18:12:00+00:00".to_string(),
            settled: true,
        }
    }

    /// Lets Kestrel change the setting, as Apply does, and returns the record the journal would hold.
    fn applied(pc: &FakeSystem, id: TweakId) -> OpenRecord {
        let prior = match targets::current(pc, id, SID, name_of(id)) {
            Read::Value(prior) => prior,
            other => panic!("expected {id:?} to read, got {other:?}"),
        };
        let planned = targets::target(id, &prior);
        targets::write(pc, id, SID, name_of(id), &planned, pc.account().differs()).unwrap();
        open(id, name_of(id), prior, planned)
    }

    /// Changes the setting to something that is neither what it was nor what Kestrel left.
    fn change_elsewhere(pc: &FakeSystem, id: TweakId) {
        match id {
            GameMode => pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 2),
            BackgroundRecording => pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 2),
            WindowedOptimizations => pc.set_string(
                SID,
                DIRECTX_KEY,
                DIRECTX_SETTINGS,
                "VRROptimizeEnable=1;SwapEffectUpgradeEnable=0;",
            ),
            GpuPreference => pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=1;"),
            PointerPrecision => {
                pc.set_string(SID, MOUSE_KEY, MOUSE_SPEED, "2");
                pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_1, "4");
                pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_2, "12");
            }
            PowerPlan | SystemProtection => unreachable!("not one of the five"),
        }
    }

    fn gpu(name: &str, vendor: GpuVendor, integrated: bool) -> GpuInfo {
        GpuInfo {
            name: name.to_string(),
            vendor,
            is_integrated: integrated,
            ..Default::default()
        }
    }

    // The rows, one for each line of the table.

    #[test]
    fn game_mode_zero_is_available_and_ticked() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        assert_eq!(row(&pc, GameMode), says(Available, true, GAME_MODE_OFF));
    }

    #[test]
    fn game_mode_absent_is_already_set() {
        let pc = FakeSystem::new();
        assert_eq!(row(&pc, GameMode), says(AlreadySet, false, GAME_MODE_ON));
    }

    #[test]
    fn game_mode_one_is_already_set() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 1);
        assert_eq!(row(&pc, GameMode), says(AlreadySet, false, GAME_MODE_ON));
        // Only a 0 turns it off; whatever else is there, it is not on Kestrel to touch it.
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 2);
        assert_eq!(row(&pc, GameMode), says(AlreadySet, false, GAME_MODE_ON));
    }

    #[test]
    fn game_mode_policy_zero_is_blocked_by_policy() {
        // The policy turns Game Mode off whatever the person's own value says.
        for value in [None, Some(0), Some(1)] {
            let pc = FakeSystem::new();
            if let Some(value) = value {
                pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, value);
            }
            pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_AUTO_GAME_MODE, 0);
            assert_eq!(
                row(&pc, GameMode),
                says(BlockedByPolicy, false, GAME_MODE_POLICY),
                "{value:?}"
            );
        }
        // A policy that allows it, or says nothing, blocks nothing.
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_AUTO_GAME_MODE, 1);
        assert_eq!(row(&pc, GameMode), says(Available, true, GAME_MODE_OFF));
    }

    #[test]
    fn recording_on_is_available_and_ticked() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1);
        assert_eq!(
            row(&pc, BackgroundRecording),
            says(Available, true, RECORDING_ON)
        );
        // Capture switched on and policy allowing it are the same as neither being set.
        pc.set_dword(SID, GAME_DVR_KEY, APP_CAPTURE, 1);
        pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_GAME_DVR, 1);
        assert_eq!(
            row(&pc, BackgroundRecording),
            says(Available, true, RECORDING_ON)
        );
    }

    #[test]
    fn recording_otherwise_is_already_set() {
        let off = says(AlreadySet, false, RECORDING_OFF);

        // Absent is off, and so is 0.
        let pc = FakeSystem::new();
        assert_eq!(row(&pc, BackgroundRecording), off);
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 0);
        assert_eq!(row(&pc, BackgroundRecording), off);

        // Recording is on but capture itself is off, so nothing is recorded.
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1);
        pc.set_dword(SID, GAME_DVR_KEY, APP_CAPTURE, 0);
        assert_eq!(row(&pc, BackgroundRecording), off);

        // Or a policy forbids it.
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1);
        pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_GAME_DVR, 0);
        assert_eq!(row(&pc, BackgroundRecording), off);

        // A value that is neither 0 nor 1 is not "on" to Kestrel.
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 2);
        assert_eq!(row(&pc, BackgroundRecording), off);
    }

    #[test]
    fn windowed_token_one_is_already_set() {
        let pc = FakeSystem::new();
        pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "VRROptimizeEnable=1;SwapEffectUpgradeEnable=1;",
        );
        assert_eq!(
            row(&pc, WindowedOptimizations),
            says(AlreadySet, false, WINDOWED_ON)
        );
        // Whatever mode the game is in.
        let windowed = PlanInput {
            fullscreen_mode: Some(2),
            ..input()
        };
        assert_eq!(
            row_with(&pc, &windowed, WindowedOptimizations),
            says(AlreadySet, false, WINDOWED_ON)
        );
    }

    #[test]
    fn windowed_in_a_windowed_mode_is_available_and_ticked() {
        let pc = FakeSystem::new();
        for mode in [1, 2] {
            let input = PlanInput {
                fullscreen_mode: Some(mode),
                ..input()
            };
            assert_eq!(
                row_with(&pc, &input, WindowedOptimizations),
                says(Available, true, WINDOWED_MODE),
                "FullscreenMode {mode}"
            );
        }
        // Off, or set to something else, it is just as available.
        pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "SwapEffectUpgradeEnable=0;",
        );
        let input = PlanInput {
            fullscreen_mode: Some(1),
            ..input()
        };
        assert_eq!(
            row_with(&pc, &input, WindowedOptimizations),
            says(Available, true, WINDOWED_MODE)
        );
    }

    #[test]
    fn windowed_otherwise_is_available_and_not_ticked() {
        let pc = FakeSystem::new();
        // Exclusive fullscreen, or a mode nobody told Kestrel about.
        for mode in [Some(0), None, Some(3)] {
            let input = PlanInput {
                fullscreen_mode: mode,
                ..input()
            };
            assert_eq!(
                row_with(&pc, &input, WindowedOptimizations),
                says(Available, false, WINDOWED_NOT_MODE),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn gpu_without_a_choice_is_not_needed() {
        let pc = arranged();
        let input = PlanInput {
            gpu_choice: false,
            ..input()
        };
        assert_eq!(
            row_with(&pc, &input, GpuPreference),
            says(NotNeeded, false, ONE_GPU)
        );
    }

    #[test]
    fn gpu_without_valorant_is_not_needed() {
        let pc = arranged();
        let input = PlanInput {
            valorant_exe: None,
            ..input()
        };
        assert_eq!(
            row_with(&pc, &input, GpuPreference),
            says(NotNeeded, false, NO_VALORANT)
        );
        // With no choice to make as well, that is what is said first.
        let input = PlanInput {
            gpu_choice: false,
            valorant_exe: None,
            ..input
        };
        assert_eq!(
            row_with(&pc, &input, GpuPreference),
            says(NotNeeded, false, ONE_GPU)
        );
    }

    #[test]
    fn gpu_token_two_is_already_set() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=2;");
        assert_eq!(row(&pc, GpuPreference), says(AlreadySet, false, GPU_HIGH));
    }

    #[test]
    fn gpu_token_one_is_available_and_not_ticked() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=1;");
        assert_eq!(
            row(&pc, GpuPreference),
            says(Available, false, GPU_POWER_SAVING)
        );
    }

    #[test]
    fn gpu_token_absent_or_zero_is_available_and_ticked() {
        let weaker = says(Available, true, GPU_WEAKER);
        // No key, no value, a value with other tokens, and an explicit "Windows decides".
        let pc = FakeSystem::new();
        assert_eq!(row(&pc, GpuPreference), weaker);
        pc.add_key(SID, DIRECTX_KEY);
        assert_eq!(row(&pc, GpuPreference), weaker);
        pc.set_string(SID, DIRECTX_KEY, EXE, "SwapEffectUpgradeEnable=1;");
        assert_eq!(row(&pc, GpuPreference), weaker);
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=0;");
        assert_eq!(row(&pc, GpuPreference), weaker);
    }

    #[test]
    fn a_gpu_choice_nobody_defined_is_couldnt_read() {
        // Windows knows three values. Anything else is something Kestrel doesn't understand, so it is left alone.
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=3;");
        assert_eq!(
            row(&pc, GpuPreference),
            couldnt("VALORANT's graphics preference is 3, which Windows doesn't define.")
        );
    }

    #[test]
    fn the_gpu_row_reads_the_value_in_the_spelling_it_already_has() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE_LOWER, "GpuPreference=2;");
        assert_eq!(row(&pc, GpuPreference), says(AlreadySet, false, GPU_HIGH));
        let pc = FakeSystem::new();
        pc.set_string(
            SID,
            DIRECTX_KEY,
            "C:/Riot Games/VALORANT/live/ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe",
            "GpuPreference=1;",
        );
        assert_eq!(
            row(&pc, GpuPreference),
            says(Available, false, GPU_POWER_SAVING)
        );
    }

    #[test]
    fn pointer_acceleration_on_is_available_and_not_ticked() {
        let pc = FakeSystem::new();
        assert_eq!(
            row(&pc, PointerPrecision),
            says(Available, false, POINTER_ON)
        );
    }

    #[test]
    fn pointer_off_is_already_set() {
        let pc = FakeSystem::new();
        pc.set_mouse([0, 0, 0]);
        assert_eq!(
            row(&pc, PointerPrecision),
            says(AlreadySet, false, POINTER_OFF)
        );
    }

    #[test]
    fn pointer_acceleration_is_the_mouse_in_force_when_the_accounts_match() {
        // The strings on disk say off, but the person's mouse is accelerating right now.
        let pc = FakeSystem::new();
        for name in [MOUSE_SPEED, MOUSE_THRESHOLD_1, MOUSE_THRESHOLD_2] {
            pc.set_string(SID, MOUSE_KEY, name, "0");
        }
        assert_eq!(
            row(&pc, PointerPrecision),
            says(Available, false, POINTER_ON)
        );
        // And the other way round.
        let pc = FakeSystem::new();
        pc.set_mouse([0, 0, 0]);
        assert_eq!(
            row(&pc, PointerPrecision),
            says(AlreadySet, false, POINTER_OFF)
        );
    }

    #[test]
    fn pointer_acceleration_is_the_registry_when_the_accounts_differ() {
        // The mouse in force is the same as ever until the next sign-in: only the strings can say what Kestrel left.
        for (speed, row_says) in [
            (None, says(Available, false, POINTER_ON)),
            (Some("1"), says(Available, false, POINTER_ON)),
            (Some("2"), says(Available, false, POINTER_ON)),
            (Some("0"), says(AlreadySet, false, POINTER_OFF)),
        ] {
            let pc = differing();
            assert_eq!(pc.mouse_live(), Read::Value([6, 10, 1]));
            match speed {
                Some(speed) => pc.set_string(SID, MOUSE_KEY, MOUSE_SPEED, speed),
                None => pc.remove_value(SID, MOUSE_KEY, MOUSE_SPEED),
            }
            assert_eq!(row(&pc, PointerPrecision), row_says, "{speed:?}");
        }
    }

    // Rows that cannot be read.

    #[test]
    fn a_wrong_type_is_couldnt_read() {
        let pc = FakeSystem::new();
        pc.fail_read("AutoGameModeEnabled");
        assert_eq!(
            row(&pc, GameMode),
            couldnt("the fake was told that reading AutoGameModeEnabled fails")
        );
        assert!(matches!(
            targets::current(&pc, GameMode, SID, None),
            Read::Failed(_)
        ));

        // The value really is the wrong type: a string where a number goes.
        let pc = FakeSystem::new();
        pc.set_string(SID, GAME_BAR_KEY, AUTO_GAME_MODE, "0");
        assert_eq!(
            row(&pc, GameMode),
            couldnt("AutoGameModeEnabled is not a DWORD value")
        );
        assert!(matches!(
            targets::current(&pc, GameMode, SID, None),
            Read::Failed(_)
        ));
        // Nothing was touched to find that out.
        assert!(pc.writes().is_empty());
        assert_eq!(
            pc.read_string(SID, GAME_BAR_KEY, AUTO_GAME_MODE),
            Read::Value("0".to_string())
        );
    }

    #[test]
    fn a_failed_read_is_couldnt_read_for_every_setting_and_says_which_read_it_was() {
        for (id, name) in [
            (GameMode, AUTO_GAME_MODE),
            // A policy that can't be read might be forbidding it.
            (GameMode, ALLOW_AUTO_GAME_MODE),
            (BackgroundRecording, HISTORICAL_CAPTURE),
            (BackgroundRecording, APP_CAPTURE),
            (BackgroundRecording, ALLOW_GAME_DVR),
            (WindowedOptimizations, DIRECTX_SETTINGS),
            (WindowedOptimizations, DIRECTX_KEY),
            (GpuPreference, EXE),
            (GpuPreference, DIRECTX_KEY),
            (PointerPrecision, MOUSE_SPEED),
            (PointerPrecision, MOUSE_THRESHOLD_1),
            (PointerPrecision, MOUSE_THRESHOLD_2),
            (PointerPrecision, MOUSE_LIVE),
        ] {
            // On a PC where the row would be on offer, so that it is the read that stops it.
            let pc = arranged();
            let input = PlanInput {
                fullscreen_mode: Some(1),
                ..input()
            };
            assert_eq!(row_with(&pc, &input, id).0, Available, "{id:?}");
            pc.fail_read(name);
            assert_eq!(
                row_with(&pc, &input, id),
                couldnt(&format!("the fake was told that reading {name} fails")),
                "{id:?}: {name}"
            );
        }
    }

    #[test]
    fn a_wrong_type_is_couldnt_read_for_every_setting_and_says_which_value_it_was() {
        let input = PlanInput {
            fullscreen_mode: Some(1),
            ..input()
        };
        let wrong = |build: &dyn Fn(&FakeSystem)| {
            let pc = arranged();
            build(&pc);
            pc
        };
        let not_a_dword = |name: &str| format!("{name} is not a DWORD value");
        let not_a_string = |name: &str| format!("{name} is not a string value");
        for (id, pc, reason) in [
            (
                GameMode,
                wrong(&|pc| pc.set_string(SID, GAME_BAR_KEY, AUTO_GAME_MODE, "0")),
                not_a_dword(AUTO_GAME_MODE),
            ),
            (
                BackgroundRecording,
                wrong(&|pc| pc.set_string(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, "1")),
                not_a_dword(HISTORICAL_CAPTURE),
            ),
            (
                BackgroundRecording,
                wrong(&|pc| pc.set_string(SID, GAME_DVR_KEY, APP_CAPTURE, "1")),
                not_a_dword(APP_CAPTURE),
            ),
            (
                WindowedOptimizations,
                wrong(&|pc| pc.set_dword(SID, DIRECTX_KEY, DIRECTX_SETTINGS, 1)),
                not_a_string(DIRECTX_SETTINGS),
            ),
            (
                GpuPreference,
                wrong(&|pc| pc.set_dword(SID, DIRECTX_KEY, EXE, 2)),
                not_a_string(EXE),
            ),
            (
                PointerPrecision,
                wrong(&|pc| pc.set_dword(SID, MOUSE_KEY, MOUSE_SPEED, 0)),
                not_a_string(MOUSE_SPEED),
            ),
        ] {
            let before = pc.dump();
            assert_eq!(row_with(&pc, &input, id), couldnt(&reason), "{id:?}");
            assert_eq!(pc.dump(), before, "{id:?}");
        }
        // The same goes for a policy of the wrong type.
        let pc = arranged();
        pc.set_policy_string(GAME_DVR_POLICY_KEY, ALLOW_AUTO_GAME_MODE, "0");
        assert_eq!(
            row_with(&pc, &input, GameMode),
            couldnt(&not_a_dword(ALLOW_AUTO_GAME_MODE))
        );
        let pc = arranged();
        pc.set_policy_string(GAME_DVR_POLICY_KEY, ALLOW_GAME_DVR, "0");
        assert_eq!(
            row_with(&pc, &input, BackgroundRecording),
            couldnt(&not_a_dword(ALLOW_GAME_DVR))
        );
    }

    #[test]
    fn no_signed_in_account_is_couldnt_read_for_every_setting() {
        let pc = arranged();
        let input = PlanInput {
            fullscreen_mode: Some(1),
            ..input()
        };
        for id in SETTINGS {
            // The row would be on offer for the account that is signed in.
            assert_eq!(row_with(&pc, &input, id).0, Available, "{id:?}");
            let row = setting_row(&pc, &input, id, None, None).expect("a row");
            assert_eq!(
                (row.state, row.ticked, row.why.as_str()),
                (CouldntRead, false, NO_ACCOUNT),
                "{id:?}"
            );
        }
    }

    // Rows for something Kestrel has changed.

    #[test]
    fn an_open_record_matching_planned_is_applied() {
        for differs in [false, true] {
            let pc = if differs { differing() } else { arranged() };
            for id in SETTINGS {
                let record = applied(&pc, id);
                let row = setting_row(&pc, &input(), id, Some(SID), Some(&record)).unwrap();
                assert_eq!(
                    (row.state, row.ticked, row.why.as_str()),
                    (Applied, false, KESTREL_DID),
                    "{id:?} (accounts differ: {differs})"
                );
            }
        }
    }

    #[test]
    fn an_open_record_otherwise_is_changed_elsewhere() {
        for id in SETTINGS {
            let pc = arranged();
            let record = applied(&pc, id);
            change_elsewhere(&pc, id);
            let row = setting_row(&pc, &input(), id, Some(SID), Some(&record)).unwrap();
            assert_eq!(
                (row.state, row.ticked, row.why.as_str()),
                (ChangedElsewhere, false, SOMETHING_ELSE),
                "{id:?}"
            );
        }
    }

    #[test]
    fn a_value_back_at_its_prior_is_changed_elsewhere_if_it_gets_this_far() {
        // The engine closes a record whose setting went back before it asks for a row. If one gets here anyway, it is
        // not what Kestrel left, so it is not Applied.
        for id in SETTINGS {
            let pc = arranged();
            let record = applied(&pc, id);
            targets::restore(
                &pc,
                id,
                SID,
                name_of(id),
                &record.prior,
                pc.account().differs(),
            )
            .unwrap();
            let row = setting_row(&pc, &input(), id, Some(SID), Some(&record)).unwrap();
            assert_eq!(row.state, ChangedElsewhere, "{id:?}");
        }
    }

    #[test]
    fn an_open_record_decides_before_the_policy_and_the_current_value() {
        // A policy that now turns Game Mode off does not undo that Kestrel turned it on.
        let pc = arranged();
        let record = applied(&pc, GameMode);
        pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_AUTO_GAME_MODE, 0);
        let row = setting_row(&pc, &input(), GameMode, Some(SID), Some(&record)).unwrap();
        assert_eq!(row.state, Applied);

        let pc = arranged();
        let record = applied(&pc, BackgroundRecording);
        pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_GAME_DVR, 0);
        let row =
            setting_row(&pc, &input(), BackgroundRecording, Some(SID), Some(&record)).unwrap();
        assert_eq!(row.state, Applied);
    }

    #[test]
    fn an_open_record_still_decides_when_the_offer_has_gone_away() {
        // The person took out the second graphics card, or uninstalled VALORANT: the change is still on the PC, and
        // the row still says so and can still undo it.
        let pc = arranged();
        let record = applied(&pc, GpuPreference);
        for input in [
            PlanInput {
                gpu_choice: false,
                ..input()
            },
            PlanInput {
                valorant_exe: None,
                ..input()
            },
        ] {
            let row = setting_row(&pc, &input, GpuPreference, Some(SID), Some(&record)).unwrap();
            assert_eq!(row.state, Applied);
        }

        // The same for a build that would hide the windowed row.
        let pc = arranged();
        let record = applied(&pc, WindowedOptimizations);
        let old = PlanInput {
            build: 22000,
            ..input()
        };
        assert!(setting_row(&pc, &old, WindowedOptimizations, Some(SID), None).is_none());
        let row = setting_row(&pc, &old, WindowedOptimizations, Some(SID), Some(&record)).unwrap();
        assert_eq!(row.state, Applied);
    }

    #[test]
    fn a_gpu_record_reads_the_value_it_names_and_not_one_worked_out_again() {
        // The game moved, or the value was written in another spelling: the record's own name is the one that counts.
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE_LOWER, "GpuPreference=2;");
        let record = open(
            GpuPreference,
            Some(EXE_LOWER),
            Saved::Token {
                token: Some("0".to_string()),
                value_existed: true,
                key_existed: true,
            },
            Saved::Token {
                token: Some("2".to_string()),
                value_existed: true,
                key_existed: true,
            },
        );
        let moved = PlanInput {
            valorant_exe: Some(
                r"D:\Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"
                    .to_string(),
            ),
            ..input()
        };
        let row = setting_row(&pc, &moved, GpuPreference, Some(SID), Some(&record)).unwrap();
        assert_eq!(row.state, Applied);

        // A record that names nothing is one that cannot be read, not one to be guessed at.
        let nameless = OpenRecord {
            value_name: None,
            ..record
        };
        let row = setting_row(&pc, &input(), GpuPreference, Some(SID), Some(&nameless)).unwrap();
        assert_eq!(
            (row.state, row.ticked, row.why),
            couldnt("VALORANT's file isn't known, so its graphics preference was left alone.")
        );
    }

    #[test]
    fn a_failed_read_beats_an_open_record() {
        for id in SETTINGS {
            let pc = arranged();
            let record = applied(&pc, id);
            let name = match id {
                GameMode => AUTO_GAME_MODE,
                BackgroundRecording => HISTORICAL_CAPTURE,
                WindowedOptimizations => DIRECTX_SETTINGS,
                GpuPreference => EXE,
                PointerPrecision => MOUSE_SPEED,
                PowerPlan | SystemProtection => unreachable!("not one of the five"),
            };
            pc.fail_read(name);
            let row = setting_row(&pc, &input(), id, Some(SID), Some(&record)).unwrap();
            assert_eq!(
                (row.state, row.ticked, row.why),
                couldnt(&format!("the fake was told that reading {name} fails")),
                "{id:?}"
            );
        }
    }

    #[test]
    fn a_record_for_a_state_that_does_not_fit_is_changed_elsewhere_not_a_crash() {
        let pc = arranged();
        let hostile = open(
            GameMode,
            None,
            Saved::Protection { on: false },
            Saved::Plan {
                guid: "4b455354-524c-4e50-a1b2-c3d4e5f60705".to_string(),
                name: "Kestrel".to_string(),
            },
        );
        let row = setting_row(&pc, &input(), GameMode, Some(SID), Some(&hostile)).unwrap();
        assert_eq!(row.state, ChangedElsewhere);
    }

    // Which rows there are.

    #[test]
    fn windowed_is_hidden_below_22621() {
        let pc = FakeSystem::new();
        for (build, shown) in [
            (0, false),
            (19045, false),
            (22000, false),
            (22620, false),
            (22621, true),
            (26200, true),
        ] {
            let input = PlanInput { build, ..input() };
            let row = setting_row(&pc, &input, WindowedOptimizations, Some(SID), None);
            assert_eq!(row.is_some(), shown, "build {build}");
            // Not having an account to ask doesn't bring back a row the build hides.
            let row = setting_row(&pc, &input, WindowedOptimizations, None, None);
            assert_eq!(row.is_some(), shown, "build {build}, no account");
        }
        // The other four don't depend on the build.
        let old = PlanInput {
            build: 22000,
            ..input()
        };
        for id in [
            GameMode,
            BackgroundRecording,
            GpuPreference,
            PointerPrecision,
        ] {
            assert!(
                setting_row(&pc, &old, id, Some(SID), None).is_some(),
                "{id:?}"
            );
        }
    }

    #[test]
    fn the_power_plan_and_system_protection_have_no_row_here() {
        let pc = FakeSystem::new();
        for id in [PowerPlan, SystemProtection] {
            for sid in [Some(SID), None] {
                assert!(
                    setting_row(&pc, &input(), id, sid, None).is_none(),
                    "{id:?} {sid:?}"
                );
            }
        }
    }

    #[test]
    fn a_row_carries_the_title_and_the_tags_of_its_tweak() {
        let pc = arranged();
        for id in SETTINGS {
            let row = setting_row(&pc, &input(), id, Some(SID), None).unwrap();
            let info = info(id);
            assert_eq!(row.id, id);
            assert_eq!(row.title, info.title);
            assert_eq!(row.impact, info.impact);
            assert_eq!(row.timing, info.timing);
        }
    }

    #[test]
    fn the_gpu_preference_can_change_nothing_in_game_with_one_graphics_card() {
        let pc = arranged();
        let one_card = PlanInput {
            gpu_choice: false,
            ..input()
        };
        // The catalog's High is for a PC with a choice to make. With one card there is nothing to gain.
        let row = setting_row(&pc, &one_card, GpuPreference, Some(SID), None).unwrap();
        assert_eq!((row.state, row.impact), (NotNeeded, Impact::NoneInGame));
        // Even a change Kestrel made before the second card came out.
        let record = applied(&pc, GpuPreference);
        let row = setting_row(&pc, &one_card, GpuPreference, Some(SID), Some(&record)).unwrap();
        assert_eq!((row.state, row.impact), (Applied, Impact::NoneInGame));

        // With a choice to make, it is the catalog's.
        let pc = arranged();
        let row = setting_row(&pc, &input(), GpuPreference, Some(SID), None).unwrap();
        assert_eq!((row.state, row.impact), (Available, Impact::High));
        // Which is about the PC, not about whether VALORANT was found.
        let no_game = PlanInput {
            valorant_exe: None,
            ..input()
        };
        let row = setting_row(&pc, &no_game, GpuPreference, Some(SID), None).unwrap();
        assert_eq!((row.state, row.impact), (NotNeeded, Impact::High));

        // No other row goes by it.
        for id in SETTINGS.into_iter().filter(|id| *id != GpuPreference) {
            let row = setting_row(&pc, &one_card, id, Some(SID), None).unwrap();
            assert_eq!(row.impact, info(id).impact, "{id:?}");
        }
    }

    #[test]
    fn only_an_available_row_is_ticked() {
        let arrangements: [(FakeSystem, PlanInput); 4] = [
            (FakeSystem::new(), input()),
            (
                arranged(),
                PlanInput {
                    fullscreen_mode: Some(2),
                    ..input()
                },
            ),
            (differing(), input()),
            (
                FakeSystem::new(),
                PlanInput {
                    gpu_choice: false,
                    ..input()
                },
            ),
        ];
        for (pc, input) in &arrangements {
            for id in SETTINGS {
                let (state, ticked, _) = row_with(pc, input, id);
                assert!(!ticked || state == Available, "{id:?}: {state:?}");
            }
        }
    }

    #[test]
    fn making_rows_writes_nothing() {
        let mut pcs = vec![FakeSystem::new(), arranged(), differing(), with_policies()];
        let failing = arranged();
        failing.fail_read(AUTO_GAME_MODE);
        failing.fail_read(DIRECTX_KEY);
        pcs.push(failing);
        for pc in &pcs {
            let before = pc.dump();
            let record = open(
                GameMode,
                None,
                Saved::Dword { value: Some(0) },
                Saved::Dword { value: Some(1) },
            );
            for id in SETTINGS {
                for held in [None, Some(&record)] {
                    for sid in [Some(SID), None] {
                        let _ = setting_row(pc, &input(), id, sid, held);
                    }
                }
            }
            assert!(pc.writes().is_empty(), "{:?}", pc.writes());
            assert_eq!(pc.dump(), before);
        }
    }

    fn with_policies() -> FakeSystem {
        let pc = arranged();
        pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_AUTO_GAME_MODE, 0);
        pc.set_policy_dword(GAME_DVR_POLICY_KEY, ALLOW_GAME_DVR, 0);
        pc
    }

    // What the rules know about the PC.

    #[test]
    fn gpu_exe_uses_backslashes() {
        let mut snapshot = owner_pc_fixture();
        snapshot.storage.valorant.as_mut().unwrap().path =
            "C:/Riot Games/VALORANT/live".to_string();
        let input = PlanInput::from_snapshot(&snapshot, None);
        let exe = input.valorant_exe.expect("VALORANT was found");
        assert!(
            exe.ends_with(r"ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"),
            "{exe}"
        );
        assert_eq!(exe, EXE);
        assert!(!exe.contains('/'), "{exe}");

        // A trailing slash on the install folder doesn't double up, and another drive is kept.
        snapshot.storage.valorant.as_mut().unwrap().path = "D:/Games/VALORANT/live/".to_string();
        let exe = PlanInput::from_snapshot(&snapshot, None)
            .valorant_exe
            .unwrap();
        assert_eq!(
            exe,
            r"D:\Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"
        );
    }

    #[test]
    fn from_snapshot_reads_the_scan() {
        let snapshot = owner_pc_fixture();
        let input = PlanInput::from_snapshot(&snapshot, Some(1));
        assert_eq!(
            input,
            PlanInput {
                build: 26200,
                form_factor: FormFactor::Desktop,
                hybrid_cpu: false,
                cpu_name: "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz".to_string(),
                // The owner has one graphics card; the other adapter is Parsec's, which only exists in software.
                gpu_choice: false,
                valorant_exe: Some(EXE.to_string()),
                fullscreen_mode: Some(1),
            }
        );
        assert_eq!(
            PlanInput::from_snapshot(&snapshot, None).fullscreen_mode,
            None
        );
    }

    #[test]
    fn from_snapshot_follows_the_scan_when_it_changes() {
        let mut snapshot = owner_pc_fixture();
        snapshot.os.build = 22000;
        snapshot.machine.form_factor = FormFactor::Laptop;
        snapshot.cpu.name = "AMD Ryzen 9 7945HX3D".to_string();
        snapshot.storage.valorant = None;
        let input = PlanInput::from_snapshot(&snapshot, None);
        assert_eq!(input.build, 22000);
        assert_eq!(input.form_factor, FormFactor::Laptop);
        assert_eq!(input.cpu_name, "AMD Ryzen 9 7945HX3D");
        assert_eq!(input.valorant_exe, None);
    }

    #[test]
    fn a_hybrid_cpu_is_one_with_performance_cores() {
        let mut snapshot = owner_pc_fixture();
        assert!(!PlanInput::from_snapshot(&snapshot, None).hybrid_cpu);
        snapshot.cpu.p_cores = Some(8);
        snapshot.cpu.e_cores = Some(8);
        assert!(PlanInput::from_snapshot(&snapshot, None).hybrid_cpu);
        // Whatever the count, having the field at all is what says it.
        snapshot.cpu.p_cores = Some(0);
        assert!(PlanInput::from_snapshot(&snapshot, None).hybrid_cpu);
    }

    #[test]
    fn a_graphics_choice_needs_two_real_graphics_chips() {
        let nvidia = gpu("NVIDIA GeForce RTX 3060 Ti", GpuVendor::Nvidia, false);
        let nvidia_too = gpu("NVIDIA GeForce RTX 3070", GpuVendor::Nvidia, false);
        let intel = gpu("Intel(R) UHD Graphics 750", GpuVendor::Intel, true);
        let amd_igpu = gpu("AMD Radeon(TM) Graphics", GpuVendor::Amd, true);
        let parsec = gpu("Parsec Virtual Display Adapter", GpuVendor::Virtual, false);
        let basic = gpu("Microsoft Basic Display Adapter", GpuVendor::Virtual, false);
        for (gpus, choice) in [
            (vec![], false),
            (vec![nvidia.clone()], false),
            (vec![parsec.clone(), nvidia.clone()], false),
            (vec![basic.clone(), intel.clone()], false),
            (vec![parsec.clone(), basic.clone()], false),
            // A laptop with both kinds.
            (vec![intel.clone(), nvidia.clone()], true),
            (vec![amd_igpu, nvidia.clone()], true),
            // Two cards, with or without a virtual adapter on top.
            (vec![nvidia.clone(), nvidia_too.clone()], true),
            (vec![parsec, nvidia, nvidia_too], true),
        ] {
            let mut snapshot = owner_pc_fixture();
            snapshot.gpus = gpus.clone();
            assert_eq!(
                PlanInput::from_snapshot(&snapshot, None).gpu_choice,
                choice,
                "{:?}",
                gpus.iter().map(|g| g.name.as_str()).collect::<Vec<_>>()
            );
        }
    }

    // What the page gets.

    #[test]
    fn row_states_are_spelled_in_kebab_case_the_way_the_page_reads_them() {
        for (state, text) in [
            (Available, "available"),
            (AlreadySet, "already-set"),
            (NotNeeded, "not-needed"),
            (BlockedByPolicy, "blocked-by-policy"),
            (Applied, "applied"),
            (ChangedElsewhere, "changed-elsewhere"),
            (RowState::PlanInactive, "plan-inactive"),
            (CouldntRead, "couldnt-read"),
        ] {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<RowState>(&json).unwrap(), state);
        }
    }

    #[test]
    fn a_row_is_spelled_the_way_the_page_reads_it() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        let row = setting_row(&pc, &input(), GameMode, Some(SID), None).unwrap();
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "game-mode",
                "title": "Game Mode",
                "state": "available",
                "ticked": true,
                "why": GAME_MODE_OFF,
                "impact": "medium",
                "timing": "now",
            })
        );
        assert_eq!(serde_json::from_value::<Row>(json).unwrap(), row);
    }

    #[test]
    fn a_preview_item_is_spelled_the_way_the_page_reads_it() {
        let item = PreviewItem {
            id: PointerPrecision,
            title: "Enhance pointer precision off".to_string(),
            location: r"HKCU\Control Panel\Mouse".to_string(),
            changes: vec![ValueChange {
                label: "Enhance pointer precision".to_string(),
                now: "on".to_string(),
                after: "off".to_string(),
            }],
            timing: Timing::NextSignIn,
            note: None,
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "pointer-precision",
                "title": "Enhance pointer precision off",
                "location": r"HKCU\Control Panel\Mouse",
                "changes": [{ "label": "Enhance pointer precision", "now": "on", "after": "off" }],
                "timing": "next-sign-in",
                "note": null,
            })
        );
        assert_eq!(serde_json::from_value::<PreviewItem>(json).unwrap(), item);
    }
}
