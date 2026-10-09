//! GPU load from Windows performance counters (what Task Manager shows) and NVIDIA temperature via NVML.
use super::pdh;
use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};

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
            nvml: Nvml::init().ok(),
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
