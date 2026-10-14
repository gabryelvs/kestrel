//! Live clock of each physical core for the Hardware tab: PDH's `Processor Frequency` (the rated MHz) times
//! `% Processor Performance` / 100 per logical processor, mapped to physical cores. Like the load sampler, the app
//! keeps one and asks it about once a second while the tab is shown.
#[cfg(windows)]
use crate::win::pdh;
use kestrel_core::topology::CoreClock;
#[cfg(windows)]
use kestrel_core::topology::{self, PhysicalCore};

pub struct ClockSampler {
    /// The physical cores from Windows' topology, in order: one clock each.
    #[cfg(windows)]
    cores: Vec<PhysicalCore>,
    /// Every logical processor's `Processor Frequency`; None when Windows can't give the counter.
    #[cfg(windows)]
    frequency: Option<pdh::Query>,
    /// Every logical processor's `% Processor Performance`; None when Windows can't give the counter.
    #[cfg(windows)]
    performance: Option<pdh::Query>,
}

// The app keeps the sampler in a `Mutex` in Tauri's state, which needs `Send`. On Windows that includes the PDH
// queries (see `pdh::Query`). This stops the build for the target being compiled if the sampler ever isn't `Send`;
// `cargo clippy --target x86_64-pc-windows-msvc` checks the Windows one from another OS.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<ClockSampler>();
};

impl Default for ClockSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockSampler {
    /// Reads the topology and collects once, so the next sample has a baseline: `% Processor Performance` is a rate.
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            let sampler = Self {
                cores: crate::win::cpu::topology()
                    .map(|(cores, _)| cores)
                    .unwrap_or_default(),
                frequency: pdh::Query::open(r"\Processor Information(*)\Processor Frequency"),
                performance: pdh::Query::open(r"\Processor Information(*)\% Processor Performance"),
            };
            for query in [&sampler.frequency, &sampler.performance]
                .into_iter()
                .flatten()
            {
                query.collect();
            }
            sampler
        }
        #[cfg(not(windows))]
        {
            Self {}
        }
    }

    /// One clock per physical core, in topology order; call about once a second. On Windows a core whose counters
    /// have no data has no MHz (every core, when a counter can't be opened). Off Windows: eight sample cores at
    /// 4800 + 50 × core MHz.
    pub fn sample(&mut self) -> Vec<CoreClock> {
        #[cfg(windows)]
        {
            let frequency = read(self.frequency.as_ref(), 0);
            // `% Processor Performance` goes above 100 when the core boosts, so it mustn't be capped.
            let performance = read(self.performance.as_ref(), pdh::FMT_NOCAP100);
            topology::core_clocks(&self.cores, &frequency, &performance)
        }
        #[cfg(not(windows))]
        {
            (0..8)
                .map(|core| CoreClock {
                    core,
                    kind: None,
                    mhz: Some(4800 + 50 * core),
                })
                .collect()
        }
    }
}

/// A new sample of every instance of the query's counter; empty when the query isn't open or the sample fails.
#[cfg(windows)]
fn read(query: Option<&pdh::Query>, flags: u32) -> Vec<(String, f64)> {
    query
        .filter(|query| query.collect())
        .and_then(|query| query.formatted_array(flags))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn off_windows_there_are_eight_sample_cores_with_a_clock_each() {
        let clocks = ClockSampler::new().sample();
        assert_eq!(clocks.len(), 8);
        for (index, clock) in clocks.iter().enumerate() {
            assert_eq!(clock.core, index as u32, "{clocks:?}");
            assert_eq!(clock.kind, None, "{clocks:?}");
            assert!(clock.mhz.is_some(), "{clocks:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_gives_a_clock_per_physical_core() {
        let mut sampler = ClockSampler::default();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let clocks = sampler.sample();
        assert!(!clocks.is_empty());
        for (index, clock) in clocks.iter().enumerate() {
            assert_eq!(clock.core, index as u32, "{clocks:?}");
        }
    }
}
