//! Registry reads. Keys are opened read-only (KEY_READ) and never written.
use super::problem;
use crate::parse;
use kestrel_core::{valorant, PowerPlan, ProbeError, SystemSnapshot};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ};
use winreg::RegKey;

const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const NET_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";
const PLAN_NAMES: [(&str, &str); 4] = [
    ("381b4222-f694-41f0-9685-ff5bb260df2e", "Balanced"),
    ("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c", "High performance"),
    ("a1841308-3541-4fab-bc81-f71556f20b4a", "Power saver"),
    (
        "e9a42b02-d5df-448d-aa00-03f14749eb61",
        "Ultimate Performance",
    ),
];

fn open(root: winreg::HKEY, path: &str) -> Option<RegKey> {
    RegKey::predef(root)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()
}
fn hklm(path: &str) -> Option<RegKey> {
    open(HKEY_LOCAL_MACHINE, path)
}
fn hkcu(path: &str) -> Option<RegKey> {
    open(HKEY_CURRENT_USER, path)
}
fn dword(key: &Option<RegKey>, name: &str) -> Option<u32> {
    key.as_ref()?.get_value::<u32, _>(name).ok()
}
fn string(key: &Option<RegKey>, name: &str) -> Option<String> {
    key.as_ref()?.get_value::<String, _>(name).ok()
}

pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    s.os.display_version = string(
        &hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
        "DisplayVersion",
    );
    s.windows.power_plan = power_plan();
    if s.windows.power_plan.is_none() {
        errors.push(problem("power-plan", "couldn't read the active power plan"));
    }
    // Missing value = Windows default (Game Mode on).
    s.windows.game_mode =
        Some(dword(&hkcu(r"Software\Microsoft\GameBar"), "AutoGameModeEnabled") != Some(0));
    let dvr = hkcu(r"Software\Microsoft\Windows\CurrentVersion\GameDVR");
    s.windows.background_recording = Some(
        dword(&dvr, "AppCaptureEnabled") != Some(0)
            && dword(&dvr, "HistoricalCaptureEnabled") == Some(1),
    );
    s.windows.hags = match dword(
        &hklm(r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers"),
        "HwSchMode",
    ) {
        Some(2) => Some(true),
        Some(1) => Some(false),
        _ => None,
    };
    s.windows.pending_restart = Some(
        hklm(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending")
            .is_some()
            || hklm(
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired",
            )
            .is_some(),
    );
    s.windows.startup_app_count = Some(startup_app_count());
    s.windows.valorant_gpu_preference = s
        .storage
        .valorant
        .as_ref()
        .and_then(|v| gpu_preference_for(&valorant::game_exe_path(&v.path)));
    s.security.secure_boot = Some(
        dword(
            &hklm(r"SYSTEM\CurrentControlSet\Control\SecureBoot\State"),
            "UEFISecureBootEnabled",
        ) == Some(1),
    );
    s.network.delivery_optimization_internet = Some(parse::delivery_optimization_internet(
        delivery_optimization_mode(),
    ));
}

fn power_plan() -> Option<PowerPlan> {
    let base = r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes";
    let guid = string(&hklm(base), "ActivePowerScheme")?.to_ascii_lowercase();
    let known = PLAN_NAMES
        .iter()
        .find(|(g, _)| *g == guid)
        .map(|(_, n)| n.to_string());
    let name = string(&hklm(&format!(r"{base}\{guid}")), "FriendlyName")
        .map(|raw| parse::power_plan_name(&raw))
        .filter(|n| !n.is_empty())
        .or(known)
        .unwrap_or_else(|| "Custom plan".into());
    Some(PowerPlan { guid, name })
}

fn startup_app_count() -> u32 {
    let run_keys = [
        (
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            "Run",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            "Run",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
            "Run32",
        ),
    ];
    let mut count = 0;
    for (root, path, approved_name) in run_keys {
        let Some(run) = open(root, path) else {
            continue;
        };
        let approved = open(
            root,
            &format!(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\{approved_name}"
            ),
        );
        for (name, _) in run.enum_values().filter_map(Result::ok) {
            let bytes = approved
                .as_ref()
                .and_then(|a| a.get_raw_value(&name).ok())
                .map(|v| v.bytes.to_vec());
            if parse::startup_entry_enabled(bytes.as_deref()) {
                count += 1;
            }
        }
    }
    count + startup_folder_count()
}

fn startup_folder_count() -> u32 {
    let folders = [
        std::env::var("APPDATA")
            .ok()
            .map(|a| format!(r"{a}\Microsoft\Windows\Start Menu\Programs\Startup")),
        std::env::var("ProgramData")
            .ok()
            .map(|p| format!(r"{p}\Microsoft\Windows\Start Menu\Programs\Startup")),
    ];
    folders
        .into_iter()
        .flatten()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|e| {
            !e.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("desktop.ini")
        })
        .count() as u32
}

fn gpu_preference_for(exe: &str) -> Option<u8> {
    let prefs = hkcu(r"Software\Microsoft\DirectX\UserGpuPreferences")?;
    let wanted = exe.replace('/', "\\").to_ascii_lowercase();
    let name = prefs
        .enum_values()
        .filter_map(Result::ok)
        .map(|(name, _)| name)
        .find(|name| name.to_ascii_lowercase() == wanted)?;
    parse::gpu_preference(&prefs.get_value::<String, _>(&name).ok()?)
}

fn delivery_optimization_mode() -> Option<u32> {
    let policy = dword(
        &hklm(r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization"),
        "DODownloadMode",
    );
    let settings = open(
        HKEY_USERS,
        r"S-1-5-20\Software\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Settings",
    );
    policy
        .or_else(|| dword(&settings, "DownloadMode"))
        .or_else(|| {
            dword(
                &hklm(r"SOFTWARE\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Config"),
                "DODownloadMode",
            )
        })
}

fn class_entries(class: &str) -> Vec<RegKey> {
    let Some(root) = hklm(class) else {
        return Vec::new();
    };
    root.enum_keys()
        .filter_map(Result::ok)
        .filter(|k| k.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|k| root.open_subkey_with_flags(&k, KEY_READ).ok())
        .collect()
}

/// Dedicated video memory (MB) of the display adapter whose driver description is `name`.
pub fn gpu_memory_mb(name: &str) -> Option<u64> {
    let key = class_entries(DISPLAY_CLASS)
        .into_iter()
        .find(|k| k.get_value::<String, _>("DriverDesc").ok().as_deref() == Some(name))?;
    let bytes = key
        .get_value::<u64, _>("HardwareInformation.qwMemorySize")
        .ok()
        .or_else(|| {
            key.get_value::<u32, _>("HardwareInformation.MemorySize")
                .ok()
                .map(u64::from)
        })
        .or_else(|| {
            let raw = key.get_raw_value("HardwareInformation.MemorySize").ok()?;
            let b = raw.bytes.get(0..4)?;
            Some(u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
        })?;
    Some(bytes / (1024 * 1024))
}

/// Whether Energy-Efficient Ethernet or a similar power-saving option is on, for the adapter with this NetCfgInstanceId.
pub fn adapter_power_saving(device_id: &str) -> Option<bool> {
    const KEYWORDS: [&str; 4] = ["*EEE", "EEE", "EnableGreenEthernet", "PowerSavingMode"];
    let key = class_entries(NET_CLASS).into_iter().find(|k| {
        k.get_value::<String, _>("NetCfgInstanceId")
            .is_ok_and(|v| v.eq_ignore_ascii_case(device_id))
    })?;
    Some(KEYWORDS.iter().any(|kw| {
        key.get_value::<String, _>(*kw)
            .is_ok_and(|v| v.trim() == "1")
    }))
}
