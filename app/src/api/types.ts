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
