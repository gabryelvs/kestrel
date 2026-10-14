//! The data model: everything Kestrel knows about a PC. `None` always means "couldn't tell".
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 2;

macro_rules! model {
    ($($item:item)*) => { $(
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        #[serde(rename_all = "camelCase", default)]
        $item
    )* };
}

macro_rules! choice {
    ($($item:item)*) => { $(
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "kebab-case")]
        $item
    )* };
}

model! {
    pub struct SystemSnapshot {
        pub schema_version: u32,
        pub collected_at: String,
        pub os: OsInfo,
        pub machine: MachineInfo,
        pub cpu: CpuInfo,
        pub gpus: Vec<GpuInfo>,
        pub memory: MemoryInfo,
        pub displays: Vec<DisplayInfo>,
        pub board: BoardInfo,
        pub storage: StorageInfo,
        pub windows: WindowsSettings,
        pub security: SecurityInfo,
        pub network: NetworkInfo,
        pub background: BackgroundInfo,
        pub probe_errors: Vec<ProbeError>,
    }
    pub struct OsInfo { pub caption: String, pub build: u32, pub display_version: Option<String> }
    pub struct MachineInfo { pub form_factor: FormFactor, pub manufacturer: String, pub model: String, pub on_battery: Option<bool> }
    pub struct CpuInfo {
        pub name: String,
        pub vendor: CpuVendor,
        pub cores: u32,
        pub threads: u32,
        /// CPUID display family and model (extended fields already added) and stepping.
        pub family: Option<u32>,
        pub model: Option<u32>,
        pub stepping: Option<u32>,
        pub codename: Option<String>,
        pub socket: Option<String>,
        pub base_mhz: Option<u32>,
        pub max_mhz: Option<u32>,
        /// Some only on hybrid CPUs (cores of more than one efficiency class).
        pub p_cores: Option<u32>,
        pub e_cores: Option<u32>,
        pub caches: Vec<CacheInfo>,
        /// Sets the CPU has and Windows enables, in this order: "SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI", "AMX".
        pub instructions: Vec<String>,
        /// Intel VT-x / AMD SVM on in the firmware, or a hypervisor running (which hides the firmware flag).
        pub virtualization: Option<bool>,
    }
    /// `count` caches of one level, kind and size (e.g. 8 × 48 KB L1 data).
    pub struct CacheInfo { pub level: u8, pub kind: CacheKind, pub size_kb: u32, pub count: u32 }
    pub struct GpuInfo {
        pub name: String,
        pub vendor: GpuVendor,
        pub driver_version: String,
        pub driver_date: Option<String>,
        pub vram_mb: Option<u64>,
        pub is_integrated: bool,
        /// Win32_VideoController.PNPDeviceID, e.g. "PCI\\VEN_10DE&DEV_2489&SUBSYS_…\\4&…". Empty when unknown.
        pub pnp_device_id: String,
        pub pcie: Option<PcieInfo>,
        /// Filled by the app from the newest benchmark measured on this card, never by the probe.
        pub link_under_load: Option<LinkUnderLoad>,
        pub largest_bar_mb: Option<u64>,
        pub resizable_bar: Option<bool>,
    }
    /// A PCIe link: generation (4 = PCIe 4.0) and lanes (16 = x16).
    pub struct PcieLink { pub gen: u8, pub width: u8 }
    /// What the card and its slot support, and the link right now (cards slow it down when idle).
    pub struct PcieInfo { pub card_max: Option<PcieLink>, pub slot_max: Option<PcieLink>, pub now: Option<PcieLink> }
    /// The fastest link seen during a benchmark on this card. `measured_at` is RFC 3339.
    pub struct LinkUnderLoad { pub link: PcieLink, pub measured_at: String, pub benchmark_id: String }
    pub struct MemoryInfo {
        pub total_mb: u64,
        pub ddr_generation: Option<u8>,
        pub modules: Vec<MemoryModule>,
        pub slots_total: Option<u32>,
        /// The kit's rated speed in MT/s from the modules' part numbers (`ram::rated_mts`); None when Kestrel doesn't
        /// recognise them. The probe fills it, so the UI never reads it out of a finding's wording.
        pub rated_mts: Option<u32>,
    }
    pub struct MemoryModule {
        pub capacity_mb: u64,
        /// The speed the module runs at now, in MT/s (`Win32_PhysicalMemory.ConfiguredClockSpeed`).
        pub configured_mts: Option<u32>,
        /// The module's own maximum from SMBIOS, in MT/s (`Win32_PhysicalMemory.Speed`). This is its JEDEC
        /// maximum, never an XMP or EXPO rating, so it must not be read as the kit's rated speed.
        pub max_mts: Option<u32>,
        pub part_number: String,
        pub manufacturer: String,
        pub bank_label: String,
        pub device_locator: String,
        /// Ranks on the module (`Win32_PhysicalMemory.Attributes`): 1 single, 2 dual.
        pub rank: Option<u32>,
    }
    pub struct DisplayInfo {
        pub name: String,
        pub adapter: String,
        pub current_hz: u32,
        pub max_hz: u32,
        pub width: u32,
        pub height: u32,
        pub is_primary: bool,
        pub is_virtual: bool,
        /// The monitor's own name from its EDID, e.g. "XL2566K".
        pub monitor_name: Option<String>,
        /// How it is plugged in: "DisplayPort", "HDMI", "DVI", "VGA", "USB-C", "Wireless" or "Internal".
        pub connection: Option<String>,
    }
    pub struct BoardInfo {
        pub manufacturer: String,
        pub product: String,
        pub bios_version: String,
        pub bios_date: Option<String>,
        pub bios_vendor: String,
        /// Maker and chipset, e.g. "Intel Z590".
        pub chipset: Option<String>,
        pub boot_mode: BootMode,
    }
    pub struct StorageInfo { pub valorant: Option<ValorantInstall>, pub drives: Vec<DriveInfo> }
    pub struct ValorantInstall { pub path: String, pub drive_letter: String, pub media: DriveMedia, pub free_gb: f64, pub total_gb: f64 }
    /// A physical drive. Sizes are GiB (bytes / 1024³), like `ValorantInstall`'s.
    pub struct DriveInfo { pub model: String, pub media: DriveMedia, pub bus: DriveBus, pub size_gb: f64, pub volumes: Vec<VolumeInfo> }
    /// A lettered volume on a drive: "C", with its free and total GiB.
    pub struct VolumeInfo { pub letter: String, pub free_gb: f64, pub total_gb: f64 }
    pub struct PowerPlan { pub guid: String, pub name: String }
    pub struct WindowsSettings { pub power_plan: Option<PowerPlan>, pub game_mode: Option<bool>, pub background_recording: Option<bool>, pub hags: Option<bool>, pub valorant_gpu_preference: Option<u8>, pub pending_restart: Option<bool>, pub startup_app_count: Option<u32> }
    pub struct SecurityInfo { pub tpm2: Option<bool>, pub secure_boot: Option<bool>, pub vbs_running: Option<bool>, pub hvci_running: Option<bool>, pub iommu: Option<bool>, pub vanguard: ServiceState }
    pub struct NetAdapter { pub name: String, pub kind: NetKind, pub link_mbps: Option<u64>, pub connected: bool, pub power_saving: Option<bool> }
    pub struct NetworkInfo { pub adapters: Vec<NetAdapter>, pub delivery_optimization_internet: Option<bool> }
    pub struct BusyProcess { pub name: String, pub cpu_percent: f32 }
    pub struct BackgroundInfo { pub busy_processes: Option<Vec<BusyProcess>> }
    pub struct ProbeError { pub probe: String, pub message: String }
}

choice! {
    pub enum FormFactor { Desktop, Laptop, #[default] Unknown }
    pub enum CpuVendor { Intel, Amd, #[default] Other }
    pub enum GpuVendor { Nvidia, Amd, Intel, Virtual, #[default] Other }
    pub enum DriveMedia { Nvme, Ssd, Hdd, #[default] Unknown }
    pub enum ServiceState { Running, Stopped, NotInstalled, #[default] Unknown }
    pub enum NetKind { Ethernet, Wifi, #[default] Other }
    pub enum CacheKind { Data, Instruction, Unified, #[default] Unknown }
    pub enum BootMode { Uefi, Legacy, #[default] Unknown }
    pub enum DriveBus { Nvme, Sata, Usb, Sas, Raid, Virtual, #[default] Other }
}

pub const OWNER_PC_JSON: &str = include_str!("../fixtures/owner-pc.json");

/// The owner's PC as a sample snapshot. Used by the mock probe on non-Windows machines and by tests.
pub fn owner_pc_fixture() -> SystemSnapshot {
    serde_json::from_str(OWNER_PC_JSON).expect("bundled fixture is valid JSON for SystemSnapshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_fixture_parses() {
        let s = owner_pc_fixture();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.cpu.threads, 16);
        assert_eq!(s.memory.modules.len(), 2);
        assert_eq!(s.security.vanguard, ServiceState::Running);
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let s = owner_pc_fixture();
        let json = serde_json::to_string(&s).unwrap();
        let again: SystemSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, again);
    }

    #[test]
    fn fixture_loses_nothing_when_parsed() {
        // Every key in the file is a field of the model (serde would skip a misspelt one), and every field is in the
        // file, so the sample PC shows every part of the Hardware tab. Sizes in GiB are f64: write 1863.0, not 1863.
        let written: serde_json::Value = serde_json::from_str(OWNER_PC_JSON).unwrap();
        assert_eq!(serde_json::to_value(owner_pc_fixture()).unwrap(), written);
    }

    #[test]
    fn owner_fixture_has_the_schema_2_parts() {
        let s = owner_pc_fixture();
        let cache = |level, kind, size_kb, count| CacheInfo {
            level,
            kind,
            size_kb,
            count,
        };
        assert_eq!(
            s.cpu,
            CpuInfo {
                name: "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz".into(),
                vendor: CpuVendor::Intel,
                cores: 8,
                threads: 16,
                family: Some(6),
                model: Some(0xA7),
                stepping: Some(1),
                codename: Some("Rocket Lake-S".into()),
                socket: Some("LGA1200".into()),
                base_mhz: Some(3500),
                max_mhz: Some(5300),
                p_cores: None,
                e_cores: None,
                caches: vec![
                    cache(1, CacheKind::Data, 48, 8),
                    cache(1, CacheKind::Instruction, 32, 8),
                    cache(2, CacheKind::Unified, 512, 8),
                    cache(3, CacheKind::Unified, 16384, 1),
                ],
                instructions: vec![
                    "SSE4.2".into(),
                    "AVX".into(),
                    "AVX2".into(),
                    "AVX-512".into()
                ],
                virtualization: Some(true),
            }
        );
        let board = &s.board;
        assert_eq!(board.bios_vendor, "American Megatrends Inc.");
        assert_eq!(board.chipset.as_deref(), Some("Intel Z590"));
        assert_eq!(board.boot_mode, BootMode::Uefi);
        assert_eq!(s.memory.slots_total, Some(4));
        assert_eq!(s.memory.rated_mts, Some(3600));
        assert!(s.memory.modules.iter().all(|m| m.rank == Some(2)));

        let parsec = &s.gpus[0];
        assert_eq!(parsec.pnp_device_id, "ROOT\\DISPLAY\\0000");
        assert_eq!(parsec.pcie, None);
        let rtx = &s.gpus[1];
        assert_eq!(
            rtx.pnp_device_id,
            "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&2B0D3A0E&0&0008"
        );
        let link = |gen, width| Some(PcieLink { gen, width });
        assert_eq!(
            rtx.pcie,
            Some(PcieInfo {
                card_max: link(4, 16),
                slot_max: link(4, 16),
                now: link(1, 16),
            })
        );
        assert_eq!(rtx.link_under_load, None);
        assert_eq!(rtx.largest_bar_mb, Some(8192));
        assert_eq!(rtx.resizable_bar, Some(true));

        assert_eq!(s.displays[0].monitor_name.as_deref(), Some("XL2566K"));
        assert_eq!(s.displays[0].connection.as_deref(), Some("DisplayPort"));
        assert_eq!(
            s.displays[1],
            DisplayInfo {
                name: "\\\\.\\DISPLAY2".into(),
                adapter: "NVIDIA GeForce RTX 3060 Ti".into(),
                current_hz: 180,
                max_hz: 180,
                width: 1920,
                height: 1080,
                is_primary: false,
                is_virtual: false,
                monitor_name: Some("24G4".into()),
                connection: Some("DisplayPort".into()),
            }
        );

        let volume = |letter: &str, free_gb, total_gb| VolumeInfo {
            letter: letter.into(),
            free_gb,
            total_gb,
        };
        assert_eq!(
            s.storage.drives,
            [
                DriveInfo {
                    model: "Samsung SSD 980 PRO 1TB".into(),
                    media: DriveMedia::Nvme,
                    bus: DriveBus::Nvme,
                    size_gb: 931.5,
                    volumes: vec![volume("C", 210.5, 931.5)],
                },
                DriveInfo {
                    model: "WDC WD20EZBX-00AYRA0".into(),
                    media: DriveMedia::Hdd,
                    bus: DriveBus::Sata,
                    size_gb: 1863.0,
                    volumes: vec![volume("D", 1228.8, 1863.0)],
                },
            ]
        );
    }

    /// The parts of a snapshot v0.2.0 saved (schema 1) that gain fields in schema 2, trimmed from its fixture.
    const SCHEMA_1_JSON: &str = r#"{
      "schemaVersion": 1,
      "collectedAt": "2026-10-09T16:30:00Z",
      "cpu": { "name": "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz", "vendor": "intel", "cores": 8, "threads": 16 },
      "gpus": [
        { "name": "Parsec Virtual Display Adapter", "vendor": "virtual", "driverVersion": "0.45.0.0", "driverDate": null, "vramMb": null, "isIntegrated": false },
        { "name": "NVIDIA GeForce RTX 3060 Ti", "vendor": "nvidia", "driverVersion": "32.0.16.1074", "driverDate": "2026-09-15", "vramMb": 8192, "isIntegrated": false }
      ],
      "memory": { "totalMb": 32768, "ddrGeneration": 4, "modules": [
        { "capacityMb": 16384, "configuredMts": 3600, "maxMts": 2133, "partNumber": "CMG32GX4M2D3600C18", "manufacturer": "Corsair", "bankLabel": "BANK 1", "deviceLocator": "ChannelA-DIMM2" }
      ] },
      "displays": [
        { "name": "\\\\.\\DISPLAY1", "adapter": "NVIDIA GeForce RTX 3060 Ti", "currentHz": 360, "maxHz": 360, "width": 1920, "height": 1080, "isPrimary": true, "isVirtual": false }
      ],
      "board": { "manufacturer": "ASUSTeK COMPUTER INC.", "product": "ROG MAXIMUS XIII HERO", "biosVersion": "2103", "biosDate": "2024-05-20" },
      "storage": { "valorant": { "path": "C:/Riot Games/VALORANT/live", "driveLetter": "C", "media": "nvme", "freeGb": 210.5, "totalGb": 931.5 } }
    }"#;

    #[test]
    fn a_schema_1_snapshot_still_loads_with_the_new_fields_empty() {
        let s: SystemSnapshot = serde_json::from_str(SCHEMA_1_JSON).expect("schema 1 still loads");
        assert_eq!(s.schema_version, 1);
        assert_eq!(
            s.cpu,
            CpuInfo {
                name: "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz".into(),
                vendor: CpuVendor::Intel,
                cores: 8,
                threads: 16,
                ..Default::default()
            }
        );
        assert_eq!(s.gpus[0].pnp_device_id, "");
        assert_eq!(
            s.gpus[1],
            GpuInfo {
                name: "NVIDIA GeForce RTX 3060 Ti".into(),
                vendor: GpuVendor::Nvidia,
                driver_version: "32.0.16.1074".into(),
                driver_date: Some("2026-09-15".into()),
                vram_mb: Some(8192),
                is_integrated: false,
                ..Default::default()
            }
        );
        assert_eq!(s.memory.slots_total, None);
        assert_eq!(s.memory.rated_mts, None);
        assert_eq!(
            s.memory.modules[0],
            MemoryModule {
                capacity_mb: 16384,
                configured_mts: Some(3600),
                max_mts: Some(2133),
                part_number: "CMG32GX4M2D3600C18".into(),
                manufacturer: "Corsair".into(),
                bank_label: "BANK 1".into(),
                device_locator: "ChannelA-DIMM2".into(),
                rank: None,
            }
        );
        let display = &s.displays[0];
        assert_eq!(display.current_hz, 360);
        assert_eq!(display.monitor_name, None);
        assert_eq!(display.connection, None);
        assert_eq!(
            s.board,
            BoardInfo {
                manufacturer: "ASUSTeK COMPUTER INC.".into(),
                product: "ROG MAXIMUS XIII HERO".into(),
                bios_version: "2103".into(),
                bios_date: Some("2024-05-20".into()),
                ..Default::default()
            }
        );
        assert_eq!(s.board.boot_mode, BootMode::Unknown);
        assert!(s.storage.drives.is_empty());
        assert_eq!(
            s.storage.valorant.map(|v| v.drive_letter).as_deref(),
            Some("C")
        );
    }

    #[test]
    fn enums_use_kebab_case_and_fields_camel_case() {
        assert_eq!(
            serde_json::to_string(&DriveMedia::Nvme).unwrap(),
            "\"nvme\""
        );
        assert_eq!(
            serde_json::to_string(&ServiceState::NotInstalled).unwrap(),
            "\"not-installed\""
        );
        let json = serde_json::to_string(&OsInfo {
            caption: "x".into(),
            build: 1,
            display_version: None,
        })
        .unwrap();
        assert!(json.contains("displayVersion"));
    }

    #[test]
    fn default_snapshot_means_unknown_everywhere() {
        let s = SystemSnapshot::default();
        assert_eq!(s.machine.form_factor, FormFactor::Unknown);
        assert_eq!(s.security.vanguard, ServiceState::Unknown);
        assert!(s.storage.valorant.is_none());
        assert_eq!(s.board.boot_mode, BootMode::Unknown);
        assert_eq!(s.cpu.virtualization, None);
        assert!(s.storage.drives.is_empty());
        assert_eq!(CacheKind::default(), CacheKind::Unknown);
        assert_eq!(DriveBus::default(), DriveBus::Other);
    }
}
