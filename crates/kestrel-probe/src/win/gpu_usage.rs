//! GPU load from Windows performance counters (what Task Manager shows) and NVIDIA temperature via NVML.
use super::pdh;
use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::MAX_PATH;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

pub struct GpuUsage {
    query: pdh::Query,
    nvml: Option<Nvml>,
}

impl GpuUsage {
    /// None if Windows doesn't offer the GPU engine counters (before Windows 10 1709, or a driver that doesn't report them).
    pub fn new() -> Option<Self> {
        let query = pdh::Query::open(r"\GPU Engine(*engtype_3D)\Utilization Percentage")?;
        // The counter is a rate, so the first reading needs an earlier sample to compare with.
        query.collect();
        Some(Self {
            query,
            nvml: load_nvml(),
        })
    }

    /// Total 3D-engine utilisation across all processes, 0–100. None if Windows can't give the counter.
    pub fn percent(&mut self) -> Option<f32> {
        if !self.query.collect() {
            return None;
        }
        // No instance with valid data means no 3D work, so the sum is 0.
        let total: f64 = self
            .query
            .formatted_array(0)?
            .iter()
            .map(|(_, percent)| percent)
            .sum();
        Some(total.clamp(0.0, 100.0) as f32)
    }

    /// GPU temperature in °C on NVIDIA cards (NVML ships with the driver); None elsewhere.
    pub fn temperature(&self) -> Option<f32> {
        let device = self.nvml.as_ref()?.device_by_index(0).ok()?;
        device
            .temperature(TemperatureSensor::Gpu)
            .ok()
            .map(|t| t as f32)
    }
}

/// Loads NVML from the Windows system folder by its full path, and from nowhere else. A bare "nvml.dll" would also
/// be searched for in the current folder and in every PATH entry, and some of those (WindowsApps in the user's
/// profile, say) are writable without administrator rights, while this process runs as administrator: a planted
/// nvml.dll would run with admin rights. Without the system copy (AMD and Intel PCs) there is no temperature.
fn load_nvml() -> Option<Nvml> {
    let path = nvml_in(&system_directory()?)?;
    Nvml::builder().lib_path(path.as_os_str()).init().ok()
}

/// The Windows system folder, such as C:\Windows\System32. None if Windows doesn't give it.
fn system_directory() -> Option<PathBuf> {
    let mut buffer = [0u16; MAX_PATH as usize];
    // SAFETY: `buffer` is writable for the `buffer.len()` UTF-16 units passed with it.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    // 0 means it failed; a length that doesn't fit is the size needed, and a path that long isn't System32.
    (length > 0 && length < buffer.len())
        .then(|| PathBuf::from(OsString::from_wide(&buffer[..length])))
}

/// `nvml.dll` inside `directory`, if that file is there.
fn nvml_in(directory: &Path) -> Option<PathBuf> {
    let path = directory.join("nvml.dll");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty folder under the temp folder, removed again when dropped.
    struct TempFolder(PathBuf);

    impl TempFolder {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("kestrel-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("the temp folder can be created");
            Self(path)
        }
    }

    impl Drop for TempFolder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn nvml_is_the_file_in_the_given_folder() {
        let folder = TempFolder::new("nvml-lookup");
        assert_eq!(nvml_in(&folder.0), None);
        // A folder with that name is not the library.
        std::fs::create_dir(folder.0.join("nvml.dll")).unwrap();
        assert_eq!(nvml_in(&folder.0), None);
        std::fs::remove_dir(folder.0.join("nvml.dll")).unwrap();
        std::fs::write(folder.0.join("nvml.dll"), b"").unwrap();
        assert_eq!(nvml_in(&folder.0), Some(folder.0.join("nvml.dll")));
    }

    #[test]
    fn the_system_folder_is_a_full_path() {
        let folder = system_directory().expect("Windows has a system folder");
        assert!(folder.is_absolute(), "{}", folder.display());
        assert!(folder.is_dir(), "{}", folder.display());
    }
}
