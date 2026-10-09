//! GPU helpers: who made it, whether it's built into the CPU, and which one games use.
use crate::snapshot::{GpuInfo, GpuVendor};
use regex::Regex;
use std::sync::OnceLock;

const VIRTUAL_HINTS: &[&str] = &[
    "virtual",
    "parsec",
    "basic display",
    "remote display",
    "indirect display",
    "displaylink",
];

/// PCI vendor ids (uppercase) of virtual adapters: Microsoft, VMware, VirtIO and Parallels.
const VIRTUAL_VENDOR_IDS: &[&str] = &["VEN_1414", "VEN_15AD", "VEN_1AF4", "VEN_1AB8"];

/// Vendor from the PCI id inside a PNP device id ("PCI\\VEN_10DE&…"), falling back to the name.
/// Virtual name hints and virtual vendor ids are checked first and override the PCI vendor.
pub fn vendor_from(pnp_device_id: &str, name: &str) -> GpuVendor {
    let id = pnp_device_id.to_ascii_uppercase();
    let lower = name.to_ascii_lowercase();
    if VIRTUAL_HINTS.iter().any(|h| lower.contains(h))
        || VIRTUAL_VENDOR_IDS.iter().any(|v| id.contains(v))
    {
        GpuVendor::Virtual
    } else if id.contains("VEN_10DE") || lower.contains("nvidia") {
        GpuVendor::Nvidia
    } else if id.contains("VEN_1002") || lower.contains("radeon") || lower.contains("amd") {
        GpuVendor::Amd
    } else if id.contains("VEN_8086") || lower.contains("intel") {
        GpuVendor::Intel
    } else {
        GpuVendor::Other
    }
}

/// Integrated graphics. Intel: anything without a discrete Arc model number (A- or B-series).
/// AMD: never a name with an RX or WX model number (RX 480, WX 7100), whatever it ends in. Otherwise a name ending
/// in "Graphics" unless it contains "Vega M " (Kaby Lake-G's on-package discrete GPU), or a name with no "RX" and a
/// known VRAM under 512 MB. Nvidia, virtual and other vendors are never integrated.
pub fn looks_integrated(vendor: GpuVendor, name: &str, vram_mb: Option<u64>) -> bool {
    static DISCRETE_ARC: OnceLock<Regex> = OnceLock::new();
    static DISCRETE_AMD_MODEL: OnceLock<Regex> = OnceLock::new();
    let lower = name.to_ascii_lowercase();
    match vendor {
        GpuVendor::Intel => !DISCRETE_ARC
            .get_or_init(|| Regex::new(r"arc(\(tm\))?\s+[ab]\d{3}").expect("valid pattern"))
            .is_match(&lower),
        GpuVendor::Amd => {
            // "RX Vega 10 Graphics" is an APU, and no digit follows its "RX", so it isn't caught here.
            let discrete_model = DISCRETE_AMD_MODEL
                .get_or_init(|| Regex::new(r"(?i)\b[rw]x\s*\d").expect("valid pattern"))
                .is_match(name);
            let on_package_vega_m = lower.contains("vega m ");
            !discrete_model
                && ((lower.ends_with("graphics") && !on_package_vega_m)
                    || (!lower.contains("rx") && vram_mb.is_some_and(|v| v < 512)))
        }
        _ => false,
    }
}

fn real(gpus: &[GpuInfo]) -> impl Iterator<Item = &GpuInfo> {
    gpus.iter().filter(|g| g.vendor != GpuVendor::Virtual)
}

/// The GPU that matters for gaming: the biggest real discrete GPU, else the first real integrated one.
pub fn main_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo> {
    real(gpus)
        .filter(|g| !g.is_integrated)
        .max_by_key(|g| g.vram_mb.unwrap_or(0))
        .or_else(|| real(gpus).next())
}

/// True when a PC has both a real integrated and a real discrete GPU (typical gaming laptop).
pub fn is_hybrid(gpus: &[GpuInfo]) -> bool {
    real(gpus).any(|g| g.is_integrated) && real(gpus).any(|g| !g.is_integrated)
}

/// Windows driver version "32.0.16.1074" → NVIDIA's "610.74".
pub fn nvidia_version(windows_version: &str) -> Option<String> {
    let parts: Vec<&str> = windows_version.trim().split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let third: u32 = parts[2].parse().ok()?;
    let fourth: u32 = parts[3].parse().ok()?;
    if fourth > 9999 {
        return None;
    }
    let digits = format!("{}{:04}", third % 10, fourth);
    Some(format!("{}.{}", &digits[..3], &digits[3..]))
}

/// The driver version as the vendor's own tools show it.
pub fn display_driver_version(gpu: &GpuInfo) -> String {
    match gpu.vendor {
        GpuVendor::Nvidia => {
            nvidia_version(&gpu.driver_version).unwrap_or_else(|| gpu.driver_version.clone())
        }
        _ => gpu.driver_version.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(name: &str, vendor: GpuVendor, integrated: bool, vram: Option<u64>) -> GpuInfo {
        GpuInfo {
            name: name.into(),
            vendor,
            driver_version: "1.0.0.0".into(),
            driver_date: None,
            vram_mb: vram,
            is_integrated: integrated,
        }
    }

    #[test]
    fn vendors_from_pci_ids_and_names() {
        assert_eq!(
            vendor_from(
                "PCI\\VEN_10DE&DEV_2489&SUBSYS_00000000",
                "NVIDIA GeForce RTX 3060 Ti"
            ),
            GpuVendor::Nvidia
        );
        assert_eq!(
            vendor_from("ROOT\\DISPLAY\\0000", "Parsec Virtual Display Adapter"),
            GpuVendor::Virtual
        );
        assert_eq!(
            vendor_from("PCI\\VEN_8086&DEV_4C8A", "Intel(R) UHD Graphics 750"),
            GpuVendor::Intel
        );
        assert_eq!(
            vendor_from("PCI\\VEN_1002&DEV_73BF", "AMD Radeon RX 6800 XT"),
            GpuVendor::Amd
        );
        assert_eq!(
            vendor_from(
                "ROOT\\BasicDisplay\\0000",
                "Microsoft Basic Display Adapter"
            ),
            GpuVendor::Virtual
        );
        assert_eq!(vendor_from("", "Mystery GPU"), GpuVendor::Other);
        assert_eq!(
            vendor_from("USB\\VID_17E9&PID_4301", "DisplayLink USB Device"),
            GpuVendor::Virtual
        );
        assert_eq!(
            vendor_from("PCI\\VEN_15AD&DEV_0405", "VMware SVGA 3D"),
            GpuVendor::Virtual
        );
        assert_eq!(
            vendor_from("PCI\\VEN_1414&DEV_008E", "Microsoft Hyper-V Video"),
            GpuVendor::Virtual
        );
        assert_eq!(
            vendor_from("PCI\\VEN_1AF4&DEV_1050", "Red Hat VirtIO GPU"),
            GpuVendor::Virtual
        );
        assert_eq!(
            vendor_from("PCI\\VEN_1AB8&DEV_4005", "Parallels Display Adapter (WDDM)"),
            GpuVendor::Virtual
        );
    }

    #[test]
    fn integrated_detection() {
        assert!(looks_integrated(
            GpuVendor::Intel,
            "Intel(R) UHD Graphics 750",
            Some(128)
        ));
        assert!(looks_integrated(
            GpuVendor::Intel,
            "Intel(R) Arc(TM) Graphics",
            Some(128)
        ));
        assert!(!looks_integrated(
            GpuVendor::Intel,
            "Intel(R) Arc(TM) A770 Graphics",
            Some(16384)
        ));
        assert!(looks_integrated(
            GpuVendor::Amd,
            "AMD Radeon(TM) Graphics",
            Some(512)
        ));
        assert!(looks_integrated(
            GpuVendor::Amd,
            "AMD Radeon 780M Graphics",
            Some(2048)
        ));
        assert!(!looks_integrated(
            GpuVendor::Amd,
            "AMD Radeon RX 7800 XT",
            Some(16384)
        ));
        assert!(!looks_integrated(
            GpuVendor::Nvidia,
            "NVIDIA GeForce RTX 4060 Laptop GPU",
            Some(8192)
        ));
        assert!(looks_integrated(
            GpuVendor::Amd,
            "AMD Radeon(TM) RX Vega 10 Graphics",
            Some(2048)
        ));
        assert!(looks_integrated(
            GpuVendor::Amd,
            "AMD Radeon(TM) RX Vega 10 Graphics",
            None
        ));
        assert!(!looks_integrated(
            GpuVendor::Amd,
            "AMD Radeon RX Vega M GH Graphics",
            Some(4096)
        ));
    }

    #[test]
    fn amd_cards_with_a_model_number_are_never_integrated() {
        // Polaris and Radeon Pro cards end their names in "Graphics" too, like the APUs do.
        for (name, vram) in [
            ("Radeon (TM) RX 480 Graphics", Some(8192)),
            ("Radeon (TM) RX 480 Graphics", None),
            ("AMD Radeon (TM) RX 470 Graphics", Some(4096)),
            ("AMD Radeon RX580 Graphics", Some(8192)),
            ("AMD Radeon Pro WX 7100 Graphics", Some(8192)),
            ("AMD Radeon Pro WX 3200 Graphics", None),
        ] {
            assert!(!looks_integrated(GpuVendor::Amd, name, vram), "{name}");
        }
        // "RX Vega 10" is an APU: no model number follows RX.
        assert!(looks_integrated(
            GpuVendor::Amd,
            "AMD Radeon(TM) RX Vega 10 Graphics",
            Some(2048)
        ));
    }

    #[test]
    fn main_gpu_skips_virtual_adapters() {
        let gpus = [
            gpu(
                "Parsec Virtual Display Adapter",
                GpuVendor::Virtual,
                false,
                None,
            ),
            gpu(
                "NVIDIA GeForce RTX 3060 Ti",
                GpuVendor::Nvidia,
                false,
                Some(8192),
            ),
        ];
        assert_eq!(main_gpu(&gpus).unwrap().name, "NVIDIA GeForce RTX 3060 Ti");
    }

    #[test]
    fn main_gpu_prefers_discrete_on_hybrid() {
        let gpus = [
            gpu("Intel(R) UHD Graphics", GpuVendor::Intel, true, Some(128)),
            gpu(
                "NVIDIA GeForce RTX 4060 Laptop GPU",
                GpuVendor::Nvidia,
                false,
                Some(8192),
            ),
        ];
        assert_eq!(main_gpu(&gpus).unwrap().vendor, GpuVendor::Nvidia);
        assert!(is_hybrid(&gpus));
    }

    #[test]
    fn main_gpu_falls_back_to_integrated_and_never_virtual() {
        let igpu = [gpu(
            "Intel(R) UHD Graphics",
            GpuVendor::Intel,
            true,
            Some(128),
        )];
        assert_eq!(main_gpu(&igpu).unwrap().vendor, GpuVendor::Intel);
        assert!(!is_hybrid(&igpu));
        let only_virtual = [gpu(
            "Microsoft Basic Display Adapter",
            GpuVendor::Virtual,
            false,
            None,
        )];
        assert!(main_gpu(&only_virtual).is_none());
    }

    #[test]
    fn nvidia_versions_decode() {
        assert_eq!(nvidia_version("32.0.16.1074").as_deref(), Some("610.74"));
        assert_eq!(nvidia_version("31.0.15.5222").as_deref(), Some("552.22"));
        assert_eq!(nvidia_version("27.21.14.5671").as_deref(), Some("456.71"));
        assert_eq!(nvidia_version("1.2.3"), None);
        assert_eq!(nvidia_version("abc"), None);
        let g = gpu("NVIDIA GeForce RTX 3060 Ti", GpuVendor::Nvidia, false, None);
        assert_eq!(
            display_driver_version(&GpuInfo {
                driver_version: "32.0.16.1074".into(),
                ..g
            }),
            "610.74"
        );
    }
}
