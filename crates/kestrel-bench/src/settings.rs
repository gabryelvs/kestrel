//! VALORANT's video settings, read from its config files and never written (decision 43). Keys confirmed on the
//! owner's PC: docs/superpowers/notes/2026-10-12-valorant-settings-keys.md. GameUserSettings.ini lists every
//! setting, so a missing key there is unknown. RiotUserSettings.ini lists only settings changed from their default,
//! so a missing key there means "default", never an error.
use crate::model::ValorantSettings;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// RiotUserSettings.ini settings kept in `quality` under their own name.
const RIOT_QUALITY: [&str; 7] = [
    "AnisotropicFiltering",
    "AntiAliasing",
    "DetailQuality",
    "MaterialQuality",
    "TextureQuality",
    "UIQuality",
    "ShadowsEnabled",
];

/// Settings from the text of GameUserSettings.ini and RiotUserSettings.ini (either may be empty).
pub fn parse(game_user_settings: &str, riot_user_settings: &str) -> ValorantSettings {
    let mut s = ValorantSettings::default();
    let (mut width, mut height) = (None, None);
    for (key, value) in pairs(game_user_settings) {
        match key {
            "ResolutionSizeX" => width = value.parse::<u32>().ok(),
            "ResolutionSizeY" => height = value.parse::<u32>().ok(),
            "FullscreenMode" => s.fullscreen_mode = value.parse().ok(),
            "bUseVSync" => s.vsync = boolean(value),
            "FrameRateLimit" => s.frame_rate_limit = value.parse().ok(),
            _ if key.starts_with("sg.") => {
                s.quality.insert(key.to_string(), value.to_string());
            }
            _ => {}
        }
    }
    if let (Some(w), Some(h)) = (width, height) {
        s.resolution = Some([w, h]);
    }
    s.riot_settings_read = !riot_user_settings.trim().is_empty();
    for (key, value) in pairs(riot_user_settings) {
        // "EAresIntSettingName::NvidiaReflexLowLatencySetting" → "NvidiaReflexLowLatencySetting"
        let name = key.rsplit("::").next().unwrap_or(key);
        let lower = name.to_ascii_lowercase();
        match name {
            "NvidiaReflexLowLatencySetting" => s.reflex = value.parse().ok(),
            "MultithreadedRenderingEnabled" => s.multithreaded_rendering = boolean(value),
            name if RIOT_QUALITY.contains(&name) => {
                s.quality.insert(name.to_string(), value.to_string());
            }
            // The in-game FPS counter: its name says "FrameRate" but it limits nothing.
            "PlayerPerfShowFrameRate" => {}
            _ if lower.contains("limit")
                || lower.contains("maxfps")
                || lower.contains("framerate") =>
            {
                s.limits.insert(name.to_string(), value.to_string());
            }
            _ => {}
        }
    }
    s
}

/// The GameUserSettings.ini and RiotUserSettings.ini to read under `config_root`
/// (`%LOCALAPPDATA%\VALORANT\Saved\Config`). There's one folder per Riot account, and both files come from one of
/// them: the folder holding the newest RiotUserSettings.ini supplies its newest GameUserSettings.ini, at any depth.
/// The newest GameUserSettings.ini anywhere is the fallback, used when there is no RiotUserSettings.ini, when that
/// file sits directly in `config_root`, or when its folder has no GameUserSettings.ini.
pub fn find_files(config_root: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut files = Vec::new();
    collect_files(config_root, 0, &mut files);
    let newest = |name: &str, within: Option<&Path>| {
        files
            .iter()
            .filter(|(path, _)| {
                path.file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .filter(|(path, _)| within.is_none_or(|dir| path.starts_with(dir)))
            .max_by_key(|(_, modified)| *modified)
            .map(|(path, _)| path.clone())
    };
    let riot = newest("RiotUserSettings.ini", None);
    let account = riot
        .as_deref()
        .and_then(|path| account_folder(config_root, path));
    let game = account
        .as_deref()
        .and_then(|dir| newest("GameUserSettings.ini", Some(dir)))
        .or_else(|| newest("GameUserSettings.ini", None));
    (game, riot)
}

/// The folder directly under `config_root` that holds `file`; None when `file` sits directly in `config_root`.
fn account_folder(config_root: &Path, file: &Path) -> Option<PathBuf> {
    let mut parts = file.strip_prefix(config_root).ok()?.components();
    let folder = parts.next()?;
    // The file's own name is the last component, so a folder must come between it and the root.
    parts.next()?;
    Some(config_root.join(folder.as_os_str()))
}

/// The settings in use, or None when VALORANT's settings files aren't there.
pub fn read(config_root: &Path) -> Option<ValorantSettings> {
    let (game, riot) = find_files(config_root);
    if game.is_none() && riot.is_none() {
        return None;
    }
    let text = |path: Option<PathBuf>| {
        path.and_then(|p| std::fs::read(p).ok())
            .map(|b| decode(&b))
            .unwrap_or_default()
    };
    Some(parse(&text(game), &text(riot)))
}

/// Names of the settings that differ between `a` and `b`, in the order the UI shows them: the named settings, then
/// quality keys, then limit keys, each sorted by key. A setting counts when both sides know it and their values
/// differ. GameUserSettings.ini lists every setting, so a key one side lacks there is unknown and never counts.
/// RiotUserSettings.ini lists only settings changed from their default: when both sides read it, a Riot setting that
/// one side lacks is at its default there, so it counts.
pub fn differences(a: &ValorantSettings, b: &ValorantSettings) -> Vec<String> {
    let both_read = a.riot_settings_read && b.riot_settings_read;
    let resolution = |s: &ValorantSettings| as_text(s.resolution.map(|[w, h]| format!("{w}x{h}")));
    // Name, value on each side, and whether the setting comes from RiotUserSettings.ini.
    let named = [
        ("Resolution", resolution(a), resolution(b), false),
        (
            "Fullscreen mode",
            as_text(a.fullscreen_mode),
            as_text(b.fullscreen_mode),
            false,
        ),
        ("V-Sync", as_text(a.vsync), as_text(b.vsync), false),
        (
            "Frame-rate limit",
            as_text(a.frame_rate_limit),
            as_text(b.frame_rate_limit),
            false,
        ),
        ("NVIDIA Reflex", as_text(a.reflex), as_text(b.reflex), true),
        (
            "Multithreaded rendering",
            as_text(a.multithreaded_rendering),
            as_text(b.multithreaded_rendering),
            true,
        ),
    ];
    let mut out = Vec::new();
    for (name, x, y, riot) in named {
        if differs(x.as_deref(), y.as_deref(), riot, both_read) {
            out.push(name.to_string());
        }
    }
    // The `sg.*` quality keys come from GameUserSettings.ini; the others come from RiotUserSettings.ini.
    let quality: BTreeSet<&String> = a.quality.keys().chain(b.quality.keys()).collect();
    for key in quality {
        let riot = RIOT_QUALITY.contains(&key.as_str());
        let (x, y) = (
            a.quality.get(key).map(String::as_str),
            b.quality.get(key).map(String::as_str),
        );
        if differs(x, y, riot, both_read) {
            out.push(key.clone());
        }
    }
    let limits: BTreeSet<&String> = a.limits.keys().chain(b.limits.keys()).collect();
    for key in limits {
        let (x, y) = (
            a.limits.get(key).map(String::as_str),
            b.limits.get(key).map(String::as_str),
        );
        if differs(x, y, true, both_read) {
            out.push(key.clone());
        }
    }
    out
}

/// A setting's value as text, so values of different types compare the same way.
fn as_text<T: ToString>(value: Option<T>) -> Option<String> {
    value.map(|v| v.to_string())
}

/// Whether a setting counts as changed. When both sides know it, the values must differ. When only one side knows
/// it, it counts only if it comes from RiotUserSettings.ini (`riot`) and both sides read that file: the side without
/// it has the default.
fn differs(x: Option<&str>, y: Option<&str>, riot: bool, both_read: bool) -> bool {
    match (x, y) {
        (Some(x), Some(y)) => x != y,
        (None, None) => false,
        _ => riot && both_read,
    }
}

fn pairs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with(&['[', ';', '#'][..]) {
            return None;
        }
        let (key, value) = line.split_once('=')?;
        let key = key.trim();
        if key.is_empty() {
            None
        } else {
            Some((key, value.trim()))
        }
    })
}

fn boolean(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// Text from UTF-8 (with or without BOM) or UTF-16 LE with BOM.
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (byte_pairs, _) = rest.as_chunks::<2>();
        let units: Vec<u16> = byte_pairs
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Collects the files under `dir`, where `depth` is how many folders `dir` sits below `config_root`. The walk stops
/// at depth four.
fn collect_files(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, SystemTime)>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            collect_files(&path, depth + 1, out);
        } else if let Ok(modified) = meta.modified() {
            out.push((path, modified));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};

    const GAME_USER_SETTINGS: &str = "[/Script/ShooterGame.ShooterGameUserSettings]\r\nsg.ResolutionQuality=100.000000\r\nsg.ShadowQuality=0\r\nsg.TextureQuality=0\r\nbUseVSync=False\r\nbUseDynamicResolution=False\r\nResolutionSizeX=1920\r\nResolutionSizeY=1080\r\nFullscreenMode=0\r\nFrameRateLimit=0.000000\r\n";
    const RIOT_USER_SETTINGS: &str = "[Settings]\nEAresBoolSettingName::MultithreadedRenderingEnabled=True\nEAresIntSettingName::NvidiaReflexLowLatencySetting=2\nEAresIntSettingName::TextureQuality=0\nEAresBoolSettingName::ShadowsEnabled=False\nEAresStringSettingName::SavedCrosshairProfileData={\"profiles\":[]}\nEAresBoolSettingName::LimitFramerateInMenu=True\nEAresIntSettingName::PlayerPerfShowFrameRate=1\n";

    #[test]
    fn reads_the_settings_that_affect_fps() {
        let s = parse(GAME_USER_SETTINGS, RIOT_USER_SETTINGS);
        assert_eq!(s.resolution, Some([1920, 1080]));
        assert_eq!(s.fullscreen_mode, Some(0));
        assert_eq!(s.vsync, Some(false));
        assert_eq!(s.frame_rate_limit, Some(0.0));
        assert_eq!(s.reflex, Some(2));
        assert_eq!(s.multithreaded_rendering, Some(true));
        assert_eq!(
            s.quality.get("sg.ShadowQuality").map(String::as_str),
            Some("0")
        );
        assert_eq!(
            s.quality.get("sg.ResolutionQuality").map(String::as_str),
            Some("100.000000")
        );
        assert_eq!(
            s.quality.get("TextureQuality").map(String::as_str),
            Some("0")
        );
        assert_eq!(
            s.quality.get("ShadowsEnabled").map(String::as_str),
            Some("False")
        );
        assert_eq!(
            s.limits.get("LimitFramerateInMenu").map(String::as_str),
            Some("True")
        );
        assert_eq!(
            s.limits.keys().collect::<Vec<_>>(),
            vec!["LimitFramerateInMenu"],
            "the FPS counter is not a limit"
        );
        assert!(!s.quality.contains_key("SavedCrosshairProfileData"));
    }

    #[test]
    fn missing_and_odd_files_are_unknown_not_errors() {
        assert_eq!(parse("", ""), ValorantSettings::default());
        let odd = parse(
            "=\nno equals sign\n;comment\n[section]\nResolutionSizeX=wide\nResolutionSizeX=1920\n",
            "EAresIntSettingName::NvidiaReflexLowLatencySetting=on\n",
        );
        assert_eq!(odd.resolution, None, "needs both X and Y");
        assert_eq!(odd.reflex, None);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(&dir.path().join("does-not-exist")), None);
        assert_eq!(read(dir.path()), None, "a folder without settings files");
        // UTF-16 (with BOM) files are read too.
        let account = dir.path().join("acc").join("Windows");
        fs::create_dir_all(&account).unwrap();
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(
            "FullscreenMode=1\r\n"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        fs::write(account.join("GameUserSettings.ini"), bytes).unwrap();
        assert_eq!(read(dir.path()).unwrap().fullscreen_mode, Some(1));
    }

    #[test]
    fn the_newest_account_folder_wins() {
        const SMALL: &str = "ResolutionSizeX=1280\nResolutionSizeY=720\n";
        const LARGE: &str = "ResolutionSizeX=1920\nResolutionSizeY=1080\n";
        let write = |path: &Path, text: &str| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        // Ages are set explicitly, so the result never depends on when the files were written.
        let age = |path: &Path, seconds: u64| {
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(SystemTime::now() - Duration::from_secs(seconds))
                .unwrap();
        };

        // "alpha" sorts first but holds the newest RiotUserSettings.ini; "zeta" sorts last but holds the newest
        // GameUserSettings.ini. Both files must come from one account, so "alpha" supplies both.
        let dir = tempfile::tempdir().unwrap();
        let (alpha_game, alpha_riot) = (
            dir.path()
                .join("alpha")
                .join("Windows")
                .join("GameUserSettings.ini"),
            dir.path().join("alpha").join("RiotUserSettings.ini"),
        );
        let (zeta_game, zeta_riot) = (
            dir.path()
                .join("zeta")
                .join("Windows")
                .join("GameUserSettings.ini"),
            dir.path().join("zeta").join("RiotUserSettings.ini"),
        );
        write(&alpha_game, SMALL);
        write(&alpha_riot, RIOT_USER_SETTINGS);
        write(&zeta_game, LARGE);
        write(&zeta_riot, RIOT_USER_SETTINGS);
        age(&alpha_riot, 30 * 60);
        age(&alpha_game, 2 * 3600);
        age(&zeta_riot, 3 * 3600);
        age(&zeta_game, 3600);
        assert_eq!(find_files(dir.path()), (Some(alpha_game), Some(alpha_riot)));
        assert_eq!(read(dir.path()).unwrap().resolution, Some([1280, 720]));

        // Without a RiotUserSettings.ini, the newest GameUserSettings.ini anywhere wins.
        let other = tempfile::tempdir().unwrap();
        let old_game = other
            .path()
            .join("old")
            .join("Windows")
            .join("GameUserSettings.ini");
        let new_game = other
            .path()
            .join("new")
            .join("Windows")
            .join("GameUserSettings.ini");
        write(&old_game, SMALL);
        write(&new_game, LARGE);
        age(&old_game, 3600);
        age(&new_game, 60);
        assert_eq!(find_files(other.path()), (Some(new_game), None));
        assert_eq!(read(other.path()).unwrap().resolution, Some([1920, 1080]));

        // A RiotUserSettings.ini directly in the config root names no account folder, so the same fallback applies.
        let root = tempfile::tempdir().unwrap();
        let account_game = root
            .path()
            .join("account")
            .join("Windows")
            .join("GameUserSettings.ini");
        let root_riot = root.path().join("RiotUserSettings.ini");
        write(&account_game, SMALL);
        write(&root_riot, RIOT_USER_SETTINGS);
        age(&account_game, 3600);
        age(&root_riot, 60);
        assert_eq!(
            find_files(root.path()),
            (Some(account_game), Some(root_riot))
        );

        // An account folder without a GameUserSettings.ini falls back to the newest one anywhere.
        let lonely = tempfile::tempdir().unwrap();
        let riot_only = lonely.path().join("riot-only").join("RiotUserSettings.ini");
        let elsewhere = lonely
            .path()
            .join("elsewhere")
            .join("Windows")
            .join("GameUserSettings.ini");
        write(&riot_only, RIOT_USER_SETTINGS);
        write(&elsewhere, LARGE);
        age(&riot_only, 60);
        age(&elsewhere, 3600);
        assert_eq!(
            find_files(lonely.path()),
            (Some(elsewhere), Some(riot_only))
        );
    }

    #[test]
    fn differences_count_riot_defaults_but_never_missing_files() {
        let a = parse(GAME_USER_SETTINGS, RIOT_USER_SETTINGS);
        assert!(a.riot_settings_read);
        let mut b = a.clone();
        assert!(differences(&a, &b).is_empty());
        // GameUserSettings.ini lists every setting, so a key one side lacks there is unknown, not changed.
        b.resolution = Some([1280, 720]);
        b.quality.insert("sg.ShadowQuality".into(), "2".into());
        b.quality.remove("sg.TextureQuality");
        // RiotUserSettings.ini lists only changed settings, so a key one side lacks is its default: a change.
        b.reflex = None;
        b.quality.remove("TextureQuality");
        b.limits
            .insert("LimitFramerateInMenu".into(), "False".into());
        assert_eq!(
            differences(&a, &b),
            vec![
                "Resolution",
                "NVIDIA Reflex",
                "TextureQuality",
                "sg.ShadowQuality",
                "LimitFramerateInMenu"
            ]
        );
        // A side without RiotUserSettings.ini knows none of its settings: nothing there counts as changed.
        let no_riot = parse(GAME_USER_SETTINGS, "");
        assert!(!no_riot.riot_settings_read);
        assert!(differences(&a, &no_riot).is_empty());
    }
}
