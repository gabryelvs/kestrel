//! Pure helpers that turn raw Windows values into Kestrel's model. No OS calls, so they're tested anywhere.
use kestrel_core::topology::PhysicalCore;
use kestrel_core::{
    BootMode, BusyProcess, CacheKind, CpuInfo, CpuVendor, DriveMedia, FormFactor, NetKind,
};
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
/// Nor are VALORANT, the Riot Client, Vanguard and Windows Defender, so a scan run mid-game never tells the player
/// to close the game or its anti-cheat, or to stop Defender. All lower case, like the names they are compared with.
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
    "valorant-win64-shipping.exe",
    "valorant.exe",
    "vgc.exe",
    "vgtray.exe",
    "riotclientservices.exe",
    "riot client.exe",
    "riotclientux.exe",
    "riotclientuxrender.exe",
    "msmpeng.exe",
    "mpdefendercoreservice.exe",
    "nissrv.exe",
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

/// A NUL-terminated UTF-16 buffer (a Toolhelp32 entry's exe name, a display device's name) → text up to the first NUL.
pub fn exe_name(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// CPUID registers that spell text → the text. Each register holds four characters, lowest byte first (the vendor
/// is leaf 0's EBX, EDX, ECX; the brand is EAX to EDX of leaves 0x80000002–4). NULs are dropped and the ends trimmed:
/// older Intel processors pad the brand with spaces.
pub fn cpuid_text(registers: &[u32]) -> String {
    let bytes: Vec<u8> = registers
        .iter()
        .flat_map(|register| register.to_le_bytes())
        .filter(|&byte| byte != 0)
        .collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// The two LOGICAL_PROCESSOR_RELATIONSHIP values Kestrel reads (winnt.h).
const RELATION_PROCESSOR_CORE: u32 = 0;
const RELATION_CACHE: u32 = 2;

/// What `GetLogicalProcessorInformationEx(RelationAll)` wrote: SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX records one
/// after another, each starting with its relationship (u32) and its size in bytes (u32) → the physical cores and one
/// (level, kind, size in KB) per cache, in Windows' order. Other relationships (packages, NUMA nodes, groups, dies)
/// are skipped. Offsets are those of 64-bit Windows (8-byte affinity masks). Nothing is read past the buffer: a
/// record that claims more room than is left, or no room at all, ends the list.
pub fn logical_processors(buffer: &[u8]) -> (Vec<PhysicalCore>, Vec<(u8, CacheKind, u32)>) {
    let (mut cores, mut caches) = (Vec::new(), Vec::new());
    let mut at = 0;
    while let (Some(relationship), Some(size)) = (le_u32(buffer, at), le_u32(buffer, at + 4)) {
        let size = size as usize;
        let Some(record) = at
            .checked_add(size)
            .and_then(|end| buffer.get(at..end))
            .filter(|_| size >= 8)
        else {
            break;
        };
        match relationship {
            RELATION_PROCESSOR_CORE => cores.extend(processor_core(record)),
            RELATION_CACHE => caches.extend(cache(record)),
            _ => {}
        }
        at += size;
    }
    (cores, caches)
}

/// A RelationProcessorCore record: EfficiencyClass (u8) at 9, GroupCount (u16) at 30, then GroupCount
/// GROUP_AFFINITY of 16 bytes from 32, each a Mask (u64) and its Group (u16). Every mask bit is a logical processor.
fn processor_core(record: &[u8]) -> Option<PhysicalCore> {
    let efficiency_class = *record.get(9)?;
    let mut logical = Vec::new();
    for index in 0..usize::from(le_u16(record, 30)?) {
        let affinity = 32 + 16 * index;
        let (Some(mask), Some(group)) = (le_u64(record, affinity), le_u16(record, affinity + 8))
        else {
            break;
        };
        logical.extend(
            (0..64u8)
                .filter(|bit| mask & (1u64 << bit) != 0)
                .map(|bit| (group, bit)),
        );
    }
    (!logical.is_empty()).then_some(PhysicalCore {
        efficiency_class,
        logical,
    })
}

/// A RelationCache record: Level (u8) at 8, CacheSize (u32, bytes) at 12, Type (u32) at 16: 0 unified,
/// 1 instruction, 2 data. 3 is a Pentium 4 trace cache, which the model has no kind for.
fn cache(record: &[u8]) -> Option<(u8, CacheKind, u32)> {
    let kind = match le_u32(record, 16)? {
        0 => CacheKind::Unified,
        1 => CacheKind::Instruction,
        2 => CacheKind::Data,
        _ => CacheKind::Unknown,
    };
    Some((*record.get(8)?, kind, le_u32(record, 12)? / 1024))
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// `GetFirmwareType`'s FIRMWARE_TYPE: 1 is BIOS (legacy boot, CSM), 2 is UEFI; 0 (unknown) and anything else unknown.
pub fn boot_mode(firmware_type: i32) -> BootMode {
    match firmware_type {
        1 => BootMode::Legacy,
        2 => BootMode::Uefi,
        _ => BootMode::Unknown,
    }
}

/// Whether VT-x / SVM is on. A running hypervisor (CPUID's hypervisor bit, or VBS) uses it and hides the firmware
/// flag, so either one means on; otherwise the firmware flag (`PF_VIRT_FIRMWARE_ENABLED`) says.
pub fn virtualization(
    hypervisor: bool,
    vbs_running: Option<bool>,
    firmware_enabled: Option<bool>,
) -> Option<bool> {
    if hypervisor || vbs_running == Some(true) {
        Some(true)
    } else {
        firmware_enabled
    }
}

/// The socket to show: the codename table names the desktop part's, and a laptop chip that shares the desktop's
/// CPUID signature (Dragon Range shares Raphael's) is soldered to the board, so a laptop has none.
pub fn cpu_socket(form_factor: FormFactor, table_socket: Option<&str>) -> Option<String> {
    if form_factor == FormFactor::Laptop {
        None
    } else {
        table_socket.map(str::to_string)
    }
}

/// The CPU's name and maker. WMI's `Win32_Processor` stays primary: what it left empty (no name, or
/// `CpuVendor::Other`, because its query failed or it didn't say) is filled from the processor itself, with CPUID's
/// brand string and vendor.
pub fn fill_missing_cpu_identity(cpu: &mut CpuInfo, brand: &str, vendor: CpuVendor) {
    if cpu.name.is_empty() {
        cpu.name = brand.to_string();
    }
    if cpu.vendor == CpuVendor::Other {
        cpu.vendor = vendor;
    }
}

/// The CPU's core and thread counts. WMI's stay primary: a count it left at 0 is filled from the cores Windows lists,
/// one per physical core, and from the logical processors on them.
pub fn fill_missing_cpu_counts(cpu: &mut CpuInfo, cores: &[PhysicalCore]) {
    if cpu.cores == 0 {
        cpu.cores = cores.len() as u32;
    }
    if cpu.threads == 0 {
        cpu.threads = cores.iter().map(|core| core.logical.len() as u32).sum();
    }
}

/// The rated clock in MHz from the instances of PDH's `Processor Information` `Processor Frequency` counter, as
/// (name, value): the `_Total` row's when it has a usable value, else the first row that has one. A usable value is a
/// finite number of at least 1 MHz; zero, negative, NaN and infinite values mean Windows has no clock to give. None
/// when no row has one.
pub fn rated_mhz(instances: &[(String, f64)]) -> Option<u32> {
    let mut usable = instances
        .iter()
        .filter(|(_, mhz)| mhz.is_finite() && *mhz >= 1.0);
    let total = usable
        .clone()
        .find(|(name, _)| name.eq_ignore_ascii_case("_Total"));
    let (_, mhz) = total.or_else(|| usable.next())?;
    Some(mhz.round() as u32)
}

/// DEVPROPTYPE values (devpropdef.h) of the device properties Kestrel reads; `win::cfgmgr` checks them against
/// windows-sys.
pub const DEVPROP_BYTE: u32 = 3;
pub const DEVPROP_UINT32: u32 = 7;
pub const DEVPROP_STRING: u32 = 18;

/// A device property stored as DEVPROP_TYPE_UINT32 (four bytes) → its value; None for any other type or size.
pub fn device_property_u32(property_type: u32, data: &[u8]) -> Option<u32> {
    if property_type != DEVPROP_UINT32 {
        return None;
    }
    Some(u32::from_le_bytes(data.try_into().ok()?))
}

/// A device property stored as DEVPROP_TYPE_BYTE (one byte) → its value; None for any other type or size.
pub fn device_property_byte(property_type: u32, data: &[u8]) -> Option<u8> {
    match data {
        [byte] if property_type == DEVPROP_BYTE => Some(*byte),
        _ => None,
    }
}

/// A device property stored as DEVPROP_TYPE_STRING (UTF-16, little-endian, NUL-terminated) → the text; None for any
/// other type or an empty text.
pub fn device_property_text(property_type: u32, data: &[u8]) -> Option<String> {
    if property_type != DEVPROP_STRING {
        return None;
    }
    let (pairs, _) = data.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    Some(exe_name(&units).trim().to_string()).filter(|text| !text.is_empty())
}

/// The data of a memory resource descriptor (`CM_Get_Res_Des_Data` for ResType_Mem or ResType_MemLarge) starts with
/// a MEM_DES or MEM_LARGE_DES header, both laid out the same: Count (u32), Type (u32), Alloc_Base (u64) at 8 and
/// Alloc_End (u64, inclusive) at 16 → the allocated range's size in bytes, End − Base + 1. None when the data is too
/// short or the range impossible.
pub fn allocated_range_bytes(data: &[u8]) -> Option<u64> {
    let (base, end) = (le_u64(data, 8)?, le_u64(data, 16)?);
    end.checked_sub(base)?.checked_add(1)
}

/// The most memory descriptors of one resource type `allocated_ranges` accepts: a card has a handful (six BARs and a
/// ROM), so a longer list is a walk gone wrong.
pub const MAX_DESCRIPTORS: usize = 64;

/// What Windows answered at one step of a walk over a configuration's memory descriptors (`win::cfgmgr`).
#[derive(Debug, Clone, PartialEq)]
pub enum WalkStep {
    /// The next descriptor, with the data read from it.
    Data(Vec<u8>),
    /// "No more descriptors" (CR_NO_MORE_RES_DES): the list is complete. Nothing else ends a list.
    End,
    /// Any other answer, or a descriptor whose data couldn't be read.
    Failed,
}

/// The size in bytes of each allocated range in a walk over the memory descriptors of one resource type, in the order
/// Windows lists them; `steps` are Windows' answers one after another. Some only when Windows ended the list itself
/// (`End`) after at most MAX_DESCRIPTORS descriptors. A failed step, a walk that stops without `End` and a longer list
/// give None: a range left out could be the card's big window, and the rest would read as Resizable BAR off. A
/// descriptor with no usable range (see `allocated_range_bytes`) adds nothing and is skipped.
pub fn allocated_ranges(steps: impl IntoIterator<Item = WalkStep>) -> Option<Vec<u64>> {
    let mut sizes = Vec::new();
    // The descriptors, then the End that shows the list is complete.
    for step in steps.into_iter().take(MAX_DESCRIPTORS + 1) {
        match step {
            WalkStep::Data(data) => sizes.extend(allocated_range_bytes(&data)),
            WalkStep::End => return Some(sizes),
            WalkStep::Failed => return None,
        }
    }
    None
}

/// NUL-terminated UTF-16 strings one after another, ended by an empty one (what `CM_Get_Device_ID_ListW` writes) →
/// the strings.
pub fn multi_sz(units: &[u16]) -> Vec<String> {
    units
        .split(|&unit| unit == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Whether a device instance id ("PCI\VEN_10DE&DEV_2489&…\4&2B5E7C3&0&0008") names a device on the PCI bus.
pub fn is_pci_instance(instance_id: &str) -> bool {
    instance_id
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("PCI\\"))
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
            // A scan run mid-game: VALORANT and Defender are well over the bar, but are not apps to close.
            ("VALORANT-Win64-Shipping.exe".to_string(), 480.0),
            ("MsMpEng.exe".to_string(), 320.0),
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
    fn the_game_riot_software_and_defender_are_never_background_apps() {
        // Instance names as the performance counters give them: no ".exe", and "#<n>" on a second process.
        for instance in [
            "VALORANT-Win64-Shipping",
            "VALORANT",
            "vgc",
            "vgtray",
            "RiotClientServices",
            "Riot Client",
            "RiotClientUx",
            "RiotClientUxRender#2",
            "MsMpEng",
            "MpDefenderCoreService",
            "NisSrv",
        ] {
            let exe = pdh_instance_to_exe(instance).expect(instance);
            let busy = busy_processes(vec![(exe.clone(), 800.0)], 16);
            assert!(busy.is_empty(), "{exe}");
        }
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

    #[test]
    fn exe_names_stop_at_the_first_nul() {
        let mut buffer = [0u16; 260];
        for (i, unit) in "VALORANT-Win64-Shipping.exe".encode_utf16().enumerate() {
            buffer[i] = unit;
        }
        assert_eq!(exe_name(&buffer), "VALORANT-Win64-Shipping.exe");
        let full: Vec<u16> = "abc".encode_utf16().collect();
        assert_eq!(exe_name(&full), "abc", "no NUL: the whole buffer");
        assert_eq!(exe_name(&[0, 65, 66]), "");
    }

    /// CPUID registers holding `text` the way the processor stores it: four bytes per register, little-endian,
    /// padded with NULs to `count` registers.
    fn registers(text: &str, count: usize) -> Vec<u32> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.resize(count * 4, 0);
        let (words, _) = bytes.as_chunks::<4>();
        words.iter().map(|word| u32::from_le_bytes(*word)).collect()
    }

    #[test]
    fn cpuid_registers_spell_the_vendor() {
        // Leaf 0 spells it across EBX, EDX and ECX.
        assert_eq!(
            cpuid_text(&[0x756e_6547, 0x4965_6e69, 0x6c65_746e]),
            "GenuineIntel"
        );
        assert_eq!(
            cpuid_text(&[0x6874_7541, 0x6974_6e65, 0x444d_4163]),
            "AuthenticAMD"
        );
    }

    #[test]
    fn cpuid_brands_lose_their_nuls_and_padding() {
        let brand = registers("11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz", 12);
        assert_eq!(
            cpuid_text(&brand),
            "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz"
        );
        // Older Intel processors right-align the brand with spaces.
        let padded = registers("       Intel(R) Core(TM) i7-4790K CPU @ 4.00GHz", 12);
        assert_eq!(
            cpuid_text(&padded),
            "Intel(R) Core(TM) i7-4790K CPU @ 4.00GHz"
        );
        assert_eq!(cpuid_text(&[0; 12]), "");
    }

    const KB: u32 = 1024;

    /// A RelationProcessorCore record as 64-bit Windows writes it: the header (relationship, size), Flags,
    /// EfficiencyClass, 20 reserved bytes, GroupCount at 30, then a 16-byte GROUP_AFFINITY (Mask, Group) per group.
    fn core_record(efficiency_class: u8, groups: &[(u16, u64)]) -> Vec<u8> {
        let size = 32 + 16 * groups.len();
        let mut record = vec![0u8; size];
        record[4..8].copy_from_slice(&(size as u32).to_le_bytes());
        let threads: u32 = groups.iter().map(|(_, mask)| mask.count_ones()).sum();
        record[8] = u8::from(threads > 1); // LTP_PC_SMT
        record[9] = efficiency_class;
        record[30..32].copy_from_slice(&(groups.len() as u16).to_le_bytes());
        for (index, (group, mask)) in groups.iter().enumerate() {
            let at = 32 + 16 * index;
            record[at..at + 8].copy_from_slice(&mask.to_le_bytes());
            record[at + 8..at + 10].copy_from_slice(&group.to_le_bytes());
        }
        record
    }

    /// A RelationCache record (56 bytes, as Windows 11 writes it): Level at 8, CacheSize in bytes at 12, Type at 16.
    fn cache_record(level: u8, size_bytes: u32, cache_type: u32) -> Vec<u8> {
        let mut record = vec![0u8; 56];
        record[0..4].copy_from_slice(&2u32.to_le_bytes());
        record[4..8].copy_from_slice(&56u32.to_le_bytes());
        record[8] = level;
        record[12..16].copy_from_slice(&size_bytes.to_le_bytes());
        record[16..20].copy_from_slice(&cache_type.to_le_bytes());
        record
    }

    /// A record of a relationship Kestrel skips: 3 is a processor package, 4 a processor group.
    fn other_record(relationship: u32, size: u32) -> Vec<u8> {
        let mut record = vec![0u8; size as usize];
        record[0..4].copy_from_slice(&relationship.to_le_bytes());
        record[4..8].copy_from_slice(&size.to_le_bytes());
        record
    }

    #[test]
    fn an_eight_core_cpu_with_two_threads_per_core() {
        // The i9-11900K: per core 48 KB L1 data, 32 KB L1 instruction and 512 KB L2; one 16 MB L3.
        let mut buffer = other_record(3, 48);
        for core in 0..8 {
            buffer.extend(core_record(0, &[(0, 0b11 << (2 * core))]));
            buffer.extend(cache_record(1, 48 * KB, 2));
            buffer.extend(cache_record(1, 32 * KB, 1));
            buffer.extend(cache_record(2, 512 * KB, 0));
        }
        buffer.extend(cache_record(3, 16 * 1024 * KB, 0));
        buffer.extend(other_record(4, 80));
        let (cores, caches) = logical_processors(&buffer);
        assert_eq!(cores.len(), 8);
        assert!(cores
            .iter()
            .all(|c| c.efficiency_class == 0 && c.logical.len() == 2));
        assert_eq!(cores[3].logical, vec![(0, 6), (0, 7)]);
        assert_eq!(caches.len(), 25);
        assert_eq!(
            caches[..3],
            [
                (1, CacheKind::Data, 48),
                (1, CacheKind::Instruction, 32),
                (2, CacheKind::Unified, 512)
            ]
        );
        assert_eq!(caches[24], (3, CacheKind::Unified, 16384));
    }

    #[test]
    fn a_hybrid_cpu_keeps_each_cores_efficiency_class() {
        // 2 P-cores (class 1) with two threads each, then 4 E-cores (class 0) with one, sharing a 2 MB L2.
        let mut buffer = core_record(1, &[(0, 0b0011)]);
        buffer.extend(core_record(1, &[(0, 0b1100)]));
        for thread in 4..8 {
            buffer.extend(core_record(0, &[(0, 1 << thread)]));
        }
        buffer.extend(cache_record(2, 1280 * KB, 0));
        buffer.extend(cache_record(2, 1280 * KB, 0));
        buffer.extend(cache_record(2, 2048 * KB, 0));
        let (cores, caches) = logical_processors(&buffer);
        let classes: Vec<u8> = cores.iter().map(|c| c.efficiency_class).collect();
        assert_eq!(classes, vec![1, 1, 0, 0, 0, 0]);
        assert_eq!(cores[1].logical, vec![(0, 2), (0, 3)]);
        assert_eq!(cores[5].logical, vec![(0, 7)]);
        assert_eq!(
            caches,
            vec![
                (2, CacheKind::Unified, 1280),
                (2, CacheKind::Unified, 1280),
                (2, CacheKind::Unified, 2048)
            ]
        );
    }

    #[test]
    fn a_core_in_the_second_processor_group_keeps_its_group() {
        // Past 64 logical processors Windows makes more groups; this core's threads are 62 and 63 of group 1.
        let (cores, _) = logical_processors(&core_record(0, &[(1, 0b11 << 62)]));
        assert_eq!(
            cores,
            vec![PhysicalCore {
                efficiency_class: 0,
                logical: vec![(1, 62), (1, 63)],
            }]
        );
    }

    #[test]
    fn a_record_cut_short_ends_the_list() {
        let mut buffer = core_record(0, &[(0, 0b11)]);
        buffer.extend(&cache_record(1, 48 * KB, 2)[..30]);
        let (cores, caches) = logical_processors(&buffer);
        assert_eq!(cores.len(), 1);
        assert!(caches.is_empty());
        // A record that claims no size would never move the walk on, so it ends it.
        let mut stuck = other_record(3, 8);
        stuck[4..8].copy_from_slice(&0u32.to_le_bytes());
        stuck.extend(core_record(0, &[(0, 0b1)]));
        assert_eq!(logical_processors(&stuck), (Vec::new(), Vec::new()));
        // A group count promising more masks than the record holds: only the masks inside it are read.
        let mut short = core_record(0, &[(0, 0b11)]);
        short[30..32].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(
            logical_processors(&short).0[0].logical,
            vec![(0, 0), (0, 1)]
        );
        assert_eq!(logical_processors(&[]), (Vec::new(), Vec::new()));
    }

    #[test]
    fn cache_types_become_kinds() {
        let mut buffer = Vec::new();
        for cache_type in 0..4 {
            buffer.extend(cache_record(1, 64 * KB, cache_type));
        }
        let kinds: Vec<CacheKind> = logical_processors(&buffer).1.iter().map(|c| c.1).collect();
        // 3 is a trace cache (Pentium 4), which the model has no name for.
        assert_eq!(
            kinds,
            vec![
                CacheKind::Unified,
                CacheKind::Instruction,
                CacheKind::Data,
                CacheKind::Unknown
            ]
        );
    }

    #[test]
    fn firmware_types_are_boot_modes() {
        assert_eq!(boot_mode(2), BootMode::Uefi);
        assert_eq!(boot_mode(1), BootMode::Legacy);
        assert_eq!(boot_mode(0), BootMode::Unknown);
        assert_eq!(boot_mode(3), BootMode::Unknown);
    }

    #[test]
    fn a_running_hypervisor_means_virtualization_is_on() {
        // A hypervisor (seen by CPUID, or VBS) takes VT-x / SVM for itself and hides the firmware flag.
        assert_eq!(virtualization(true, None, Some(false)), Some(true));
        assert_eq!(virtualization(false, Some(true), Some(false)), Some(true));
        // Otherwise the firmware flag says, and without it nothing is known.
        assert_eq!(virtualization(false, Some(false), Some(true)), Some(true));
        assert_eq!(virtualization(false, None, Some(false)), Some(false));
        assert_eq!(virtualization(false, None, None), None);
    }

    #[test]
    fn a_laptop_has_no_cpu_socket() {
        // Dragon Range (laptop) shares Raphael's signature, so the table says AM5; its chip is soldered on.
        assert_eq!(cpu_socket(FormFactor::Laptop, Some("AM5")), None);
        assert_eq!(
            cpu_socket(FormFactor::Desktop, Some("AM5")).as_deref(),
            Some("AM5")
        );
        // An unknown form factor (WMI couldn't tell) keeps the table's socket.
        assert_eq!(
            cpu_socket(FormFactor::Unknown, Some("LGA1200")).as_deref(),
            Some("LGA1200")
        );
        assert_eq!(cpu_socket(FormFactor::Desktop, None), None);
    }

    const INTEL_BRAND: &str = "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz";

    /// 2 P-cores with two threads each and 4 E-cores with one: 6 physical cores, 8 logical processors.
    fn hybrid_cores() -> Vec<PhysicalCore> {
        let core = |efficiency_class, threads: &[u8]| PhysicalCore {
            efficiency_class,
            logical: threads.iter().map(|&thread| (0, thread)).collect(),
        };
        vec![
            core(1, &[0, 1]),
            core(1, &[2, 3]),
            core(0, &[4]),
            core(0, &[5]),
            core(0, &[6]),
            core(0, &[7]),
        ]
    }

    #[test]
    fn what_wmi_reported_about_the_cpu_is_kept() {
        // WMI answered, so the processor's own brand, vendor and cores change nothing, even where they differ.
        let wmi = CpuInfo {
            name: "AMD Ryzen 7 7800X3D 8-Core Processor".into(),
            vendor: CpuVendor::Amd,
            cores: 8,
            threads: 16,
            ..Default::default()
        };
        let mut cpu = wmi.clone();
        fill_missing_cpu_identity(&mut cpu, INTEL_BRAND, CpuVendor::Intel);
        fill_missing_cpu_counts(&mut cpu, &hybrid_cores());
        assert_eq!(cpu, wmi);
    }

    #[test]
    fn cpuid_names_a_cpu_that_wmi_did_not() {
        // WMI failed outright: the brand string is the name and CPUID's own vendor the vendor.
        let mut cpu = CpuInfo::default();
        fill_missing_cpu_identity(&mut cpu, INTEL_BRAND, CpuVendor::Intel);
        assert_eq!(
            (cpu.name.as_str(), cpu.vendor),
            (INTEL_BRAND, CpuVendor::Intel)
        );
        // Each is a fallback on its own: WMI gave a name but no maker Kestrel knows...
        let mut cpu = CpuInfo {
            name: "Intel Core".into(),
            ..Default::default()
        };
        fill_missing_cpu_identity(&mut cpu, INTEL_BRAND, CpuVendor::Intel);
        assert_eq!(
            (cpu.name.as_str(), cpu.vendor),
            ("Intel Core", CpuVendor::Intel)
        );
        // ...or the maker but no name.
        let mut cpu = CpuInfo {
            vendor: CpuVendor::Amd,
            ..Default::default()
        };
        fill_missing_cpu_identity(&mut cpu, INTEL_BRAND, CpuVendor::Intel);
        assert_eq!(
            (cpu.name.as_str(), cpu.vendor),
            (INTEL_BRAND, CpuVendor::Amd)
        );
    }

    #[test]
    fn the_core_list_counts_cores_and_threads_that_wmi_did_not() {
        // WMI failed outright: physical cores are the list's entries, logical processors their threads added up.
        let mut cpu = CpuInfo::default();
        fill_missing_cpu_counts(&mut cpu, &hybrid_cores());
        assert_eq!((cpu.cores, cpu.threads), (6, 8));
        // Each is a fallback on its own: WMI gave the threads but not the cores...
        let mut cpu = CpuInfo {
            threads: 12,
            ..Default::default()
        };
        fill_missing_cpu_counts(&mut cpu, &hybrid_cores());
        assert_eq!((cpu.cores, cpu.threads), (6, 12));
        // ...or the cores but not the threads.
        let mut cpu = CpuInfo {
            cores: 4,
            ..Default::default()
        };
        fill_missing_cpu_counts(&mut cpu, &hybrid_cores());
        assert_eq!((cpu.cores, cpu.threads), (4, 8));
    }

    #[test]
    fn nothing_is_made_up_when_the_processor_has_no_answer_either() {
        // CPUID unreadable (no brand, a maker Kestrel doesn't know) and no core list: the CPU stays as empty as WMI
        // left it, which the UI shows as Not reported.
        let mut cpu = CpuInfo::default();
        fill_missing_cpu_identity(&mut cpu, "", CpuVendor::Other);
        fill_missing_cpu_counts(&mut cpu, &[]);
        assert_eq!(cpu, CpuInfo::default());
    }

    /// Instances of a `Processor Information` counter as PDH lists them, as (name, value).
    fn instances(rows: &[(&str, f64)]) -> Vec<(String, f64)> {
        rows.iter()
            .map(|&(name, value)| (name.to_string(), value))
            .collect()
    }

    #[test]
    fn the_total_row_gives_the_rated_clock() {
        // The total is preferred over the per-processor rows and the per-group total, wherever it comes in the list.
        let rows = instances(&[
            ("0,0", 3200.0),
            ("0,1", 3200.0),
            ("0,_Total", 2900.0),
            ("_Total", 2800.0),
        ]);
        assert_eq!(rated_mhz(&rows), Some(2800));
        let rows = instances(&[("_Total", 2800.0), ("0,0", 3200.0)]);
        assert_eq!(rated_mhz(&rows), Some(2800));
        // PDH matches instance names without regard to case.
        assert_eq!(
            rated_mhz(&instances(&[("0,0", 3200.0), ("_total", 2800.0)])),
            Some(2800)
        );
    }

    #[test]
    fn without_a_total_the_first_row_with_a_clock_stands_in() {
        let rows = instances(&[("0,0", 4200.0), ("0,1", 4000.0)]);
        assert_eq!(rated_mhz(&rows), Some(4200));
    }

    #[test]
    fn rows_without_a_usable_clock_are_skipped() {
        // Zero, negative, not-a-number and infinite values mean Windows has no clock to give, on the total as on any
        // other row.
        let rows = instances(&[
            ("_Total", 0.0),
            ("0,0", f64::NAN),
            ("0,1", -1.0),
            ("0,2", f64::INFINITY),
            ("0,3", 3600.0),
            ("0,4", 4000.0),
        ]);
        assert_eq!(rated_mhz(&rows), Some(3600));
        let rows = instances(&[("0,0", 3600.0), ("_Total", f64::NAN)]);
        assert_eq!(rated_mhz(&rows), Some(3600));
        // Under 1 MHz would round to a base clock of 0.
        assert_eq!(rated_mhz(&instances(&[("_Total", 0.4)])), None);
    }

    #[test]
    fn no_usable_clock_anywhere_is_not_reported() {
        assert_eq!(rated_mhz(&[]), None);
        let rows = instances(&[("_Total", 0.0), ("0,0", f64::NAN), ("0,1", -3.0)]);
        assert_eq!(rated_mhz(&rows), None);
    }

    #[test]
    fn the_rated_clock_is_rounded_to_whole_megahertz() {
        assert_eq!(rated_mhz(&instances(&[("_Total", 3499.6)])), Some(3500));
        assert_eq!(rated_mhz(&instances(&[("_Total", 3499.4)])), Some(3499));
    }

    #[test]
    fn device_properties_are_read_only_with_their_expected_type() {
        assert_eq!(device_property_u32(DEVPROP_UINT32, &[4, 0, 0, 0]), Some(4));
        assert_eq!(
            device_property_u32(DEVPROP_UINT32, &[16, 0, 0, 0]),
            Some(16)
        );
        // Another type or size is not a u32, whatever its bytes say.
        assert_eq!(device_property_u32(DEVPROP_BYTE, &[4, 0, 0, 0]), None);
        assert_eq!(device_property_u32(DEVPROP_UINT32, &[4, 0]), None);
        assert_eq!(device_property_byte(DEVPROP_BYTE, &[0x06]), Some(0x06));
        assert_eq!(device_property_byte(DEVPROP_UINT32, &[0x06, 0, 0, 0]), None);
        assert_eq!(device_property_byte(DEVPROP_BYTE, &[]), None);
    }

    #[test]
    fn device_property_text_is_utf16_up_to_its_nul() {
        let bytes: Vec<u8> = "Intel(R) Z590 LPC/eSPI Controller - 4385\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            device_property_text(DEVPROP_STRING, &bytes).as_deref(),
            Some("Intel(R) Z590 LPC/eSPI Controller - 4385")
        );
        assert_eq!(device_property_text(DEVPROP_UINT32, &bytes), None);
        assert_eq!(device_property_text(DEVPROP_STRING, &[0, 0]), None);
    }

    /// The 32-byte header of a memory resource descriptor (MEM_DES or MEM_LARGE_DES): Count, Type, Alloc_Base,
    /// Alloc_End, Flags, Reserved.
    fn memory_header(base: u64, end: u64) -> Vec<u8> {
        let mut header = vec![0u8; 32];
        header[8..16].copy_from_slice(&base.to_le_bytes());
        header[16..24].copy_from_slice(&end.to_le_bytes());
        header
    }

    #[test]
    fn an_allocated_range_runs_from_its_base_to_its_end_inclusive() {
        // A card's 16 MB of registers, the 256 MB window it gets without Resizable BAR, and 8 GB with it.
        assert_eq!(
            allocated_range_bytes(&memory_header(0xF600_0000, 0xF6FF_FFFF)),
            Some(16 << 20)
        );
        assert_eq!(
            allocated_range_bytes(&memory_header(0xE000_0000, 0xEFFF_FFFF)),
            Some(256 << 20)
        );
        assert_eq!(
            allocated_range_bytes(&memory_header(0x40_0000_0000, 0x41_FFFF_FFFF)),
            Some(8 << 30)
        );
        // Data cut before the end address, a range that ends before it starts, and one with no size that fits.
        assert_eq!(
            allocated_range_bytes(&memory_header(0xE000_0000, 0xEFFF_FFFF)[..20]),
            None
        );
        assert_eq!(allocated_range_bytes(&memory_header(0x2000, 0x1FFF)), None);
        assert_eq!(allocated_range_bytes(&memory_header(0, u64::MAX)), None);
    }

    #[test]
    fn a_device_id_list_splits_at_each_nul() {
        let list: Vec<u16> =
            "PCI\\VEN_8086&DEV_4385\\3&11583659&0&F8\0PCI\\VEN_10DE&DEV_2489\\4&2B5E7C3&0&0008\0\0"
                .encode_utf16()
                .collect();
        assert_eq!(
            multi_sz(&list),
            vec![
                "PCI\\VEN_8086&DEV_4385\\3&11583659&0&F8",
                "PCI\\VEN_10DE&DEV_2489\\4&2B5E7C3&0&0008"
            ]
        );
        assert!(multi_sz(&[0]).is_empty());
        assert!(multi_sz(&[]).is_empty());
    }

    #[test]
    fn only_pci_instance_ids_are_pci_devices() {
        assert!(is_pci_instance(
            "PCI\\VEN_10DE&DEV_2489&SUBSYS_88131043&REV_A1\\4&2B5E7C3&0&0008"
        ));
        assert!(is_pci_instance(
            "pci\\ven_1002&dev_73bf\\6&1a2b3c4d&0&00000019"
        ));
        // Virtual adapters, Hyper-V's video, the PCI root bus itself and an IDE channel are not.
        for id in [
            "ROOT\\DISPLAY\\0000",
            "ROOT\\BASICDISPLAY\\0000",
            "VMBUS\\{da0a7802-e377-4aac-8e77-0558eb1073f8}\\{5620e0c7-8062-4dce-aeb7-520c7ef76171}",
            "ACPI\\PNP0A08\\0",
            "PCIIDE\\IDECHANNEL\\4&1&0&0",
            "",
        ] {
            assert!(!is_pci_instance(id), "{id}");
        }
    }

    /// The step of a walk that reads a descriptor whose allocated range runs from `base` to `end` (inclusive).
    fn descriptor(base: u64, end: u64) -> WalkStep {
        WalkStep::Data(memory_header(base, end))
    }

    #[test]
    fn a_list_windows_ends_gives_every_range_in_its_order() {
        // A card's registers, then its big window with Resizable BAR.
        let walk = [
            descriptor(0xF600_0000, 0xF6FF_FFFF),
            descriptor(0x40_0000_0000, 0x41_FFFF_FFFF),
            WalkStep::End,
        ];
        assert_eq!(allocated_ranges(walk), Some(vec![16 << 20, 8 << 30]));
        // A configuration with no descriptor of the type is an empty list, not a failure.
        assert_eq!(allocated_ranges([WalkStep::End]), Some(Vec::new()));
    }

    #[test]
    fn a_walk_that_goes_wrong_gives_no_ranges_at_all() {
        let registers = descriptor(0xF600_0000, 0xF6FF_FFFF);
        let window = descriptor(0x40_0000_0000, 0x41_FFFF_FFFF);
        // The step that failed may have been the card's big window: the ranges before it alone would read as
        // Resizable BAR off.
        let walk = [
            registers.clone(),
            WalkStep::Failed,
            window.clone(),
            WalkStep::End,
        ];
        assert_eq!(allocated_ranges(walk), None);
        assert_eq!(allocated_ranges([WalkStep::Failed]), None);
        // Only Windows saying there are no more descriptors ends a list; a walk that merely stops is not a whole one.
        assert_eq!(allocated_ranges([registers, window]), None);
        assert_eq!(allocated_ranges([]), None);
    }

    #[test]
    fn a_descriptor_with_an_impossible_range_is_skipped_not_failed() {
        // These add nothing (see `allocated_range_bytes`), but the walk is still a whole one.
        let walk = [
            descriptor(0x2000, 0x1FFF),
            descriptor(0, u64::MAX),
            descriptor(0xE000_0000, 0xEFFF_FFFF),
            WalkStep::End,
        ];
        assert_eq!(allocated_ranges(walk), Some(vec![256 << 20]));
    }

    #[test]
    fn a_walk_is_cut_off_after_the_most_descriptors() {
        let registers = descriptor(0xF600_0000, 0xF6FF_FFFF);
        let list =
            |count: usize| std::iter::repeat_n(registers.clone(), count).chain([WalkStep::End]);
        let full =
            allocated_ranges(list(MAX_DESCRIPTORS)).expect("a list of the most is still a list");
        assert_eq!(full.len(), MAX_DESCRIPTORS);
        assert_eq!(allocated_ranges(list(MAX_DESCRIPTORS + 1)), None);
        // A walk that never ends is not followed forever.
        assert_eq!(allocated_ranges(std::iter::repeat(registers)), None);
    }
}
