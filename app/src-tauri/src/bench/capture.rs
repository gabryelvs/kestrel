//! Running a capture. On Windows: the bundled PresentMon, hidden, limited to the game's process, for a fixed
//! time (spec "PresentMon"). Off Windows (development on a Mac): a simulated capture with made-up frames.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// PresentMon's ETW session name; a stale one with this name is cleaned up.
pub const SESSION_NAME: &str = "KestrelBench";
/// The error text of a cancelled capture.
pub const CANCELLED: &str = "cancelled";
/// The error when Kestrel's copy of PresentMon isn't there (spec Errors table: "PresentMon missing").
pub const MISSING: &str = "PresentMon is missing. Reinstall Kestrel and try again.";

#[derive(Debug, Clone, Default)]
pub struct CaptureOutput {
    pub csv: String,
    pub stderr: String,
}

pub trait Capture: Send + Sync {
    /// Captures `process_name` for `seconds`, using `csv_path` for PresentMon's output. Returns
    /// `Err(CANCELLED)` soon after `cancel` is set.
    fn run(
        &self,
        process_name: &str,
        seconds: u32,
        csv_path: &Path,
        cancel: &AtomicBool,
    ) -> Result<CaptureOutput, String>;
    /// Ends a capture session a crash or a cancel left running.
    fn clean_up_stale_session(&self);
}

/// The arguments of one PresentMon capture, in the order the spec lists them.
pub fn presentmon_args(process_name: &str, seconds: u32, csv_path: &Path) -> Vec<String> {
    [
        "--process_name",
        process_name,
        "--output_file",
        &csv_path.display().to_string(),
        "--timed",
        &seconds.to_string(),
        "--terminate_after_timed",
        "--session_name",
        SESSION_NAME,
        "--stop_existing_session",
        "--no_console_stats",
        "--set_circular_buffer_size",
        "8192",
        "--track_pc_latency",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The bundled PresentMon.
pub struct PresentMon {
    exe: PathBuf,
}

enum Attempt {
    Done(CaptureOutput),
    /// Exit code 6: the session name is in use.
    SessionInUse,
    Failed(String),
}

impl PresentMon {
    pub fn new(exe: PathBuf) -> Self {
        Self { exe }
    }

    fn command(&self) -> Command {
        #[cfg_attr(not(windows), allow(unused_mut))] // only Windows changes it (no console window)
        let mut command = Command::new(&self.exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }

    fn attempt(
        &self,
        process_name: &str,
        seconds: u32,
        csv_path: &Path,
        cancel: &AtomicBool,
    ) -> Attempt {
        // Only Kestrel's own copy runs: a missing file (or the empty path `capture_engine` passes when the resource
        // can't be found) fails here, before Windows could go looking for another PresentMon.exe.
        if !self.exe.is_file() {
            return Attempt::Failed(MISSING.into());
        }
        let _ = std::fs::remove_file(csv_path);
        let spawned = self
            .command()
            .args(presentmon_args(process_name, seconds, csv_path))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => return Attempt::Failed(format!("Couldn't start PresentMon: {e}")),
        };
        let mut pipe = child.stderr.take();
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(pipe) = pipe.as_mut() {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        });
        let deadline = Instant::now() + Duration::from_secs(u64::from(seconds) + 30);
        let status = loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                self.clean_up_stale_session();
                // A stopped capture's partial CSV is never read: delete it now (spec: temporary CSVs are deleted).
                let _ = std::fs::remove_file(csv_path);
                return Attempt::Failed(CANCELLED.into());
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() > deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    self.clean_up_stale_session();
                    let _ = std::fs::remove_file(csv_path);
                    return Attempt::Failed("PresentMon didn't finish in time.".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                Err(e) => return Attempt::Failed(format!("Lost track of PresentMon: {e}")),
            }
        };
        let stderr = reader.join().unwrap_or_default();
        if status.code() == Some(6) {
            return Attempt::SessionInUse;
        }
        match std::fs::read_to_string(csv_path) {
            Ok(csv) => {
                let _ = std::fs::remove_file(csv_path);
                Attempt::Done(CaptureOutput { csv, stderr })
            }
            Err(_) => Attempt::Failed(format!(
                "PresentMon saved no frames ({status}). {}",
                stderr.trim()
            )),
        }
    }
}

impl Capture for PresentMon {
    fn run(
        &self,
        process_name: &str,
        seconds: u32,
        csv_path: &Path,
        cancel: &AtomicBool,
    ) -> Result<CaptureOutput, String> {
        match self.attempt(process_name, seconds, csv_path, cancel) {
            Attempt::Done(output) => Ok(output),
            Attempt::Failed(message) => Err(message),
            Attempt::SessionInUse => {
                // A session named KestrelBench is still running (a crash, say): end it and try once more.
                self.clean_up_stale_session();
                match self.attempt(process_name, seconds, csv_path, cancel) {
                    Attempt::Done(output) => Ok(output),
                    Attempt::Failed(message) => Err(message),
                    Attempt::SessionInUse => Err(
                        "PresentMon's capture session is busy. Restart the PC and try again."
                            .into(),
                    ),
                }
            }
        }
    }

    fn clean_up_stale_session(&self) {
        // The same rule as `attempt`: only Kestrel's own copy runs, never a PresentMon.exe found on PATH.
        if !self.exe.is_file() {
            return;
        }
        let _ = self
            .command()
            .args([
                "--terminate_existing_session",
                "--session_name",
                SESSION_NAME,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Made-up frames for development off Windows; each capture takes `seconds_per_capture` of real time.
pub struct Simulated {
    pub seconds_per_capture: f32,
}

impl Capture for Simulated {
    fn run(
        &self,
        _process_name: &str,
        seconds: u32,
        _csv_path: &Path,
        cancel: &AtomicBool,
    ) -> Result<CaptureOutput, String> {
        let end = Instant::now() + Duration::from_secs_f32(self.seconds_per_capture.max(0.0));
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(CANCELLED.into());
            }
            let now = Instant::now();
            if now >= end {
                break;
            }
            std::thread::sleep((end - now).min(Duration::from_millis(50)));
        }
        Ok(CaptureOutput {
            csv: simulated_csv(seconds),
            stderr: String::new(),
        })
    }

    fn clean_up_stale_session(&self) {}
}

/// About 400 FPS with small noise and a 12 ms hitch every 500 frames, `seconds` long.
pub fn simulated_csv(seconds: u32) -> String {
    let mut csv = String::from("Application,PresentMode,MsBetweenPresents,MsGPUBusy,MsPCLatency\n");
    let mut seed: u32 = 12_345;
    let (mut elapsed, mut frame) = (0.0_f64, 0_u32);
    while elapsed < f64::from(seconds) * 1000.0 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = f64::from(seed >> 8) / f64::from(1_u32 << 24);
        let ms = if frame % 500 == 499 {
            12.0
        } else {
            2.2 + noise * 0.6
        };
        csv.push_str(&format!(
            "VALORANT-Win64-Shipping.exe,Hardware: Independent Flip,{ms:.3},{:.3},{:.2}\n",
            ms * 0.82,
            9.0 + noise * 2.0
        ));
        elapsed += ms;
        frame += 1;
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_bench::csv::parse;
    use kestrel_bench::metrics::measure;

    #[test]
    fn presentmon_gets_the_flags_from_the_spec() {
        let args = presentmon_args(
            "VALORANT-Win64-Shipping.exe",
            90,
            Path::new("C:\\k\\run-1.csv"),
        );
        assert_eq!(
            args,
            [
                "--process_name",
                "VALORANT-Win64-Shipping.exe",
                "--output_file",
                "C:\\k\\run-1.csv",
                "--timed",
                "90",
                "--terminate_after_timed",
                "--session_name",
                "KestrelBench",
                "--stop_existing_session",
                "--no_console_stats",
                "--set_circular_buffer_size",
                "8192",
                "--track_pc_latency",
            ]
        );
    }

    #[test]
    fn the_simulated_capture_looks_like_a_real_run() {
        let frames = parse(&simulated_csv(90)).unwrap();
        let (m, _) = measure(&frames);
        assert!((m.duration_s - 90.0).abs() < 0.1, "{}", m.duration_s);
        assert!(m.avg_fps > 300.0 && m.avg_fps < 500.0, "{}", m.avg_fps);
        assert!(m.pc_latency_ms.is_some() && m.stutters_per_min > 0.0);
    }

    #[test]
    fn cancelling_a_simulated_capture_stops_it() {
        let cancel = AtomicBool::new(true);
        let started = std::time::Instant::now();
        let result = Simulated {
            seconds_per_capture: 5.0,
        }
        .run("x.exe", 90, Path::new("unused.csv"), &cancel);
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        let done = Simulated {
            seconds_per_capture: 0.0,
        }
        .run("x.exe", 2, Path::new("unused.csv"), &AtomicBool::new(false))
        .unwrap();
        assert!(done
            .csv
            .starts_with("Application,PresentMode,MsBetweenPresents"));
    }
}
