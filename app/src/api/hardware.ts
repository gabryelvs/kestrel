// Formatting for the Hardware tab and Copy specs. Pure: every value comes from the report; nothing is guessed.
import { strings } from '../strings';
import { cpuName, mainGpu, ramSpeed } from './findings';
import { osLabel } from './format';
import type {
  BiosTopic, CacheInfo, CpuInfo, DisplayInfo, DriveBus, DriveMedia, Finding, GpuInfo, MemoryModule, PcieLink, ProbeError,
  Report, SystemSnapshot,
} from './types';

const h = strings.hardwarePage;

/** 4800 → "4.80 GHz". */
export const ghz = (mhz: number): string => `${(mhz / 1000).toFixed(2)} GHz`;

/** A PCIe generation: 4 → "4.0" (mirrors kestrel-core pcie::gen_label). */
export const genLabel = (gen: number): string => `${gen}.0`;

/** Whether Windows reported the link: a generation and a lane count, neither 0. */
export const linkReported = (link: PcieLink | null | undefined): link is PcieLink => !!link && link.gen > 0 && link.width > 0;

/**
 * { gen: 4, width: 16 } → "4.0 x16"; "Not reported" for a link with a 0 in it, so it never reads "x0" (mirrors
 * kestrel-bench text::link_text).
 */
export const linkLabel = (link: PcieLink): string => (linkReported(link) ? `${genLabel(link.gen)} x${link.width}` : h.notReported);

/** A cache size: 48 → "48 KB", 16384 → "16 MB", 1280 → "1.25 MB". */
export const kbLabel = (kb: number): string => (kb < 1024 ? `${kb} KB` : `${Number((kb / 1024).toFixed(2))} MB`);

/** One cache entry: "8 × 48 KB", or just "16 MB" when there's one of it. */
export const cacheLabel = (c: CacheInfo): string => (c.count > 1 ? `${c.count} × ${kbLabel(c.sizeKb)}` : kbLabel(c.sizeKb));

/** "L1 data", "L1 instruction", "L2", "L3". */
export function cacheName(c: CacheInfo): string {
  if (c.level === 1 && c.kind === 'data') return h.cache.l1Data;
  if (c.level === 1 && c.kind === 'instruction') return h.cache.l1Instruction;
  return h.cache.level(c.level);
}

/**
 * One row per cache name, in the order kestrel-core sorts them. A hybrid CPU's two sizes of one level share the row:
 * ["L2", "4 × 4 MB + 8 × 2 MB"].
 */
export function cacheRows(caches: CacheInfo[]): [string, string][] {
  const rows = new Map<string, string[]>();
  for (const c of caches) rows.set(cacheName(c), [...(rows.get(cacheName(c)) ?? []), cacheLabel(c)]);
  return [...rows].map(([name, sizes]) => [name, sizes.join(' + ')]);
}

/**
 * A GiB amount the way Windows Explorer shows it: three significant digits, cut off rather than rounded, so free space
 * is never overstated. 931.5 → "931 GB", 1863 → "1.81 TB".
 */
export function sizeLabel(gib: number): string {
  const tb = gib >= 1000;
  const value = tb ? gib / 1024 : gib;
  const decimals = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  const scale = 10 ** decimals;
  // The nudge keeps a product like 1.2 × 100 = 119.99999… from losing its last digit.
  return `${(Math.floor(value * scale + 1e-9) / scale).toFixed(decimals)} ${tb ? 'TB' : 'GB'}`;
}

/** "8 cores · 16 threads", or "8 P-cores + 16 E-cores · 32 threads" on a hybrid CPU. Needs a core count. */
export function coresLabel(cpu: CpuInfo): string {
  const cores = cpu.pCores !== null && cpu.eCores !== null ? h.hybridCores(cpu.pCores, cpu.eCores) : h.cores(cpu.cores);
  return cpu.threads > 0 ? `${cores} · ${h.threads(cpu.threads)}` : cores;
}

// The speed the RAM runs at now (the slowest module's, in MT/s) is defined in findings.ts, so ramLabel and this tab share
// one copy of that code; it is re-exported here with the other Hardware tab helpers.
export { ramSpeed };

/** "32 GB DDR4-3600"; "32 GB 3600 MT/s" without a DDR generation; null when Windows reported no memory. */
export function memorySummary(s: SystemSnapshot): string | null {
  if (s.memory.totalMb <= 0) return null;
  const gb = `${Math.round(s.memory.totalMb / 1024)} GB`;
  const ddr = s.memory.ddrGeneration;
  const mts = ramSpeed(s);
  if (ddr) return mts ? `${gb} DDR${ddr}-${mts}` : `${gb} DDR${ddr}`;
  return mts ? `${gb} ${mts} MT/s` : gb;
}

const finding = (report: Report, id: string): Finding | undefined => report.findings.find((f) => f.id === id);

/** Dual or single channel, as the RAM channel check found it; null when it couldn't tell. */
export function ramChannels(report: Report): 'dual' | 'single' | null {
  const status = finding(report, 'ram-channels')?.status;
  return status === 'pass' ? 'dual' : status === 'problem' ? 'single' : null;
}

/** What this board's BIOS calls the memory profile (mirrors kestrel-core bios_guide::memory_profile_name). */
export function memoryProfile(report: Report): string {
  const p = h.profiles;
  const { cpu, memory } = report.snapshot;
  if (cpu.vendor !== 'amd') return p.xmp;
  if (memory.ddrGeneration === 5) return p.expo;
  return report.guide.vendor === 'asus' ? p.docp : report.guide.vendor === 'msi' ? p.axmp : p.xmp;
}

/** The board maker's short name (ASUS, MSI, Gigabyte, ASRock), else the name Windows reports. */
export function boardMaker(report: Report): string {
  const vendor = report.guide.vendor;
  return vendor === 'other' ? report.snapshot.board.manufacturer.trim() : h.makers[vendor];
}

/**
 * "NVIDIA GeForce RTX 3060 Ti" → "RTX 3060 Ti", "Intel(R) Arc(TM) A770 Graphics" → "Arc A770 Graphics". The one short
 * name for a graphics card: Home's Hardware panel and the Hardware tab both use it.
 */
export const gpuShortName = (name: string): string =>
  name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/\s+/g, ' ')
    .trim()
    // Only when at least two words follow, so "AMD Radeon Graphics" keeps "Radeon".
    .replace(/^(NVIDIA GeForce|NVIDIA|AMD Radeon|AMD|Intel)\s+(?=\S+\s+\S)/i, '');

/**
 * The main graphics card when it is a separate card (not integrated, not virtual), as the PCIe checks see it (mirrors
 * kestrel-core health::separate_card). Anything about one card of a PC, such as Resizable BAR, is read from this card:
 * an integrated GPU's aperture can read as on.
 */
export function separateGpu(gpus: GpuInfo[]): GpuInfo | undefined {
  const gpu = mainGpu(gpus);
  return gpu && !gpu.isIntegrated ? gpu : undefined;
}

/** The page's one-line summary: "Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO". */
export function summaryLine(s: SystemSnapshot): string {
  const gpu = mainGpu(s.gpus);
  return [cpuName(s).replace(/^(Intel|AMD)\s+/i, ''), gpu ? gpuShortName(gpu.name) : '', memorySummary(s) ?? '', s.board.product.trim()]
    .filter((part) => part !== '')
    .join(' · ');
}

/** "Windows 11 Pro 25H2 · build 26200". */
export const windowsLine = (s: SystemSnapshot): string =>
  [osLabel(s), s.os.build > 0 ? h.build(s.os.build) : ''].filter((part) => part !== '').join(' · ');

function toDate(iso: string): Date | null {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d;
}

const dayOf = (d: Date): string => `${d.getDate()} ${h.months[d.getMonth()]}`;

/** "2026-10-13T21:40:00Z" → local "13 Oct"; null when it isn't a date. */
export function dayMonth(iso: string): string | null {
  const d = toDate(iso);
  return d ? dayOf(d) : null;
}

/** "2026-10-13T21:04:00Z" → local "13 Oct 21:04"; null when it isn't a date. */
export function dayMonthTime(iso: string): string | null {
  const d = toDate(iso);
  const two = (n: number) => n.toString().padStart(2, '0');
  return d ? `${dayOf(d)} ${two(d.getHours())}:${two(d.getMinutes())}` : null;
}

/** 8192 → "8 GB", 512 → "512 MB". */
export const vramLabel = (mb: number): string => (mb >= 1024 ? `${Math.round(mb / 1024)} GB` : `${mb} MB`);

/** 2500 → "2.5 Gbps", 100 → "100 Mbps". */
export const linkSpeedLabel = (mbps: number): string =>
  mbps >= 1000 ? `${Number((mbps / 1000).toFixed(1))} Gbps` : `${mbps} Mbps`;

/** NVMe, SSD or HDD; null when Windows didn't say. */
export const mediaLabel = (media: DriveMedia): string | null => (media === 'unknown' ? null : h.media[media]);

export const busLabel = (bus: DriveBus): string => h.bus[bus];

/** The monitor's EDID name, else "Display 1", "Display 2"… by its place among the real displays (`n` counts from 1). */
export const displayName = (d: DisplayInfo, n: number): string => d.monitorName?.trim() || h.unnamedDisplay(n);

/**
 * A module's slot the way boards print it: "ChannelA-DIMM2" or "DIMM_A2" → "A2". Other locators are shown as Windows
 * reports them; null when it reports none.
 */
export function slotLabel(m: MemoryModule): string | null {
  const text = `${m.deviceLocator} ${m.bankLabel}`;
  const match = /channel\s*([a-h])\W*dimm\s*(\d)/i.exec(text) ?? /\bdimm[\s_-]*([a-h])(\d)\b/i.exec(text);
  if (match) return `${match[1].toUpperCase()}${match[2]}`;
  return m.deviceLocator.trim() || m.bankLabel.trim() || null;
}

/** The games Kestrel found installed on a volume: VALORANT, from its install folder. */
export function gamesOn(s: SystemSnapshot, letter: string): string[] {
  const norm = (l: string) => l.replace(':', '').trim().toUpperCase();
  const valorant = s.storage.valorant;
  return valorant && norm(letter) !== '' && norm(valorant.driveLetter) === norm(letter) ? [h.games.valorant] : [];
}

export type PanelId = 'processor' | 'graphics' | 'memory' | 'board' | 'displays' | 'storage' | 'network';

/**
 * The probes behind each panel, by the names kestrel-probe gives its errors. A failed "wmi" or "probe" sits behind every
 * panel. The board's "tpm" and "device-guard" feed the TPM 2.0 and Virtualization rows of its BIOS settings.
 */
const PANEL_PROBES: Record<PanelId, string[]> = {
  processor: ['cpu', 'cpu-id', 'topology'],
  graphics: ['gpus', 'pcie'],
  memory: ['memory'],
  board: ['board', 'chipset', 'firmware', 'tpm', 'device-guard'],
  displays: ['displays'],
  storage: ['storage', 'drives'],
  network: ['network'],
};

/** True when a probe behind this panel failed, so the panel shows the notice. "mock" only shows the sample banner. */
export const panelFailed = (errors: ProbeError[], panel: PanelId): boolean =>
  errors.some((e) => e.probe === 'wmi' || e.probe === 'probe' || PANEL_PROBES[panel].includes(e.probe));

/** Live clocks poll only while the Hardware tab is shown and Kestrel's window is visible and focused (a game in front isn't Kestrel). */
export const shouldPoll = (tabShown: boolean, visible: boolean, focused: boolean): boolean => tabShown && visible && focused;

export type SettingState = 'on' | 'off' | 'unknown';

/** One row of "BIOS settings Kestrel can see". */
export interface BiosSetting {
  label: string;
  /** On, Off, UEFI or Legacy (CSM); null when Windows didn't report it. */
  value: string | null;
  state: SettingState;
  /** Its Health Check item is a Problem, so the row shows in amber. */
  problem: boolean;
  note: string | null;
  /** The BIOS-guide topic to open: set only while the setting is off and its Health Check item points at the guide. */
  topic: BiosTopic | null;
}

/**
 * XMP (from the RAM speed check), Resizable BAR (from the main separate graphics card, never an integrated GPU; left out
 * without one), virtualization, IOMMU, Secure Boot, TPM 2.0 and the boot mode.
 */
export function biosSettings(report: Report): BiosSetting[] {
  const s = report.snapshot;
  const t = h.settings;
  const row = (label: string, on: boolean | null, checkId: string, note: string | null = null): BiosSetting => {
    const check = finding(report, checkId);
    const state: SettingState = on === null ? 'unknown' : on ? 'on' : 'off';
    return {
      label,
      value: on === null ? null : on ? t.on : t.off,
      state,
      problem: check?.status === 'problem',
      note,
      topic: state === 'off' && check?.fix.kind === 'bios-guide' ? check.fix.topic : null,
    };
  };
  const ram = finding(report, 'ram-speed');
  const xmp = ram?.status === 'pass' ? true : ram?.status === 'problem' ? false : null;
  const rated = s.memory.ratedMts;
  const xmpNote = xmp === true && rated !== null ? t.ratedSpeed(rated) : xmp === false ? (ram?.title ?? null) : null;
  const gpu = separateGpu(s.gpus);
  const boot = s.board.bootMode;
  return [
    row(memoryProfile(report), xmp, 'ram-speed', xmpNote),
    ...(gpu ? [row(t.resizableBar, gpu.resizableBar, 'resizable-bar')] : []),
    row(t.virtualization[s.cpu.vendor], s.cpu.virtualization, 'virtualization'),
    row(t.iommu[s.cpu.vendor], s.security.iommu, 'iommu', s.security.iommu === false ? t.onDemandOnly : null),
    row(t.secureBoot, s.security.secureBoot, 'secure-boot'),
    row(t.tpm, s.security.tpm2, 'tpm'),
    {
      label: t.bootMode,
      value: boot === 'uefi' ? t.uefi : boot === 'legacy' ? t.legacy : null,
      state: boot === 'uefi' ? 'on' : boot === 'legacy' ? 'off' : 'unknown',
      problem: false,
      note: null,
      topic: null,
    },
  ];
}
