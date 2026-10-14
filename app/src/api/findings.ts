import type { Category, Finding, GpuInfo, Severity, SystemSnapshot } from './types';

const SEVERITY_RANK: Record<Severity, number> = { high: 0, medium: 1, low: 2, info: 3 };
export const CATEGORY_ORDER: Category[] = ['hardware', 'display', 'gpu', 'windows', 'storage', 'background', 'vanguard', 'network'];

/** Problems only, most severe first; equal severities keep the check order. */
export function topProblems(findings: Finding[], limit = 3): Finding[] {
  return findings
    .filter((f) => f.status === 'problem')
    .sort((a, b) => SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity])
    .slice(0, limit);
}

export function groupByCategory(findings: Finding[]): { category: Category; findings: Finding[] }[] {
  return CATEGORY_ORDER.map((category) => ({ category, findings: findings.filter((f) => f.category === category) })).filter(
    (group) => group.findings.length > 0,
  );
}

/**
 * The GPU that matters for gaming: the biggest real discrete GPU, else the first real one (mirrors kestrel-core
 * gpu::main_gpu). Of discrete cards with equal video memory, or none reported for either, the LAST wins, as Rust's
 * `max_by_key` keeps it: the Hardware tab and the Health Check have to name the same card.
 */
export function mainGpu(gpus: GpuInfo[]): GpuInfo | undefined {
  const real = gpus.filter((g) => g.vendor !== 'virtual');
  // A card replaces the best so far unless it has less video memory, so an equal card (later in the list) wins.
  const biggest = real
    .filter((g) => !g.isIntegrated)
    .reduce<GpuInfo | undefined>((best, g) => (best && (g.vramMb ?? 0) < (best.vramMb ?? 0) ? best : g), undefined);
  return biggest ?? real[0];
}

/** Windows driver version "32.0.16.1074" → NVIDIA's "610.74"; other vendors unchanged. */
export function driverLabel(gpu: GpuInfo): string {
  const parts = gpu.driverVersion.split('.');
  if (gpu.vendor !== 'nvidia' || parts.length !== 4 || parts.some((p) => !/^\d+$/.test(p))) return gpu.driverVersion;
  const digits = `${Number(parts[2]) % 10}${parts[3].padStart(4, '0')}`;
  return `${digits.slice(0, 3)}.${digits.slice(3)}`;
}

/** The CPU's name without trademarks, generation, clock or core-count words: "Intel Core i9-11900K". */
export function cpuName(s: SystemSnapshot): string {
  return s.cpu.name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/@.*$/, '')
    .replace(/\s+CPU\b/i, '')
    .replace(/^\d+(st|nd|rd|th) Gen\s+/i, '')
    .replace(/\s+\d+-Core Processor$/i, '')
    .replace(/\s+/g, ' ')
    .trim();
}

export function cpuLabel(s: SystemSnapshot): string {
  const name = cpuName(s);
  return s.cpu.threads ? `${name} · ${s.cpu.cores}C/${s.cpu.threads}T` : name;
}

/** The speed the RAM runs at now: the slowest module's, in MT/s; null when no module reports one. */
export function ramSpeed(s: SystemSnapshot): number | null {
  const speeds = s.memory.modules.map((m) => m.configuredMts ?? 0).filter((v) => v > 0);
  return speeds.length > 0 ? Math.min(...speeds) : null;
}

export function ramLabel(s: SystemSnapshot): string {
  const gb = Math.round(s.memory.totalMb / 1024);
  const ddr = s.memory.ddrGeneration ? ` DDR${s.memory.ddrGeneration}` : '';
  const mts = ramSpeed(s);
  return `${gb} GB${ddr}${mts ? ` · ${mts} MT/s` : ''}`;
}

export function displayLabel(s: SystemSnapshot): string {
  const main = s.displays.find((d) => d.isPrimary && !d.isVirtual) ?? s.displays.find((d) => !d.isVirtual);
  return main ? `${main.currentHz} Hz · ${main.width}×${main.height}` : '—';
}
