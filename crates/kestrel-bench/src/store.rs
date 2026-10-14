//! Saved benchmarks, in Kestrel's own folder (decision 40):
//!
//! ```text
//! index.json          {"ids": [oldest … newest], "baseline": id}
//! <id>/summary.json   the Benchmark
//! <id>/run-<n>.bin    run n's frame times, little-endian f32
//! ```
//!
//! Each file is written under a temporary name and then renamed, so a crash never leaves half a file.
use crate::model::{Benchmark, BenchmarkSummary, STARTING_POINT};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Index {
    ids: Vec<String>,
    baseline: Option<String>,
}

pub struct Store {
    dir: PathBuf,
    /// Held through every read-change-write, so two callers at once (the capture thread saving, a command deleting)
    /// can't lose each other's change to the index.
    changing: Mutex<()>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            changing: Mutex::new(()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Saves a new benchmark and its runs' frame times; returns its id, which is also set on `bench`. With no baseline
    /// yet, it is named "Starting point" and becomes the baseline (decision 41). A benchmark that can't be read back is
    /// refused before anything is written, and a write that fails leaves no folder behind.
    pub fn save(&self, bench: &mut Benchmark, frame_times: &[Vec<f32>]) -> io::Result<String> {
        let _changing = self.change();
        let mut index = self.index();
        let base = id_from(&bench.created_at);
        let mut id = base.clone();
        let mut n = 2;
        while index.ids.contains(&id) || self.dir.join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        // Everything is prepared on a copy, so the caller's benchmark changes only once every write has succeeded.
        let mut saved = bench.clone();
        saved.id = id.clone();
        let is_baseline = self.baseline_in(&index).is_none();
        if is_baseline {
            saved.name = STARTING_POINT.into();
        }
        let json = serde_json::to_vec_pretty(&saved)?;
        // serde_json writes NaN and infinity as null, which won't load back, so refuse before anything is written.
        if serde_json::from_slice::<Benchmark>(&json).is_err() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The benchmark couldn't be saved: some of its numbers are invalid.",
            ));
        }
        index.ids.push(id.clone());
        if is_baseline {
            index.baseline = Some(id.clone());
        }
        let folder = self.dir.join(&id);
        if let Err(e) = self.write_benchmark(&folder, frame_times, &json, &index) {
            // Nothing in the index names the folder yet, so it can go. The write's own error is the one returned.
            let _ = std::fs::remove_dir_all(&folder);
            return Err(e);
        }
        bench.id = id.clone();
        bench.name = saved.name;
        Ok(id)
    }

    /// Newest first. Benchmarks whose files can't be read are left out.
    pub fn list(&self) -> Vec<BenchmarkSummary> {
        let index = self.index();
        let baseline = self.baseline_in(&index);
        index
            .ids
            .iter()
            .rev()
            .filter_map(|id| self.get(id))
            .map(|b| summary(&b, baseline.as_deref() == Some(b.id.as_str())))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Benchmark> {
        if !safe_id(id) {
            return None;
        }
        let bytes = std::fs::read(self.dir.join(id).join("summary.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Run `run`'s frame times (runs count from 1).
    pub fn frame_times(&self, id: &str, run: usize) -> Option<Vec<f32>> {
        if !safe_id(id) {
            return None;
        }
        let bytes = std::fs::read(self.dir.join(id).join(format!("run-{run}.bin"))).ok()?;
        let (quads, _) = bytes.as_chunks::<4>();
        Some(quads.iter().map(|&quad| f32::from_le_bytes(quad)).collect())
    }

    pub fn rename(&self, id: &str, name: &str) -> io::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A name can't be empty.",
            ));
        }
        let _changing = self.change();
        let mut bench = self.get(id).ok_or_else(not_found)?;
        // Cut to 60 characters, then trim again so a cut name never ends in a space.
        let cut: String = name.chars().take(60).collect();
        bench.name = cut.trim().to_string();
        write_atomic(
            &self.dir.join(id).join("summary.json"),
            &serde_json::to_vec_pretty(&bench)?,
        )
    }

    pub fn delete(&self, id: &str) -> io::Result<()> {
        if !safe_id(id) {
            return Err(not_found());
        }
        let _changing = self.change();
        let mut index = self.index();
        index.ids.retain(|i| i != id);
        if index.baseline.as_deref() == Some(id) {
            index.baseline = None;
        }
        self.write_index(&index)?;
        match std::fs::remove_dir_all(self.dir.join(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    pub fn set_baseline(&self, id: &str) -> io::Result<()> {
        let _changing = self.change();
        let mut index = self.index();
        if !index.ids.iter().any(|i| i == id) || self.get(id).is_none() {
            return Err(not_found());
        }
        index.baseline = Some(id.to_string());
        self.write_index(&index)
    }

    /// The baseline's id, if it still exists.
    pub fn baseline(&self) -> Option<String> {
        self.baseline_in(&self.index())
    }

    fn baseline_in(&self, index: &Index) -> Option<String> {
        index.baseline.clone().filter(|id| self.get(id).is_some())
    }

    /// The saved index. A missing or damaged one is rebuilt from the folders (see `rebuild_index`); the next writer
    /// saves the rebuilt index.
    fn index(&self) -> Index {
        std::fs::read(self.dir.join("index.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| self.rebuild_index())
    }

    /// Every folder whose name is a safe id and whose summary.json loads, oldest first (by created_at, then id), with
    /// the oldest as the baseline. Only reads, so readers can use it without the lock. A store with no folders gives
    /// an empty index.
    fn rebuild_index(&self) -> Index {
        let mut found: Vec<(String, String)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|id| safe_id(id))
            .filter_map(|id| self.get(&id).map(|bench| (bench.created_at, id)))
            .collect();
        found.sort();
        Index {
            baseline: found.first().map(|(_, id)| id.clone()),
            ids: found.into_iter().map(|(_, id)| id).collect(),
        }
    }

    fn write_index(&self, index: &Index) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        write_atomic(
            &self.dir.join("index.json"),
            &serde_json::to_vec_pretty(index)?,
        )
    }

    /// The runs, then the summary, then the index: the index comes last, so a benchmark is listed only once it's whole.
    fn write_benchmark(
        &self,
        folder: &Path,
        frame_times: &[Vec<f32>],
        json: &[u8],
        index: &Index,
    ) -> io::Result<()> {
        std::fs::create_dir_all(folder)?;
        for (i, times) in frame_times.iter().enumerate() {
            let bytes: Vec<u8> = times.iter().flat_map(|t| t.to_le_bytes()).collect();
            write_atomic(&folder.join(format!("run-{}.bin", i + 1)), &bytes)?;
        }
        write_atomic(&folder.join("summary.json"), json)?;
        self.write_index(index)
    }

    /// One change at a time. Every write is atomic, so a panic mid-change leaves nothing half-written and a poisoned
    /// lock is simply taken over.
    fn change(&self) -> MutexGuard<'_, ()> {
        self.changing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn summary(b: &Benchmark, is_baseline: bool) -> BenchmarkSummary {
    BenchmarkSummary {
        id: b.id.clone(),
        name: b.name.clone(),
        created_at: b.created_at.clone(),
        profile_id: b.profile_id.clone(),
        route_version: b.route_version,
        result: b.result.clone(),
        is_baseline,
        flagged_runs_kept: b.runs.iter().filter(|r| r.kept).count() as u32,
    }
}

/// "2026-10-12T21:30:05Z" → "20261012-213005".
fn id_from(created_at: &str) -> String {
    let digits: String = created_at
        .chars()
        .filter(char::is_ascii_digit)
        .take(14)
        .collect();
    if digits.len() < 9 {
        return "benchmark".into();
    }
    format!("{}-{}", &digits[..8], &digits[8..])
}

/// Ids are letters, digits and dashes only, so an id from the UI can't point outside the folder.
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "That benchmark doesn't exist.")
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        // Flushed before the rename: a power cut must not leave an empty file under the real name.
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BenchResult, GpuLink, RunMetrics};

    fn bench(created_at: &str, avg: f64) -> Benchmark {
        Benchmark {
            name: format!("Benchmark {created_at}"),
            created_at: created_at.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            result: BenchResult {
                metrics: RunMetrics {
                    avg_fps: avg,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn saves_lists_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("benchmarks"));
        let mut first = bench("2026-10-12T21:30:05Z", 410.0);
        let first_id = store
            .save(&mut first, &[vec![4.0, 4.5], vec![3.9], vec![4.1]])
            .unwrap();
        assert_eq!(first_id, "20261012-213005");
        assert_eq!(
            (first.id.as_str(), first.name.as_str()),
            ("20261012-213005", STARTING_POINT)
        );
        let mut second = bench("2026-10-13T08:00:00Z", 430.0);
        let second_id = store.save(&mut second, &[vec![3.0]]).unwrap();
        assert_eq!(
            second.name, "Benchmark 2026-10-13T08:00:00Z",
            "only the first is renamed"
        );

        let list = store.list();
        assert_eq!(
            list.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec![second_id.as_str(), first_id.as_str()]
        );
        assert!(list[1].is_baseline && !list[0].is_baseline);
        assert_eq!(store.get(&second_id).unwrap(), second);
        assert_eq!(store.frame_times(&first_id, 1), Some(vec![4.0, 4.5]));
        assert_eq!(store.frame_times(&first_id, 3), Some(vec![4.1]));
        assert_eq!(store.frame_times(&first_id, 4), None);
        assert_eq!(store.baseline(), Some(first_id.clone()));
        // A benchmark that can't be read back is refused before anything is written.
        let before: Vec<String> = store.list().into_iter().map(|s| s.id).collect();
        let entries = std::fs::read_dir(store.dir()).unwrap().count();
        let mut broken = bench("2026-10-14T08:00:00Z", f64::NAN);
        let err = store.save(&mut broken, &[]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            std::fs::read_dir(store.dir()).unwrap().count(),
            entries,
            "no folder appears"
        );
        assert_eq!(
            store
                .list()
                .into_iter()
                .map(|s| s.id)
                .collect::<Vec<String>>(),
            before
        );
        assert!(
            broken.id.is_empty(),
            "the caller's benchmark is left as it was"
        );
        let leftovers: Vec<_> = walk(store.dir())
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temporary files stay behind: {leftovers:?}"
        );
    }

    /// A summary.json as v0.2.0 wrote it: its context has no `gpuPnpDeviceId` and no `gpuLink`.
    const SAVED_BY_0_2_0: &str = r#"{
  "id": "20261013-191500",
  "name": "Starting point",
  "createdAt": "2026-10-13T19:15:00Z",
  "profileId": "valorant",
  "routeVersion": 1,
  "kestrelVersion": "0.2.0",
  "presentmonVersion": "2.6.0",
  "runs": [
    {
      "metrics": {
        "frames": 36000,
        "durationS": 90.0,
        "avgFps": 400.0,
        "low1Fps": 280.0,
        "low01Fps": 200.0,
        "stuttersPerMin": 0.4,
        "pcLatencyMs": 9.8,
        "gpuBusyRatio": 0.82,
        "bottleneck": "gpu"
      },
      "flags": [],
      "kept": false
    }
  ],
  "result": {
    "metrics": {
      "frames": 36000,
      "durationS": 90.0,
      "avgFps": 400.0,
      "low1Fps": 280.0,
      "low01Fps": 200.0,
      "stuttersPerMin": 0.4,
      "pcLatencyMs": 9.8,
      "gpuBusyRatio": 0.82,
      "bottleneck": "gpu"
    },
    "spread": {
      "avgFps": 0.0,
      "low1Fps": 0.0,
      "low01Fps": 0.0
    }
  },
  "context": {
    "powerPlan": "EXM Free Power Plan V8",
    "ramMts": 3600,
    "gpu": "NVIDIA GeForce RTX 3060 Ti",
    "gpuDriver": "610.74",
    "windowsBuild": 26200,
    "displayHz": 360,
    "gameSettings": {
      "resolution": [1920, 1080],
      "fullscreenMode": 0,
      "vsync": false,
      "frameRateLimit": 0.0,
      "reflex": 1,
      "multithreadedRendering": true,
      "quality": {
        "sg.ShadowQuality": "0"
      },
      "limits": {},
      "riotSettingsRead": true
    }
  }
}"#;

    #[test]
    fn a_benchmark_saved_by_v0_2_0_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("20261013-191500");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("summary.json"), SAVED_BY_0_2_0).unwrap();
        std::fs::write(
            dir.path().join("index.json"),
            r#"{"ids": ["20261013-191500"], "baseline": "20261013-191500"}"#,
        )
        .unwrap();
        let store = Store::new(dir.path());
        let old = store
            .get("20261013-191500")
            .expect("a v0.2.0 benchmark loads");
        assert_eq!(
            old.context.gpu.as_deref(),
            Some("NVIDIA GeForce RTX 3060 Ti")
        );
        assert_eq!(old.context.windows_build, 26200);
        assert_eq!(
            (old.context.gpu_pnp_device_id, old.context.gpu_link),
            (None, None),
            "the new fields are simply missing"
        );
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert!(list[0].is_baseline);
        assert_eq!(list[0].result.metrics.avg_fps, 400.0);
    }

    #[test]
    fn the_card_and_its_link_survive_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let card = "PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&1A2B3C4D&0&0008";
        let mut linked = bench("2026-10-13T20:00:00Z", 410.0);
        linked.context.gpu_pnp_device_id = Some(card.into());
        linked.context.gpu_link = Some(GpuLink { gen: 4, width: 16 });
        let id = store.save(&mut linked, &[]).unwrap();
        let back = store.get(&id).unwrap();
        assert_eq!(back, linked);
        assert_eq!(back.context.gpu_link, Some(GpuLink { gen: 4, width: 16 }));
        // The UI reads these names (camelCase, like every other field).
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.path().join(&id).join("summary.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(json["context"]["gpuPnpDeviceId"], serde_json::json!(card));
        assert_eq!(
            json["context"]["gpuLink"],
            serde_json::json!({"gen": 4, "width": 16})
        );
    }

    #[test]
    fn clashing_ids_get_a_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let ids: Vec<String> = (0..3)
            .map(|_| {
                store
                    .save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[])
                    .unwrap()
            })
            .collect();
        assert_eq!(
            ids,
            vec!["20261012-213005", "20261012-213005-2", "20261012-213005-3"]
        );
    }

    #[test]
    fn rename_delete_and_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let a = store
            .save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[])
            .unwrap();
        let b = store
            .save(&mut bench("2026-10-13T08:00:00Z", 2.0), &[])
            .unwrap();
        store.rename(&b, "  After tweaks  ").unwrap();
        assert_eq!(store.get(&b).unwrap().name, "After tweaks");
        assert_eq!(
            store.rename(&b, "   ").unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
        store.rename(&b, &"x".repeat(80)).unwrap();
        assert_eq!(store.get(&b).unwrap().name.chars().count(), 60);
        store.rename(&b, &format!("{} x", "a".repeat(59))).unwrap();
        assert_eq!(
            store.get(&b).unwrap().name,
            "a".repeat(59),
            "a cut name never ends in a space"
        );
        store.set_baseline(&b).unwrap();
        assert_eq!(store.baseline(), Some(b.clone()));
        assert_eq!(
            store.set_baseline("nope").unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        store.delete(&b).unwrap();
        assert_eq!(store.baseline(), None);
        assert_eq!(store.list().len(), 1);
        let mut c = bench("2026-10-14T08:00:00Z", 3.0);
        store.save(&mut c, &[]).unwrap();
        assert_eq!(
            c.name, STARTING_POINT,
            "with no baseline left, the next benchmark becomes the starting point"
        );
        assert!(store.get(&a).is_some());
    }

    #[test]
    fn damaged_folders_are_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let a = store
            .save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[])
            .unwrap();
        let b = store
            .save(&mut bench("2026-10-13T08:00:00Z", 2.0), &[])
            .unwrap();
        std::fs::remove_dir_all(dir.path().join(&a)).unwrap();
        std::fs::write(dir.path().join(&b).join("summary.json"), "{not json").unwrap();
        assert!(store.list().is_empty());
        assert_eq!(store.baseline(), None, "the baseline's folder is gone");
        std::fs::write(dir.path().join("index.json"), "garbage").unwrap();
        assert!(store.list().is_empty());
        let c = store
            .save(&mut bench("2026-10-14T08:00:00Z", 3.0), &[])
            .unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(store.get("../outside").is_none() && store.get("").is_none());
        assert_eq!(
            store.delete("../outside").unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        assert!(store.get(&c).is_some());

        // A lost or damaged index is rebuilt from the folders, oldest first, and the oldest is the baseline. The
        // rebuilt index is saved with the next benchmark, which keeps its own name.
        for lose_index in [false, true] {
            let rebuilt_dir = tempfile::tempdir().unwrap();
            let rebuilt = Store::new(rebuilt_dir.path());
            let first = rebuilt
                .save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[])
                .unwrap();
            let second = rebuilt
                .save(&mut bench("2026-10-13T08:00:00Z", 2.0), &[])
                .unwrap();
            let third = rebuilt
                .save(&mut bench("2026-10-14T08:00:00Z", 3.0), &[])
                .unwrap();
            assert_eq!(rebuilt.get(&first).unwrap().name, STARTING_POINT);
            let index = rebuilt_dir.path().join("index.json");
            if lose_index {
                std::fs::remove_file(&index).unwrap();
            } else {
                std::fs::write(&index, "garbage").unwrap();
            }
            let ids = |s: &Store| -> Vec<String> { s.list().into_iter().map(|b| b.id).collect() };
            assert_eq!(
                ids(&rebuilt),
                vec![third.clone(), second.clone(), first.clone()]
            );
            assert_eq!(rebuilt.baseline(), Some(first.clone()));
            assert!(rebuilt.list()[2].is_baseline);
            let fourth = rebuilt
                .save(&mut bench("2026-10-15T08:00:00Z", 4.0), &[])
                .unwrap();
            assert_eq!(
                rebuilt.get(&fourth).unwrap().name,
                "Benchmark 2026-10-15T08:00:00Z",
                "a later benchmark keeps its own name"
            );
            assert_eq!(ids(&rebuilt), vec![fourth, third, second, first.clone()]);
            assert_eq!(rebuilt.baseline(), Some(first));
        }

        // A write that fails leaves no folder behind, and the caller's benchmark keeps its empty id.
        let blocked_dir = tempfile::tempdir().unwrap();
        let blocked = Store::new(blocked_dir.path());
        blocked
            .save(&mut bench("2026-10-12T21:30:05Z", 1.0), &[])
            .unwrap();
        std::fs::remove_file(blocked_dir.path().join("index.json")).unwrap();
        std::fs::create_dir(blocked_dir.path().join("index.json")).unwrap();
        let mut doomed = bench("2026-10-13T08:00:00Z", 2.0);
        assert!(blocked.save(&mut doomed, &[vec![1.0]]).is_err());
        assert!(!blocked_dir.path().join("20261013-080000").exists());
        assert!(doomed.id.is_empty());
        assert_eq!(blocked.list().len(), 1);
    }

    fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }
}
