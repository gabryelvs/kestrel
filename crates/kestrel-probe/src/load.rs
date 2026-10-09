//! Live load for the home screen graph: CPU and RAM everywhere, GPU usage and temperature on Windows.
use serde::Serialize;
use sysinfo::System;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSample {
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub gpu_percent: Option<f32>,
    pub gpu_temp_c: Option<f32>,
}

pub struct LoadSampler {
    sys: System,
    #[cfg(windows)]
    gpu: Option<crate::win::gpu_usage::GpuUsage>,
}

impl Default for LoadSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadSampler {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self {
            sys,
            #[cfg(windows)]
            gpu: crate::win::gpu_usage::GpuUsage::new(),
        }
    }

    /// Call about once a second. The first CPU reading after `new` can be 0.
    pub fn sample(&mut self) -> LoadSample {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        let total = self.sys.total_memory();
        let ram = if total == 0 {
            0.0
        } else {
            self.sys.used_memory() as f32 / total as f32 * 100.0
        };
        #[cfg(windows)]
        let (gpu_percent, gpu_temp_c) = match self.gpu.as_mut() {
            Some(g) => (g.percent(), g.temperature()),
            None => (None, None),
        };
        #[cfg(not(windows))]
        let (gpu_percent, gpu_temp_c) = (None, None);
        LoadSample {
            cpu_percent: percent(self.sys.global_cpu_usage()),
            ram_percent: percent(ram),
            gpu_percent: gpu_percent.map(percent),
            gpu_temp_c,
        }
    }
}

fn percent(value: f32) -> f32 {
    (value.clamp(0.0, 100.0) * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_percentages() {
        let mut sampler = LoadSampler::new();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let s = sampler.sample();
        assert!((0.0..=100.0).contains(&s.cpu_percent), "{s:?}");
        assert!(s.ram_percent > 0.0 && s.ram_percent <= 100.0, "{s:?}");
        #[cfg(not(windows))]
        assert!(s.gpu_percent.is_none() && s.gpu_temp_c.is_none());
    }

    /// The app keeps the sampler in a `Mutex` in Tauri's state, which needs `Send`. On Windows that includes the
    /// GPU counters, so this is only checked by compiling for Windows.
    #[test]
    fn the_sampler_can_move_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<LoadSampler>();
    }

    #[test]
    fn serializes_camel_case() {
        let json = serde_json::to_string(&LoadSample {
            cpu_percent: 1.0,
            ram_percent: 2.0,
            gpu_percent: None,
            gpu_temp_c: Some(60.0),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"cpuPercent":1.0,"ramPercent":2.0,"gpuPercent":null,"gpuTempC":60.0}"#
        );
    }
}
