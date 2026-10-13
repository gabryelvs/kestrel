//! Running a capture. On Windows: the bundled PresentMon, hidden, limited to the game's process, for a fixed
//! time (spec "PresentMon"). Off Windows (development on a Mac): a simulated capture with made-up frames.
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
    /// `Err(CANCELLED)` soon after `cancel` is set. An empty `csv` in the output means PresentMon ran to the end but
    /// saw no frames (the game minimised, say): the caller turns that into a run that is too short.
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

    /// Ends a capture that must stop now (cancelled, timed out, or no longer trackable): kills and reaps PresentMon,
    /// deletes its partial CSV (never read; spec: temporary CSVs are deleted), then ends the session it left open.
    fn abort(&self, child: &mut Child, csv_path: &Path) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(csv_path);
        self.clean_up_stale_session();
    }

    /// One PresentMon capture. `Done` with an empty `csv` means PresentMon ran to the end but saw no frames.
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
            let mut bytes = Vec::new();
            if let Some(pipe) = pipe.as_mut() {
                let _ = pipe.read_to_end(&mut bytes);
            }
            // Lossy on purpose: one stray non-UTF-8 byte must not cost the "lost" or "overflow" warning around it.
            String::from_utf8_lossy(&bytes).into_owned()
        });
        let deadline = Instant::now() + Duration::from_secs(u64::from(seconds) + 30);
        let status = loop {
            if cancel.load(Ordering::SeqCst) {
                self.abort(&mut child, csv_path);
                return Attempt::Failed(CANCELLED.into());
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() > deadline => {
                    self.abort(&mut child, csv_path);
                    return Attempt::Failed("PresentMon didn't finish in time.".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                Err(e) => {
                    // Can't tell what PresentMon is doing any more, so stop it like any other abandoned capture.
                    self.abort(&mut child, csv_path);
                    return Attempt::Failed(format!("Lost track of PresentMon: {e}"));
                }
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
            // PresentMon finished cleanly and left no file: it saw no frames (the game minimised, say). The caller
            // turns the empty text into a run that is too short, not into a failed session.
            Err(e) if status.success() && e.kind() == ErrorKind::NotFound => {
                Attempt::Done(CaptureOutput {
                    csv: String::new(),
                    stderr,
                })
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

    /// The fakes are shell scripts, so this runs on Unix only (the real PresentMon is Windows-only anyway).
    #[cfg(unix)]
    #[test]
    fn the_runner_only_runs_its_own_presentmon_and_reports_what_it_saw() {
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;

        // A scratch folder, and the PATH to put back, that clean up after themselves even when an assertion fails.
        struct Scratch {
            dir: PathBuf,
            path: OsString,
        }
        impl Drop for Scratch {
            fn drop(&mut self) {
                std::env::set_var("PATH", &self.path);
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }
        let scratch = Scratch {
            dir: std::env::temp_dir().join(format!("kestrel-capture-test-{}", std::process::id())),
            path: std::env::var_os("PATH").unwrap_or_default(),
        };
        let dir = scratch.dir.as_path();
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();

        // A fake PresentMon: a shell script that finds its --output_file argument in $out, then runs `body`.
        let script = |path: &Path, body: &str| {
            let text = format!(
                "#!/bin/sh\nout=''\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = --output_file ]; then out=\"$2\"; fi\n  shift\ndone\n{body}\n"
            );
            std::fs::write(path, text).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let csv_path = dir.join("run.csv");
        let capture = |exe: &Path| {
            PresentMon::new(exe.to_path_buf()).run("x.exe", 1, &csv_path, &AtomicBool::new(false))
        };

        // 1. Only Kestrel's own copy runs: not a missing file, not the empty path, and not a PresentMon.exe on PATH.
        let on_path = dir.join("on-path");
        std::fs::create_dir_all(&on_path).unwrap();
        let marker = on_path.join("ran");
        script(
            &on_path.join("PresentMon.exe"),
            &format!("touch '{}'", marker.display()),
        );
        let mut search = vec![on_path];
        search.extend(std::env::split_paths(&scratch.path));
        std::env::set_var("PATH", std::env::join_paths(search).unwrap());
        // The control: a search of PATH does find the planted file here, so the checks below can't pass for nothing.
        assert!(std::process::Command::new("PresentMon.exe")
            .status()
            .unwrap()
            .success());
        assert!(
            marker.exists(),
            "the planted PresentMon.exe is what a search of PATH finds"
        );
        std::fs::remove_file(&marker).unwrap();
        assert_eq!(
            capture(&dir.join("nowhere").join("PresentMon.exe")).unwrap_err(),
            MISSING
        );
        assert_eq!(capture(Path::new("")).unwrap_err(), MISSING);
        assert_eq!(capture(Path::new("PresentMon.exe")).unwrap_err(), MISSING);
        PresentMon::new(PathBuf::from("PresentMon.exe")).clean_up_stale_session();
        assert!(
            !marker.exists(),
            "a PresentMon.exe found on PATH must never run"
        );
        std::env::set_var("PATH", &scratch.path);

        // 2. A capture that saved frames: the CSV comes back, and what PresentMon said on stderr (it stays a pipe).
        let good = dir.join("good");
        script(
            &good,
            "printf 'Application,MsBetweenPresents\\nx.exe,2.5\\n' > \"$out\"\necho 'warning: 3 ETW events were lost.' >&2",
        );
        let got = capture(&good).unwrap();
        assert_eq!(got.csv, "Application,MsBetweenPresents\nx.exe,2.5\n");
        assert!(got.stderr.contains("lost"), "{:?}", got.stderr);
        assert!(!csv_path.exists(), "the CSV is deleted once it is read");

        // 3. Exit 0 and no file: PresentMon saw no frames (the game minimised, say). That's a capture with no frames.
        let quiet = dir.join("quiet");
        script(&quiet, "exit 0");
        let got = capture(&quiet).unwrap();
        assert_eq!(got.csv, "");

        // 4. Exit 1 and no file: PresentMon failed, and the error says what it said.
        let failing = dir.join("failing");
        script(&failing, "echo 'no such process' >&2\nexit 1");
        let error = capture(&failing).unwrap_err();
        assert!(error.starts_with("PresentMon saved no frames ("), "{error}");
        assert!(error.contains("no such process"), "{error}");

        // 5. One stray non-UTF-8 byte on stderr doesn't cost the warning that comes with it.
        let stray = dir.join("stray");
        script(
            &stray,
            "printf 'Application,MsBetweenPresents\\nx.exe,2.5\\n' > \"$out\"\nprintf 'warning: 3 ETW events were lost.\\377\\n' >&2",
        );
        let got = capture(&stray).unwrap();
        assert!(
            got.stderr.contains("3 ETW events were lost."),
            "{:?}",
            got.stderr
        );
    }
}
