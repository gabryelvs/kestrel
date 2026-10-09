//! Pure helpers that turn raw Windows values into Kestrel's model. No OS calls, so they're tested anywhere.
use kestrel_core::{BusyProcess, CpuVendor, DriveMedia, FormFactor, NetKind};
use std::collections::HashMap;

/// CIM datetime "20240520000000.000000+000" → "2024-05-20".
pub fn cim_date(raw: &str) -> Option<String> {
    let digits = raw.get(0..8)?;
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (&digits[0..4], &digits[4..6], &digits[6..8]);
    chrono::NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)?;
    Some(format!("{y}-{m}-{d}"))
}

/// Built-in plan names are stored as "@…powrprof.dll,-14,Balanced"; custom names are plain.
/// A resource reference with no name after it ("@…powrprof.dll,-14") has nothing to show, so it gives "".
pub fn power_plan_name(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with('@') {
        let last = raw.rsplit(',').next().unwrap_or(raw).trim();
        if last.parse::<i64>().is_ok() {
            String::new()
        } else {
            last.to_string()
        }
    } else {
        raw.to_string()
    }
}

/// "SwapEffectUpgradeEnable=1;GpuPreference=2;" → Some(2).
pub fn gpu_preference(raw: &str) -> Option<u8> {
    raw.split(';')
        .find_map(|part| part.trim().strip_prefix("GpuPreference=")?.parse().ok())
}

/// Delivery Optimization download mode 3 means "PCs on my local network and on the internet".
pub fn delivery_optimization_internet(mode: Option<u32>) -> bool {
    mode == Some(3)
}

/// StartupApproved entries: even first byte (0x02, 0x06) = enabled, odd (0x03, 0x07) = disabled. Missing = enabled.
pub fn startup_entry_enabled(approved: Option<&[u8]>) -> bool {
    approved
        .and_then(|bytes| bytes.first())
        .is_none_or(|first| first % 2 == 0)
}

/// MSFT_NetAdapter.NdisPhysicalMedium: 14 = 802.3 Ethernet, 9 = native 802.11 Wi-Fi.
pub fn net_kind(ndis_physical_medium: u64) -> NetKind {
    match ndis_physical_medium {
        14 => NetKind::Ethernet,
        9 => NetKind::Wifi,
        _ => NetKind::Other,
    }
}

/// MSFT_PhysicalDisk: BusType 17 = NVMe; MediaType 4 = SSD, 3 = HDD.
pub fn drive_media(bus_type: Option<u64>, media_type: Option<u64>) -> DriveMedia {
    if bus_type == Some(17) {
        return DriveMedia::Nvme;
    }
    match media_type {
        Some(4) => DriveMedia::Ssd,
        Some(3) => DriveMedia::Hdd,
        _ => DriveMedia::Unknown,
    }
}

/// Win32_ComputerSystem.PCSystemType decides when it is known: 2 = mobile, while 1 (desktop) and 3-8 (workstation,
/// servers, appliance) are never laptops, even if a battery shows up (a USB UPS looks like one). When it is 0
/// (unspecified) or missing, a battery means laptop and no battery means desktop.
pub fn form_factor(pc_system_type: Option<u64>, has_battery: bool) -> FormFactor {
    match pc_system_type {
        Some(2) => FormFactor::Laptop,
        Some(1 | 3..=8) => FormFactor::Desktop,
        _ if has_battery => FormFactor::Laptop,
        _ => FormFactor::Desktop,
    }
}

/// Win32_Battery.BatteryStatus: 1 (discharging), 4 (low) and 5 (critical) mean the PC runs on its battery. Anything
/// else (2 on AC, 3 fully charged, 6-9 charging, 10 undefined, 11 partially charged) means mains power.
pub fn on_battery(battery_status: u64) -> bool {
    matches!(battery_status, 1 | 4 | 5)
}

pub fn cpu_vendor(manufacturer: &str) -> CpuVendor {
    if manufacturer.contains("Intel") {
        CpuVendor::Intel
    } else if manufacturer.contains("AMD") {
        CpuVendor::Amd
    } else {
        CpuVendor::Other
    }
}

/// SMBIOSMemoryType: 24 = DDR3, 26 = DDR4, 34 = DDR5.
pub fn ddr_generation(smbios_memory_type: u64) -> Option<u8> {
    match smbios_memory_type {
        24 => Some(3),
        26 => Some(4),
        34 => Some(5),
        _ => None,
    }
}

/// Entries of the "Process" performance counters that are Windows itself, not an app.
const PDH_NOT_APPS: [&str; 6] = [
    "_Total",
    "Idle",
    "System",
    "Registry",
    "Memory Compression",
    "Secure System",
];

/// A "Process" performance counter instance name → the process's exe name: "chrome#3" → "chrome.exe".
/// The counters tell apart processes that share a name by adding "#<digits>", and leave out ".exe".
/// The totals and Windows' own entries (`PDH_NOT_APPS`) give None.
pub fn pdh_instance_to_exe(instance: &str) -> Option<String> {
    let name = instance
        .rsplit_once('#')
        .filter(|(_, digits)| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .map_or(instance, |(name, _)| name);
    if name.is_empty() || PDH_NOT_APPS.iter().any(|n| n.eq_ignore_ascii_case(name)) {
        return None;
    }
    Some(format!("{name}.exe"))
}

/// Kestrel's own work (its window, its probe, and the WMI host answering its queries) isn't background load, and
/// neither is Windows' own: svchost.exe alone adds up dozens of services, which no one can close to free the CPU.
const NOT_BACKGROUND: &[&str] = &[
    "system idle process",
    "idle",
    "kestrel.exe",
    "kestrel-probe.exe",
    "wmiprvse.exe",
    "msedgewebview2.exe",
    "svchost.exe",
    "dwm.exe",
    "csrss.exe",
    "lsass.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "smss.exe",
    "audiodg.exe",
    "fontdrvhost.exe",
];

/// Per-process CPU samples (100 = one full core) → share of the whole CPU, ≥ 5%, busiest first, at most 5.
/// An app often runs as many processes (a browser has dozens), so samples with the same name (any letter case)
/// are added up first, under the spelling of the first one seen.
pub fn busy_processes(samples: Vec<(String, f32)>, logical_cpus: u32) -> Vec<BusyProcess> {
    let cores = logical_cpus.max(1) as f32;
    let mut position_by_name: HashMap<String, usize> = HashMap::new();
    let mut totals: Vec<(String, f32)> = Vec::new();
    for (name, usage) in samples {
        let key = name.to_ascii_lowercase();
        if NOT_BACKGROUND.contains(&key.as_str()) {
            continue;
        }
        match position_by_name.get(&key) {
            Some(&at) => totals[at].1 += usage,
            None => {
                position_by_name.insert(key, totals.len());
                totals.push((name, usage));
            }
        }
    }
    let mut busy: Vec<BusyProcess> = totals
        .into_iter()
        .map(|(name, usage)| BusyProcess {
            name,
            cpu_percent: usage / cores,
        })
        .filter(|p| p.cpu_percent >= 5.0)
        .collect();
    busy.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
    busy.truncate(5);
    busy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cim_dates() {
        assert_eq!(
            cim_date("20240520000000.000000+000").as_deref(),
            Some("2024-05-20")
        );
        assert_eq!(cim_date("2024"), None);
        assert_eq!(cim_date("20241340000000.000000+000"), None);
        assert_eq!(cim_date(""), None);
    }

    #[test]
    fn power_plan_names() {
        assert_eq!(
            power_plan_name("@%SystemRoot%\\system32\\powrprof.dll,-14,Balanced"),
            "Balanced"
        );
        assert_eq!(power_plan_name("Kestrel"), "Kestrel");
        assert_eq!(power_plan_name("  My plan, tuned "), "My plan, tuned");
    }

    #[test]
    fn power_plan_resource_id_without_a_name_is_empty() {
        // A bare string-resource id carries no name; the caller falls back to its table of known plan GUIDs.
        assert_eq!(
            power_plan_name("@%SystemRoot%\\system32\\powrprof.dll,-14"),
            ""
        );
    }

    #[test]
    fn gpu_preferences() {
        assert_eq!(gpu_preference("GpuPreference=2;"), Some(2));
        assert_eq!(
            gpu_preference("SwapEffectUpgradeEnable=1;GpuPreference=1;"),
            Some(1)
        );
        assert_eq!(gpu_preference("SwapEffectUpgradeEnable=1;"), None);
    }

    #[test]
    fn delivery_optimization_modes() {
        assert!(delivery_optimization_internet(Some(3)));
        assert!(!delivery_optimization_internet(Some(1)));
        assert!(!delivery_optimization_internet(None));
    }

    #[test]
    fn startup_approved_bytes() {
        assert!(startup_entry_enabled(None));
        assert!(startup_entry_enabled(Some(&[0x02, 0, 0, 0])));
        assert!(startup_entry_enabled(Some(&[0x06, 0, 0, 0])));
        assert!(!startup_entry_enabled(Some(&[0x03, 0, 0, 0])));
    }

    #[test]
    fn hardware_codes() {
        assert_eq!(net_kind(14), NetKind::Ethernet);
        assert_eq!(net_kind(9), NetKind::Wifi);
        assert_eq!(net_kind(0), NetKind::Other);
        assert_eq!(drive_media(Some(17), Some(4)), DriveMedia::Nvme);
        assert_eq!(drive_media(Some(11), Some(4)), DriveMedia::Ssd);
        assert_eq!(drive_media(Some(11), Some(3)), DriveMedia::Hdd);
        assert_eq!(drive_media(None, None), DriveMedia::Unknown);
        assert_eq!(cpu_vendor("GenuineIntel"), CpuVendor::Intel);
        assert_eq!(cpu_vendor("AuthenticAMD"), CpuVendor::Amd);
        assert_eq!(ddr_generation(26), Some(4));
        assert_eq!(ddr_generation(34), Some(5));
        assert_eq!(ddr_generation(0), None);
    }

    #[test]
    fn battery_status_says_whether_the_pc_runs_on_battery() {
        // Win32_Battery.BatteryStatus: 1 discharging, 2 on AC, 3 fully charged, 4 low, 5 critical, 6 charging.
        let cases = [
            (1, true),
            (2, false),
            (3, false),
            (4, true),
            (5, true),
            (6, false),
        ];
        for (status, discharging) in cases {
            assert_eq!(on_battery(status), discharging, "BatteryStatus {status}");
        }
        // Charging (7-9), undefined (10) and partially charged (11) are all on mains power.
        for status in 7..=11 {
            assert!(!on_battery(status), "BatteryStatus {status}");
        }
    }

    #[test]
    fn the_system_type_decides_the_form_factor_when_it_is_known() {
        // 2 is mobile. 1 (desktop) and 3-8 (workstation, servers, appliance) are not laptops, even when a battery
        // shows up: a USB UPS looks like one.
        for has_battery in [false, true] {
            assert_eq!(form_factor(Some(2), has_battery), FormFactor::Laptop);
            for system_type in [1, 3, 4, 5, 6, 7, 8] {
                assert_eq!(
                    form_factor(Some(system_type), has_battery),
                    FormFactor::Desktop,
                    "PCSystemType {system_type}, battery {has_battery}"
                );
            }
        }
    }

    #[test]
    fn a_battery_decides_the_form_factor_only_when_the_system_type_is_unspecified() {
        for unspecified in [Some(0), None] {
            assert_eq!(form_factor(unspecified, true), FormFactor::Laptop);
            // No battery: a desktop, which is what the probe has always answered here.
            assert_eq!(form_factor(unspecified, false), FormFactor::Desktop);
        }
    }

    #[test]
    fn busy_process_list() {
        let samples = vec![
            ("chrome.exe".to_string(), 160.0),
            ("System Idle Process".to_string(), 1500.0),
            ("Kestrel.exe".to_string(), 80.0),
            ("explorer.exe".to_string(), 4.0),
        ];
        let busy = busy_processes(samples, 16);
        assert_eq!(busy.len(), 1);
        assert_eq!(busy[0].name, "chrome.exe");
        assert!((busy[0].cpu_percent - 10.0).abs() < 0.01);
    }

    #[test]
    fn busy_processes_with_the_same_name_are_added_up() {
        // One app runs as many processes: each is under the 5% bar alone, but together they are over it.
        let samples = vec![
            ("chrome.exe".to_string(), 48.0),
            ("Chrome.exe".to_string(), 40.0),
            ("discord.exe".to_string(), 8.0),
        ];
        let busy = busy_processes(samples, 16);
        assert_eq!(busy.len(), 1);
        assert_eq!(busy[0].name, "chrome.exe");
        assert!((busy[0].cpu_percent - 5.5).abs() < 0.01);
    }

    #[test]
    fn busy_processes_keep_the_spelling_of_the_first_sample() {
        let samples = vec![
            ("Chrome.exe".to_string(), 48.0),
            ("chrome.exe".to_string(), 40.0),
        ];
        let busy = busy_processes(samples, 16);
        assert_eq!(busy.len(), 1);
        assert_eq!(busy[0].name, "Chrome.exe");
    }

    #[test]
    fn pdh_instances_become_exe_names() {
        assert_eq!(
            pdh_instance_to_exe("chrome#3").as_deref(),
            Some("chrome.exe")
        );
        assert_eq!(
            pdh_instance_to_exe("Kestrel").as_deref(),
            Some("Kestrel.exe")
        );
        assert_eq!(
            pdh_instance_to_exe("svchost#12").as_deref(),
            Some("svchost.exe")
        );
        // Only a trailing "#<digits>" marks a second process with the same name.
        assert_eq!(pdh_instance_to_exe("C#Tool").as_deref(), Some("C#Tool.exe"));
    }

    #[test]
    fn pdh_system_instances_are_not_apps() {
        for name in [
            "_Total",
            "Idle",
            "System",
            "Registry",
            "Memory Compression",
            "Secure System",
        ] {
            assert_eq!(pdh_instance_to_exe(name), None, "{name}");
        }
        assert_eq!(pdh_instance_to_exe(""), None);
        assert_eq!(pdh_instance_to_exe("#2"), None);
    }

    #[test]
    fn windows_system_processes_are_never_background_apps() {
        // svchost.exe hosts dozens of services; added up, it would otherwise be named as an app to close.
        let svchost = vec![("svchost.exe".to_string(), 200.0)];
        assert!(busy_processes(svchost, 16).is_empty());
        for name in [
            "svchost.exe",
            "dwm.exe",
            "csrss.exe",
            "lsass.exe",
            "services.exe",
            "wininit.exe",
            "winlogon.exe",
            "smss.exe",
            "audiodg.exe",
            "fontdrvhost.exe",
        ] {
            let busy = busy_processes(vec![(name.to_string(), 800.0)], 16);
            assert!(busy.is_empty(), "{name}");
        }
    }
}
