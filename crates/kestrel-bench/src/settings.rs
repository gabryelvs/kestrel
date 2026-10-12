//! VALORANT's video settings, read from its config files and never written (decision 43). Keys confirmed on the
//! owner's PC: docs/superpowers/notes/2026-10-12-valorant-settings-keys.md. VALORANT only writes settings changed
//! from their default, so a missing key means "default", never an error.
use crate::model::ValorantSettings;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
    for (key, value) in pairs(riot_user_settings) {
        // "EAresIntSettingName::NvidiaReflexLowLatencySetting" → "NvidiaReflexLowLatencySetting"
        let name = key.rsplit("::").next().unwrap_or(key);
        let lower = name.to_ascii_lowercase();
        match name {
            "NvidiaReflexLowLatencySetting" => s.reflex = value.parse().ok(),
            "MultithreadedRenderingEnabled" => s.multithreaded_rendering = boolean(value),
            "AnisotropicFiltering"
            | "AntiAliasing"
            | "DetailQuality"
            | "MaterialQuality"
            | "TextureQuality"
            | "UIQuality"
            | "ShadowsEnabled" => {
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

/// The most recently written GameUserSettings.ini and RiotUserSettings.ini under `config_root`
/// (`%LOCALAPPDATA%\VALORANT\Saved\Config`). There's one folder per Riot account; the newest is the one in use.
pub fn find_files(config_root: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut files = Vec::new();
    collect_files(config_root, 0, &mut files);
    let newest = |name: &str| {
        files
            .iter()
            .filter(|(path, _)| {
                path.file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .max_by_key(|(_, modified)| *modified)
            .map(|(path, _)| path.clone())
    };
    (
        newest("GameUserSettings.ini"),
        newest("RiotUserSettings.ini"),
    )
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

/// Names of settings that differ. Settings either side doesn't know are skipped, so missing data never counts as
/// a difference.
pub fn differences(a: &ValorantSettings, b: &ValorantSettings) -> Vec<String> {
    let resolution = |s: &ValorantSettings| s.resolution.map(|[w, h]| format!("{w}x{h}"));
    let known = [
        ("Resolution", resolution(a), resolution(b)),
        (
            "Fullscreen mode",
            a.fullscreen_mode.map(|v| v.to_string()),
            b.fullscreen_mode.map(|v| v.to_string()),
        ),
        (
            "V-Sync",
            a.vsync.map(|v| v.to_string()),
            b.vsync.map(|v| v.to_string()),
        ),
        (
            "Frame-rate limit",
            a.frame_rate_limit.map(|v| v.to_string()),
            b.frame_rate_limit.map(|v| v.to_string()),
        ),
        (
            "NVIDIA Reflex",
            a.reflex.map(|v| v.to_string()),
            b.reflex.map(|v| v.to_string()),
        ),
        (
            "Multithreaded rendering",
            a.multithreaded_rendering.map(|v| v.to_string()),
            b.multithreaded_rendering.map(|v| v.to_string()),
        ),
    ];
    let mut out: Vec<String> = known
        .into_iter()
        .filter_map(|(name, x, y)| match (x, y) {
            (Some(x), Some(y)) if x != y => Some(name.to_string()),
            _ => None,
        })
        .collect();
    for (key, value) in &a.quality {
        if b.quality.get(key).is_some_and(|other| other != value) {
            out.push(key.clone());
        }
    }
    out
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

/// Files up to four folders deep (Config\<account>\Windows\file.ini is three).
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
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old-account").join("Windows");
        let new = dir.path().join("new-account").join("Windows");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(
            old.join("GameUserSettings.ini"),
            "ResolutionSizeX=1280\nResolutionSizeY=720\n",
        )
        .unwrap();
        fs::write(
            new.join("GameUserSettings.ini"),
            "ResolutionSizeX=1920\nResolutionSizeY=1080\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("new-account").join("RiotUserSettings.ini"),
            RIOT_USER_SETTINGS,
        )
        .unwrap();
        let an_hour_ago = SystemTime::now() - Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(old.join("GameUserSettings.ini"))
            .unwrap()
            .set_modified(an_hour_ago)
            .unwrap();
        let (game, riot) = find_files(dir.path());
        assert_eq!(game.unwrap(), new.join("GameUserSettings.ini"));
        assert_eq!(
            riot.unwrap(),
            dir.path().join("new-account").join("RiotUserSettings.ini")
        );
        assert_eq!(read(dir.path()).unwrap().resolution, Some([1920, 1080]));
    }

    #[test]
    fn differences_name_only_settings_both_sides_know() {
        let a = parse(GAME_USER_SETTINGS, RIOT_USER_SETTINGS);
        let mut b = a.clone();
        assert!(differences(&a, &b).is_empty());
        b.resolution = Some([1280, 720]);
        b.quality.insert("sg.ShadowQuality".into(), "2".into());
        b.reflex = None;
        assert_eq!(
            differences(&a, &b),
            vec!["Resolution".to_string(), "sg.ShadowQuality".to_string()]
        );
    }
}
