// Mirrors the Rust types in crates/kestrel-core (serde camelCase / kebab-case).
export type Severity = 'high' | 'medium' | 'low' | 'info';
export type Category = 'hardware' | 'display' | 'gpu' | 'windows' | 'storage' | 'background' | 'vanguard' | 'network';
export type Status = 'pass' | 'problem' | 'info' | 'unknown';
export type BiosTopic = 'xmp' | 'memory-gear' | 'iommu' | 'resizable-bar' | 'update-bios' | 'pcie-slot' | 'virtualization';

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

/** A PCIe link: generation 1–6 and lane count. */
export interface PcieLink { gen: number; width: number }
/** What the card and its slot support, and the link right now (a card slows it when idle). */
export interface PcieInfo { cardMax: PcieLink | null; slotMax: PcieLink | null; now: PcieLink | null }
/** The highest link seen during the newest benchmark on this card. The app fills it, never the probe. */
export interface LinkUnderLoad { link: PcieLink; measuredAt: string; benchmarkId: string }
export interface GpuInfo {
  name: string;
  vendor: 'nvidia' | 'amd' | 'intel' | 'virtual' | 'other';
  driverVersion: string;
  driverDate: string | null;
  vramMb: number | null;
  isIntegrated: boolean;
  /** Win32_VideoController.PNPDeviceID; empty when unknown. */
  pnpDeviceId: string;
  pcie: PcieInfo | null;
  linkUnderLoad: LinkUnderLoad | null;
  largestBarMb: number | null;
  resizableBar: boolean | null;
}
export interface MemoryModule { capacityMb: number; configuredMts: number | null; maxMts: number | null; partNumber: string; manufacturer: string; bankLabel: string; deviceLocator: string; rank: number | null }
export interface DisplayInfo {
  name: string;
  adapter: string;
  currentHz: number;
  maxHz: number;
  width: number;
  height: number;
  isPrimary: boolean;
  isVirtual: boolean;
  /** The name in the monitor's EDID, e.g. "XL2566K". */
  monitorName: string | null;
  /** "DisplayPort", "HDMI", "DVI", "VGA", "Internal", "USB-C" or "Wireless". */
  connection: string | null;
}
export type CacheKind = 'data' | 'instruction' | 'unified' | 'unknown';
export interface CacheInfo { level: number; kind: CacheKind; sizeKb: number; count: number }
export interface CpuInfo {
  name: string;
  vendor: 'intel' | 'amd' | 'other';
  cores: number;
  threads: number;
  family: number | null;
  model: number | null;
  stepping: number | null;
  codename: string | null;
  socket: string | null;
  baseMhz: number | null;
  maxMhz: number | null;
  /** Set only on hybrid CPUs (cores of more than one efficiency class). */
  pCores: number | null;
  eCores: number | null;
  caches: CacheInfo[];
  /** The sets the CPU has and Windows enables, in this order: SSE4.2, AVX, AVX2, AVX-512, AVX-VNNI, AMX. */
  instructions: string[];
  virtualization: boolean | null;
}
export interface BoardInfo {
  manufacturer: string;
  product: string;
  biosVersion: string;
  biosDate: string | null;
  biosVendor: string;
  chipset: string | null;
  bootMode: 'uefi' | 'legacy' | 'unknown';
}
export type DriveMedia = 'nvme' | 'ssd' | 'hdd' | 'unknown';
export type DriveBus = 'nvme' | 'sata' | 'usb' | 'sas' | 'raid' | 'virtual' | 'other';
/** Sizes are GiB (bytes / 1024³), like the VALORANT install's. */
export interface VolumeInfo { letter: string; freeGb: number; totalGb: number }
export interface DriveInfo { model: string; media: DriveMedia; bus: DriveBus; sizeGb: number; volumes: VolumeInfo[] }
export interface SecurityInfo {
  tpm2: boolean | null;
  secureBoot: boolean | null;
  vbsRunning: boolean | null;
  hvciRunning: boolean | null;
  iommu: boolean | null;
  vanguard: 'running' | 'stopped' | 'not-installed' | 'unknown';
}
export interface NetAdapter { name: string; kind: 'ethernet' | 'wifi' | 'other'; linkMbps: number | null; connected: boolean; powerSaving: boolean | null }
export interface ProbeError { probe: string; message: string }

export interface SystemSnapshot {
  schemaVersion: number;
  collectedAt: string;
  os: { caption: string; build: number; displayVersion: string | null };
  machine: { formFactor: 'desktop' | 'laptop' | 'unknown'; manufacturer: string; model: string; onBattery: boolean | null };
  cpu: CpuInfo;
  gpus: GpuInfo[];
  /** `ratedMts`: the kit's rating from the part numbers (kestrel-core's ram::rated_mts); null when Kestrel doesn't know the kit. */
  memory: { totalMb: number; ddrGeneration: number | null; modules: MemoryModule[]; slotsTotal: number | null; ratedMts: number | null };
  displays: DisplayInfo[];
  board: BoardInfo;
  storage: {
    valorant: { path: string; driveLetter: string; media: DriveMedia; freeGb: number; totalGb: number } | null;
    drives: DriveInfo[];
  };
  security: SecurityInfo;
  network: { adapters: NetAdapter[]; deliveryOptimizationInternet: boolean | null };
  probeErrors: ProbeError[];
}

export interface Report { appVersion: string; snapshot: SystemSnapshot; findings: Finding[]; score: Score; guide: BiosGuide }
export interface LoadSample { cpuPercent: number; ramPercent: number; gpuPercent: number | null; gpuTempC: number | null }
/** One physical core's clock (Rust: kestrel_core::topology::CoreClock). `kind` is set only on hybrid CPUs. */
export interface CoreClock { core: number; kind: 'performance' | 'efficiency' | null; mhz: number | null }

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
  /** The main graphics card's PNPDeviceID when the benchmark ran. */
  gpuPnpDeviceId: string | null;
  /** The highest PCIe link seen during the runs (kestrel-bench's GpuLink has PcieLink's shape). */
  gpuLink: PcieLink | null;
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
