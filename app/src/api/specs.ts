import { strings } from '../strings';
import { cpuName, driverLabel, mainGpu } from './findings';
import { osLabel } from './format';
import { boardMaker, linkLabel, linkReported, mediaLabel, memorySummary, ramChannels, vramLabel } from './hardware';
import type { MemoryModule, Report, SystemSnapshot } from './types';

const t = strings.specs;
const h = strings.hardwarePage;

const line = (label: string, text: string) => `${label}: ${text}`;

/** Copy specs (decision 77): one plain-text line per part, leaving out any line Kestrel can't fill. */
export function specsText(report: Report): string {
  const s = report.snapshot;
  return [cpuLine(s), gpuLine(s), ramLine(report), boardLine(report), displaysLine(s), storageLine(s), osLine(s)]
    .filter((text): text is string => text !== null)
    .join('\n');
}

/** "CPU: Intel Core i9-11900K (8 cores, 16 threads)"; hybrid CPUs count P-cores and E-cores. */
function cpuLine(s: SystemSnapshot): string | null {
  const name = cpuName(s);
  if (!name) return null;
  const { cores, threads, pCores, eCores } = s.cpu;
  const counts: string[] = [];
  if (cores > 0) counts.push(pCores !== null && eCores !== null ? h.hybridCores(pCores, eCores) : h.cores(cores));
  if (threads > 0) counts.push(h.threads(threads));
  return line(t.labels.cpu, counts.length > 0 ? `${name} (${counts.join(', ')})` : name);
}

/**
 * The main graphics card. Its PCIe part is the link under load, or only the lanes in use before any benchmark; a link
 * Windows didn't report (a 0 in it) is left out rather than written as "Not reported" or "x0".
 */
function gpuLine(s: SystemSnapshot): string | null {
  const gpu = mainGpu(s.gpus);
  if (!gpu) return null;
  const vram = gpu.vramMb !== null && gpu.vramMb >= 1024 ? ` ${vramLabel(gpu.vramMb)}` : '';
  const parts = [`${gpu.name}${vram}`];
  if (gpu.driverVersion) parts.push(t.driver(driverLabel(gpu)));
  const underLoad = gpu.linkUnderLoad?.link;
  const lanes = gpu.pcie?.now?.width ?? 0;
  if (linkReported(underLoad)) parts.push(t.pcie(linkLabel(underLoad)));
  else if (lanes > 0) parts.push(t.pcie(`x${lanes}`));
  return line(t.labels.gpu, parts.join(' · '));
}

/** "2 × 16 GB Corsair CMG32GX4M2D3600C18", or each module on its own, joined with " + ", when they differ. */
function modulesText(modules: MemoryModule[]): string | null {
  if (modules.length === 0) return null;
  const texts = modules.map((m) => [`${Math.round(m.capacityMb / 1024)} GB`, m.manufacturer.trim(), m.partNumber.trim()].filter(Boolean).join(' '));
  return texts.every((text) => text === texts[0]) ? `${texts.length} × ${texts[0]}` : texts.join(' + ');
}

function ramLine(report: Report): string | null {
  const total = memorySummary(report.snapshot);
  if (!total) return null;
  const channels = ramChannels(report);
  const modules = modulesText(report.snapshot.memory.modules);
  return line(t.labels.ram, [total, channels ? t.channels[channels] : null, modules].filter(Boolean).join(' · '));
}

function boardLine(report: Report): string | null {
  const { board } = report.snapshot;
  const product = board.product.trim();
  if (!product) return null;
  const version = board.biosVersion.trim();
  const bios = version ? ` · ${t.bios(version)}${board.biosDate ? ` (${board.biosDate})` : ''}` : '';
  return line(t.labels.board, `${[boardMaker(report), product].filter(Boolean).join(' ')}${bios}`);
}

function displaysLine(s: SystemSnapshot): string | null {
  const real = s.displays.filter((d) => !d.isVirtual && d.width > 0 && d.height > 0 && d.currentHz > 1);
  if (real.length === 0) return null;
  return line(t.labels.displays, real.map((d) => t.display(d.width, d.height, d.currentHz)).join(' · '));
}

function storageLine(s: SystemSnapshot): string | null {
  const drives = s.storage.drives.filter((d) => d.model.trim() !== '');
  if (drives.length === 0) return null;
  return line(t.labels.storage, drives.map((d) => {
    const media = mediaLabel(d.media);
    return media ? `${d.model.trim()} (${media})` : d.model.trim();
  }).join(' · '));
}

function osLine(s: SystemSnapshot): string | null {
  const os = osLabel(s);
  if (!os) return null;
  return line(t.labels.os, s.os.build > 0 ? `${os} (${h.build(s.os.build)})` : os);
}
