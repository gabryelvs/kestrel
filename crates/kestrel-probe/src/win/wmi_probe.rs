//! WMI queries. Each query is independent; a failing one adds a ProbeError and leaves its fields as None.
use super::{problem, registry_probe, system_info};
use crate::parse;
use kestrel_core::hardware::{self, DiskRecord};
use kestrel_core::{
    gpu, ram, GpuInfo, MemoryModule, NetAdapter, ProbeError, ServiceState, SystemSnapshot,
};
use std::collections::HashMap;
use wmi::{Variant, WMIConnection};

type Row = HashMap<String, Variant>;
type Outcome = Result<(), Box<dyn std::error::Error>>;

pub struct Wmi {
    cimv2: WMIConnection,
}

impl Wmi {
    /// Connects to the default (CIMv2) namespace. On a thread with no COM state, the wmi crate starts COM
    /// (multithreaded) and sets default process security, and it accepts security the process already set.
    /// The connection can't leave this thread, so connect on the thread that probes.
    pub fn connect() -> Result<Self, wmi::WMIError> {
        Ok(Self {
            cimv2: WMIConnection::new()?,
        })
    }

    fn rows(&self, query: &str) -> Result<Vec<Row>, wmi::WMIError> {
        self.cimv2.raw_query(query)
    }

    fn rows_in(&self, namespace: &str, query: &str) -> Result<Vec<Row>, wmi::WMIError> {
        WMIConnection::with_namespace_path(namespace)?.raw_query(query)
    }

    /// `space` is each volume's (letter, free GiB, total GiB), which the scan reads once for everything that needs it.
    pub fn fill(
        &self,
        s: &mut SystemSnapshot,
        errors: &mut Vec<ProbeError>,
        space: &[(char, f64, f64)],
    ) {
        let mut attempt = |name: &str, result: Outcome| {
            if let Err(e) = result {
                errors.push(problem(name, e));
            }
        };
        attempt("os", self.os(s));
        attempt("cpu", self.cpu(s));
        attempt("machine", self.machine(s));
        attempt("gpus", self.gpus(s));
        attempt("memory", self.memory(s));
        attempt("board", self.board(s));
        attempt("tpm", self.tpm(s));
        attempt("device-guard", self.device_guard(s));
        attempt("vanguard", self.vanguard(s));
        attempt("network", self.network(s));
        attempt("drives", self.drives(s, space));
        attempt("storage", valorant_media(s));
    }

    fn os(&self, s: &mut SystemSnapshot) -> Outcome {
        let row = first(self.rows("SELECT Caption, BuildNumber FROM Win32_OperatingSystem")?)?;
        s.os.caption = text(&row, "Caption");
        s.os.build = num(&row, "BuildNumber").unwrap_or(0) as u32;
        Ok(())
    }

    fn cpu(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows(
            "SELECT Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors FROM Win32_Processor",
        )?;
        let row = rows.first().ok_or("no processor reported")?;
        s.cpu.name = text(row, "Name");
        s.cpu.vendor = parse::cpu_vendor(&text(row, "Manufacturer"));
        s.cpu.cores = rows
            .iter()
            .filter_map(|r| num(r, "NumberOfCores"))
            .sum::<u64>() as u32;
        s.cpu.threads = rows
            .iter()
            .filter_map(|r| num(r, "NumberOfLogicalProcessors"))
            .sum::<u64>() as u32;
        Ok(())
    }

    fn machine(&self, s: &mut SystemSnapshot) -> Outcome {
        let row = first(
            self.rows("SELECT Manufacturer, Model, PCSystemType FROM Win32_ComputerSystem")?,
        )?;
        s.machine.manufacturer = real_name(&row, "Manufacturer");
        s.machine.model = real_name(&row, "Model");
        // The battery query can fail on its own. It is reported at the end, after everything the other query told
        // us is kept; meanwhile the PC counts as having no battery, and on_battery stays None.
        let batteries = self.rows("SELECT BatteryStatus FROM Win32_Battery");
        let battery_rows = batteries.as_deref().unwrap_or_default();
        s.machine.form_factor =
            parse::form_factor(num(&row, "PCSystemType"), !battery_rows.is_empty());
        s.machine.on_battery = battery_rows
            .first()
            .and_then(|b| num(b, "BatteryStatus"))
            .map(parse::on_battery);
        if let Err(e) = batteries {
            return Err(format!("couldn't read the battery (Win32_Battery): {e}").into());
        }
        Ok(())
    }

    fn gpus(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows(
            "SELECT Name, PNPDeviceID, DriverVersion, DriverDate FROM Win32_VideoController",
        )?;
        s.gpus = rows
            .iter()
            .map(|row| {
                let name = text(row, "Name");
                let pnp_device_id = text(row, "PNPDeviceID");
                let vendor = gpu::vendor_from(&pnp_device_id, &name);
                let vram_mb = registry_probe::gpu_memory_mb(&name);
                GpuInfo {
                    is_integrated: gpu::looks_integrated(vendor, &name, vram_mb),
                    driver_version: text(row, "DriverVersion"),
                    driver_date: parse::cim_date(&text(row, "DriverDate")),
                    vram_mb,
                    vendor,
                    name,
                    pnp_device_id,
                    ..Default::default()
                }
            })
            .collect();
        Ok(())
    }

    fn memory(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows(
            "SELECT Capacity, ConfiguredClockSpeed, Speed, PartNumber, Manufacturer, BankLabel, DeviceLocator, SMBIOSMemoryType, Attributes FROM Win32_PhysicalMemory",
        )?;
        s.memory.modules = rows
            .iter()
            .map(|row| MemoryModule {
                capacity_mb: num(row, "Capacity").unwrap_or(0) / (1024 * 1024),
                configured_mts: num(row, "ConfiguredClockSpeed").map(|v| v as u32),
                max_mts: num(row, "Speed").map(|v| v as u32),
                part_number: text(row, "PartNumber"),
                manufacturer: text(row, "Manufacturer"),
                bank_label: text(row, "BankLabel"),
                device_locator: text(row, "DeviceLocator"),
                rank: num(row, "Attributes").and_then(parse::memory_rank),
            })
            .collect();
        s.memory.total_mb = s.memory.modules.iter().map(|m| m.capacity_mb).sum();
        s.memory.ddr_generation = rows
            .first()
            .and_then(|r| num(r, "SMBIOSMemoryType"))
            .and_then(parse::ddr_generation);
        // Some boards report the clock in MHz (half the MT/s); this puts every module in MT/s.
        ram::normalize_configured(&mut s.memory.modules, s.memory.ddr_generation);
        // The kit's rating from its part numbers, which the Hardware tab shows (the speeds don't change it).
        s.memory.rated_mts = ram::rated_mts(&s.memory.modules);
        // The slot count can fail on its own: the modules above are kept, and the error is reported after them.
        let arrays = self
            .rows("SELECT Use, MemoryDevices FROM Win32_PhysicalMemoryArray")
            .map_err(|e| {
                format!("couldn't read the memory slots (Win32_PhysicalMemoryArray): {e}")
            })?;
        let arrays: Vec<(Option<u64>, Option<u64>)> = arrays
            .iter()
            .map(|a| (num(a, "Use"), num(a, "MemoryDevices")))
            .collect();
        s.memory.slots_total = parse::memory_slots(&arrays);
        Ok(())
    }

    fn board(&self, s: &mut SystemSnapshot) -> Outcome {
        let board = first(self.rows("SELECT Manufacturer, Product FROM Win32_BaseBoard")?)?;
        s.board.manufacturer = real_name(&board, "Manufacturer");
        s.board.product = real_name(&board, "Product");
        let bios = first(
            self.rows("SELECT Manufacturer, SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS")?,
        )?;
        s.board.bios_vendor = real_name(&bios, "Manufacturer");
        s.board.bios_version = text(&bios, "SMBIOSBIOSVersion");
        s.board.bios_date = parse::cim_date(&text(&bios, "ReleaseDate"));
        Ok(())
    }

    fn tpm(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows_in(
            "root\\CIMV2\\Security\\MicrosoftTpm",
            "SELECT IsEnabled_InitialValue, SpecVersion FROM Win32_Tpm",
        )?;
        s.security.tpm2 = Some(rows.first().is_some_and(|r| {
            flag(r, "IsEnabled_InitialValue") == Some(true)
                && text(r, "SpecVersion").starts_with("2.0")
        }));
        Ok(())
    }

    fn device_guard(&self, s: &mut SystemSnapshot) -> Outcome {
        let row = first(self.rows_in(
            "root\\Microsoft\\Windows\\DeviceGuard",
            "SELECT VirtualizationBasedSecurityStatus, SecurityServicesRunning FROM Win32_DeviceGuard",
        )?)?;
        s.security.vbs_running =
            num(&row, "VirtualizationBasedSecurityStatus").map(|status| status == 2);
        s.security.hvci_running = Some(nums(&row, "SecurityServicesRunning").contains(&2));
        Ok(())
    }

    fn vanguard(&self, s: &mut SystemSnapshot) -> Outcome {
        // Read-only: we only ask whether Vanguard's service exists and runs. Kestrel never starts, stops or changes it.
        let rows = self.rows("SELECT State FROM Win32_Service WHERE Name = 'vgc'")?;
        s.security.vanguard = match rows.first().map(|r| text(r, "State")) {
            None => ServiceState::NotInstalled,
            Some(state) if state == "Running" => ServiceState::Running,
            Some(_) => ServiceState::Stopped,
        };
        Ok(())
    }

    fn network(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows_in(
            "root\\StandardCimv2",
            "SELECT Name, DeviceID, NdisPhysicalMedium, Speed, MediaConnectState, HardwareInterface, Virtual FROM MSFT_NetAdapter",
        )?;
        s.network.adapters = rows
            .iter()
            .filter(|r| {
                flag(r, "HardwareInterface") == Some(true) && flag(r, "Virtual") != Some(true)
            })
            .map(|r| NetAdapter {
                name: text(r, "Name"),
                kind: parse::net_kind(num(r, "NdisPhysicalMedium").unwrap_or(0)),
                link_mbps: num(r, "Speed")
                    .filter(|&bps| bps > 0)
                    .map(|bps| bps / 1_000_000),
                connected: num(r, "MediaConnectState") == Some(1),
                power_saving: registry_probe::adapter_power_saving(&text(r, "DeviceID")),
            })
            .collect();
        Ok(())
    }

    /// Every physical drive with its lettered volumes and their space (`storage.drives`). `space` is each volume's
    /// (letter, free GiB, total GiB).
    fn drives(&self, s: &mut SystemSnapshot, space: &[(char, f64, f64)]) -> Outcome {
        let namespace = "root\\Microsoft\\Windows\\Storage";
        let disks: Vec<DiskRecord> = self
            .rows_in(
                namespace,
                "SELECT DeviceId, FriendlyName, MediaType, BusType, Size FROM MSFT_PhysicalDisk",
            )?
            .iter()
            .filter_map(|d| {
                let bus_type = num(d, "BusType");
                Some(DiskRecord {
                    number: u32::try_from(num(d, "DeviceId")?).ok()?,
                    model: text(d, "FriendlyName"),
                    media: parse::drive_media(bus_type, num(d, "MediaType")),
                    bus: parse::drive_bus(bus_type),
                    size_gb: num(d, "Size").unwrap_or(0) as f64 / system_info::GIB,
                })
            })
            .collect();
        let partitions: Vec<(u32, char)> = self
            .rows_in(
                namespace,
                "SELECT DiskNumber, DriveLetter FROM MSFT_Partition",
            )?
            .iter()
            .filter_map(|p| {
                Some((
                    u32::try_from(num(p, "DiskNumber")?).ok()?,
                    drive_letter_of(p)?,
                ))
            })
            .collect();
        s.storage.drives = hardware::drives(&disks, &partitions, space);
        Ok(())
    }
}

/// VALORANT's drive type: the type of the drive that holds its letter, from the list `drives` just read. An empty
/// list has its own error already.
fn valorant_media(s: &mut SystemSnapshot) -> Outcome {
    let Some(install) = s.storage.valorant.as_mut() else {
        return Ok(());
    };
    if s.storage.drives.is_empty() {
        return Ok(());
    }
    let drive = s
        .storage
        .drives
        .iter()
        .find(|d| {
            d.volumes.iter().any(|v| {
                v.letter
                    .trim_end_matches(':')
                    .eq_ignore_ascii_case(&install.drive_letter)
            })
        })
        .ok_or("VALORANT's drive isn't among the drives Windows listed")?;
    install.media = drive.media;
    Ok(())
}

fn first(mut rows: Vec<Row>) -> Result<Row, Box<dyn std::error::Error>> {
    if rows.is_empty() {
        Err("no rows returned".into())
    } else {
        Ok(rows.swap_remove(0))
    }
}

fn text(row: &Row, key: &str) -> String {
    match row.get(key) {
        Some(Variant::String(v)) => v.trim().to_string(),
        _ => String::new(),
    }
}

/// A maker's or model's name, or "" when the firmware left filler there ("To be filled by O.E.M.", "Default string"…).
fn real_name(row: &Row, key: &str) -> String {
    let value = text(row, key);
    if hardware::is_placeholder(&value) {
        String::new()
    } else {
        value
    }
}

fn variant_num(v: &Variant) -> Option<u64> {
    match v {
        Variant::UI1(x) => Some(u64::from(*x)),
        Variant::UI2(x) => Some(u64::from(*x)),
        Variant::UI4(x) => Some(u64::from(*x)),
        Variant::UI8(x) => Some(*x),
        Variant::I1(x) => u64::try_from(*x).ok(),
        Variant::I2(x) => u64::try_from(*x).ok(),
        Variant::I4(x) => u64::try_from(*x).ok(),
        Variant::I8(x) => u64::try_from(*x).ok(),
        Variant::String(x) => x.trim().parse().ok(),
        _ => None,
    }
}

fn num(row: &Row, key: &str) -> Option<u64> {
    variant_num(row.get(key)?)
}

fn nums(row: &Row, key: &str) -> Vec<u64> {
    match row.get(key) {
        Some(Variant::Array(items)) => items.iter().filter_map(variant_num).collect(),
        _ => Vec::new(),
    }
}

fn flag(row: &Row, key: &str) -> Option<bool> {
    match row.get(key)? {
        Variant::Bool(v) => Some(*v),
        _ => None,
    }
}

/// MSFT_Partition.DriveLetter is a CIM char16. The wmi crate returns it as a one-letter string (a partition
/// with no letter comes back as "\0", which is None here); a plain number is handled too.
fn drive_letter_of(row: &Row) -> Option<char> {
    match row.get("DriveLetter")? {
        Variant::String(v) => v.chars().next(),
        other => variant_num(other).and_then(|n| char::from_u32(n as u32)),
    }
    .filter(char::is_ascii_alphabetic)
    .map(|c| c.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_core::{DriveInfo, DriveMedia, StorageInfo, ValorantInstall, VolumeInfo};

    fn string(text: &str) -> Variant {
        Variant::String(text.into())
    }

    fn row(entries: &[(&str, Variant)]) -> Row {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn firmware_filler_is_not_a_name() {
        let board = row(&[
            ("Manufacturer", string("To be filled by O.E.M.")),
            ("Product", string("  ROG STRIX B550-F GAMING  ")),
        ]);
        assert_eq!(real_name(&board, "Manufacturer"), "");
        assert_eq!(real_name(&board, "Product"), "ROG STRIX B550-F GAMING");
        // A property the row doesn't have is no name either.
        assert_eq!(real_name(&board, "Model"), "");
    }

    #[test]
    fn a_partition_has_a_drive_letter_only_when_windows_gives_a_letter() {
        let letter = |value| drive_letter_of(&row(&[("DriveLetter", value)]));
        assert_eq!(letter(string("c")), Some('C'));
        // A plain number is a character code.
        assert_eq!(letter(Variant::UI2(u16::from(b'D'))), Some('D'));
        // No letter comes back as a NUL character (or its code), or as no value at all.
        assert_eq!(letter(string("\0")), None);
        assert_eq!(letter(Variant::UI2(0)), None);
        assert_eq!(letter(Variant::Null), None);
        assert_eq!(drive_letter_of(&row(&[])), None);
    }

    /// A drive of `media` with a volume for each of `letters`.
    fn drive(media: DriveMedia, letters: &[&str]) -> DriveInfo {
        DriveInfo {
            media,
            volumes: letters
                .iter()
                .map(|letter| VolumeInfo {
                    letter: letter.to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// A scan that found VALORANT on `valorant_letter` (if installed) and listed `drives`.
    fn scanned(valorant_letter: Option<&str>, drives: Vec<DriveInfo>) -> SystemSnapshot {
        SystemSnapshot {
            storage: StorageInfo {
                valorant: valorant_letter.map(|letter| ValorantInstall {
                    drive_letter: letter.into(),
                    ..Default::default()
                }),
                drives,
            },
            ..Default::default()
        }
    }

    #[test]
    fn valorants_drive_type_is_that_of_the_drive_holding_its_letter() {
        let drives = vec![
            drive(DriveMedia::Nvme, &["C"]),
            drive(DriveMedia::Hdd, &["D", "E"]),
        ];
        // The letter is compared without regard to case.
        let mut s = scanned(Some("e"), drives);
        valorant_media(&mut s).expect("the drive is listed");
        assert_eq!(s.storage.valorant.map(|v| v.media), Some(DriveMedia::Hdd));
    }

    #[test]
    fn a_letter_on_none_of_the_listed_drives_is_reported_and_the_type_stays_unknown() {
        let mut s = scanned(Some("F"), vec![drive(DriveMedia::Nvme, &["C"])]);
        let error = valorant_media(&mut s).expect_err("no drive holds F");
        assert_eq!(
            error.to_string(),
            "VALORANT's drive isn't among the drives Windows listed"
        );
        assert_eq!(
            s.storage.valorant.map(|v| v.media),
            Some(DriveMedia::Unknown)
        );
    }

    #[test]
    fn without_a_drive_list_or_an_install_there_is_no_error_to_add() {
        // A failed drive list has its own error, and no VALORANT means no drive to find.
        let mut no_list = scanned(Some("C"), Vec::new());
        valorant_media(&mut no_list).expect("an empty list is reported by `drives`");
        assert_eq!(
            no_list.storage.valorant.map(|v| v.media),
            Some(DriveMedia::Unknown)
        );
        let mut no_install = scanned(None, vec![drive(DriveMedia::Nvme, &["C"])]);
        valorant_media(&mut no_install).expect("nothing to look for");
        assert_eq!(no_install.storage.valorant, None);
    }
}
