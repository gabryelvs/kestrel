//! How each of the five Windows settings is read, written, put back and compared: Game Mode, background recording,
//! optimizations for windowed games, the graphics preference for VALORANT, and pointer precision. The sixth Essential
//! tweak, the power plan, and System Protection are not here; `power` and the engine own them.
//!
//! A setting is read as the `Saved` value the journal keeps for it, so what Kestrel found before a change, what it
//! planned and what it finds afterwards can all be written down, put back and compared by what they mean.
//!
//! Registry paths have no hive. `System` opens the per-user keys under `HKEY_USERS\<sid>` (`HKCU` is shorthand for that
//! everywhere in Kestrel) and the policy key under `HKEY_LOCAL_MACHINE`, which Kestrel only reads. Kestrel writes the
//! values named below and nothing else: never `AppCaptureEnabled`, `GameDVR_Enabled`, `AllowGameDVR` or
//! `AllowAutoGameMode`, which would also switch off manual recording and screenshots or fight a policy.
//!
//! A failed read is never "absent". `current` says `Read::Failed` for a value of the wrong type or a refused read, and
//! the writes into a `Name=Value;` string refuse to touch one they can't read, so an undo can't delete a real value by
//! mistaking it for one Kestrel made. (Deciding whether to write at all, from `current`, is the engine's.)
//!
//! What goes in is untrusted: a `Saved` from a journal file is checked against its tweak before anything is written.
use std::io;

use kestrel_core::tweak_id::TweakId;

use crate::catalog::info;
use crate::journal::Saved;
use crate::system::{Read, System};
use crate::tokens;

/// `HKCU\Software\Microsoft\GameBar`, where Game Mode lives.
pub const GAME_BAR_KEY: &str = r"Software\Microsoft\GameBar";
/// Game Mode: 1 is on, 0 is off, and absent is on.
pub const AUTO_GAME_MODE: &str = "AutoGameModeEnabled";
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR`, where Game Bar's capture settings live.
pub const GAME_DVR_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";
/// Recording in the background: 1 is on, 0 is off, and absent is off.
pub const HISTORICAL_CAPTURE: &str = "HistoricalCaptureEnabled";
/// The master switch for capture, which also stops manual recording and screenshots. Only ever read.
pub const APP_CAPTURE: &str = "AppCaptureEnabled";
/// `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`: one string value of `Name=Value;` tokens for every game that
/// has a preference, named by its executable, and one for the settings that apply to all windowed games.
pub const DIRECTX_KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
/// The value that holds the settings for all games, among them Optimizations for windowed games.
pub const DIRECTX_SETTINGS: &str = "DirectXUserGlobalSettings";
/// The token for Optimizations for windowed games, in `DIRECTX_SETTINGS`.
pub const SWAP_EFFECT_TOKEN: &str = "SwapEffectUpgradeEnable";
/// The token for a game's graphics preference (0 Windows decides, 1 power saving, 2 high performance), in the value
/// named by the game's executable.
pub const GPU_PREFERENCE_TOKEN: &str = "GpuPreference";
/// `HKCU\Control Panel\Mouse`, where Windows keeps the mouse settings as strings.
pub const MOUSE_KEY: &str = r"Control Panel\Mouse";
/// Pointer acceleration: "0" is off, "1" and "2" are on, and absent counts as the default, "1".
pub const MOUSE_SPEED: &str = "MouseSpeed";
/// The first and second acceleration threshold, "6" and "10" by default.
pub const MOUSE_THRESHOLD_1: &str = "MouseThreshold1";
pub const MOUSE_THRESHOLD_2: &str = "MouseThreshold2";
/// `HKLM\Software\Policies\Microsoft\Windows\GameDVR`, where an organisation's policy may switch Game Mode or capture
/// off. Only ever read.
pub const GAME_DVR_POLICY_KEY: &str = r"Software\Policies\Microsoft\Windows\GameDVR";
/// 0 turns Game Mode off for everyone.
pub const ALLOW_AUTO_GAME_MODE: &str = "AllowAutoGameMode";
/// 0 turns capture, recording included, off for everyone.
pub const ALLOW_GAME_DVR: &str = "AllowGameDVR";

/// What an absent mouse string means: Windows' own setting, acceleration on.
const DEFAULT_SPEED: &str = "1";
const DEFAULT_THRESHOLD_1: &str = "6";
const DEFAULT_THRESHOLD_2: &str = "10";

const NO_GAME: &str = "VALORANT's file isn't known, so its graphics preference was left alone.";

/// A read as what it found: the value, nothing because the value isn't there, or why it couldn't read.
pub(crate) fn found<T>(read: Read<T>) -> Result<Option<T>, String> {
    match read {
        Read::Absent => Ok(None),
        Read::Value(value) => Ok(Some(value)),
        Read::Failed(why) => Err(why),
    }
}

/// The name of the game's value, when there is one. An empty name would be the key's default value, which is nobody's.
fn game_value(value_name: Option<&str>) -> Option<&str> {
    value_name.filter(|name| !name.is_empty())
}

/// True unless pointer acceleration is off. An absent `MouseSpeed` is Windows' default, which is on.
pub(crate) fn acceleration_on(speed: Option<&str>) -> bool {
    speed.unwrap_or(DEFAULT_SPEED) != "0"
}

/// What the setting holds now, for this account. A setting that isn't set is a value (`Dword { value: None }`, or a token
/// that isn't there); only a read that failed is not. `value_name` is the value the game's graphics preference lives
/// in (see `gpu_value_name`) and is ignored for every other setting.
pub fn current(
    sys: &dyn System,
    tweak: TweakId,
    sid: &str,
    value_name: Option<&str>,
) -> Read<Saved> {
    let read = match tweak {
        TweakId::GameMode => read_dword(sys, sid, GAME_BAR_KEY, AUTO_GAME_MODE),
        TweakId::BackgroundRecording => read_dword(sys, sid, GAME_DVR_KEY, HISTORICAL_CAPTURE),
        TweakId::WindowedOptimizations => read_token(sys, sid, DIRECTX_SETTINGS, SWAP_EFFECT_TOKEN),
        TweakId::GpuPreference => match game_value(value_name) {
            Some(name) => read_token(sys, sid, name, GPU_PREFERENCE_TOKEN),
            None => Err(NO_GAME.to_string()),
        },
        TweakId::PointerPrecision => read_mouse(sys, sid),
        TweakId::PowerPlan | TweakId::SystemProtection => Err(format!(
            "{} isn't one of the Windows settings read here.",
            info(tweak).title
        )),
    };
    match read {
        Ok(saved) => Read::Value(saved),
        Err(why) => Read::Failed(why),
    }
}

fn read_dword(sys: &dyn System, sid: &str, key: &str, name: &str) -> Result<Saved, String> {
    Ok(Saved::Dword {
        value: found(sys.read_dword(sid, key, name))?,
    })
}

/// One token of a string value, with whether the value and its key were there. The journal keeps those two so that an
/// undo removes only what Kestrel made.
fn read_token(
    sys: &dyn System,
    sid: &str,
    value_name: &str,
    token_name: &str,
) -> Result<Saved, String> {
    let text = found(sys.read_string(sid, DIRECTX_KEY, value_name))?;
    // Whether the key is there is a read of its own.
    let key_existed = found(sys.value_names(sid, DIRECTX_KEY))?.is_some();
    Ok(Saved::Token {
        token: text
            .as_deref()
            .and_then(|text| tokens::get(text, token_name)),
        value_existed: text.is_some(),
        key_existed,
    })
}

/// The three strings on disk, which are the person's, and the setting in force, which is the session's.
fn read_mouse(sys: &dyn System, sid: &str) -> Result<Saved, String> {
    let speed = found(sys.read_string(sid, MOUSE_KEY, MOUSE_SPEED))?;
    let threshold1 = found(sys.read_string(sid, MOUSE_KEY, MOUSE_THRESHOLD_1))?;
    let threshold2 = found(sys.read_string(sid, MOUSE_KEY, MOUSE_THRESHOLD_2))?;
    let live = found(sys.mouse_live())?
        .ok_or_else(|| "Windows didn't say how the mouse is set.".to_string())?;
    Ok(Saved::Mouse {
        speed,
        threshold1,
        threshold2,
        live,
    })
}

/// What Kestrel writes for the setting, given what it found. A token keeps whether its value and key existed, so that
/// undoing the change can tell what Kestrel made. The plan and System Protection are not changed by these functions;
/// their targets are the power plan's and the engine's to say, and for them this answers with the prior.
pub fn target(tweak: TweakId, prior: &Saved) -> Saved {
    // A prior that isn't a token can't say what existed, so everything counts as having been there and no undo ever
    // removes a value or a key.
    let (value_existed, key_existed) = match prior {
        Saved::Token {
            value_existed,
            key_existed,
            ..
        } => (*value_existed, *key_existed),
        _ => (true, true),
    };
    let token = |text: &str| Saved::Token {
        token: Some(text.to_string()),
        value_existed,
        key_existed,
    };
    match tweak {
        TweakId::GameMode => Saved::Dword { value: Some(1) },
        TweakId::BackgroundRecording => Saved::Dword { value: Some(0) },
        TweakId::WindowedOptimizations => token("1"),
        TweakId::GpuPreference => token("2"),
        TweakId::PointerPrecision => Saved::Mouse {
            speed: Some("0".to_string()),
            threshold1: Some("0".to_string()),
            threshold2: Some("0".to_string()),
            live: [0, 0, 0],
        },
        TweakId::PowerPlan | TweakId::SystemProtection => prior.clone(),
    }
}

/// Makes the setting hold `value`, which is what `target` said. A token goes into its string and leaves every other
/// token as it is.
///
/// `accounts_differ` is `Account::differs`. The mouse is set through Windows when the accounts are the same; when they
/// differ, Windows would save it in the profile of the account Kestrel runs as, so only the signed-in account's strings
/// are written, and the mouse in force changes at the next sign-in. A state that doesn't belong to the tweak, a GPU
/// preference with no value name, or a token Kestrel can't read is refused and nothing is written.
pub fn write(
    sys: &dyn System,
    tweak: TweakId,
    sid: &str,
    value_name: Option<&str>,
    value: &Saved,
    accounts_differ: bool,
) -> io::Result<()> {
    match (tweak, value) {
        (TweakId::GameMode, Saved::Dword { value }) => {
            put_dword(sys, sid, GAME_BAR_KEY, AUTO_GAME_MODE, *value)
        }
        (TweakId::BackgroundRecording, Saved::Dword { value }) => {
            put_dword(sys, sid, GAME_DVR_KEY, HISTORICAL_CAPTURE, *value)
        }
        (TweakId::WindowedOptimizations, Saved::Token { token, .. }) => put_token(
            sys,
            sid,
            DIRECTX_SETTINGS,
            SWAP_EFFECT_TOKEN,
            token.as_deref(),
            None,
        ),
        (TweakId::GpuPreference, Saved::Token { token, .. }) => {
            let name = game_value(value_name).ok_or_else(|| invalid(NO_GAME))?;
            put_token(sys, sid, name, GPU_PREFERENCE_TOKEN, token.as_deref(), None)
        }
        (
            TweakId::PointerPrecision,
            Saved::Mouse {
                speed,
                threshold1,
                threshold2,
                live,
            },
        ) => {
            if accounts_differ {
                put_strings(
                    sys,
                    sid,
                    speed.as_deref(),
                    threshold1.as_deref(),
                    threshold2.as_deref(),
                )
            } else {
                sys.set_mouse_live(*live, true)
            }
        }
        _ => Err(refused(tweak)),
    }
}

/// Puts the setting back as `prior` says it was. Where `write` adds Kestrel's token, this puts the old one back or takes
/// it out, and removes the string, and its key, if Kestrel made them and nothing else has been put in since. A DWORD
/// that wasn't there is deleted, and the three mouse strings are written back or deleted exactly, after the mouse in
/// force: the strings are what say whether Kestrel's change still holds, so if this stops before them, the setting
/// still reads as Kestrel's and the undo can be tried again.
///
/// It refuses what `write` refuses. It does not check that the setting still holds what Kestrel left; that is for the
/// caller to have decided.
pub fn restore(
    sys: &dyn System,
    tweak: TweakId,
    sid: &str,
    value_name: Option<&str>,
    prior: &Saved,
    accounts_differ: bool,
) -> io::Result<()> {
    match (tweak, prior) {
        (TweakId::GameMode, Saved::Dword { value }) => {
            put_dword(sys, sid, GAME_BAR_KEY, AUTO_GAME_MODE, *value)
        }
        (TweakId::BackgroundRecording, Saved::Dword { value }) => {
            put_dword(sys, sid, GAME_DVR_KEY, HISTORICAL_CAPTURE, *value)
        }
        (
            TweakId::WindowedOptimizations,
            Saved::Token {
                token,
                value_existed,
                key_existed,
            },
        ) => put_token(
            sys,
            sid,
            DIRECTX_SETTINGS,
            SWAP_EFFECT_TOKEN,
            token.as_deref(),
            Some(Made {
                value: !value_existed,
                key: !key_existed,
            }),
        ),
        (
            TweakId::GpuPreference,
            Saved::Token {
                token,
                value_existed,
                key_existed,
            },
        ) => {
            let name = game_value(value_name).ok_or_else(|| invalid(NO_GAME))?;
            put_token(
                sys,
                sid,
                name,
                GPU_PREFERENCE_TOKEN,
                token.as_deref(),
                Some(Made {
                    value: !value_existed,
                    key: !key_existed,
                }),
            )
        }
        (
            TweakId::PointerPrecision,
            Saved::Mouse {
                speed,
                threshold1,
                threshold2,
                live,
            },
        ) => {
            // The mouse in force goes back first, unless Kestrel never changed it because the accounts differ. Then the
            // strings are written exactly as they were, or deleted if they weren't there.
            if !accounts_differ {
                sys.set_mouse_live(*live, false)?;
            }
            put_strings(
                sys,
                sid,
                speed.as_deref(),
                threshold1.as_deref(),
                threshold2.as_deref(),
            )
        }
        _ => Err(refused(tweak)),
    }
}

/// What Kestrel made, which an undo may take out again once nothing else is in it.
struct Made {
    value: bool,
    key: bool,
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

/// A state that doesn't belong to its tweak: the journal is a file, and a file can say anything.
fn refused(tweak: TweakId) -> io::Error {
    invalid(&format!(
        "{} can't be set to that here, so nothing was changed.",
        info(tweak).title
    ))
}

/// Deletes a value, and counts a value that is already gone as deleted.
fn remove_value(sys: &dyn System, sid: &str, key: &str, name: &str) -> io::Result<()> {
    match sys.delete_value(sid, key, name) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Writes a DWORD, or deletes it when the value is to be absent.
fn put_dword(
    sys: &dyn System,
    sid: &str,
    key: &str,
    name: &str,
    value: Option<u32>,
) -> io::Result<()> {
    match value {
        Some(value) => sys.write_dword(sid, key, name, value),
        None => remove_value(sys, sid, key, name),
    }
}

/// Writes the three mouse strings, or deletes the ones that are to be absent.
fn put_strings(
    sys: &dyn System,
    sid: &str,
    speed: Option<&str>,
    threshold1: Option<&str>,
    threshold2: Option<&str>,
) -> io::Result<()> {
    for (name, text) in [
        (MOUSE_SPEED, speed),
        (MOUSE_THRESHOLD_1, threshold1),
        (MOUSE_THRESHOLD_2, threshold2),
    ] {
        match text {
            Some(text) => sys.write_string(sid, MOUSE_KEY, name, text)?,
            None => remove_value(sys, sid, MOUSE_KEY, name)?,
        }
    }
    Ok(())
}

/// A token's text goes into a `Name=Value;` string as it is, so it can't hold what separates tokens or what isn't text.
fn is_plain(text: &str) -> bool {
    !text.contains(';') && !text.chars().any(char::is_control)
}

/// Sets the token to `token`, or removes it when that is `None`, in the string value `value_name`, and leaves every
/// other part of the string as it is. With `made`, this is an undo: a value that Kestrel made is deleted once nothing
/// is left in it, and so is the key if Kestrel made that too and it is empty.
fn put_token(
    sys: &dyn System,
    sid: &str,
    value_name: &str,
    token_name: &str,
    token: Option<&str>,
    made: Option<Made>,
) -> io::Result<()> {
    if token.is_some_and(|text| !is_plain(text)) {
        return Err(invalid(
            "A setting can't hold that text, so nothing was changed.",
        ));
    }
    // A string that can't be read is not one that isn't there: writing over it would lose what the person had.
    let current = found(sys.read_string(sid, DIRECTX_KEY, value_name)).map_err(io::Error::other)?;
    let before = current.as_deref().unwrap_or("");
    let after = match token {
        Some(text) => tokens::set(before, token_name, text),
        None => tokens::remove(before, token_name),
    };
    if let Some(made) = made {
        if after.is_empty() && made.value {
            if current.is_some() {
                remove_value(sys, sid, DIRECTX_KEY, value_name)?;
            }
            if made.key {
                sys.delete_key_if_empty(sid, DIRECTX_KEY)?;
            }
            return Ok(());
        }
    }
    // The token is already as it should be, or isn't there and shouldn't be: nothing to change, and the person's own text
    // is left exactly as it is, down to the way it is spelled. That includes there being no string at all.
    if tokens::get(before, token_name).as_deref() == token {
        return Ok(());
    }
    sys.write_string(sid, DIRECTX_KEY, value_name, &after)
}

/// The value the game's graphics preference lives in, named in the spelling it already has. Windows keeps a value under
/// the name it was first written with, and the person's PC may hold VALORANT's with another letter case or other
/// slashes than the path Kestrel works out; using that spelling updates the value rather than adding a second one. When
/// the game has no value yet, this is `exe`. A key that can't be listed is a failed read.
pub fn gpu_value_name(sys: &dyn System, sid: &str, exe: &str) -> Read<String> {
    let names = match sys.value_names(sid, DIRECTX_KEY) {
        Read::Absent => return Read::Value(exe.to_string()),
        Read::Failed(why) => return Read::Failed(why),
        Read::Value(names) => names,
    };
    // Names match ignoring case, as the registry matches them. A value written with the other slashes is the game's
    // too, but the one with the slashes Windows itself uses is the one it looks up, so that wins if both are there.
    let fold = |name: &str| name.to_lowercase();
    let backslashes = |name: &str| fold(&name.replace('/', "\\"));
    let same_case_folded = names.iter().find(|name| fold(name) == fold(exe));
    let same_path = || {
        names
            .iter()
            .find(|name| backslashes(name) == backslashes(exe))
    };
    Read::Value(
        same_case_folded
            .or_else(same_path)
            .map_or_else(|| exe.to_string(), Clone::clone),
    )
}

/// Whether two states of the same setting mean the same, which is what the journal's three-way check needs: a value that
/// isn't there counts as the setting's default, and what only says how a state came about does not count. Absent means
/// Game Mode on, background recording off, the graphics preference 0, and the mouse strings 1, 6 and 10; a windowed
/// token has no default, so absent is equal only to absent. The mouse in force, and whether the string value and its key
/// existed, are ignored (`is_back_at` adds the mouse in force). States of different kinds never mean the same.
pub fn same_meaning(tweak: TweakId, a: &Saved, b: &Saved) -> bool {
    match (a, b) {
        (Saved::Dword { value: a }, Saved::Dword { value: b }) => {
            dword_meaning(tweak, *a) == dword_meaning(tweak, *b)
        }
        (Saved::Token { token: a, .. }, Saved::Token { token: b, .. }) => {
            token_meaning(tweak, a.as_deref()) == token_meaning(tweak, b.as_deref())
        }
        (
            Saved::Mouse {
                speed: speed_a,
                threshold1: first_a,
                threshold2: second_a,
                ..
            },
            Saved::Mouse {
                speed: speed_b,
                threshold1: first_b,
                threshold2: second_b,
                ..
            },
        ) => mouse_meaning(speed_a, first_a, second_a) == mouse_meaning(speed_b, first_b, second_b),
        (Saved::Plan { guid: a, .. }, Saved::Plan { guid: b, .. }) => a.eq_ignore_ascii_case(b),
        (Saved::Protection { on: a }, Saved::Protection { on: b }) => a == b,
        _ => false,
    }
}

/// Whether a setting that reads as `now` has gone back to `prior`: what closes a record, and what an undo has to see
/// before it counts. That is `same_meaning`, and for the mouse one thing more. What the mouse means is in the strings,
/// but when the accounts are the same Kestrel changes the mouse in force as well, and it isn't back while that is
/// anything else. When they differ Kestrel never touches the mouse in force, so it has no say.
pub fn is_back_at(tweak: TweakId, now: &Saved, prior: &Saved, accounts_differ: bool) -> bool {
    let in_force_is_back = match (now, prior) {
        (Saved::Mouse { live: now, .. }, Saved::Mouse { live: prior, .. }) => {
            accounts_differ || now == prior
        }
        _ => true,
    };
    same_meaning(tweak, now, prior) && in_force_is_back
}

fn dword_meaning(tweak: TweakId, value: Option<u32>) -> Option<u32> {
    match tweak {
        TweakId::GameMode => Some(value.unwrap_or(1)),
        TweakId::BackgroundRecording => Some(value.unwrap_or(0)),
        _ => value,
    }
}

fn token_meaning(tweak: TweakId, token: Option<&str>) -> Option<&str> {
    match tweak {
        TweakId::GpuPreference => Some(token.unwrap_or("0")),
        _ => token,
    }
}

fn mouse_meaning<'a>(
    speed: &'a Option<String>,
    threshold1: &'a Option<String>,
    threshold2: &'a Option<String>,
) -> (&'a str, &'a str, &'a str) {
    (
        speed.as_deref().unwrap_or(DEFAULT_SPEED),
        threshold1.as_deref().unwrap_or(DEFAULT_THRESHOLD_1),
        threshold2.as_deref().unwrap_or(DEFAULT_THRESHOLD_2),
    )
}

/// A state in words, for the Preview and the report: "on", "off", "High performance", "Windows decides", "Power saving",
/// "not set", or a plan's name. Every kind of state has words, even one that doesn't belong to its tweak, because the
/// journal may hold one and the report still has to say something.
pub fn describe(tweak: TweakId, value: &Saved) -> String {
    let on_off = |on: bool| if on { "on" } else { "off" }.to_string();
    match (tweak, value) {
        (TweakId::GameMode, Saved::Dword { value }) => on_off(value.unwrap_or(1) != 0),
        (TweakId::BackgroundRecording, Saved::Dword { value }) => on_off(value.unwrap_or(0) != 0),
        (TweakId::WindowedOptimizations, Saved::Token { token, .. }) => match token.as_deref() {
            None => "not set".to_string(),
            Some("1") => "on".to_string(),
            Some("0") => "off".to_string(),
            Some(other) => other.to_string(),
        },
        (TweakId::GpuPreference, Saved::Token { token, .. }) => match token.as_deref() {
            None | Some("0") => "Windows decides".to_string(),
            Some("1") => "Power saving".to_string(),
            Some("2") => "High performance".to_string(),
            Some(other) => other.to_string(),
        },
        (_, Saved::Mouse { speed, .. }) => on_off(acceleration_on(speed.as_deref())),
        (_, Saved::Plan { guid, name }) => {
            if name.trim().is_empty() {
                guid.clone()
            } else {
                name.clone()
            }
        }
        (_, Saved::Protection { on }) => on_off(*on),
        (_, Saved::Dword { value }) => {
            value.map_or_else(|| "not set".to_string(), |value| value.to_string())
        }
        (_, Saved::Token { token, .. }) => token.clone().unwrap_or_else(|| "not set".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeSystem, MOUSE_LIVE, SID};
    use crate::tokens;
    use kestrel_probe::account::Account;
    use std::io;
    use TweakId::{
        BackgroundRecording, GameMode, GpuPreference, PointerPrecision, PowerPlan,
        SystemProtection, WindowedOptimizations,
    };

    const OTHER_SID: &str = "S-1-5-21-1-2-3-1002";
    const EXE: &str =
        r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe";
    const EXE_LOWER: &str =
        r"c:\riot games\valorant\live\shootergame\binaries\win64\valorant-win64-shipping.exe";
    const EXE_SLASHES: &str =
        "C:/Riot Games/VALORANT/live/ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe";
    const OTHER_EXE: &str = r"D:\Games\Other\Binaries\Other.exe";
    const KESTREL_PLAN: &str = "4b455354-524c-4e50-a1b2-c3d4e5f60705";

    /// The five settings, in the order of the Tweaks tab.
    const SETTINGS: [TweakId; 5] = [
        GameMode,
        BackgroundRecording,
        WindowedOptimizations,
        GpuPreference,
        PointerPrecision,
    ];

    fn dword(value: Option<u32>) -> Saved {
        Saved::Dword { value }
    }

    fn token(token: Option<&str>, value_existed: bool, key_existed: bool) -> Saved {
        Saved::Token {
            token: token.map(str::to_string),
            value_existed,
            key_existed,
        }
    }

    fn mouse(
        speed: Option<&str>,
        threshold1: Option<&str>,
        threshold2: Option<&str>,
        live: [i32; 3],
    ) -> Saved {
        Saved::Mouse {
            speed: speed.map(str::to_string),
            threshold1: threshold1.map(str::to_string),
            threshold2: threshold2.map(str::to_string),
            live,
        }
    }

    /// The mouse as Windows ships it.
    fn default_mouse() -> Saved {
        mouse(Some("1"), Some("6"), Some("10"), [6, 10, 1])
    }

    fn no_acceleration() -> Saved {
        mouse(Some("0"), Some("0"), Some("0"), [0, 0, 0])
    }

    fn plan() -> Saved {
        Saved::Plan {
            guid: KESTREL_PLAN.to_string(),
            name: "Kestrel".to_string(),
        }
    }

    /// The value a tweak's data lives in, when it has one of its own: only the game's graphics preference does.
    fn name_of(tweak: TweakId) -> Option<&'static str> {
        (tweak == GpuPreference).then_some(EXE)
    }

    /// What `current` finds, for a setting that reads.
    fn now(pc: &FakeSystem, tweak: TweakId) -> Saved {
        match current(pc, tweak, SID, name_of(tweak)) {
            Read::Value(saved) => saved,
            other => panic!("expected {tweak:?} to read, got {other:?}"),
        }
    }

    /// Writes what Kestrel plans for the setting, as Apply does.
    fn apply(pc: &FakeSystem, tweak: TweakId, prior: &Saved) {
        let planned = target(tweak, prior);
        let differs = pc.account().differs();
        write(pc, tweak, SID, name_of(tweak), &planned, differs).unwrap();
    }

    fn undo(pc: &FakeSystem, tweak: TweakId, prior: &Saved) {
        let differs = pc.account().differs();
        restore(pc, tweak, SID, name_of(tweak), prior, differs).unwrap();
    }

    /// A PC where Kestrel runs as another account than the one at the screen.
    fn differing() -> FakeSystem {
        let pc = FakeSystem::new();
        pc.set_account(Account {
            signed_in: Some(SID.to_string()),
            this_process: Some(OTHER_SID.to_string()),
        });
        pc
    }

    /// A PC on which every setting has something to change and a value to change it from.
    fn arranged(pc: &FakeSystem) {
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1);
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=1;");
    }

    /// Like `arranged`, but the DirectX key and everything in it is missing, as on a PC that never opened Graphics settings.
    fn arranged_without_the_directx_key(pc: &FakeSystem) {
        pc.set_dword(SID, GAME_BAR_KEY, AUTO_GAME_MODE, 0);
        pc.set_dword(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1);
    }

    /// The names in the DirectX key, sorted; the key has to exist.
    fn names(pc: &FakeSystem) -> Vec<String> {
        match pc.value_names(SID, DIRECTX_KEY) {
            Read::Value(mut names) => {
                names.sort();
                names
            }
            other => panic!("expected the key to exist, got {other:?}"),
        }
    }

    /// These names, sorted the way `names` sorts them.
    fn sorted<const N: usize>(names: [&str; N]) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        names.sort();
        names
    }

    /// The graphics preference read from the value with this name.
    fn now_at(pc: &FakeSystem, name: &str) -> Saved {
        match current(pc, GpuPreference, SID, Some(name)) {
            Read::Value(saved) => saved,
            other => panic!("expected {name} to read, got {other:?}"),
        }
    }

    /// The name `gpu_value_name` gives the game's value.
    fn game_value(pc: &FakeSystem) -> String {
        match gpu_value_name(pc, SID, EXE) {
            Read::Value(name) => name,
            other => panic!("expected a name for the game's value, got {other:?}"),
        }
    }

    /// A PC where the value a setting lives in holds the wrong type.
    fn with_a_wrong_type(tweak: TweakId) -> FakeSystem {
        let pc = FakeSystem::new();
        match tweak {
            GameMode => pc.set_string(SID, GAME_BAR_KEY, AUTO_GAME_MODE, "1"),
            BackgroundRecording => pc.set_string(SID, GAME_DVR_KEY, HISTORICAL_CAPTURE, "1"),
            WindowedOptimizations => pc.set_dword(SID, DIRECTX_KEY, DIRECTX_SETTINGS, 1),
            GpuPreference => pc.set_dword(SID, DIRECTX_KEY, EXE, 2),
            PointerPrecision => pc.set_dword(SID, MOUSE_KEY, MOUSE_SPEED, 1),
            PowerPlan | SystemProtection => unreachable!("not one of the five"),
        }
        pc
    }

    fn kind(result: io::Result<()>) -> io::ErrorKind {
        result.expect_err("expected an error").kind()
    }

    // The keys.

    #[test]
    fn the_keys_and_values_are_the_ones_the_spec_names() {
        assert_eq!(GAME_BAR_KEY, r"Software\Microsoft\GameBar");
        assert_eq!(AUTO_GAME_MODE, "AutoGameModeEnabled");
        assert_eq!(
            GAME_DVR_KEY,
            r"Software\Microsoft\Windows\CurrentVersion\GameDVR"
        );
        assert_eq!(HISTORICAL_CAPTURE, "HistoricalCaptureEnabled");
        assert_eq!(APP_CAPTURE, "AppCaptureEnabled");
        assert_eq!(
            DIRECTX_KEY,
            r"Software\Microsoft\DirectX\UserGpuPreferences"
        );
        assert_eq!(DIRECTX_SETTINGS, "DirectXUserGlobalSettings");
        assert_eq!(SWAP_EFFECT_TOKEN, "SwapEffectUpgradeEnable");
        assert_eq!(GPU_PREFERENCE_TOKEN, "GpuPreference");
        assert_eq!(MOUSE_KEY, r"Control Panel\Mouse");
        assert_eq!(MOUSE_SPEED, "MouseSpeed");
        assert_eq!(MOUSE_THRESHOLD_1, "MouseThreshold1");
        assert_eq!(MOUSE_THRESHOLD_2, "MouseThreshold2");
        assert_eq!(
            GAME_DVR_POLICY_KEY,
            r"Software\Policies\Microsoft\Windows\GameDVR"
        );
        assert_eq!(ALLOW_AUTO_GAME_MODE, "AllowAutoGameMode");
        assert_eq!(ALLOW_GAME_DVR, "AllowGameDVR");
    }

    // Reading.

    #[test]
    fn a_dword_setting_reads_as_its_value_or_as_not_there() {
        for (tweak, key, name) in [
            (GameMode, GAME_BAR_KEY, AUTO_GAME_MODE),
            (BackgroundRecording, GAME_DVR_KEY, HISTORICAL_CAPTURE),
        ] {
            let pc = FakeSystem::new();
            assert_eq!(
                current(&pc, tweak, SID, None),
                Read::Value(dword(None)),
                "{tweak:?}"
            );
            pc.set_dword(SID, key, name, 0);
            assert_eq!(
                current(&pc, tweak, SID, None),
                Read::Value(dword(Some(0))),
                "{tweak:?}"
            );
            pc.set_dword(SID, key, name, 1);
            assert_eq!(
                current(&pc, tweak, SID, None),
                Read::Value(dword(Some(1))),
                "{tweak:?}"
            );
            // Another account's value is not this account's.
            assert_eq!(
                current(&pc, tweak, OTHER_SID, None),
                Read::Value(dword(None)),
                "{tweak:?}"
            );
        }
    }

    #[test]
    fn background_recording_is_its_own_value_and_not_the_master_switch_beside_it() {
        let pc = FakeSystem::new();
        pc.set_dword(SID, GAME_DVR_KEY, APP_CAPTURE, 0);
        assert_eq!(
            current(&pc, BackgroundRecording, SID, None),
            Read::Value(dword(None))
        );
    }

    #[test]
    fn a_token_setting_reads_with_the_value_and_the_key_it_lives_in() {
        let pc = FakeSystem::new();
        // Neither the key nor the value is there.
        assert_eq!(now(&pc, WindowedOptimizations), token(None, false, false));
        assert_eq!(now(&pc, GpuPreference), token(None, false, false));
        // The key is there and the value is not.
        pc.add_key(SID, DIRECTX_KEY);
        assert_eq!(now(&pc, WindowedOptimizations), token(None, false, true));
        // The value is there, with other tokens in it.
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        assert_eq!(now(&pc, WindowedOptimizations), token(None, true, true));
        // The name of the token is matched in any case.
        pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "VRROptimizeEnable=1;swapeffectupgradeenable=0;",
        );
        assert_eq!(
            now(&pc, WindowedOptimizations),
            token(Some("0"), true, true)
        );
        // The game's value is another value in the same key.
        assert_eq!(now(&pc, GpuPreference), token(None, false, true));
        pc.set_string(
            SID,
            DIRECTX_KEY,
            EXE,
            "SwapEffectUpgradeEnable=1;GpuPreference=2;",
        );
        assert_eq!(now(&pc, GpuPreference), token(Some("2"), true, true));
        assert_eq!(
            now(&pc, WindowedOptimizations),
            token(Some("0"), true, true)
        );
    }

    #[test]
    fn the_mouse_reads_as_its_three_strings_and_the_setting_in_force() {
        let pc = FakeSystem::new();
        assert_eq!(now(&pc, PointerPrecision), default_mouse());
        pc.set_mouse([0, 0, 0]);
        assert_eq!(
            now(&pc, PointerPrecision),
            mouse(Some("1"), Some("6"), Some("10"), [0, 0, 0])
        );
        for name in [MOUSE_SPEED, MOUSE_THRESHOLD_1, MOUSE_THRESHOLD_2] {
            pc.remove_value(SID, MOUSE_KEY, name);
        }
        assert_eq!(
            now(&pc, PointerPrecision),
            mouse(None, None, None, [0, 0, 0])
        );
    }

    #[test]
    fn a_wrong_type_is_failed_for_every_setting_never_not_there() {
        for tweak in SETTINGS {
            let pc = with_a_wrong_type(tweak);
            let read = current(&pc, tweak, SID, name_of(tweak));
            match read {
                Read::Failed(why) => assert!(!why.is_empty(), "{tweak:?}"),
                other => panic!("{tweak:?}: expected a failed read, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_refused_read_is_failed_whatever_part_of_the_setting_it_is() {
        for (tweak, name) in [
            (GameMode, AUTO_GAME_MODE),
            (BackgroundRecording, HISTORICAL_CAPTURE),
            (WindowedOptimizations, DIRECTX_SETTINGS),
            (GpuPreference, EXE),
            // Whether the key is there is a read of its own.
            (WindowedOptimizations, DIRECTX_KEY),
            (GpuPreference, DIRECTX_KEY),
            (PointerPrecision, MOUSE_SPEED),
            (PointerPrecision, MOUSE_THRESHOLD_1),
            (PointerPrecision, MOUSE_THRESHOLD_2),
            (PointerPrecision, MOUSE_LIVE),
        ] {
            let pc = FakeSystem::new();
            pc.fail_read(name);
            let read = current(&pc, tweak, SID, name_of(tweak));
            assert!(
                matches!(read, Read::Failed(_)),
                "{tweak:?}, {name}: {read:?}"
            );
        }
    }

    #[test]
    fn the_power_plan_and_system_protection_are_not_read_here() {
        let pc = FakeSystem::new();
        for tweak in [PowerPlan, SystemProtection] {
            let read = current(&pc, tweak, SID, None);
            assert!(matches!(read, Read::Failed(_)), "{tweak:?}: {read:?}");
        }
    }

    #[test]
    fn the_gpu_preference_needs_the_name_of_the_value_it_lives_in() {
        let pc = FakeSystem::new();
        // A decoy: the settings value must not be taken for the game's.
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "GpuPreference=1;");
        let before = pc.dump();
        for name in [None, Some("")] {
            let read = current(&pc, GpuPreference, SID, name);
            assert!(matches!(read, Read::Failed(_)), "{name:?}: {read:?}");
            assert_eq!(
                kind(write(
                    &pc,
                    GpuPreference,
                    SID,
                    name,
                    &token(Some("2"), true, true),
                    false
                )),
                io::ErrorKind::InvalidData,
                "{name:?}"
            );
            assert_eq!(
                kind(restore(
                    &pc,
                    GpuPreference,
                    SID,
                    name,
                    &token(None, true, true),
                    false
                )),
                io::ErrorKind::InvalidData,
                "{name:?}"
            );
        }
        assert_eq!(pc.dump(), before);
        assert!(pc.writes().is_empty());
    }

    // What Kestrel writes.

    #[test]
    fn the_targets_are_what_the_spec_writes_and_keep_what_existed() {
        assert_eq!(target(GameMode, &dword(Some(0))), dword(Some(1)));
        assert_eq!(target(GameMode, &dword(None)), dword(Some(1)));
        assert_eq!(target(BackgroundRecording, &dword(Some(1))), dword(Some(0)));
        assert_eq!(
            target(WindowedOptimizations, &token(Some("0"), true, false)),
            token(Some("1"), true, false)
        );
        assert_eq!(
            target(GpuPreference, &token(None, false, true)),
            token(Some("2"), false, true)
        );
        assert_eq!(
            target(PointerPrecision, &default_mouse()),
            no_acceleration()
        );
    }

    #[test]
    fn a_prior_of_the_wrong_kind_cannot_say_what_existed_so_the_target_assumes_everything_did() {
        // Undo then deletes nothing: it can only leave an empty value behind, never remove one that was the person's.
        let everything = token(Some("1"), true, true);
        assert_eq!(target(WindowedOptimizations, &dword(Some(1))), everything);
        assert_eq!(
            target(GpuPreference, &Saved::Protection { on: true }),
            token(Some("2"), true, true)
        );
    }

    #[test]
    fn the_plan_and_system_protection_are_not_targeted_here() {
        assert_eq!(target(PowerPlan, &plan()), plan());
        assert_eq!(
            target(SystemProtection, &Saved::Protection { on: false }),
            Saved::Protection { on: false }
        );
    }

    #[test]
    fn a_dword_setting_is_written_and_put_back() {
        for (tweak, key, name, start, planned) in [
            (GameMode, GAME_BAR_KEY, AUTO_GAME_MODE, 0, 1),
            (BackgroundRecording, GAME_DVR_KEY, HISTORICAL_CAPTURE, 1, 0),
        ] {
            let pc = FakeSystem::new();
            pc.set_dword(SID, key, name, start);
            let before = pc.dump();
            let prior = now(&pc, tweak);
            apply(&pc, tweak, &prior);
            assert_eq!(pc.dword(SID, key, name), Some(planned), "{tweak:?}");
            undo(&pc, tweak, &prior);
            assert_eq!(pc.dword(SID, key, name), Some(start), "{tweak:?}");
            assert_eq!(pc.dump(), before, "{tweak:?}");
        }
    }

    #[test]
    fn a_dword_that_was_not_there_is_deleted_by_the_restore_and_a_missing_one_is_no_error() {
        for (tweak, key, name) in [
            (GameMode, GAME_BAR_KEY, AUTO_GAME_MODE),
            (BackgroundRecording, GAME_DVR_KEY, HISTORICAL_CAPTURE),
        ] {
            let pc = FakeSystem::new();
            apply(&pc, tweak, &dword(None));
            assert!(pc.dword(SID, key, name).is_some(), "{tweak:?}");
            undo(&pc, tweak, &dword(None));
            assert_eq!(pc.read_dword(SID, key, name), Read::Absent, "{tweak:?}");
            // Again: nothing is there to delete, and that is the state asked for.
            undo(&pc, tweak, &dword(None));
            assert_eq!(pc.read_dword(SID, key, name), Read::Absent, "{tweak:?}");
        }
    }

    // Token settings.

    #[test]
    fn token_write_and_restore_leave_other_tokens() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        let prior = now(&pc, WindowedOptimizations);
        assert_eq!(prior, token(None, true, true));

        apply(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("VRROptimizeEnable=1;SwapEffectUpgradeEnable=1;")
        );
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("VRROptimizeEnable=1;")
        );
        assert_eq!(names(&pc), [DIRECTX_SETTINGS]);
    }

    #[test]
    fn a_token_that_was_there_goes_back_where_it_was_with_the_rest_untouched() {
        let pc = FakeSystem::new();
        pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "AutoHDREnable=2097;swapeffectupgradeenable=0;VRROptimizeEnable=1;",
        );
        let prior = now(&pc, WindowedOptimizations);
        assert_eq!(prior, token(Some("0"), true, true));
        apply(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("AutoHDREnable=2097;swapeffectupgradeenable=1;VRROptimizeEnable=1;")
        );
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("AutoHDREnable=2097;swapeffectupgradeenable=0;VRROptimizeEnable=1;")
        );
    }

    #[test]
    fn a_change_made_later_to_another_token_survives_the_restore() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=0;");
        let prior = now(&pc, WindowedOptimizations);
        apply(&pc, WindowedOptimizations, &prior);
        // The person turns on Auto HDR and VRR afterwards.
        pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "VRROptimizeEnable=1;SwapEffectUpgradeEnable=1;AutoHDREnable=2097;",
        );
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("VRROptimizeEnable=1;AutoHDREnable=2097;")
        );
    }

    #[test]
    fn restore_removes_what_kestrel_created() {
        let pc = FakeSystem::new();
        let before = pc.dump();
        let prior = now(&pc, WindowedOptimizations);
        assert_eq!(prior, token(None, false, false));

        apply(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("SwapEffectUpgradeEnable=1;")
        );
        undo(&pc, WindowedOptimizations, &prior);
        // The value is gone, and so is the key Kestrel made for it.
        assert_eq!(pc.value_names(SID, DIRECTX_KEY), Read::Absent);
        assert_eq!(pc.dump(), before);
    }

    #[test]
    fn a_key_that_existed_stays_when_its_value_goes() {
        let pc = FakeSystem::new();
        pc.add_key(SID, DIRECTX_KEY);
        let prior = now(&pc, WindowedOptimizations);
        assert_eq!(prior, token(None, false, true));
        apply(&pc, WindowedOptimizations, &prior);
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(pc.value_names(SID, DIRECTX_KEY), Read::Value(vec![]));
    }

    #[test]
    fn what_someone_else_put_there_since_stays_when_kestrels_token_goes() {
        let pc = FakeSystem::new();
        let prior = now(&pc, WindowedOptimizations);
        apply(&pc, WindowedOptimizations, &prior);
        // Windows adds its own token to the value Kestrel made, and another program puts a value in the key.
        pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "SwapEffectUpgradeEnable=1;VRROptimizeEnable=1;",
        );
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("VRROptimizeEnable=1;")
        );

        // And once more, with only the key's other value left to find: the key is not Kestrel's to delete.
        let pc = FakeSystem::new();
        apply(&pc, WindowedOptimizations, &prior);
        pc.set_string(SID, DIRECTX_KEY, OTHER_EXE, "GpuPreference=1;");
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(names(&pc), [OTHER_EXE]);
    }

    #[test]
    fn a_value_that_existed_empty_stays_empty() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "");
        let prior = now(&pc, WindowedOptimizations);
        assert_eq!(prior, token(None, true, true));
        apply(&pc, WindowedOptimizations, &prior);
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("")
        );
    }

    #[test]
    fn a_value_that_is_already_gone_is_no_error_and_nothing_is_made_to_take_its_place() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        let prior = now(&pc, WindowedOptimizations);
        apply(&pc, WindowedOptimizations, &prior);
        // The person deleted the whole value.
        pc.remove_value(SID, DIRECTX_KEY, DIRECTX_SETTINGS);
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(
            pc.read_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS),
            Read::Absent
        );

        // The same for a value Kestrel made, with the empty key it made left behind.
        let pc = FakeSystem::new();
        let prior = now(&pc, WindowedOptimizations);
        apply(&pc, WindowedOptimizations, &prior);
        pc.remove_value(SID, DIRECTX_KEY, DIRECTX_SETTINGS);
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(pc.value_names(SID, DIRECTX_KEY), Read::Absent);
    }

    #[test]
    fn restoring_twice_is_harmless() {
        let pc = FakeSystem::new();
        let before = pc.dump();
        let prior = now(&pc, WindowedOptimizations);
        apply(&pc, WindowedOptimizations, &prior);
        undo(&pc, WindowedOptimizations, &prior);
        undo(&pc, WindowedOptimizations, &prior);
        assert_eq!(pc.dump(), before);
    }

    #[test]
    fn odd_strings_survive_apply_and_undo_part_for_part() {
        // The same inputs the token merge is tested with, now through the setting itself.
        for before in [
            "",
            ";",
            ";;;",
            "Foo",
            "Foo;;A=b=c;Bare",
            "=;==;=x",
            "a=1;A=2;b=3",
            "SwapEffectUpgradeEnable=0;x=1;swapeffectupgradeenable=5",
            "Sp ace=1 ; tab\t=2",
        ] {
            let pc = FakeSystem::new();
            pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, before);
            let prior = now(&pc, WindowedOptimizations);
            apply(&pc, WindowedOptimizations, &prior);
            assert_eq!(
                now(&pc, WindowedOptimizations),
                token(Some("1"), true, true),
                "{before:?}"
            );
            undo(&pc, WindowedOptimizations, &prior);
            let after = pc
                .string(SID, DIRECTX_KEY, DIRECTX_SETTINGS)
                .expect("the value is still there");
            // Kestrel's token is back as it was and every other part came through as it went in.
            assert_eq!(
                tokens::get(&after, SWAP_EFFECT_TOKEN),
                tokens::get(before, SWAP_EFFECT_TOKEN),
                "{before:?}"
            );
            assert_eq!(
                tokens::remove(&after, SWAP_EFFECT_TOKEN),
                tokens::remove(before, SWAP_EFFECT_TOKEN),
                "{before:?}"
            );
        }
    }

    #[test]
    fn a_token_is_never_written_over_a_value_that_cannot_be_read() {
        for tweak in [WindowedOptimizations, GpuPreference] {
            // The wrong type, and a read that is refused.
            let wrong_type = with_a_wrong_type(tweak);
            let refused = FakeSystem::new();
            refused.fail_read(if tweak == GpuPreference {
                EXE
            } else {
                DIRECTX_SETTINGS
            });
            for pc in [wrong_type, refused] {
                let before = pc.dump();
                let prior = token(None, true, true);
                let planned = target(tweak, &prior);
                assert!(
                    write(&pc, tweak, SID, name_of(tweak), &planned, false).is_err(),
                    "{tweak:?}"
                );
                assert!(
                    restore(&pc, tweak, SID, name_of(tweak), &prior, false).is_err(),
                    "{tweak:?}"
                );
                assert_eq!(pc.dump(), before, "{tweak:?}");
                assert!(pc.writes().is_empty(), "{tweak:?}");
            }
        }
    }

    #[test]
    fn a_refused_write_is_an_error_and_changes_nothing() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        let prior = now(&pc, WindowedOptimizations);
        let planned = target(WindowedOptimizations, &prior);

        pc.fail_write(DIRECTX_SETTINGS);
        let before = pc.dump();
        assert_eq!(
            kind(write(
                &pc,
                WindowedOptimizations,
                SID,
                None,
                &planned,
                false
            )),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(pc.dump(), before);

        // And a restore that has something to take out.
        pc.clear_failures();
        apply(&pc, WindowedOptimizations, &prior);
        pc.fail_write(DIRECTX_SETTINGS);
        let applied = pc.dump();
        assert_eq!(
            kind(restore(
                &pc,
                WindowedOptimizations,
                SID,
                None,
                &prior,
                false
            )),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(pc.dump(), applied);
    }

    #[test]
    fn a_restore_with_nothing_to_change_writes_nothing() {
        // Kestrel's token is not in the string (the person has already put it back), so the person's string is left
        // exactly as it is, down to the way it is spelled.
        let pc = FakeSystem::new();
        pc.set_string(
            SID,
            DIRECTX_KEY,
            DIRECTX_SETTINGS,
            "Foo;VRROptimizeEnable=1",
        );
        let prior = token(None, true, true);
        restore(&pc, WindowedOptimizations, SID, None, &prior, false).unwrap();
        assert!(pc.writes().is_empty());
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, DIRECTX_SETTINGS).as_deref(),
            Some("Foo;VRROptimizeEnable=1")
        );
    }

    #[test]
    fn a_token_that_would_smuggle_in_another_is_refused() {
        // The journal is a file anyone with the right to change it can edit: a value with a `;` in it would be written as
        // two tokens, and a control character has no place in either.
        for text in ["1;AutoHDREnable=1", "1\n", "\u{0}"] {
            let pc = FakeSystem::new();
            pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
            let before = pc.dump();
            let hostile = token(Some(text), true, true);
            assert_eq!(
                kind(write(
                    &pc,
                    WindowedOptimizations,
                    SID,
                    None,
                    &hostile,
                    false
                )),
                io::ErrorKind::InvalidData,
                "{text:?}"
            );
            assert_eq!(
                kind(restore(
                    &pc,
                    WindowedOptimizations,
                    SID,
                    None,
                    &hostile,
                    false
                )),
                io::ErrorKind::InvalidData,
                "{text:?}"
            );
            assert_eq!(pc.dump(), before, "{text:?}");
        }
    }

    // The game's graphics preference.

    #[test]
    fn gpu_restore_deletes_rather_than_writing_zero() {
        let pc = FakeSystem::new();
        // The key is there because of other games, but VALORANT has no value yet.
        pc.set_string(SID, DIRECTX_KEY, OTHER_EXE, "GpuPreference=1;");
        let before = pc.dump();
        let prior = now(&pc, GpuPreference);
        assert_eq!(prior, token(None, false, true));

        apply(&pc, GpuPreference, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, EXE).as_deref(),
            Some("GpuPreference=2;")
        );
        undo(&pc, GpuPreference, &prior);
        assert_eq!(names(&pc), [OTHER_EXE]);
        assert_eq!(pc.dump(), before);
        for name in names(&pc) {
            let text = pc.string(SID, DIRECTX_KEY, &name).unwrap();
            assert!(!text.contains("GpuPreference=0"), "{name}: {text}");
        }
    }

    #[test]
    fn gpu_restore_removes_the_key_kestrel_made_and_keeps_one_that_was_there() {
        // No key at all: Kestrel makes the key and the game's value, and takes both out again.
        let pc = FakeSystem::new();
        let before = pc.dump();
        let prior = now(&pc, GpuPreference);
        assert_eq!(prior, token(None, false, false));
        apply(&pc, GpuPreference, &prior);
        assert_eq!(names(&pc), [EXE]);
        undo(&pc, GpuPreference, &prior);
        assert_eq!(pc.value_names(SID, DIRECTX_KEY), Read::Absent);
        assert_eq!(pc.dump(), before);

        // An empty key that was already there is not Kestrel's to delete.
        let pc = FakeSystem::new();
        pc.add_key(SID, DIRECTX_KEY);
        let before = pc.dump();
        let prior = now(&pc, GpuPreference);
        assert_eq!(prior, token(None, false, true));
        apply(&pc, GpuPreference, &prior);
        undo(&pc, GpuPreference, &prior);
        assert_eq!(pc.value_names(SID, DIRECTX_KEY), Read::Value(vec![]));
        assert_eq!(pc.dump(), before);
    }

    #[test]
    fn a_gpu_value_that_held_other_tokens_keeps_them() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE, "SwapEffectUpgradeEnable=1;");
        let prior = now(&pc, GpuPreference);
        assert_eq!(prior, token(None, true, true));
        apply(&pc, GpuPreference, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, EXE).as_deref(),
            Some("SwapEffectUpgradeEnable=1;GpuPreference=2;")
        );
        undo(&pc, GpuPreference, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, EXE).as_deref(),
            Some("SwapEffectUpgradeEnable=1;")
        );
    }

    #[test]
    fn a_gpu_choice_the_person_made_before_comes_back() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE, "GpuPreference=1;");
        let prior = now(&pc, GpuPreference);
        assert_eq!(prior, token(Some("1"), true, true));
        apply(&pc, GpuPreference, &prior);
        undo(&pc, GpuPreference, &prior);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, EXE).as_deref(),
            Some("GpuPreference=1;")
        );
    }

    #[test]
    fn gpu_value_name_reuses_the_existing_spelling() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE_LOWER, "GpuPreference=1;");
        pc.set_string(SID, DIRECTX_KEY, OTHER_EXE, "GpuPreference=1;");

        let name = game_value(&pc);
        assert_eq!(name, EXE_LOWER);

        let prior = now_at(&pc, &name);
        assert_eq!(prior, token(Some("1"), true, true));
        write(
            &pc,
            GpuPreference,
            SID,
            Some(&name),
            &target(GpuPreference, &prior),
            false,
        )
        .unwrap();
        // One value for the game, in the spelling it already had.
        assert_eq!(names(&pc), sorted([EXE_LOWER, OTHER_EXE]));
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, EXE_LOWER).as_deref(),
            Some("GpuPreference=2;")
        );
        restore(&pc, GpuPreference, SID, Some(&name), &prior, false).unwrap();
        assert_eq!(names(&pc), sorted([EXE_LOWER, OTHER_EXE]));
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, EXE_LOWER).as_deref(),
            Some("GpuPreference=1;")
        );
    }

    #[test]
    fn gpu_value_name_reuses_a_value_written_with_the_other_slashes() {
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE_SLASHES, "GpuPreference=0;");
        let name = game_value(&pc);
        assert_eq!(name, EXE_SLASHES);
        let prior = now_at(&pc, &name);
        write(
            &pc,
            GpuPreference,
            SID,
            Some(&name),
            &target(GpuPreference, &prior),
            false,
        )
        .unwrap();
        // The value the person's Windows already had was updated; a second one was not added.
        assert_eq!(names(&pc), [EXE_SLASHES]);
        assert_eq!(
            pc.string(SID, DIRECTX_KEY, EXE_SLASHES).as_deref(),
            Some("GpuPreference=2;")
        );
    }

    #[test]
    fn gpu_value_name_prefers_the_spelling_windows_itself_uses() {
        // Both spellings are in the key: the one with backslashes is the one Windows looks up.
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, EXE_SLASHES, "GpuPreference=1;");
        pc.set_string(SID, DIRECTX_KEY, EXE_LOWER, "GpuPreference=2;");
        assert_eq!(
            gpu_value_name(&pc, SID, EXE),
            Read::Value(EXE_LOWER.to_string())
        );
    }

    #[test]
    fn gpu_value_name_is_the_exe_itself_when_nothing_is_there() {
        let pc = FakeSystem::new();
        // No key at all.
        assert_eq!(gpu_value_name(&pc, SID, EXE), Read::Value(EXE.to_string()));
        // A key with other games and other values only.
        pc.set_string(SID, DIRECTX_KEY, OTHER_EXE, "GpuPreference=1;");
        pc.set_string(SID, DIRECTX_KEY, DIRECTX_SETTINGS, "VRROptimizeEnable=1;");
        pc.set_string(SID, DIRECTX_KEY, &format!("{EXE}.bak"), "GpuPreference=1;");
        assert_eq!(gpu_value_name(&pc, SID, EXE), Read::Value(EXE.to_string()));
        // Another account's values are not this account's.
        let pc = FakeSystem::new();
        pc.set_string(OTHER_SID, DIRECTX_KEY, EXE_LOWER, "GpuPreference=1;");
        assert_eq!(gpu_value_name(&pc, SID, EXE), Read::Value(EXE.to_string()));
    }

    #[test]
    fn gpu_value_name_fails_when_the_key_cannot_be_listed() {
        let pc = FakeSystem::new();
        pc.fail_read(DIRECTX_KEY);
        assert!(matches!(
            gpu_value_name(&pc, SID, EXE),
            Read::Failed(why) if !why.is_empty()
        ));
    }

    #[test]
    fn a_non_ascii_path_is_matched_the_way_the_registry_matches_names() {
        let exe = r"C:\Jogos\Veríssimo\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe";
        let shouted = exe.to_uppercase();
        let pc = FakeSystem::new();
        pc.set_string(SID, DIRECTX_KEY, &shouted, "GpuPreference=1;");
        assert_eq!(gpu_value_name(&pc, SID, exe), Read::Value(shouted));
    }

    // The mouse.

    #[test]
    fn mouse_write_is_registry_only_when_accounts_differ() {
        // Kestrel runs as another account: SPIF_UPDATEINIFILE would write that account's profile, so the person's
        // strings are written by hand, and the mouse in force is left for the next sign-in.
        let pc = differing();
        assert!(pc.account().differs());
        write(
            &pc,
            PointerPrecision,
            SID,
            None,
            &target(PointerPrecision, &default_mouse()),
            true,
        )
        .unwrap();
        for name in [MOUSE_SPEED, MOUSE_THRESHOLD_1, MOUSE_THRESHOLD_2] {
            assert_eq!(
                pc.string(SID, MOUSE_KEY, name).as_deref(),
                Some("0"),
                "{name}"
            );
            assert_eq!(pc.string(OTHER_SID, MOUSE_KEY, name), None, "{name}");
        }
        assert_eq!(pc.mouse_live(), Read::Value([6, 10, 1]));
        assert!(
            pc.writes().iter().all(|line| !line.starts_with("mouse")),
            "{:?}",
            pc.writes()
        );

        // The same account: Windows saves the strings itself, in the same call that changes the mouse.
        let pc = FakeSystem::new();
        write(
            &pc,
            PointerPrecision,
            SID,
            None,
            &target(PointerPrecision, &default_mouse()),
            false,
        )
        .unwrap();
        assert_eq!(pc.writes(), ["mouse [0, 0, 0] persist"]);
        assert_eq!(pc.mouse_live(), Read::Value([0, 0, 0]));
        assert_eq!(now(&pc, PointerPrecision), no_acceleration());
    }

    #[test]
    fn mouse_restore_puts_strings_and_live_back() {
        let pc = FakeSystem::new();
        let before = pc.dump();
        let prior = now(&pc, PointerPrecision);
        assert_eq!(prior, default_mouse());
        apply(&pc, PointerPrecision, &prior);
        assert_eq!(pc.mouse_live(), Read::Value([0, 0, 0]));
        undo(&pc, PointerPrecision, &prior);
        assert_eq!(pc.mouse_live(), Read::Value([6, 10, 1]));
        for (name, value) in [
            (MOUSE_SPEED, "1"),
            (MOUSE_THRESHOLD_1, "6"),
            (MOUSE_THRESHOLD_2, "10"),
        ] {
            assert_eq!(pc.string(SID, MOUSE_KEY, name).as_deref(), Some(value));
        }
        assert_eq!(pc.dump(), before);
    }

    #[test]
    fn being_back_takes_the_mouse_in_force_too_when_kestrel_changes_it() {
        let found = default_mouse();
        assert!(is_back_at(
            PointerPrecision,
            &default_mouse(),
            &found,
            false
        ));
        // Strings that weren't there mean the same as their defaults, as ever.
        let no_strings = mouse(None, None, None, [6, 10, 1]);
        assert!(is_back_at(PointerPrecision, &no_strings, &found, false));

        // The strings are back and Kestrel's mouse is still in force: what they mean can't tell, and this can.
        let still_in_force = mouse(Some("1"), Some("6"), Some("10"), [0, 0, 0]);
        assert!(same_meaning(PointerPrecision, &still_in_force, &found));
        assert!(!is_back_at(
            PointerPrecision,
            &still_in_force,
            &found,
            false
        ));
        // Or some other one is.
        let another = mouse(Some("1"), Some("6"), Some("10"), [1, 2, 3]);
        assert!(!is_back_at(PointerPrecision, &another, &found, false));
        // When the accounts differ Kestrel never touched the mouse in force, so it has no say.
        assert!(is_back_at(PointerPrecision, &still_in_force, &found, true));

        // Strings that aren't back never are, whatever is in force.
        for accounts_differ in [false, true] {
            let kestrels = no_acceleration();
            assert!(!is_back_at(
                PointerPrecision,
                &kestrels,
                &found,
                accounts_differ
            ));
            let same_in_force = mouse(Some("0"), Some("0"), Some("0"), [6, 10, 1]);
            assert!(!is_back_at(
                PointerPrecision,
                &same_in_force,
                &found,
                accounts_differ
            ));
        }
    }

    #[test]
    fn being_back_is_what_it_means_for_every_other_setting() {
        for accounts_differ in [false, true] {
            // Absent is that setting's default, and whether a value or a key existed has no say.
            let (absent, one, zero) = (dword(None), dword(Some(1)), dword(Some(0)));
            assert!(is_back_at(GameMode, &absent, &one, accounts_differ));
            assert!(!is_back_at(GameMode, &zero, &one, accounts_differ));
            assert!(is_back_at(
                BackgroundRecording,
                &absent,
                &zero,
                accounts_differ
            ));
            let (later, first) = (token(None, true, true), token(None, false, false));
            assert!(is_back_at(
                WindowedOptimizations,
                &later,
                &first,
                accounts_differ
            ));
            assert!(is_back_at(
                GpuPreference,
                &token(Some("0"), true, true),
                &first,
                accounts_differ
            ));
            assert!(!is_back_at(
                GpuPreference,
                &token(Some("2"), true, true),
                &first,
                accounts_differ
            ));
            // States of different kinds aren't.
            assert!(!is_back_at(GameMode, &plan(), &one, accounts_differ));
            assert!(!is_back_at(
                PointerPrecision,
                &one,
                &default_mouse(),
                accounts_differ
            ));
        }
    }

    #[test]
    fn mouse_restore_sets_the_mouse_in_force_before_the_strings() {
        // The strings say whether Kestrel's change still holds, so they go last: stopping anywhere before them leaves a
        // setting that reads as Kestrel's, and an undo that can be tried again.
        let pc = FakeSystem::new();
        let prior = now(&pc, PointerPrecision);
        apply(&pc, PointerPrecision, &prior);
        let logged = pc.writes().len();

        undo(&pc, PointerPrecision, &prior);

        assert_eq!(pc.writes()[logged].as_str(), "mouse [6, 10, 1]");
        assert!(pc.writes()[logged + 1..]
            .iter()
            .all(|line| line.starts_with("string")));
        assert_eq!(pc.writes().len(), logged + 4);

        // Refused, and nothing has moved.
        let pc = FakeSystem::new();
        let prior = now(&pc, PointerPrecision);
        apply(&pc, PointerPrecision, &prior);
        let held = pc.dump();
        pc.fail_write(MOUSE_LIVE);

        let refused = restore(&pc, PointerPrecision, SID, None, &prior, false).unwrap_err();

        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(pc.dump(), held);
    }

    #[test]
    fn mouse_restore_when_accounts_differ_puts_the_strings_back_and_leaves_the_mouse_in_force() {
        let pc = differing();
        let before = pc.dump();
        let prior = now(&pc, PointerPrecision);
        apply(&pc, PointerPrecision, &prior);
        let logged = pc.writes().len();
        undo(&pc, PointerPrecision, &prior);
        assert_eq!(now(&pc, PointerPrecision), default_mouse());
        assert_eq!(pc.dump(), before);
        // Nothing but the strings was written, and no call changed the mouse in force.
        assert!(pc.writes()[logged..]
            .iter()
            .all(|line| line.starts_with("string")));
    }

    #[test]
    fn mouse_restore_deletes_the_strings_that_were_not_there() {
        let pc = FakeSystem::new();
        pc.set_string(SID, MOUSE_KEY, "DoubleClickSpeed", "500");
        for name in [MOUSE_SPEED, MOUSE_THRESHOLD_1, MOUSE_THRESHOLD_2] {
            pc.remove_value(SID, MOUSE_KEY, name);
        }
        let before = pc.dump();
        let prior = now(&pc, PointerPrecision);
        assert_eq!(prior, mouse(None, None, None, [6, 10, 1]));
        apply(&pc, PointerPrecision, &prior);
        assert_eq!(now(&pc, PointerPrecision), no_acceleration());
        undo(&pc, PointerPrecision, &prior);
        assert_eq!(
            pc.value_names(SID, MOUSE_KEY),
            Read::Value(vec!["DoubleClickSpeed".to_string()])
        );
        assert_eq!(pc.dump(), before);
    }

    #[test]
    fn mouse_strings_that_are_not_the_usual_ones_come_back_exactly() {
        let prior = mouse(Some("2"), Some("4"), Some("12"), [4, 12, 2]);
        for pc in [FakeSystem::new(), differing()] {
            pc.set_string(SID, MOUSE_KEY, MOUSE_SPEED, "2");
            pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_1, "4");
            pc.set_string(SID, MOUSE_KEY, MOUSE_THRESHOLD_2, "12");
            pc.set_mouse([4, 12, 2]);
            let before = pc.dump();
            assert_eq!(now(&pc, PointerPrecision), prior);
            apply(&pc, PointerPrecision, &prior);
            undo(&pc, PointerPrecision, &prior);
            assert_eq!(pc.dump(), before);
        }
    }

    // Writing what doesn't belong.

    #[test]
    fn a_saved_state_that_does_not_fit_its_tweak_is_refused_and_nothing_is_written() {
        let states = [
            dword(Some(1)),
            token(Some("1"), true, true),
            default_mouse(),
            plan(),
            Saved::Protection { on: true },
        ];
        let fits = |tweak: TweakId, saved: &Saved| {
            matches!(
                (tweak, saved),
                (GameMode | BackgroundRecording, Saved::Dword { .. })
                    | (WindowedOptimizations | GpuPreference, Saved::Token { .. })
                    | (PointerPrecision, Saved::Mouse { .. })
            )
        };
        let mut refused = 0;
        for tweak in [
            PowerPlan,
            GameMode,
            BackgroundRecording,
            WindowedOptimizations,
            GpuPreference,
            PointerPrecision,
            SystemProtection,
        ] {
            for saved in &states {
                if fits(tweak, saved) {
                    continue;
                }
                let pc = FakeSystem::new();
                arranged(&pc);
                let before = pc.dump();
                for differs in [false, true] {
                    assert_eq!(
                        kind(write(&pc, tweak, SID, name_of(tweak), saved, differs)),
                        io::ErrorKind::InvalidData,
                        "write {tweak:?} {saved:?}"
                    );
                    assert_eq!(
                        kind(restore(&pc, tweak, SID, name_of(tweak), saved, differs)),
                        io::ErrorKind::InvalidData,
                        "restore {tweak:?} {saved:?}"
                    );
                }
                assert_eq!(pc.dump(), before, "{tweak:?} {saved:?}");
                assert!(pc.writes().is_empty(), "{tweak:?} {saved:?}");
                refused += 1;
            }
        }
        // 7 tweaks x 5 states, less the 5 pairs that fit.
        assert_eq!(refused, 30);
    }

    // Settings that go back exactly as they were.

    #[test]
    fn every_setting_goes_back_exactly_as_it_was() {
        for differs in [false, true] {
            for start in [arranged, arranged_without_the_directx_key] {
                let pc = if differs {
                    differing()
                } else {
                    FakeSystem::new()
                };
                start(&pc);
                let before = pc.dump();

                let mut priors = Vec::new();
                for tweak in SETTINGS {
                    let prior = now(&pc, tweak);
                    apply(&pc, tweak, &prior);
                    let planned = target(tweak, &prior);
                    assert!(
                        same_meaning(tweak, &now(&pc, tweak), &planned),
                        "{tweak:?} (accounts differ: {differs})"
                    );
                    priors.push((tweak, prior));
                }
                assert_ne!(pc.dump(), before);

                // Newest first, as Revert everything does.
                for (tweak, prior) in priors.iter().rev() {
                    undo(&pc, *tweak, prior);
                    assert!(
                        same_meaning(*tweak, &now(&pc, *tweak), prior),
                        "{tweak:?} (accounts differ: {differs})"
                    );
                }
                assert_eq!(pc.dump(), before, "accounts differ: {differs}");
            }
        }
    }

    #[test]
    fn nothing_but_the_settings_own_values_is_ever_written() {
        let allowed = [
            AUTO_GAME_MODE,
            HISTORICAL_CAPTURE,
            DIRECTX_SETTINGS,
            EXE,
            MOUSE_SPEED,
            MOUSE_THRESHOLD_1,
            MOUSE_THRESHOLD_2,
            "mouse [",
        ];
        for differs in [false, true] {
            let pc = if differs {
                differing()
            } else {
                FakeSystem::new()
            };
            arranged(&pc);
            let mut priors = Vec::new();
            for tweak in SETTINGS {
                let prior = now(&pc, tweak);
                apply(&pc, tweak, &prior);
                priors.push((tweak, prior));
            }
            for (tweak, prior) in priors.iter().rev() {
                undo(&pc, *tweak, prior);
            }
            let lines = pc.writes();
            assert!(!lines.is_empty());
            for line in &lines {
                assert!(
                    allowed.iter().any(|name| line.contains(name)),
                    "an unexpected write: {line}"
                );
                for never in [
                    APP_CAPTURE,
                    "GameDVR_Enabled",
                    ALLOW_GAME_DVR,
                    ALLOW_AUTO_GAME_MODE,
                    "Policies",
                    "SystemRestorePointCreationFrequency",
                ] {
                    assert!(!line.contains(never), "{line}");
                }
            }
        }
    }

    // Comparing.

    #[test]
    fn absent_means_the_default_when_comparing() {
        assert!(same_meaning(GameMode, &dword(None), &dword(Some(1))));
        assert!(same_meaning(GameMode, &dword(Some(1)), &dword(None)));
        assert!(same_meaning(
            BackgroundRecording,
            &dword(None),
            &dword(Some(0))
        ));
        assert!(same_meaning(
            GpuPreference,
            &token(None, false, false),
            &token(Some("0"), true, true)
        ));
        assert!(same_meaning(
            GpuPreference,
            &token(Some("0"), true, true),
            &token(None, false, false)
        ));
        // A windowed token has no default: absent equals only absent.
        assert!(!same_meaning(
            WindowedOptimizations,
            &token(None, false, false),
            &token(Some("0"), true, true)
        ));
        assert!(!same_meaning(
            WindowedOptimizations,
            &token(Some("0"), true, true),
            &token(None, false, false)
        ));
        assert!(same_meaning(
            WindowedOptimizations,
            &token(None, false, false),
            &token(None, true, true)
        ));

        // The other default is not the default.
        assert!(!same_meaning(GameMode, &dword(None), &dword(Some(0))));
        assert!(!same_meaning(
            BackgroundRecording,
            &dword(None),
            &dword(Some(1))
        ));
        assert!(!same_meaning(
            GpuPreference,
            &token(None, false, false),
            &token(Some("2"), true, true)
        ));
    }

    #[test]
    fn absent_mouse_strings_mean_one_six_and_ten() {
        assert!(same_meaning(
            PointerPrecision,
            &mouse(None, None, None, [0, 0, 0]),
            &default_mouse()
        ));
        // Each string has a default of its own.
        assert!(same_meaning(
            PointerPrecision,
            &mouse(Some("1"), None, Some("10"), [6, 10, 1]),
            &default_mouse()
        ));
        assert!(same_meaning(
            PointerPrecision,
            &mouse(None, Some("6"), None, [6, 10, 1]),
            &default_mouse()
        ));
        for changed in [
            mouse(Some("0"), None, None, [6, 10, 1]),
            mouse(None, Some("0"), None, [6, 10, 1]),
            mouse(None, None, Some("0"), [6, 10, 1]),
            mouse(Some("2"), Some("6"), Some("10"), [6, 10, 1]),
        ] {
            assert!(!same_meaning(PointerPrecision, &changed, &default_mouse()));
        }
        assert!(!same_meaning(
            PointerPrecision,
            &no_acceleration(),
            &default_mouse()
        ));
        assert!(same_meaning(
            PointerPrecision,
            &no_acceleration(),
            &no_acceleration()
        ));
    }

    #[test]
    fn comparing_ignores_the_mouse_in_force_and_what_existed_before() {
        assert!(same_meaning(
            PointerPrecision,
            &mouse(Some("0"), Some("0"), Some("0"), [6, 10, 1]),
            &no_acceleration()
        ));
        assert!(same_meaning(
            WindowedOptimizations,
            &token(Some("1"), true, true),
            &token(Some("1"), false, false)
        ));
        assert!(same_meaning(
            GpuPreference,
            &token(Some("2"), true, false),
            &token(Some("2"), false, true)
        ));
    }

    #[test]
    fn values_that_differ_do_not_mean_the_same() {
        assert!(!same_meaning(GameMode, &dword(Some(0)), &dword(Some(1))));
        assert!(!same_meaning(
            BackgroundRecording,
            &dword(Some(0)),
            &dword(Some(1))
        ));
        // A value Windows doesn't write is not the one Kestrel wrote.
        assert!(!same_meaning(GameMode, &dword(Some(2)), &dword(Some(1))));
        assert!(!same_meaning(
            WindowedOptimizations,
            &token(Some("0"), true, true),
            &token(Some("1"), true, true)
        ));
        assert!(!same_meaning(
            GpuPreference,
            &token(Some("1"), true, true),
            &token(Some("2"), true, true)
        ));
        assert!(same_meaning(GameMode, &dword(Some(0)), &dword(Some(0))));
    }

    #[test]
    fn states_of_different_kinds_never_mean_the_same() {
        let states = [
            dword(Some(1)),
            token(Some("1"), true, true),
            default_mouse(),
            plan(),
            Saved::Protection { on: true },
        ];
        for tweak in SETTINGS {
            for (at, a) in states.iter().enumerate() {
                for (other, b) in states.iter().enumerate() {
                    assert_eq!(
                        same_meaning(tweak, a, b),
                        at == other,
                        "{tweak:?}: {a:?} and {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_plan_is_the_same_plan_by_its_guid_and_protection_by_its_state() {
        let named = |guid: &str, name: &str| Saved::Plan {
            guid: guid.to_string(),
            name: name.to_string(),
        };
        assert!(same_meaning(
            PowerPlan,
            &named(KESTREL_PLAN, "Kestrel"),
            &named(&KESTREL_PLAN.to_uppercase(), "Renamed")
        ));
        assert!(!same_meaning(
            PowerPlan,
            &named(KESTREL_PLAN, "Kestrel"),
            &named("381b4222-f694-41f0-9685-ff5bb260df2e", "Kestrel")
        ));
        assert!(same_meaning(
            SystemProtection,
            &Saved::Protection { on: true },
            &Saved::Protection { on: true }
        ));
        assert!(!same_meaning(
            SystemProtection,
            &Saved::Protection { on: true },
            &Saved::Protection { on: false }
        ));
    }

    // Saying it in words.

    #[test]
    fn describe_says_every_state_in_words() {
        for (tweak, saved, words) in [
            (GameMode, dword(Some(0)), "off"),
            (GameMode, dword(Some(1)), "on"),
            (GameMode, dword(None), "on"),
            (BackgroundRecording, dword(Some(1)), "on"),
            (BackgroundRecording, dword(Some(0)), "off"),
            (BackgroundRecording, dword(None), "off"),
            (WindowedOptimizations, token(Some("1"), true, true), "on"),
            (WindowedOptimizations, token(Some("0"), true, true), "off"),
            (WindowedOptimizations, token(None, false, false), "not set"),
            (WindowedOptimizations, token(Some("7"), true, true), "7"),
            (
                GpuPreference,
                token(Some("2"), true, true),
                "High performance",
            ),
            (GpuPreference, token(Some("1"), true, true), "Power saving"),
            (
                GpuPreference,
                token(Some("0"), true, true),
                "Windows decides",
            ),
            (GpuPreference, token(None, false, false), "Windows decides"),
            (GpuPreference, token(Some("7"), true, true), "7"),
            (PointerPrecision, default_mouse(), "on"),
            (PointerPrecision, no_acceleration(), "off"),
            (PointerPrecision, mouse(None, None, None, [0, 0, 0]), "on"),
            (
                PointerPrecision,
                mouse(Some("2"), None, None, [0, 0, 2]),
                "on",
            ),
            (PowerPlan, plan(), "Kestrel"),
            (SystemProtection, Saved::Protection { on: true }, "on"),
            (SystemProtection, Saved::Protection { on: false }, "off"),
        ] {
            assert_eq!(describe(tweak, &saved), words, "{tweak:?} {saved:?}");
        }
    }

    #[test]
    fn describe_has_words_for_a_state_that_does_not_belong_to_its_tweak() {
        // A hand-written journal can hold anything, and the report still has to say something.
        assert_eq!(describe(GameMode, &plan()), "Kestrel");
        assert_eq!(describe(GameMode, &Saved::Protection { on: true }), "on");
        assert_eq!(describe(GameMode, &default_mouse()), "on");
        assert_eq!(describe(GameMode, &token(Some("2"), true, true)), "2");
        assert_eq!(describe(GameMode, &token(None, true, true)), "not set");
        assert_eq!(describe(GpuPreference, &dword(Some(5))), "5");
        assert_eq!(describe(GpuPreference, &dword(None)), "not set");
        let unnamed = Saved::Plan {
            guid: KESTREL_PLAN.to_string(),
            name: " ".to_string(),
        };
        assert_eq!(describe(PowerPlan, &unnamed), KESTREL_PLAN);
    }
}
