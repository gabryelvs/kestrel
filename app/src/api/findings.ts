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

/** Same rule as kestrel-core's gpu::main_gpu: biggest real discrete GPU, else first real one. */
export function mainGpu(gpus: GpuInfo[]): GpuInfo | undefined {
  const real = gpus.filter((g) => g.vendor !== 'virtual');
  const discrete = real.filter((g) => !g.isIntegrated).sort((a, b) => (b.vramMb ?? 0) - (a.vramMb ?? 0));
  return discrete[0] ?? real[0];
}

/** Windows driver version "32.0.16.1074" → NVIDIA's "610.74"; other vendors unchanged. */
export function driverLabel(gpu: GpuInfo): string {
  const parts = gpu.driverVersion.split('.');
  if (gpu.vendor !== 'nvidia' || parts.length !== 4 || parts.some((p) => !/^\d+$/.test(p))) return gpu.driverVersion;
  const digits = `${Number(parts[2]) % 10}${parts[3].padStart(4, '0')}`;
  return `${digits.slice(0, 3)}.${digits.slice(3)}`;
}

export function cpuLabel(s: SystemSnapshot): string {
  const name = s.cpu.name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/@.*$/, '')
    .replace(/^\d+(st|nd|rd|th) Gen\s+/i, '')
    .replace(/\s+\d+-Core Processor$/i, '')
    .replace(/\s+/g, ' ')
    .trim();
  return s.cpu.threads ? `${name} · ${s.cpu.cores}C/${s.cpu.threads}T` : name;
}

export function ramLabel(s: SystemSnapshot): string {
  const gb = Math.round(s.memory.totalMb / 1024);
  const ddr = s.memory.ddrGeneration ? ` DDR${s.memory.ddrGeneration}` : '';
  const speeds = s.memory.modules.map((m) => m.configuredMts ?? 0).filter((v) => v > 0);
  return `${gb} GB${ddr}${speeds.length ? ` · ${Math.min(...speeds)} MT/s` : ''}`;
}

export function displayLabel(s: SystemSnapshot): string {
  const main = s.displays.find((d) => d.isPrimary && !d.isVirtual) ?? s.displays.find((d) => !d.isVirtual);
  return main ? `${main.currentHz} Hz · ${main.width}×${main.height}` : '—';
}
