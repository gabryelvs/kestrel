//! The data model: everything Kestrel knows about a PC. `None` always means "couldn't tell".
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

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
    pub struct CpuInfo { pub name: String, pub vendor: CpuVendor, pub cores: u32, pub threads: u32 }
    pub struct GpuInfo { pub name: String, pub vendor: GpuVendor, pub driver_version: String, pub driver_date: Option<String>, pub vram_mb: Option<u64>, pub is_integrated: bool }
    pub struct MemoryInfo { pub total_mb: u64, pub ddr_generation: Option<u8>, pub modules: Vec<MemoryModule> }
    pub struct MemoryModule { pub capacity_mb: u64, pub configured_mts: Option<u32>, pub max_mts: Option<u32>, pub part_number: String, pub manufacturer: String, pub bank_label: String, pub device_locator: String }
    pub struct DisplayInfo { pub name: String, pub adapter: String, pub current_hz: u32, pub max_hz: u32, pub width: u32, pub height: u32, pub is_primary: bool, pub is_virtual: bool }
    pub struct BoardInfo { pub manufacturer: String, pub product: String, pub bios_version: String, pub bios_date: Option<String> }
    pub struct StorageInfo { pub valorant: Option<ValorantInstall> }
    pub struct ValorantInstall { pub path: String, pub drive_letter: String, pub media: DriveMedia, pub free_gb: f64, pub total_gb: f64 }
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
    }
}
