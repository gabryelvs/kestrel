// Mirrors the Rust types in crates/kestrel-core (serde camelCase / kebab-case).
export type Severity = 'high' | 'medium' | 'low' | 'info';
export type Category = 'hardware' | 'display' | 'gpu' | 'windows' | 'storage' | 'background' | 'vanguard' | 'network';
export type Status = 'pass' | 'problem' | 'info' | 'unknown';
export type BiosTopic = 'xmp' | 'memory-gear' | 'iommu' | 'resizable-bar' | 'update-bios';

export type Fix =
  | { kind: 'bios-guide'; topic: BiosTopic }
  | { kind: 'later-stage'; stage: number }
  | { kind: 'steps'; steps: string[] }
  | { kind: 'link'; label: string; url: string }
  | { kind: 'nothing' };

export interface Finding { id: string; category: Category; severity: Severity; status: Status; title: string; detail: string; fix: Fix }
export interface Score { value: number; problems: number; potentialGain: number }
export interface GuideSection { topic: BiosTopic; title: string; why: string; steps: string[]; caution: string | null; link: { label: string; url: string } | null }
export interface BiosGuide { vendor: 'asus' | 'msi' | 'gigabyte' | 'asrock' | 'other'; board: string; enterKey: string; tip: string; sections: GuideSection[] }
export interface GpuInfo { name: string; vendor: 'nvidia' | 'amd' | 'intel' | 'virtual' | 'other'; driverVersion: string; driverDate: string | null; vramMb: number | null; isIntegrated: boolean }
export interface MemoryModule { capacityMb: number; configuredMts: number | null; maxMts: number | null; partNumber: string; manufacturer: string; bankLabel: string; deviceLocator: string }
export interface DisplayInfo { name: string; adapter: string; currentHz: number; maxHz: number; width: number; height: number; isPrimary: boolean; isVirtual: boolean }

export interface SystemSnapshot {
  schemaVersion: number;
  collectedAt: string;
  os: { caption: string; build: number; displayVersion: string | null };
  machine: { formFactor: 'desktop' | 'laptop' | 'unknown'; manufacturer: string; model: string; onBattery: boolean | null };
  cpu: { name: string; vendor: 'intel' | 'amd' | 'other'; cores: number; threads: number };
  gpus: GpuInfo[];
  memory: { totalMb: number; ddrGeneration: number | null; modules: MemoryModule[] };
  displays: DisplayInfo[];
  board: { manufacturer: string; product: string; biosVersion: string; biosDate: string | null };
  storage: { valorant: { path: string; driveLetter: string; media: 'nvme' | 'ssd' | 'hdd' | 'unknown'; freeGb: number; totalGb: number } | null };
  probeErrors: { probe: string; message: string }[];
}

export interface Report { appVersion: string; snapshot: SystemSnapshot; findings: Finding[]; score: Score; guide: BiosGuide }
export interface LoadSample { cpuPercent: number; ramPercent: number; gpuPercent: number | null; gpuTempC: number | null }

// Stage 2: the benchmark (Rust: crates/kestrel-bench and app/src-tauri/src/bench).
export type Bottleneck = 'gpu' | 'cpu' | 'mixed' | 'unknown';
export interface RunMetrics {
  frames: number;
  durationS: number;
  avgFps: number;
  low1Fps: number;
  low01Fps: number;
  stuttersPerMin: number;
  pcLatencyMs: number | null;
  gpuBusyRatio: number | null;
  bottleneck: Bottleneck;
}
export type Flag =
  | { kind: 'too-short' }
  | { kind: 'not-in-front' }
  | { kind: 'fps-capped' }
  | { kind: 'dropped-data' }
  | { kind: 'busy-background'; name: string };
export interface RunRecord { metrics: RunMetrics; flags: Flag[]; kept: boolean }
export interface BenchResult { metrics: RunMetrics; spread: { avgFps: number; low1Fps: number; low01Fps: number } }
export interface ValorantSettings {
  resolution: [number, number] | null;
  fullscreenMode: number | null;
  vsync: boolean | null;
  frameRateLimit: number | null;
  reflex: number | null;
  multithreadedRendering: boolean | null;
  quality: Record<string, string>;
  limits: Record<string, string>;
}
export interface BenchContext {
  powerPlan: string | null;
  ramMts: number | null;
  gpu: string | null;
  gpuDriver: string | null;
  windowsBuild: number;
  displayHz: number | null;
  gameSettings: ValorantSettings | null;
}
export interface Benchmark {
  id: string;
  name: string;
  createdAt: string;
  profileId: string;
  routeVersion: number;
  kestrelVersion: string;
  presentmonVersion: string;
  runs: RunRecord[];
  result: BenchResult;
  context: BenchContext;
}
export interface BenchmarkSummary {
  id: string;
  name: string;
  createdAt: string;
  profileId: string;
  routeVersion: number;
  result: BenchResult;
  isBaseline: boolean;
  flaggedRunsKept: number;
}
export type MetricId = 'avg-fps' | 'low1-fps' | 'low01-fps' | 'stutters-per-min' | 'pc-latency-ms';
export type Verdict = 'real-gain' | 'real-loss' | 'noise';
export interface MetricDelta { metric: MetricId; baseline: number; current: number; deltaPct: number | null; verdict: Verdict }
export interface Comparison { baselineId: string; currentId: string; metrics: MetricDelta[]; settingsDiffer: string[]; routeMismatch: boolean }
export interface BenchDetail { benchmark: Benchmark; graph: number[]; comparison: Comparison | null; baselineName: string | null }
export interface GameProfile { id: string; name: string; processName: string; route: { version: number; captureSeconds: number; switchAtSeconds: number } }
export type SessionPhase =
  | { phase: 'waiting' }
  | { phase: 'capturing' }
  | { phase: 'review'; flags: Flag[] }
  | { phase: 'done' }
  | { phase: 'failed'; message: string }
  | { phase: 'cancelled' };
export type BenchState = SessionPhase & {
  profileId: string;
  pass: number;
  gameRunning: boolean;
  runs: (RunRecord | null)[];
  resultId: string | null;
  startMode: 'hotkey' | 'countdown';
  startKey: string;
  captureSeconds: number;
  switchAtSeconds: number;
  secondsLeft: number | null;
  countdown: number | null;
};
