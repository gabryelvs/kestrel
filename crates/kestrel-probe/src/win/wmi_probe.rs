//! WMI queries. Each query is independent; a failing one adds a ProbeError and leaves its fields as None.
use super::{problem, registry_probe};
use crate::parse;
use kestrel_core::{
    gpu, GpuInfo, MemoryModule, NetAdapter, ProbeError, ServiceState, SystemSnapshot,
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

    pub fn fill(&self, s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
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
        attempt("storage", self.storage(s));
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
        let batteries = self
            .rows("SELECT BatteryStatus FROM Win32_Battery")
            .unwrap_or_default();
        s.machine.manufacturer = text(&row, "Manufacturer");
        s.machine.model = text(&row, "Model");
        s.machine.form_factor =
            parse::form_factor(num(&row, "PCSystemType"), !batteries.is_empty());
        // BatteryStatus 1 = discharging (on battery); anything else = on AC power.
        s.machine.on_battery = batteries
            .first()
            .and_then(|b| num(b, "BatteryStatus"))
            .map(|status| status == 1);
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
                let vendor = gpu::vendor_from(&text(row, "PNPDeviceID"), &name);
                let vram_mb = registry_probe::gpu_memory_mb(&name);
                GpuInfo {
                    is_integrated: gpu::looks_integrated(vendor, &name, vram_mb),
                    driver_version: text(row, "DriverVersion"),
                    driver_date: parse::cim_date(&text(row, "DriverDate")),
                    vram_mb,
                    vendor,
                    name,
                }
            })
            .collect();
        Ok(())
    }

    fn memory(&self, s: &mut SystemSnapshot) -> Outcome {
        let rows = self.rows(
            "SELECT Capacity, ConfiguredClockSpeed, Speed, PartNumber, Manufacturer, BankLabel, DeviceLocator, SMBIOSMemoryType FROM Win32_PhysicalMemory",
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
            })
            .collect();
        s.memory.total_mb = s.memory.modules.iter().map(|m| m.capacity_mb).sum();
        s.memory.ddr_generation = rows
            .first()
            .and_then(|r| num(r, "SMBIOSMemoryType"))
            .and_then(parse::ddr_generation);
        Ok(())
    }

    fn board(&self, s: &mut SystemSnapshot) -> Outcome {
        let board = first(self.rows("SELECT Manufacturer, Product FROM Win32_BaseBoard")?)?;
        s.board.manufacturer = text(&board, "Manufacturer");
        s.board.product = text(&board, "Product");
        let bios = first(self.rows("SELECT SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS")?)?;
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

    fn storage(&self, s: &mut SystemSnapshot) -> Outcome {
        let Some(install) = s.storage.valorant.as_mut() else {
            return Ok(());
        };
        let letter = install.drive_letter.chars().next().unwrap_or('C');
        let namespace = "root\\Microsoft\\Windows\\Storage";
        let partitions = self.rows_in(
            namespace,
            "SELECT DriveLetter, DiskNumber FROM MSFT_Partition",
        )?;
        let disk = partitions
            .iter()
            .find(|p| drive_letter_of(p) == Some(letter))
            .and_then(|p| num(p, "DiskNumber"))
            .ok_or("VALORANT's drive has no partition record")?;
        let disks = self.rows_in(
            namespace,
            "SELECT DeviceId, MediaType, BusType FROM MSFT_PhysicalDisk",
        )?;
        let row = disks
            .iter()
            .find(|d| text(d, "DeviceId") == disk.to_string())
            .ok_or("VALORANT's disk has no physical disk record")?;
        install.media = parse::drive_media(num(row, "BusType"), num(row, "MediaType"));
        Ok(())
    }
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
/// with no letter comes back as "\0"); a plain number is handled too.
fn drive_letter_of(row: &Row) -> Option<char> {
    match row.get("DriveLetter")? {
        Variant::String(v) => v.chars().next(),
        other => variant_num(other).and_then(|n| char::from_u32(n as u32)),
    }
    .map(|c| c.to_ascii_uppercase())
}
