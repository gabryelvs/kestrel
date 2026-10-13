//! Real probes for Windows. Each one is independent: a failure becomes `None` plus a ProbeError.
mod display;
mod firmware;
pub(crate) mod gpu_usage;
mod pdh;
mod registry_probe;
pub(crate) mod system_info;
mod wmi_probe;

use kestrel_core::{valorant, ProbeError, SystemSnapshot, ValorantInstall, SCHEMA_VERSION};

pub fn collect() -> SystemSnapshot {
    // WMI needs a thread whose COM state we control (WebView2 owns the UI thread's), so probe on a fresh one.
    std::thread::spawn(collect_here).join().unwrap_or_else(|_| {
        let mut s = SystemSnapshot {
            schema_version: SCHEMA_VERSION,
            ..Default::default()
        };
        s.probe_errors
            .push(problem("probe", "the probe thread crashed"));
        s
    })
}

fn collect_here() -> SystemSnapshot {
    let mut s = SystemSnapshot {
        schema_version: SCHEMA_VERSION,
        collected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ..Default::default()
    };
    let mut errors = Vec::new();
    s.storage.valorant = find_valorant_install().map(|path| ValorantInstall {
        drive_letter: valorant::drive_letter(&path).unwrap_or_else(|| "C".into()),
        path,
        ..Default::default()
    });
    match wmi_probe::Wmi::connect() {
        Ok(wmi) => wmi.fill(&mut s, &mut errors),
        Err(e) => errors.push(problem("wmi", e)),
    }
    registry_probe::fill(&mut s, &mut errors);
    display::fill(&mut s);
    s.security.iommu = Some(firmware::iommu_enabled());
    system_info::fill(&mut s, &mut errors);
    s.probe_errors = errors;
    s
}

/// VALORANT's install folder from Riot's install record, else the default folder, if it exists.
fn find_valorant_install() -> Option<String> {
    let program_data = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let record = format!(
        "{program_data}\\Riot Games\\Metadata\\valorant.live\\valorant.live.product_settings.yaml"
    );
    std::fs::read_to_string(record)
        .ok()
        .and_then(|yaml| valorant::install_path_from_product_settings(&yaml))
        .or_else(|| Some(valorant::DEFAULT_INSTALL.to_string()))
        .filter(|path| std::path::Path::new(path).exists())
}

pub(crate) fn problem(probe: &str, message: impl std::fmt::Display) -> ProbeError {
    ProbeError {
        probe: probe.into(),
        message: message.to_string(),
    }
}
