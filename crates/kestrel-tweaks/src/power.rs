//! The Kestrel power plan: which variant a PC gets, what its row says, how the plan is made and switched on, and how it
//! is undone. (The five Windows settings have their rules in `plan` and `targets`.)
//!
//! Kestrel's plan is a copy of one of Windows' own plans, named "Kestrel", with up to three settings changed. It always
//! has one GUID, `KESTREL_PLAN_GUID`, so a crash between making the plan and writing it down can't leave a plan nobody
//! can name. That is the only plan Kestrel ever deletes, and nothing is copied from another tool's plan, EXM's
//! included: the copy starts from High performance, or from Balanced when that is missing or the PC only suits Balanced.
//!
//! Kestrel's plan stays on the PC when the person picks another plan by hand, and its record stays open for as long as
//! it does. `use_again` switches back to it, `remove` deletes it and leaves the person's choice in use, and `undo`
//! goes back to the plan that was in use before it.
//!
//! Only `create_and_activate`, `undo`, `remove` and `use_again` change anything. The row and the Preview only read, and
//! `prior` is the read the journal's intent is written from, before anything is created.
use std::io;

use kestrel_core::cpu;
use kestrel_core::snapshot::FormFactor;
use kestrel_core::tweak_id::TweakId;

use crate::catalog::info;
use crate::journal::{OpenRecord, Saved};
use crate::plan::{PlanInput, PreviewItem, Row, RowState, ValueChange};
use crate::system::{Read, System};
use crate::targets;

/// The one GUID Kestrel's plan ever has. It goes to `PowerDuplicateScheme` as the new plan's GUID.
pub const KESTREL_PLAN_GUID: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";
/// What Windows calls Kestrel's plan, and what the journal calls the plan Kestrel means to switch to.
pub const KESTREL_PLAN_NAME: &str = "Kestrel";
const PLAN_DESCRIPTION: &str = "Created by Kestrel. Undo it from Kestrel's Tweaks tab.";

// Windows' own plans. Their names follow the person's language, so they are known by GUID.
const BALANCED_PLAN: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const HIGH_PERFORMANCE_PLAN: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const POWER_SAVER_PLAN: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";
const ULTIMATE_PLAN: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

// The settings Kestrel's plan sets, each a subgroup and a setting.
const PROCESSOR_SUBGROUP: &str = "54533251-82be-4824-96c1-47b60b740d00";
/// Core parking: the minimum percentage of cores kept awake. At 100 nothing parks.
const CORE_PARKING_MIN_CORES: &str = "0cc5b647-c1df-4637-891a-dec35c318583";
const USB_SUBGROUP: &str = "2a737441-1930-4402-8d77-b2bebba308a3";
/// USB selective suspend: 0 is off, 1 is on.
const USB_SELECTIVE_SUSPEND: &str = "48e6b7a6-50f5-4782-a5d4-53bb8f07e226";
const PCIE_SUBGROUP: &str = "501a4d13-42af-4429-9fd1-a8218c268e20";
/// PCIe link-state power management: 0 is off, 1 moderate, 2 maximum power saving.
const PCIE_LINK_STATE: &str = "ee12f906-d277-404b-b6da-e5fa1a576df5";

/// Under `HKEY_LOCAL_MACHINE`, where an organisation's policy can pick the power plan. Only ever read.
const POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Power\PowerSettings";
const POLICY_VALUE: &str = "ActivePowerScheme";

const AMD_GUIDANCE: &str = "AMD's guidance for this chip is Balanced with Game Mode on.";
const NO_PLAN_IN_USE: &str = "Windows didn't say which power plan is in use.";

/// Which plan a PC is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// A desktop with one kind of core and no Modern Standby: a copy of High performance with core parking, USB
    /// selective suspend and PCIe link-state power management all turned off.
    Full,
    /// Every other PC (a laptop, a PC that doesn't say what it is, a CPU with two kinds of core, Modern Standby): a copy
    /// of Balanced with USB and PCIe changed. Core parking stays as Balanced has it, because what forcing every core
    /// awake does on a CPU with two kinds of core hasn't been measured, and a Modern Standby PC only runs plans made
    /// from Balanced.
    Light,
    /// A dual-CCD X3D Ryzen. AMD's guidance is Balanced with Game Mode on, so no plan is offered.
    NotOffered,
}

/// The plan a PC is offered. `modern_standby` is `System::modern_standby`: Windows not saying counts as no.
pub fn variant(input: &PlanInput, modern_standby: Option<bool>) -> Variant {
    if cpu::is_dual_ccd_x3d(&input.cpu_name) {
        Variant::NotOffered
    } else if input.form_factor == FormFactor::Desktop
        && !input.hybrid_cpu
        && modern_standby != Some(true)
    {
        Variant::Full
    } else {
        Variant::Light
    }
}

/// One setting Kestrel's plan sets.
struct Setting {
    /// What the Preview calls it.
    label: &'static str,
    subgroup: &'static str,
    setting: &'static str,
    /// What Kestrel's plan holds for it, before the range this PC accepts is applied.
    value: u32,
    /// A value in words, for the Preview.
    words: fn(u32) -> String,
}

const PARKING_SETTING: Setting = Setting {
    label: "Core parking minimum cores",
    subgroup: PROCESSOR_SUBGROUP,
    setting: CORE_PARKING_MIN_CORES,
    value: 100,
    words: percent,
};
const USB_SETTING: Setting = Setting {
    label: "USB selective suspend",
    subgroup: USB_SUBGROUP,
    setting: USB_SELECTIVE_SUSPEND,
    value: 0,
    words: on_off,
};
const PCIE_SETTING: Setting = Setting {
    label: "PCIe link-state power management",
    subgroup: PCIE_SUBGROUP,
    setting: PCIE_LINK_STATE,
    value: 0,
    words: link_state,
};

fn percent(value: u32) -> String {
    format!("{value}%")
}

fn on_off(value: u32) -> String {
    match value {
        0 => "Off".to_string(),
        1 => "On".to_string(),
        other => other.to_string(),
    }
}

fn link_state(value: u32) -> String {
    match value {
        0 => "Off".to_string(),
        1 => "Moderate".to_string(),
        2 => "Maximum".to_string(),
        other => other.to_string(),
    }
}

/// The settings a variant's plan sets, in the order the Preview lists them and Apply writes them.
fn settings(variant: Variant) -> &'static [Setting] {
    match variant {
        Variant::Full => &[PARKING_SETTING, USB_SETTING, PCIE_SETTING],
        Variant::Light => &[USB_SETTING, PCIE_SETTING],
        Variant::NotOffered => &[],
    }
}

/// The plans a variant's copy may start from, best first, with the names to call them. Never another tool's plan.
fn bases(variant: Variant) -> &'static [(&'static str, &'static str)] {
    match variant {
        Variant::Full => &[
            (HIGH_PERFORMANCE_PLAN, "High performance"),
            (BALANCED_PLAN, "Balanced"),
        ],
        Variant::Light => &[(BALANCED_PLAN, "Balanced")],
        Variant::NotOffered => &[],
    }
}

/// What Kestrel writes for a setting: the value it wants, held to the range Windows accepts on this PC. Some machines
/// take a narrower range for core parking than 0 to 100. A range that makes no sense (a minimum above its maximum)
/// still gives a value, so a strange answer from Windows can't stop the plan.
fn planned(sys: &dyn System, setting: &Setting) -> u32 {
    match sys.plan_value_range(setting.subgroup, setting.setting) {
        Some((min, max)) => setting.value.max(min).min(max),
        None => setting.value,
    }
}

/// GUIDs are lower-case in `System`, but a journal's may not be.
fn same_guid(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn is_one_of(guid: &str, plans: &[&str]) -> bool {
    plans.iter().any(|plan| same_guid(guid, plan))
}

/// The name of a plan as Windows shows it, or its GUID when Windows gives none.
fn label(sys: &dyn System, guid: &str) -> String {
    sys.plan_name(guid)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| guid.to_string())
}

/// The GUID of the plan in use, or why that can't be said.
fn plan_in_use(sys: &dyn System) -> Result<String, String> {
    match sys.active_plan() {
        Read::Value(guid) => Ok(guid),
        Read::Absent => Err(NO_PLAN_IN_USE.to_string()),
        Read::Failed(why) => Err(format!("Couldn't read your power plan: {why}")),
    }
}

/// What Kestrel was doing, and what Windows said.
fn failed(doing: &str, why: &io::Error) -> String {
    format!("Couldn't {doing}: {why}")
}

// The row.

/// What a row says: its state, whether it is ticked, and why.
type Verdict = (RowState, bool, String);

fn couldnt_read() -> Verdict {
    (
        RowState::CouldntRead,
        false,
        "Couldn't read your power plan.".to_string(),
    )
}

/// The power plan's row. `open` is the change Kestrel has open for it, if any. Reading only.
///
/// A change Kestrel made decides first, so that it can always be undone, whatever has come since (a policy, or a chip
/// that has gone on the list of those that aren't offered the plan). Otherwise the order is the PC's: whether the plan
/// is offered, then a policy, then the plan in use. A policy or a plan that can't be read is `CouldntRead`, never
/// "not there".
pub fn row(sys: &dyn System, input: &PlanInput, open: Option<&OpenRecord>) -> Row {
    let (state, ticked, why) = verdict(sys, input, open);
    let tags = info(TweakId::PowerPlan);
    Row {
        id: tags.id,
        title: tags.title.to_string(),
        state,
        ticked,
        why,
        impact: tags.impact,
        timing: tags.timing,
    }
}

fn verdict(sys: &dyn System, input: &PlanInput, open: Option<&OpenRecord>) -> Verdict {
    if let Some(found) = open.and_then(|_| recorded(sys)) {
        return found;
    }
    if variant(input, sys.modern_standby()) == Variant::NotOffered {
        return (RowState::NotNeeded, false, AMD_GUIDANCE.to_string());
    }
    match targets::found(sys.read_policy_string(POLICY_KEY, POLICY_VALUE)) {
        Err(_) => return couldnt_read(),
        Ok(Some(_)) => {
            return (
                RowState::BlockedByPolicy,
                false,
                "An organisation policy picks the power plan.".to_string(),
            )
        }
        Ok(None) => {}
    }
    let Ok(active) = plan_in_use(sys) else {
        return couldnt_read();
    };
    if same_guid(&active, KESTREL_PLAN_GUID) {
        return (
            RowState::AlreadySet,
            false,
            "Kestrel's plan is already active.".to_string(),
        );
    }
    let name = label(sys, &active);
    if is_one_of(&active, &[BALANCED_PLAN, POWER_SAVER_PLAN]) {
        (
            RowState::Available,
            true,
            format!("{name} lets the CPU slow down between frames. The difference may be small or none: measure it on your PC."),
        )
    } else if is_one_of(&active, &[HIGH_PERFORMANCE_PLAN, ULTIMATE_PLAN]) {
        (
            RowState::Available,
            false,
            "You're already on a fast plan. Kestrel's also turns off USB and PCIe power saving."
                .to_string(),
        )
    } else {
        (
            RowState::Available,
            false,
            format!("You're on {name}. Measure before and after to see which is better."),
        )
    }
}

/// The row of a change Kestrel made: Applied while its plan is the one in use, PlanInactive while the plan is on the
/// PC and another is in use. None when the plan isn't on the PC at all (something deleted it, and the engine is about
/// to close the record), so the row is for the plan in use instead of for one there is nothing to undo.
fn recorded(sys: &dyn System) -> Option<Verdict> {
    let Ok(active) = plan_in_use(sys) else {
        return Some(couldnt_read());
    };
    if same_guid(&active, KESTREL_PLAN_GUID) {
        Some((
            RowState::Applied,
            false,
            "Kestrel's plan is active.".to_string(),
        ))
    } else if sys.plan_exists(KESTREL_PLAN_GUID) {
        Some((
            RowState::PlanInactive,
            false,
            format!("Another plan is active: {}.", label(sys, &active)),
        ))
    } else {
        None
    }
}

// What is there before.

/// The plan in use, as the journal's intent records it before anything is created: its GUID and the name Windows shows
/// for it (the GUID when Windows gives none). Undo goes back to this one. A plan in use that can't be read, or that
/// Windows won't name, is `Failed`: Apply goes no further without knowing where to come back to.
pub fn prior(sys: &dyn System) -> Read<Saved> {
    match plan_in_use(sys) {
        Ok(guid) => Read::Value(Saved::Plan {
            name: label(sys, &guid),
            guid,
        }),
        Err(why) => Read::Failed(why),
    }
}

// Making the plan.

/// Makes Kestrel's plan and switches to it, or does nothing and says why. It never leaves a half-made plan behind: if
/// anything fails after the copy was made, the copy is deleted again and the plan in use is the one it was, because
/// switching is the last step.
///
/// In order: a Kestrel plan left from an earlier try (a crash, say) is deleted; the plan is copied from the variant's
/// base, falling back from High performance to Balanced; it is named; each setting is written, held to the range this PC
/// accepts, and read back; and it is switched on. A dual-CCD X3D is refused. If Kestrel's plan is already the one in
/// use there is nothing to do.
pub fn create_and_activate(sys: &dyn System, input: &PlanInput) -> Result<(), String> {
    let kind = variant(input, sys.modern_standby());
    if kind == Variant::NotOffered {
        return Err(AMD_GUIDANCE.to_string());
    }
    if same_guid(&plan_in_use(sys)?, KESTREL_PLAN_GUID) {
        return Ok(());
    }
    // A leftover is only deleted once it is known not to be the plan in use, which Windows would refuse to delete.
    if sys.plan_exists(KESTREL_PLAN_GUID) {
        sys.delete_plan(KESTREL_PLAN_GUID)
            .map_err(|why| failed("replace the Kestrel plan that was left on this PC", &why))?;
    }
    copy_base(sys, kind)?;
    set_up_and_switch(sys, kind).map_err(|why| roll_back(sys, &why))
}

/// Copies the first of the variant's base plans that Windows will copy. Stock plans can be missing: some optimisers
/// delete them.
fn copy_base(sys: &dyn System, kind: Variant) -> Result<(), String> {
    let mut refused = Vec::new();
    for (guid, name) in bases(kind) {
        match sys.duplicate_plan(guid, KESTREL_PLAN_GUID) {
            Ok(()) => return Ok(()),
            Err(why) => refused.push(format!("{name} ({why})")),
        }
    }
    Err(format!(
        "Couldn't make Kestrel's power plan: Windows wouldn't copy {}.",
        refused.join(" or ")
    ))
}

/// Everything after the copy, ending with the switch.
fn set_up_and_switch(sys: &dyn System, kind: Variant) -> Result<(), String> {
    sys.describe_plan(KESTREL_PLAN_GUID, KESTREL_PLAN_NAME, PLAN_DESCRIPTION)
        .map_err(|why| failed("name Kestrel's power plan", &why))?;
    for setting in settings(kind) {
        set(sys, setting)?;
    }
    sys.activate_plan(KESTREL_PLAN_GUID)
        .map_err(|why| failed("switch to Kestrel's power plan", &why))
}

/// Writes one setting of Kestrel's plan and reads it back, so that a value Windows didn't keep is a failure now and
/// not a surprise later.
fn set(sys: &dyn System, setting: &Setting) -> Result<(), String> {
    let value = planned(sys, setting);
    sys.write_plan_value(KESTREL_PLAN_GUID, setting.subgroup, setting.setting, value)
        .map_err(|why| {
            failed(
                &format!("set {} in Kestrel's power plan", setting.label),
                &why,
            )
        })?;
    let held = match sys.read_plan_value(KESTREL_PLAN_GUID, setting.subgroup, setting.setting) {
        Read::Value(now) => Some(now),
        Read::Absent => None,
        Read::Failed(why) => {
            return Err(format!(
                "Couldn't read {} back from Kestrel's power plan: {why}",
                setting.label
            ))
        }
    };
    if held == Some(value) {
        return Ok(());
    }
    Err(format!(
        "Windows didn't keep {} in Kestrel's power plan: it holds {} instead of {}.",
        setting.label,
        held.map_or_else(|| "nothing".to_string(), setting.words),
        (setting.words)(value)
    ))
}

/// Deletes the half-made plan, and adds to `why` what became of it.
fn roll_back(sys: &dyn System, why: &str) -> String {
    let why = why.trim_end_matches('.');
    match sys.delete_plan(KESTREL_PLAN_GUID) {
        Ok(()) => format!("{why}. Kestrel took its plan out again, so nothing is left behind."),
        Err(gone) => format!(
            "{why}. Kestrel also couldn't take its plan out again ({gone}), so it is still on this PC."
        ),
    }
}

// The Preview.

/// What Apply would do, for the Preview: each setting beside the value in the plan in use now, and the value Kestrel's
/// plan would hold (the same one Apply writes, held to this PC's range). The note says what the Preview can't list:
/// the rest of the plan comes from the base plan. A value that can't be read is "Unknown". Reading only.
pub fn preview(sys: &dyn System, input: &PlanInput) -> PreviewItem {
    let kind = variant(input, sys.modern_standby());
    let tags = info(TweakId::PowerPlan);
    let active = plan_in_use(sys).ok();
    let changes = settings(kind)
        .iter()
        .map(|setting| ValueChange {
            label: setting.label.to_string(),
            now: active
                .as_deref()
                .and_then(|guid| {
                    match sys.read_plan_value(guid, setting.subgroup, setting.setting) {
                        Read::Value(now) => Some((setting.words)(now)),
                        Read::Absent | Read::Failed(_) => None,
                    }
                })
                .unwrap_or_else(|| "Unknown".to_string()),
            after: (setting.words)(planned(sys, setting)),
        })
        .collect();
    // The name of the base is the first one tried, even if that stock plan has gone missing from this PC.
    let note = match bases(kind).first() {
        None => AMD_GUIDANCE.to_string(),
        Some((_, base)) => {
            let current = active
                .as_deref()
                .map_or_else(|| "your current plan".to_string(), |guid| label(sys, guid));
            format!("Switching plans also replaces everything else in {current} with the settings of {base}. Undo returns to your plan as it was.")
        }
    };
    PreviewItem {
        id: tags.id,
        title: tags.title.to_string(),
        location: "Power plan".to_string(),
        changes,
        timing: tags.timing,
        note: Some(note),
    }
}

// Going back.

/// Undoes the plan: switches to `prior`, the plan that was in use before, and then deletes Kestrel's. If that plan is
/// gone (or won't switch on), Balanced is switched on instead and the message says so. It acts only while Kestrel's
/// plan is the one in use: if the person has picked another since, that choice is theirs, and `remove` is what finishes
/// the job. If nothing can be switched to, Kestrel's plan stays in place and in use.
///
/// `prior` is untrusted, as everything from the journal is: it must be a plan, its GUID is read in any case, and Kestrel's
/// own plan is never taken for the one to go back to.
pub fn undo(sys: &dyn System, prior: &Saved) -> Result<String, String> {
    let Saved::Plan { guid, .. } = prior else {
        return Err("The journal doesn't name a power plan to go back to.".to_string());
    };
    if !same_guid(&plan_in_use(sys)?, KESTREL_PLAN_GUID) {
        return Err(
            "Kestrel's plan isn't in use, so there is nothing to switch back from.".to_string(),
        );
    }
    let name = targets::describe(TweakId::PowerPlan, prior);
    let said = switch_back(sys, &guid.to_ascii_lowercase(), &name)?;
    // Only now: Windows won't delete the plan in use.
    if sys.plan_exists(KESTREL_PLAN_GUID) {
        sys.delete_plan(KESTREL_PLAN_GUID).map_err(|why| {
            format!("{said} Kestrel's plan couldn't be removed: {why}. You can remove it from the Tweaks tab.")
        })?;
    }
    Ok(said)
}

/// Puts the plan `guid` in use, or Balanced when that can't be done, and says which.
fn switch_back(sys: &dyn System, guid: &str, name: &str) -> Result<String, String> {
    // Kestrel's own plan is no plan to go back to. A journal that says so is no better than one naming a plan that is gone.
    let refused = if same_guid(guid, KESTREL_PLAN_GUID) || !sys.plan_exists(guid) {
        None
    } else {
        match sys.activate_plan(guid) {
            Ok(()) => return Ok(format!("Back to {name}.")),
            Err(why) => Some(why),
        }
    };
    match (sys.activate_plan(BALANCED_PLAN), refused) {
        (Ok(()), None) => {
            Ok("Your previous plan is gone, so Kestrel switched to Balanced.".to_string())
        }
        (Ok(()), Some(why)) => Ok(format!(
            "Couldn't switch back to {name}: {why}. Kestrel switched to Balanced instead."
        )),
        (Err(balanced), None) => Err(format!(
            "Your previous plan is gone, and Windows wouldn't switch to Balanced: {balanced}. Kestrel's plan is still in use."
        )),
        (Err(balanced), Some(why)) => Err(format!(
            "Couldn't switch back to {name}: {why}. Balanced didn't work either: {balanced}. Kestrel's plan is still in use."
        )),
    }
}

/// Deletes Kestrel's plan and leaves the plan in use as it is. It refuses while Kestrel's plan is the one in use (and
/// when it can't tell), because Windows won't delete that plan and Kestrel doesn't ask it to. A plan that isn't there is
/// already removed.
pub fn remove(sys: &dyn System) -> Result<(), String> {
    if !sys.plan_exists(KESTREL_PLAN_GUID) {
        return Ok(());
    }
    if same_guid(&plan_in_use(sys)?, KESTREL_PLAN_GUID) {
        return Err(
            "Kestrel's plan is in use, so it can't be removed. Switch to another plan first."
                .to_string(),
        );
    }
    sys.delete_plan(KESTREL_PLAN_GUID)
        .map_err(|why| failed("remove Kestrel's power plan", &why))
}

/// Switches back to Kestrel's plan when another plan was picked after it was made.
pub fn use_again(sys: &dyn System) -> Result<(), String> {
    if !sys.plan_exists(KESTREL_PLAN_GUID) {
        return Err(
            "Kestrel's power plan is gone, so there is nothing to switch back to.".to_string(),
        );
    }
    sys.activate_plan(KESTREL_PLAN_GUID)
        .map_err(|why| failed("switch to Kestrel's power plan", &why))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Impact, Timing};
    use crate::fake::{FakeSystem, ACTIVE_PLAN, BALANCED, HIGH_PERFORMANCE, POWER_SAVER};
    use RowState::{
        AlreadySet, Applied, Available, BlockedByPolicy, CouldntRead, NotNeeded, PlanInactive,
    };

    const KESTREL: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";
    // The owner's plan, and somebody else's that nothing of Kestrel's may touch. NEVER was never on the PC.
    const EXM: &str = "0e1d2c3b-4a59-4687-b6a5-948372615049";
    const EXM_NAME: &str = "EXM Free Power Plan V8";
    const OTHER: &str = "f0e1d2c3-b4a5-4968-8776-655443322110";
    const NEVER: &str = "99999999-9999-4999-8999-999999999999";
    const ULTIMATE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

    // The settings Kestrel's plan sets, as the spec's table gives them.
    const CPU_GROUP: &str = "54533251-82be-4824-96c1-47b60b740d00";
    const PARKING: &str = "0cc5b647-c1df-4637-891a-dec35c318583";
    const USB_GROUP: &str = "2a737441-1930-4402-8d77-b2bebba308a3";
    const USB: &str = "48e6b7a6-50f5-4782-a5d4-53bb8f07e226";
    const PCIE_GROUP: &str = "501a4d13-42af-4429-9fd1-a8218c268e20";
    const PCIE: &str = "ee12f906-d277-404b-b6da-e5fa1a576df5";

    const POLICY_KEY_PATH: &str = r"SOFTWARE\Policies\Microsoft\Power\PowerSettings";
    const POLICY_VALUE_NAME: &str = "ActivePowerScheme";

    // The words of the rows and messages, as the brief gives them.
    const AMD: &str = "AMD's guidance for this chip is Balanced with Game Mode on.";
    const POLICY: &str = "An organisation policy picks the power plan.";
    const IS_ACTIVE: &str = "Kestrel's plan is active.";
    const ALREADY_ACTIVE: &str = "Kestrel's plan is already active.";
    const FAST_PLAN: &str =
        "You're already on a fast plan. Kestrel's also turns off USB and PCIe power saving.";
    const UNREADABLE: &str = "Couldn't read your power plan.";
    const GONE_SO_BALANCED: &str = "Your previous plan is gone, so Kestrel switched to Balanced.";

    /// A desktop with one kind of core: the PC that gets the full plan.
    fn desktop() -> PlanInput {
        PlanInput {
            build: 26200,
            form_factor: FormFactor::Desktop,
            hybrid_cpu: false,
            cpu_name: "Intel(R) Core(TM) i9-11900K".to_string(),
            gpu_choice: false,
            valorant_exe: None,
            fullscreen_mode: None,
        }
    }

    fn laptop() -> PlanInput {
        PlanInput {
            form_factor: FormFactor::Laptop,
            ..desktop()
        }
    }

    fn x3d() -> PlanInput {
        PlanInput {
            cpu_name: "AMD Ryzen 9 9950X3D".to_string(),
            ..desktop()
        }
    }

    /// The owner's PC: EXM's plan is the one in use.
    fn exm_pc() -> FakeSystem {
        let pc = FakeSystem::new();
        pc.add_plan(EXM, EXM_NAME);
        pc.set_active(EXM);
        pc
    }

    fn plan(guid: &str, name: &str) -> Saved {
        Saved::Plan {
            guid: guid.to_string(),
            name: name.to_string(),
        }
    }

    fn exm() -> Saved {
        plan(EXM, EXM_NAME)
    }

    /// The change Kestrel has open once it has made its plan.
    fn record() -> OpenRecord {
        OpenRecord {
            tweak: TweakId::PowerPlan,
            sid: None,
            value_name: None,
            prior: exm(),
            planned: plan(KESTREL, "Kestrel"),
            at: "2026-11-02T18:12:00+00:00".to_string(),
            settled: true,
        }
    }

    /// What a row says, which is what the table gives: the state, whether it is ticked, and why.
    type Says = (RowState, bool, String);

    fn says(state: RowState, ticked: bool, why: &str) -> Says {
        (state, ticked, why.to_string())
    }

    fn row_with(pc: &FakeSystem, input: &PlanInput, open: Option<&OpenRecord>) -> Says {
        let found = row(pc, input, open);
        (found.state, found.ticked, found.why)
    }

    /// The row for the owner's kind of PC, with no record open.
    fn said(pc: &FakeSystem) -> Says {
        row_with(pc, &desktop(), None)
    }

    fn change(label: &str, now: &str, after: &str) -> ValueChange {
        ValueChange {
            label: label.to_string(),
            now: now.to_string(),
            after: after.to_string(),
        }
    }

    fn is_active(pc: &FakeSystem, guid: &str) -> bool {
        pc.active_plan() == Read::Value(guid.to_string())
    }

    /// The plans the PC was told to delete, in order.
    fn deleted(pc: &FakeSystem) -> Vec<String> {
        pc.writes()
            .into_iter()
            .filter_map(|line| line.strip_prefix("delete-plan ").map(str::to_string))
            .collect()
    }

    /// Makes Kestrel's plan on `pc` as Apply does, and returns the plan that was in use before it.
    fn applied(pc: &FakeSystem) -> Saved {
        let Read::Value(before) = prior(pc) else {
            panic!("the plan in use should read");
        };
        create_and_activate(pc, &desktop()).unwrap();
        before
    }

    // The ids and values.

    #[test]
    fn the_guids_and_words_are_the_specs() {
        assert_eq!(KESTREL_PLAN_GUID, KESTREL);
        assert_eq!(KESTREL_PLAN_NAME, "Kestrel");
        assert_eq!(
            PLAN_DESCRIPTION,
            "Created by Kestrel. Undo it from Kestrel's Tweaks tab."
        );
        assert_eq!(PROCESSOR_SUBGROUP, CPU_GROUP);
        assert_eq!(CORE_PARKING_MIN_CORES, PARKING);
        assert_eq!(USB_SUBGROUP, USB_GROUP);
        assert_eq!(USB_SELECTIVE_SUSPEND, USB);
        assert_eq!(PCIE_SUBGROUP, PCIE_GROUP);
        assert_eq!(PCIE_LINK_STATE, PCIE);
        assert_eq!(BALANCED_PLAN, BALANCED);
        assert_eq!(HIGH_PERFORMANCE_PLAN, HIGH_PERFORMANCE);
        assert_eq!(POWER_SAVER_PLAN, POWER_SAVER);
        assert_eq!(ULTIMATE_PLAN, ULTIMATE);
        assert_eq!(POLICY_KEY, POLICY_KEY_PATH);
        assert_eq!(POLICY_VALUE, POLICY_VALUE_NAME);
        assert_eq!(AMD_GUIDANCE, AMD);
    }

    // Which variant a PC gets.

    #[test]
    fn variant_by_pc() {
        // A desktop with one kind of core gets the full plan, and Windows not saying about Modern Standby counts as no.
        assert_eq!(variant(&desktop(), Some(false)), Variant::Full);
        assert_eq!(variant(&desktop(), None), Variant::Full);

        // Anything else gets the light one: a laptop, a PC that won't say what it is, two kinds of core, Modern Standby.
        assert_eq!(variant(&laptop(), Some(false)), Variant::Light);
        let unknown = PlanInput {
            form_factor: FormFactor::Unknown,
            ..desktop()
        };
        assert_eq!(variant(&unknown, Some(false)), Variant::Light);
        let hybrid = PlanInput {
            hybrid_cpu: true,
            ..desktop()
        };
        assert_eq!(variant(&hybrid, Some(false)), Variant::Light);
        assert_eq!(variant(&desktop(), Some(true)), Variant::Light);

        // A dual-CCD X3D isn't offered the plan, whatever else is true of it.
        for name in [
            "AMD Ryzen 9 9950X3D",
            "AMD Ryzen 9 7950X3D 16-Core Processor",
            "AMD Ryzen 9 7900X3D",
            "AMD Ryzen 9 9900X3D",
        ] {
            let chip = PlanInput {
                cpu_name: name.to_string(),
                ..desktop()
            };
            assert_eq!(variant(&chip, Some(false)), Variant::NotOffered, "{name}");
            let odd = PlanInput {
                form_factor: FormFactor::Laptop,
                hybrid_cpu: true,
                ..chip
            };
            assert_eq!(variant(&odd, Some(true)), Variant::NotOffered, "{name}");
        }

        // One with a single CCD is an ordinary desktop.
        let single = PlanInput {
            cpu_name: "AMD Ryzen 7 9800X3D".to_string(),
            ..desktop()
        };
        assert_eq!(variant(&single, Some(false)), Variant::Full);
    }

    // The row, one test for each line of the table.

    #[test]
    fn a_dual_ccd_x3d_is_not_needed() {
        let pc = FakeSystem::new();
        assert_eq!(row_with(&pc, &x3d(), None), says(NotNeeded, false, AMD));
        pc.set_active(HIGH_PERFORMANCE);
        assert_eq!(row_with(&pc, &x3d(), None), says(NotNeeded, false, AMD));
        // It doesn't matter whether the plan in use can be read.
        pc.fail_read(ACTIVE_PLAN);
        assert_eq!(row_with(&pc, &x3d(), None), says(NotNeeded, false, AMD));
    }

    #[test]
    fn a_policy_that_sets_the_plan_is_blocked_by_policy() {
        for plan_in_use in [BALANCED, HIGH_PERFORMANCE, EXM] {
            let pc = exm_pc();
            pc.set_active(plan_in_use);
            pc.set_policy_string(POLICY_KEY_PATH, POLICY_VALUE_NAME, EXM);
            assert_eq!(
                said(&pc),
                says(BlockedByPolicy, false, POLICY),
                "{plan_in_use}"
            );
        }
        // No policy value is no policy.
        assert_eq!(said(&FakeSystem::new()).0, Available);
    }

    #[test]
    fn kestrels_plan_in_use_with_a_record_is_applied() {
        let pc = exm_pc();
        applied(&pc);
        assert_eq!(
            row_with(&pc, &desktop(), Some(&record())),
            says(Applied, false, IS_ACTIVE)
        );
    }

    #[test]
    fn kestrels_plan_on_disk_but_not_in_use_with_a_record_is_plan_inactive() {
        let pc = exm_pc();
        applied(&pc);
        pc.set_active(EXM);
        assert_eq!(
            row_with(&pc, &desktop(), Some(&record())),
            says(
                PlanInactive,
                false,
                "Another plan is active: EXM Free Power Plan V8."
            )
        );
        pc.set_active(BALANCED);
        assert_eq!(
            row_with(&pc, &desktop(), Some(&record())),
            says(PlanInactive, false, "Another plan is active: Balanced.")
        );
    }

    #[test]
    fn kestrels_plan_in_use_without_a_record_is_already_set() {
        let pc = exm_pc();
        applied(&pc);
        assert_eq!(said(&pc), says(AlreadySet, false, ALREADY_ACTIVE));
    }

    #[test]
    fn balanced_and_power_saver_are_available_and_ticked() {
        let pc = FakeSystem::new();
        assert_eq!(
            said(&pc),
            says(
                Available,
                true,
                "Balanced lets the CPU slow down between frames. The difference may be small or none: measure it on your PC."
            )
        );
        pc.set_active(POWER_SAVER);
        assert_eq!(
            said(&pc),
            says(
                Available,
                true,
                "Power saver lets the CPU slow down between frames. The difference may be small or none: measure it on your PC."
            )
        );
        // It goes by the plan's GUID, and says the name Windows shows.
        pc.add_plan(BALANCED, "Equilibrado");
        pc.set_active(BALANCED);
        assert_eq!(
            said(&pc),
            says(
                Available,
                true,
                "Equilibrado lets the CPU slow down between frames. The difference may be small or none: measure it on your PC."
            )
        );
    }

    #[test]
    fn high_performance_and_ultimate_performance_are_available_and_not_ticked() {
        let pc = FakeSystem::new();
        pc.set_active(HIGH_PERFORMANCE);
        assert_eq!(said(&pc), says(Available, false, FAST_PLAN));
        pc.add_plan(ULTIMATE, "Ultimate Performance");
        pc.set_active(ULTIMATE);
        assert_eq!(said(&pc), says(Available, false, FAST_PLAN));
    }

    #[test]
    fn plans_are_known_by_guid_whatever_its_case() {
        // `System` gives lower-case, but the same plan under another spelling is the same plan.
        for (plan_in_use, expected) in [
            (BALANCED, Available),
            (HIGH_PERFORMANCE, Available),
            (KESTREL, AlreadySet),
        ] {
            let pc = FakeSystem::new();
            pc.add_plan(&plan_in_use.to_uppercase(), "Some name");
            pc.set_active(&plan_in_use.to_uppercase());
            let (state, ticked, _) = said(&pc);
            assert_eq!(state, expected, "{plan_in_use}");
            assert_eq!(ticked, plan_in_use == BALANCED, "{plan_in_use}");
        }
    }

    #[test]
    fn any_other_plan_is_available_and_not_ticked() {
        let pc = exm_pc();
        assert_eq!(
            said(&pc),
            says(
                Available,
                false,
                "You're on EXM Free Power Plan V8. Measure before and after to see which is better."
            )
        );
        // A plan that Windows gives no name is shown by its GUID.
        pc.add_plan(OTHER, "");
        pc.set_active(OTHER);
        assert_eq!(
            said(&pc),
            says(
                Available,
                false,
                &format!("You're on {OTHER}. Measure before and after to see which is better.")
            )
        );
        // Kestrel's plan left behind, with another in use and nothing recorded, is only another plan to be on.
        let pc = exm_pc();
        pc.add_plan(KESTREL, "Kestrel");
        assert_eq!(
            said(&pc),
            says(
                Available,
                false,
                "You're on EXM Free Power Plan V8. Measure before and after to see which is better."
            )
        );
    }

    #[test]
    fn an_unreadable_plan_is_couldnt_read() {
        let pc = exm_pc();
        pc.fail_read(ACTIVE_PLAN);
        assert_eq!(said(&pc), says(CouldntRead, false, UNREADABLE));
        // A record can't make it readable, and nothing else is said in its place: not that the plan isn't offered, and
        // not that a policy decides.
        for input in [desktop(), x3d()] {
            assert_eq!(
                row_with(&pc, &input, Some(&record())),
                says(CouldntRead, false, UNREADABLE)
            );
        }
        pc.set_policy_string(POLICY_KEY_PATH, POLICY_VALUE_NAME, EXM);
        assert_eq!(
            row_with(&pc, &desktop(), Some(&record())),
            says(CouldntRead, false, UNREADABLE)
        );

        // A policy that can't be read is not "no policy": a value of the wrong type and a refused read both fail.
        let pc = exm_pc();
        pc.set_policy_dword(POLICY_KEY_PATH, POLICY_VALUE_NAME, 1);
        assert_eq!(said(&pc), says(CouldntRead, false, UNREADABLE));
        let pc = exm_pc();
        pc.fail_read(POLICY_VALUE_NAME);
        assert_eq!(said(&pc), says(CouldntRead, false, UNREADABLE));
    }

    #[test]
    fn the_row_is_the_power_plans_with_its_title_and_tags() {
        let found = row(&exm_pc(), &desktop(), None);
        assert_eq!(found.id, TweakId::PowerPlan);
        assert_eq!(found.title, "Kestrel power plan");
        assert_eq!(found.impact, Impact::Low);
        assert_eq!(found.timing, Timing::Now);
    }

    #[test]
    fn only_an_available_row_is_ticked() {
        for plan_in_use in [BALANCED, POWER_SAVER, HIGH_PERFORMANCE, EXM, KESTREL] {
            let pc = exm_pc();
            pc.add_plan(KESTREL, "Kestrel");
            pc.set_active(plan_in_use);
            for open in [None, Some(record())] {
                for input in [desktop(), laptop(), x3d()] {
                    let found = row(&pc, &input, open.as_ref());
                    assert!(
                        !found.ticked || found.state == Available,
                        "{plan_in_use} {open:?} {found:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_open_record_decides_before_the_offer_and_before_a_policy() {
        // The plan was applied; then a policy came, or the chip went on the list. Undo has to stay possible.
        let pc = exm_pc();
        applied(&pc);
        pc.set_policy_string(POLICY_KEY_PATH, POLICY_VALUE_NAME, EXM);
        assert_eq!(
            row_with(&pc, &desktop(), Some(&record())),
            says(Applied, false, IS_ACTIVE)
        );
        assert_eq!(
            row_with(&pc, &x3d(), Some(&record())),
            says(Applied, false, IS_ACTIVE)
        );
        // Without the record, they decide.
        assert_eq!(said(&pc), says(BlockedByPolicy, false, POLICY));
        assert_eq!(row_with(&pc, &x3d(), None), says(NotNeeded, false, AMD));
    }

    #[test]
    fn a_record_for_a_plan_that_is_gone_is_left_out_of_it() {
        // The engine closes such a record. If one gets here anyway, the row is for the plan in use, not for a plan
        // that isn't there to be undone.
        let pc = exm_pc();
        assert_eq!(row_with(&pc, &desktop(), Some(&record())), said(&pc));
        assert_eq!(said(&pc).0, Available);
    }

    #[test]
    fn making_a_row_or_a_preview_writes_nothing() {
        let pc = exm_pc();
        pc.add_plan(KESTREL, "Kestrel");
        let before = pc.dump();
        for plan_in_use in [BALANCED, EXM, KESTREL] {
            pc.set_active(plan_in_use);
            let _ = row(&pc, &desktop(), None);
            let _ = row(&pc, &desktop(), Some(&record()));
            let _ = row(&pc, &x3d(), None);
            let _ = preview(&pc, &desktop());
            let _ = preview(&pc, &laptop());
            let _ = preview(&pc, &x3d());
            let _ = prior(&pc);
        }
        pc.set_active(EXM);
        assert_eq!(pc.dump(), before);
        assert!(pc.writes().is_empty());
    }

    // The plan in use now.

    #[test]
    fn prior_is_the_plan_in_use_with_the_name_windows_gives_it() {
        let pc = exm_pc();
        assert_eq!(prior(&pc), Read::Value(exm()));
        pc.set_active(BALANCED);
        assert_eq!(prior(&pc), Read::Value(plan(BALANCED, "Balanced")));
    }

    #[test]
    fn prior_names_a_plan_by_its_guid_when_windows_gives_it_no_name() {
        let pc = FakeSystem::new();
        pc.add_plan(OTHER, "");
        pc.set_active(OTHER);
        assert_eq!(prior(&pc), Read::Value(plan(OTHER, OTHER)));
        pc.add_plan(OTHER, "  ");
        assert_eq!(prior(&pc), Read::Value(plan(OTHER, OTHER)));
    }

    #[test]
    fn prior_fails_when_the_plan_in_use_cant_be_read() {
        let pc = exm_pc();
        pc.fail_read(ACTIVE_PLAN);
        assert!(matches!(prior(&pc), Read::Failed(_)));
        assert!(pc.writes().is_empty());
    }

    // Making the plan and switching to it.

    #[test]
    fn full_writes_all_three_on_a_high_performance_copy() {
        let pc = exm_pc();
        assert_eq!(prior(&pc), Read::Value(exm()));

        create_and_activate(&pc, &desktop()).unwrap();

        // In this order: copy, name, the three values, and only then switch.
        assert_eq!(
            pc.writes(),
            [
                format!("duplicate-plan {HIGH_PERFORMANCE} -> {KESTREL}"),
                format!("describe-plan {KESTREL} Kestrel"),
                format!("plan-value {KESTREL} {CPU_GROUP}/{PARKING} = 100"),
                format!("plan-value {KESTREL} {USB_GROUP}/{USB} = 0"),
                format!("plan-value {KESTREL} {PCIE_GROUP}/{PCIE} = 0"),
                format!("activate-plan {KESTREL}"),
            ]
        );
        assert_eq!(pc.plan_name(KESTREL).as_deref(), Some("Kestrel"));
        assert_eq!(
            pc.plan_description(KESTREL).as_deref(),
            Some("Created by Kestrel. Undo it from Kestrel's Tweaks tab.")
        );
        assert_eq!(pc.plan_value(KESTREL, CPU_GROUP, PARKING), Some(100));
        assert_eq!(pc.plan_value(KESTREL, USB_GROUP, USB), Some(0));
        assert_eq!(pc.plan_value(KESTREL, PCIE_GROUP, PCIE), Some(0));
        assert!(is_active(&pc, KESTREL));
        // The person's own plan is still there, as it was.
        assert!(pc.plan_exists(EXM));
        assert_eq!(pc.plan_name(EXM).as_deref(), Some(EXM_NAME));
    }

    #[test]
    fn light_leaves_parking_alone_on_a_balanced_copy() {
        let hybrid = PlanInput {
            hybrid_cpu: true,
            ..desktop()
        };
        let unknown = PlanInput {
            form_factor: FormFactor::Unknown,
            ..desktop()
        };
        for (input, modern_standby) in [
            (laptop(), Some(false)),
            (hybrid, Some(false)),
            (unknown, None),
            (desktop(), Some(true)),
        ] {
            let pc = exm_pc();
            pc.set_modern_standby(modern_standby);
            // What the base plan holds for parking is the plan's own to keep.
            pc.set_plan_value(BALANCED, CPU_GROUP, PARKING, 10);

            create_and_activate(&pc, &input).unwrap();

            assert_eq!(
                pc.writes(),
                [
                    format!("duplicate-plan {BALANCED} -> {KESTREL}"),
                    format!("describe-plan {KESTREL} Kestrel"),
                    format!("plan-value {KESTREL} {USB_GROUP}/{USB} = 0"),
                    format!("plan-value {KESTREL} {PCIE_GROUP}/{PCIE} = 0"),
                    format!("activate-plan {KESTREL}"),
                ],
                "{input:?} {modern_standby:?}"
            );
            assert_eq!(pc.plan_value(KESTREL, CPU_GROUP, PARKING), Some(10));
            assert!(is_active(&pc, KESTREL));
        }
    }

    #[test]
    fn values_are_clamped_to_the_machines_range() {
        // This machine takes parking only up to 50, and USB selective suspend only on.
        let pc = exm_pc();
        pc.set_plan_range(CPU_GROUP, PARKING, 0, 50);
        pc.set_plan_range(USB_GROUP, USB, 1, 1);
        // A range that holds the value changes nothing.
        pc.set_plan_range(PCIE_GROUP, PCIE, 0, 2);

        create_and_activate(&pc, &desktop()).unwrap();

        assert_eq!(pc.plan_value(KESTREL, CPU_GROUP, PARKING), Some(50));
        assert_eq!(pc.plan_value(KESTREL, USB_GROUP, USB), Some(1));
        assert_eq!(pc.plan_value(KESTREL, PCIE_GROUP, PCIE), Some(0));
    }

    #[test]
    fn a_range_that_makes_no_sense_does_not_stop_the_plan() {
        // A minimum above the maximum would panic in `clamp`.
        let pc = exm_pc();
        pc.set_plan_range(CPU_GROUP, PARKING, 80, 20);
        create_and_activate(&pc, &desktop()).unwrap();
        assert!(is_active(&pc, KESTREL));
    }

    #[test]
    fn a_missing_high_performance_falls_back_to_balanced() {
        // Told to fail: Windows won't copy it.
        let pc = exm_pc();
        pc.fail_duplicate(HIGH_PERFORMANCE);
        create_and_activate(&pc, &desktop()).unwrap();
        assert_eq!(
            pc.writes()[0],
            format!("duplicate-plan {BALANCED} -> {KESTREL}")
        );
        assert!(is_active(&pc, KESTREL));
        // All three are still written: the full plan wants parking off whatever it started from.
        assert_eq!(pc.plan_value(KESTREL, CPU_GROUP, PARKING), Some(100));
        assert_eq!(pc.plan_value(KESTREL, USB_GROUP, USB), Some(0));
        assert_eq!(pc.plan_value(KESTREL, PCIE_GROUP, PCIE), Some(0));

        // Deleted by another tool: not there to copy.
        let pc = exm_pc();
        pc.delete_plan(HIGH_PERFORMANCE).unwrap();
        create_and_activate(&pc, &desktop()).unwrap();
        let copies: Vec<_> = pc
            .writes()
            .into_iter()
            .filter(|line| line.starts_with("duplicate-plan"))
            .collect();
        assert_eq!(copies, [format!("duplicate-plan {BALANCED} -> {KESTREL}")]);
        assert!(is_active(&pc, KESTREL));
    }

    #[test]
    fn no_stock_plan_fails_and_leaves_nothing() {
        let pc = exm_pc();
        pc.fail_duplicate(HIGH_PERFORMANCE);
        pc.fail_duplicate(BALANCED);
        let before = (pc.plans(), pc.active_plan());

        let err = create_and_activate(&pc, &desktop()).unwrap_err();

        assert!(
            err.contains("High performance") && err.contains("Balanced"),
            "{err}"
        );
        assert_eq!((pc.plans(), pc.active_plan()), before);
        assert!(pc.writes().is_empty());

        // The light plan only ever starts from Balanced: a High performance that would copy is no way out.
        let pc = exm_pc();
        pc.fail_duplicate(BALANCED);
        let before = (pc.plans(), pc.active_plan());
        let err = create_and_activate(&pc, &laptop()).unwrap_err();
        assert!(
            err.contains("Balanced") && !err.contains("High performance"),
            "{err}"
        );
        assert_eq!((pc.plans(), pc.active_plan()), before);
        assert!(pc.writes().is_empty());
    }

    #[test]
    fn a_failed_activation_deletes_the_plan_again() {
        let pc = exm_pc();
        let before = pc.dump();
        pc.fail_activate(KESTREL);

        let err = create_and_activate(&pc, &desktop()).unwrap_err();

        // The reason Windows gave is in it, and the PC is as it was.
        assert!(err.contains("activating power plan"), "{err}");
        assert!(!pc.plan_exists(KESTREL));
        assert!(is_active(&pc, EXM));
        assert_eq!(pc.dump(), before);
        assert_eq!(deleted(&pc), [KESTREL]);
        assert_eq!(pc.writes().last(), Some(&format!("delete-plan {KESTREL}")));
    }

    #[test]
    fn a_failure_at_any_step_after_the_copy_deletes_the_plan_again() {
        type Spoil = fn(&FakeSystem);
        let spoils: [(&str, Spoil); 9] = [
            ("the plan won't take its name", |pc| pc.fail_write(KESTREL)),
            ("the first value won't write", |pc| pc.fail_write(PARKING)),
            ("a later value won't write", |pc| pc.fail_write(PCIE)),
            ("the first value won't read back", |pc| {
                pc.fail_read(PARKING)
            }),
            ("a later value won't read back", |pc| pc.fail_read(PCIE)),
            ("the first value isn't kept", |pc| {
                pc.lie_on_readback(PARKING)
            }),
            ("a later value isn't kept", |pc| pc.lie_on_readback(PCIE)),
            ("the switch fails", |pc| pc.fail_activate(KESTREL)),
            ("the switch fails and so does the first write", |pc| {
                pc.fail_activate(KESTREL);
                pc.fail_write(PARKING);
            }),
        ];
        for (what, spoil) in spoils {
            let pc = exm_pc();
            let before = pc.dump();
            spoil(&pc);

            let err = create_and_activate(&pc, &desktop()).unwrap_err();

            assert!(!err.trim().is_empty(), "{what}");
            assert!(!pc.plan_exists(KESTREL), "{what}");
            assert!(is_active(&pc, EXM), "{what}");
            assert_eq!(pc.dump(), before, "{what}");
            assert_eq!(deleted(&pc), [KESTREL], "{what}");
        }
    }

    #[test]
    fn a_failure_says_what_it_was_doing_and_what_windows_said() {
        let pc = exm_pc();
        pc.fail_write(USB);
        let err = create_and_activate(&pc, &desktop()).unwrap_err();
        assert!(err.contains("USB selective suspend"), "{err}");
        assert!(err.contains("writing"), "Windows' own reason: {err}");

        let pc = exm_pc();
        pc.fail_read(PARKING);
        let err = create_and_activate(&pc, &desktop()).unwrap_err();
        assert!(err.contains("Core parking minimum cores"), "{err}");
        assert!(err.contains("reading"), "Windows' own reason: {err}");

        let pc = exm_pc();
        pc.fail_write(KESTREL);
        let err = create_and_activate(&pc, &desktop()).unwrap_err();
        assert!(
            err.starts_with("Couldn't name Kestrel's power plan"),
            "{err}"
        );
        assert!(err.contains("writing"), "Windows' own reason: {err}");
    }

    #[test]
    fn a_value_windows_did_not_keep_is_a_failure_that_says_so() {
        // The value reads back as something else than it was given.
        let pc = exm_pc();
        pc.lie_on_readback(PARKING);
        let err = create_and_activate(&pc, &desktop()).unwrap_err();
        assert!(
            err.starts_with(
                "Windows didn't keep Core parking minimum cores in Kestrel's power plan"
            ),
            "{err}"
        );
        assert!(err.contains("instead of 100%"), "{err}");
        // The sentence ended with a full stop already, and the account of the rollback follows it.
        assert!(!err.contains(".."), "{err}");
        assert!(err.ends_with("so nothing is left behind."), "{err}");
        assert!(!pc.plan_exists(KESTREL));
        assert!(is_active(&pc, EXM));
    }

    #[test]
    fn a_rollback_that_cant_delete_the_plan_says_so() {
        let pc = exm_pc();
        pc.fail_activate(KESTREL);
        pc.fail_delete(KESTREL);

        let err = create_and_activate(&pc, &desktop()).unwrap_err();

        assert!(err.contains("activating power plan"), "{err}");
        assert!(err.contains("deleting power plan"), "{err}");
        assert!(err.contains("still on this PC"), "{err}");
        // The person's plan is still in use, and Kestrel's is left for Remove it.
        assert!(is_active(&pc, EXM));
        assert!(pc.plan_exists(KESTREL));
    }

    #[test]
    fn a_leftover_inactive_plan_is_replaced() {
        let pc = exm_pc();
        // Kestrel's plan from a crash, with stale values in it and no record.
        pc.add_plan(KESTREL, "Kestrel");
        pc.set_plan_value(KESTREL, CPU_GROUP, PARKING, 7);
        pc.set_plan_value(KESTREL, USB_GROUP, "something-else", 9);

        create_and_activate(&pc, &desktop()).unwrap();

        let writes = pc.writes();
        assert_eq!(writes[0], format!("delete-plan {KESTREL}"));
        assert_eq!(
            writes[1],
            format!("duplicate-plan {HIGH_PERFORMANCE} -> {KESTREL}")
        );
        assert!(is_active(&pc, KESTREL));
        assert_eq!(pc.plan_value(KESTREL, CPU_GROUP, PARKING), Some(100));
        assert_eq!(pc.plan_value(KESTREL, USB_GROUP, "something-else"), None);
        let ours = pc.plans().into_iter().filter(|(guid, _)| guid == KESTREL);
        assert_eq!(ours.count(), 1);
        assert_eq!(deleted(&pc), [KESTREL]);
    }

    #[test]
    fn a_leftover_that_cant_be_deleted_stops_before_anything_is_made() {
        let pc = exm_pc();
        pc.add_plan(KESTREL, "Kestrel");
        pc.fail_delete(KESTREL);

        let err = create_and_activate(&pc, &desktop()).unwrap_err();

        assert!(err.contains("deleting power plan"), "{err}");
        assert!(pc.writes().is_empty());
        assert!(is_active(&pc, EXM));
        assert!(pc.plan_exists(KESTREL));
    }

    #[test]
    fn nothing_is_deleted_when_the_plan_in_use_cant_be_read() {
        // Kestrel can't tell whether the leftover is the plan in use, and doesn't leave that to Windows to refuse.
        let pc = exm_pc();
        pc.add_plan(KESTREL, "Kestrel");
        pc.fail_read(ACTIVE_PLAN);

        let err = create_and_activate(&pc, &desktop()).unwrap_err();

        assert!(err.starts_with("Couldn't read your power plan"), "{err}");
        assert!(pc.writes().is_empty());
        assert!(pc.plan_exists(KESTREL));
    }

    #[test]
    fn kestrels_plan_already_in_use_is_left_alone() {
        let pc = exm_pc();
        applied(&pc);
        let before = pc.dump();
        let writes = pc.writes();

        create_and_activate(&pc, &desktop()).unwrap();

        assert_eq!(pc.dump(), before);
        assert_eq!(pc.writes(), writes);
    }

    #[test]
    fn a_dual_ccd_x3d_is_never_given_the_plan() {
        let pc = FakeSystem::new();
        let before = pc.dump();

        let err = create_and_activate(&pc, &x3d()).unwrap_err();

        assert_eq!(err, AMD);
        assert_eq!(pc.dump(), before);
        assert!(pc.writes().is_empty());
    }

    // Going back.

    #[test]
    fn undo_returns_to_the_old_plan_and_deletes_kestrels() {
        let pc = exm_pc();
        let start = pc.dump();
        let before = applied(&pc);
        assert_eq!(before, exm());

        let message = undo(&pc, &before).unwrap();

        assert_eq!(message, "Back to EXM Free Power Plan V8.");
        assert!(is_active(&pc, EXM));
        assert!(!pc.plan_exists(KESTREL));
        assert_eq!(pc.dump(), start);
        // The old plan was switched on before Kestrel's was deleted: Windows won't delete the plan in use.
        let writes = pc.writes();
        assert_eq!(
            writes[writes.len() - 2..],
            [
                format!("activate-plan {EXM}"),
                format!("delete-plan {KESTREL}")
            ]
        );
    }

    #[test]
    fn undo_falls_back_to_balanced_when_the_old_plan_is_gone() {
        let pc = exm_pc();
        let before = applied(&pc);
        // Another tool deleted the owner's plan in the meantime.
        pc.delete_plan(EXM).unwrap();

        let message = undo(&pc, &before).unwrap();

        assert_eq!(message, GONE_SO_BALANCED);
        assert!(is_active(&pc, BALANCED));
        assert!(!pc.plan_exists(KESTREL));
    }

    #[test]
    fn undo_falls_back_to_balanced_when_the_old_plan_wont_switch_on() {
        let pc = exm_pc();
        let before = applied(&pc);
        pc.fail_activate(EXM);

        let message = undo(&pc, &before).unwrap();

        assert!(
            message.starts_with("Couldn't switch back to EXM Free Power Plan V8"),
            "{message}"
        );
        assert!(message.contains("activating power plan"), "{message}");
        assert!(message.contains("Balanced"), "{message}");
        assert!(is_active(&pc, BALANCED));
        assert!(!pc.plan_exists(KESTREL));
        assert!(pc.plan_exists(EXM));
    }

    #[test]
    fn undo_that_cant_switch_anywhere_leaves_kestrels_plan_in_place_and_in_use() {
        // The old plan is gone and Balanced won't switch on.
        let pc = exm_pc();
        let before = applied(&pc);
        pc.delete_plan(EXM).unwrap();
        pc.fail_activate(BALANCED);
        let err = undo(&pc, &before).unwrap_err();
        assert!(err.contains("gone"), "{err}");
        assert!(err.contains("Balanced"), "{err}");
        assert!(err.contains("activating power plan"), "{err}");
        assert!(is_active(&pc, KESTREL));
        assert!(pc.plan_exists(KESTREL));

        // Neither will switch on.
        let pc = exm_pc();
        let before = applied(&pc);
        pc.fail_activate(EXM);
        pc.fail_activate(BALANCED);
        let err = undo(&pc, &before).unwrap_err();
        assert!(
            err.contains("EXM Free Power Plan V8") && err.contains("Balanced"),
            "{err}"
        );
        assert!(is_active(&pc, KESTREL));
        assert!(pc.plan_exists(KESTREL));
        assert!(pc.plan_exists(EXM));
        assert!(deleted(&pc).is_empty());
    }

    #[test]
    fn undo_that_switches_but_cant_delete_says_so_and_keeps_the_plan() {
        let pc = exm_pc();
        let before = applied(&pc);
        pc.fail_delete(KESTREL);

        let err = undo(&pc, &before).unwrap_err();

        // The switch is done and said; the plan is left, not in use, for Remove it.
        assert!(err.starts_with("Back to EXM Free Power Plan V8."), "{err}");
        assert!(err.contains("deleting power plan"), "{err}");
        assert!(is_active(&pc, EXM));
        assert!(pc.plan_exists(KESTREL));
    }

    #[test]
    fn undo_does_nothing_when_kestrels_plan_is_not_the_one_in_use() {
        let pc = exm_pc();
        let before = applied(&pc);
        // The person picked Balanced by hand.
        pc.activate_plan(BALANCED).unwrap();
        let written = pc.writes();

        let err = undo(&pc, &before).unwrap_err();

        assert!(err.contains("isn't in use"), "{err}");
        assert_eq!(pc.writes(), written);
        assert!(is_active(&pc, BALANCED));
        assert!(pc.plan_exists(KESTREL));

        // Nor does it do anything when the plan in use can't be read.
        pc.fail_read(ACTIVE_PLAN);
        let err = undo(&pc, &before).unwrap_err();
        assert!(err.starts_with("Couldn't read your power plan"), "{err}");
        assert_eq!(pc.writes(), written);
    }

    #[test]
    fn undo_will_not_go_back_to_what_is_not_a_plan_and_takes_kestrels_own_for_gone() {
        let pc = exm_pc();
        applied(&pc);
        let written = pc.writes();
        for saved in [
            Saved::Protection { on: true },
            Saved::Dword { value: Some(1) },
        ] {
            assert!(undo(&pc, &saved).is_err(), "{saved:?}");
        }
        assert_eq!(pc.writes(), written);
        assert!(is_active(&pc, KESTREL));

        // A journal that names Kestrel's own plan as the one to go back to: that can only be Balanced.
        let message = undo(&pc, &plan(KESTREL, "Kestrel")).unwrap();
        assert_eq!(message, GONE_SO_BALANCED);
        assert!(is_active(&pc, BALANCED));
        assert!(!pc.plan_exists(KESTREL));
        assert!(pc.plan_exists(EXM));
    }

    #[test]
    fn undo_reads_the_journals_guid_in_any_case() {
        let pc = exm_pc();
        applied(&pc);
        let shouted = plan(&EXM.to_uppercase(), EXM_NAME);

        let message = undo(&pc, &shouted).unwrap();

        assert_eq!(message, "Back to EXM Free Power Plan V8.");
        assert!(is_active(&pc, EXM));
    }

    #[test]
    fn remove_keeps_the_plan_now_active() {
        let pc = exm_pc();
        applied(&pc);
        // The person picks Balanced by hand.
        pc.activate_plan(BALANCED).unwrap();

        remove(&pc).unwrap();

        assert!(!pc.plan_exists(KESTREL));
        assert!(is_active(&pc, BALANCED));
        assert!(pc.plan_exists(EXM));
        assert_eq!(deleted(&pc), [KESTREL]);
    }

    #[test]
    fn remove_never_deletes_the_plan_in_use() {
        let pc = exm_pc();
        applied(&pc);
        let written = pc.writes();

        let err = remove(&pc).unwrap_err();

        assert!(err.contains("in use"), "{err}");
        assert!(pc.plan_exists(KESTREL));
        assert!(is_active(&pc, KESTREL));
        assert_eq!(pc.writes(), written);

        // Nor when it can't tell.
        pc.activate_plan(EXM).unwrap();
        let written = pc.writes();
        pc.fail_read(ACTIVE_PLAN);
        let err = remove(&pc).unwrap_err();
        assert!(err.starts_with("Couldn't read your power plan"), "{err}");
        assert!(pc.plan_exists(KESTREL));
        assert_eq!(pc.writes(), written);
    }

    #[test]
    fn remove_that_windows_refuses_says_why() {
        let pc = exm_pc();
        applied(&pc);
        pc.activate_plan(EXM).unwrap();
        pc.fail_delete(KESTREL);

        let err = remove(&pc).unwrap_err();

        assert!(err.contains("deleting power plan"), "{err}");
        assert!(pc.plan_exists(KESTREL));
        assert!(is_active(&pc, EXM));
    }

    #[test]
    fn remove_with_no_plan_to_remove_is_fine() {
        let pc = exm_pc();
        let before = pc.dump();
        remove(&pc).unwrap();
        assert_eq!(pc.dump(), before);
        assert!(pc.writes().is_empty());
        // Even when the plan in use can't be read: there is nothing to delete.
        pc.fail_read(ACTIVE_PLAN);
        remove(&pc).unwrap();
    }

    #[test]
    fn use_again_switches_back_to_kestrels_plan() {
        let pc = exm_pc();
        applied(&pc);
        pc.activate_plan(BALANCED).unwrap();

        use_again(&pc).unwrap();

        assert!(is_active(&pc, KESTREL));
        assert!(pc.plan_exists(EXM));
    }

    #[test]
    fn use_again_without_the_plan_says_so_and_changes_nothing() {
        let pc = exm_pc();
        let before = pc.dump();
        let err = use_again(&pc).unwrap_err();
        assert!(err.contains("gone"), "{err}");
        assert_eq!(pc.dump(), before);
        assert!(pc.writes().is_empty());
    }

    #[test]
    fn use_again_that_windows_refuses_fails_with_the_reason() {
        let pc = exm_pc();
        applied(&pc);
        pc.activate_plan(BALANCED).unwrap();
        pc.fail_activate(KESTREL);
        let err = use_again(&pc).unwrap_err();
        assert!(err.contains("activating power plan"), "{err}");
        assert!(is_active(&pc, BALANCED));
        assert!(pc.plan_exists(KESTREL));
    }

    #[test]
    fn nothing_but_kestrels_plan_is_ever_deleted() {
        type Path = fn(&FakeSystem);
        let paths: [(&str, Path); 17] = [
            ("make it and undo it", |pc| {
                let before = applied(pc);
                undo(pc, &before).unwrap();
            }),
            ("make it twice", |pc| {
                applied(pc);
                create_and_activate(pc, &desktop()).unwrap();
            }),
            ("the switch fails", |pc| {
                pc.fail_activate(KESTREL);
                create_and_activate(pc, &desktop()).unwrap_err();
            }),
            ("a value won't write", |pc| {
                pc.fail_write(USB);
                create_and_activate(pc, &desktop()).unwrap_err();
            }),
            ("a value won't read back", |pc| {
                pc.fail_read(PCIE);
                create_and_activate(pc, &desktop()).unwrap_err();
            }),
            ("a value isn't kept", |pc| {
                pc.lie_on_readback(PARKING);
                create_and_activate(pc, &desktop()).unwrap_err();
            }),
            ("the plan won't take its name", |pc| {
                pc.fail_write(KESTREL);
                create_and_activate(pc, &desktop()).unwrap_err();
            }),
            ("no stock plan to start from", |pc| {
                pc.fail_duplicate(HIGH_PERFORMANCE);
                pc.fail_duplicate(BALANCED);
                create_and_activate(pc, &desktop()).unwrap_err();
            }),
            ("a leftover is replaced", |pc| {
                pc.add_plan(KESTREL, "Kestrel");
                create_and_activate(pc, &desktop()).unwrap();
            }),
            ("Remove it after a switch by hand", |pc| {
                applied(pc);
                pc.activate_plan(OTHER).unwrap();
                remove(pc).unwrap();
            }),
            ("Remove it while it is in use", |pc| {
                applied(pc);
                remove(pc).unwrap_err();
            }),
            ("Use it again", |pc| {
                applied(pc);
                pc.activate_plan(OTHER).unwrap();
                use_again(pc).unwrap();
            }),
            ("undo to a plan that was never there", |pc| {
                applied(pc);
                undo(pc, &plan(NEVER, "Never")).unwrap();
            }),
            ("undo that can't switch", |pc| {
                let before = applied(pc);
                pc.fail_activate(EXM);
                pc.fail_activate(BALANCED);
                undo(pc, &before).unwrap_err();
            }),
            ("undo that can't delete", |pc| {
                let before = applied(pc);
                pc.fail_delete(KESTREL);
                undo(pc, &before).unwrap_err();
            }),
            ("undo to what isn't a plan", |pc| {
                applied(pc);
                undo(pc, &Saved::Protection { on: false }).unwrap_err();
            }),
            ("undo to Kestrel's own", |pc| {
                applied(pc);
                undo(pc, &plan(KESTREL, "Kestrel")).unwrap();
            }),
        ];
        for (what, path) in paths {
            let pc = exm_pc();
            pc.add_plan(OTHER, "Somebody else's plan");
            let others = |pc: &FakeSystem| -> Vec<(String, String)> {
                pc.plans()
                    .into_iter()
                    .filter(|(guid, _)| guid != KESTREL)
                    .collect()
            };
            let start = others(&pc);

            path(&pc);

            for gone in deleted(&pc) {
                assert_eq!(gone, KESTREL, "{what}");
            }
            assert_eq!(others(&pc), start, "{what}");
        }
    }

    // The Preview.

    #[test]
    fn preview_compares_with_the_current_plan_and_says_the_rest_comes_from_the_base() {
        let pc = exm_pc();
        pc.set_plan_value(EXM, CPU_GROUP, PARKING, 5);
        pc.set_plan_value(EXM, USB_GROUP, USB, 1);
        pc.set_plan_value(EXM, PCIE_GROUP, PCIE, 2);

        let item = preview(&pc, &desktop());

        assert_eq!(item.id, TweakId::PowerPlan);
        assert_eq!(item.title, "Kestrel power plan");
        assert_eq!(item.location, "Power plan");
        assert_eq!(item.timing, Timing::Now);
        assert_eq!(
            item.changes,
            [
                change("Core parking minimum cores", "5%", "100%"),
                change("USB selective suspend", "On", "Off"),
                change("PCIe link-state power management", "Maximum", "Off"),
            ]
        );
        assert_eq!(
            item.note.as_deref(),
            Some("Switching plans also replaces everything else in EXM Free Power Plan V8 with the settings of High performance. Undo returns to your plan as it was.")
        );
        assert!(pc.writes().is_empty());
    }

    #[test]
    fn the_light_preview_lists_two_changes_and_names_balanced() {
        let pc = exm_pc();
        pc.set_plan_value(EXM, USB_GROUP, USB, 0);
        pc.set_plan_value(EXM, PCIE_GROUP, PCIE, 1);

        let item = preview(&pc, &laptop());

        assert_eq!(
            item.changes,
            [
                change("USB selective suspend", "Off", "Off"),
                change("PCIe link-state power management", "Moderate", "Off"),
            ]
        );
        assert_eq!(
            item.note.as_deref(),
            Some("Switching plans also replaces everything else in EXM Free Power Plan V8 with the settings of Balanced. Undo returns to your plan as it was.")
        );
        // The same for every reason a PC gets the light plan.
        let hybrid = PlanInput {
            hybrid_cpu: true,
            ..desktop()
        };
        assert_eq!(preview(&pc, &hybrid).changes, item.changes);
        pc.set_modern_standby(Some(true));
        assert_eq!(preview(&pc, &desktop()).changes, item.changes);
    }

    #[test]
    fn the_preview_names_the_base_even_when_that_stock_plan_is_missing() {
        let pc = exm_pc();
        pc.delete_plan(HIGH_PERFORMANCE).unwrap();
        pc.delete_plan(BALANCED).unwrap();
        let note = preview(&pc, &desktop()).note.unwrap();
        assert!(
            note.ends_with(
                "with the settings of High performance. Undo returns to your plan as it was."
            ),
            "{note}"
        );
        let note = preview(&pc, &laptop()).note.unwrap();
        assert!(
            note.ends_with("with the settings of Balanced. Undo returns to your plan as it was."),
            "{note}"
        );
    }

    #[test]
    fn the_preview_says_unknown_for_a_value_it_cant_read() {
        let pc = exm_pc();
        // No value in the plan in use at all.
        let item = preview(&pc, &desktop());
        assert!(item.changes.iter().all(|c| c.now == "Unknown"), "{item:?}");

        // One that can't be read is Unknown, and the others are as they were.
        pc.set_plan_value(EXM, CPU_GROUP, PARKING, 100);
        pc.set_plan_value(EXM, USB_GROUP, USB, 1);
        pc.set_plan_value(EXM, PCIE_GROUP, PCIE, 1);
        pc.fail_read(USB);
        let item = preview(&pc, &desktop());
        let now: Vec<_> = item.changes.iter().map(|c| c.now.as_str()).collect();
        assert_eq!(now, ["100%", "Unknown", "Moderate"]);

        // If the plan in use can't be read, none of them can, and the note doesn't name it.
        let pc = exm_pc();
        pc.set_plan_value(EXM, CPU_GROUP, PARKING, 100);
        pc.fail_read(ACTIVE_PLAN);
        let item = preview(&pc, &desktop());
        assert!(item.changes.iter().all(|c| c.now == "Unknown"), "{item:?}");
        assert_eq!(
            item.note.as_deref(),
            Some("Switching plans also replaces everything else in your current plan with the settings of High performance. Undo returns to your plan as it was.")
        );
    }

    #[test]
    fn the_preview_says_values_in_words() {
        let now_of = |at: usize, group: &str, setting: &str, value: u32| -> String {
            let pc = exm_pc();
            pc.set_plan_value(EXM, group, setting, value);
            preview(&pc, &desktop()).changes[at].now.clone()
        };
        for (value, words) in [(0, "0%"), (1, "1%"), (50, "50%"), (100, "100%")] {
            assert_eq!(now_of(0, CPU_GROUP, PARKING, value), words);
        }
        for (value, words) in [(0, "Off"), (1, "On"), (2, "2"), (7, "7")] {
            assert_eq!(now_of(1, USB_GROUP, USB, value), words);
        }
        for (value, words) in [(0, "Off"), (1, "Moderate"), (2, "Maximum"), (3, "3")] {
            assert_eq!(now_of(2, PCIE_GROUP, PCIE, value), words);
        }
    }

    #[test]
    fn the_preview_shows_the_clamped_value_apply_will_write() {
        let pc = exm_pc();
        pc.set_plan_range(CPU_GROUP, PARKING, 0, 50);
        pc.set_plan_range(USB_GROUP, USB, 1, 1);
        let item = preview(&pc, &desktop());
        let after: Vec<_> = item.changes.iter().map(|c| c.after.as_str()).collect();
        assert_eq!(after, ["50%", "On", "Off"]);

        // And that is what Apply writes.
        create_and_activate(&pc, &desktop()).unwrap();
        assert_eq!(pc.plan_value(KESTREL, CPU_GROUP, PARKING), Some(50));
        assert_eq!(pc.plan_value(KESTREL, USB_GROUP, USB), Some(1));
        assert_eq!(pc.plan_value(KESTREL, PCIE_GROUP, PCIE), Some(0));
    }

    #[test]
    fn a_dual_ccd_x3d_previews_no_change() {
        let pc = FakeSystem::new();
        let item = preview(&pc, &x3d());
        assert_eq!(item.id, TweakId::PowerPlan);
        assert!(item.changes.is_empty());
        assert_eq!(item.note.as_deref(), Some(AMD));
        assert!(pc.writes().is_empty());
    }
}
