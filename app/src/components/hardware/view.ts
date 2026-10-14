// What each Hardware panel shows, as plain data: the components only lay it out. Pure, so it's tested in node.
import { cpuName, driverLabel, mainGpu } from '../../api/findings';
import {
  biosSettings, boardMaker, busLabel, cacheRows, coresLabel, dayMonth, displayName, gamesOn, ghz, gpuShortName, linkLabel,
  linkSpeedLabel, mediaLabel, ramChannels, ramSpeed, separateGpu, sizeLabel, slotLabel, vramLabel,
} from '../../api/hardware';
import type { BiosTopic, CoreClock, GpuInfo, PcieLink, Report, SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';

const h = strings.hardwarePage;

/** One label–value line of a panel. A null value shows Not reported. */
export interface Fact {
  label: string;
  value: string | null;
  /** good: lime (on, or as good as it gets); warn: amber (a Health Check problem); muted: a hint rather than a value. */
  tone?: 'good' | 'warn' | 'muted';
  note?: string;
  /** Shows the BIOS guide link for this topic. */
  topic?: BiosTopic;
}

/** A device's panel: its name (null: Not reported) and its lines. */
export interface Card { name: string | null; facts: Fact[] }

/** The instruction sets the Processor panel always shows, in the probe's order; the ones the CPU lacks are dimmed. */
const INSTRUCTIONS = ['SSE4.2', 'AVX', 'AVX2', 'AVX-512', 'AVX-VNNI', 'AMX'];

const onOff = (value: boolean | null): string | null => (value === null ? null : value ? h.settings.on : h.settings.off);

/** Trademarks and Windows' instance numbers out: "Intel(R) Ethernet Controller (3) I225-V" → "Intel Ethernet Controller I225-V". */
const cleanName = (name: string): string => name.replace(/\((R|TM)\)/gi, '').replace(/\s*\(\d+\)/g, '').replace(/\s+/g, ' ').trim();

/** A Health Check item that is a Problem: amber, plus its BIOS-guide topic when it has one. */
function flagged(report: Report, id: string): Pick<Fact, 'tone' | 'topic'> {
  const f = report.findings.find((x) => x.id === id);
  if (f?.status !== 'problem') return {};
  return f.fix.kind === 'bios-guide' ? { tone: 'warn', topic: f.fix.topic } : { tone: 'warn' };
}

export function processorView(s: SystemSnapshot): Card & { chips: { name: string; on: boolean }[] | null } {
  const { cpu } = s;
  const p = h.processor;
  const caches = cacheRows(cpu.caches);
  return {
    name: cpuName(s) || null,
    facts: [
      { label: p.codename, value: cpu.codename },
      { label: p.socket, value: cpu.socket },
      { label: p.cores, value: cpu.cores > 0 ? coresLabel(cpu) : null },
      { label: p.base, value: cpu.baseMhz === null ? null : ghz(cpu.baseMhz) },
      { label: p.maxBoost, value: cpu.maxMhz === null ? null : ghz(cpu.maxMhz) },
      ...(caches.length > 0 ? caches.map(([label, value]) => ({ label, value })) : [{ label: p.caches, value: null }]),
    ],
    // No sets at all means CPUID couldn't be read, not a CPU without SSE4.2: Not reported, never all dimmed.
    chips: cpu.instructions.length > 0 ? INSTRUCTIONS.map((name) => ({ name, on: cpu.instructions.includes(name) })) : null,
  };
}

export interface ClockCell { label: string; tag: string | null; text: string; percent: number }

/**
 * The live clocks: one cell per physical core (P or E on hybrid CPUs) with a bar scaled to the max boost clock, else to
 * the fastest core; Average and Highest over the cores Windows reported; the 1 Hz badge, or Paused (cells dimmed).
 */
export function clockView(clocks: CoreClock[], maxMhz: number | null, polling: boolean) {
  const c = h.clocks;
  const values = clocks.map((x) => x.mhz).filter((v): v is number => v !== null);
  const top = maxMhz ?? Math.max(0, ...values);
  const cells: ClockCell[] = clocks.map((x) => ({
    label: c.core(x.core),
    tag: x.kind === null ? null : c.kinds[x.kind],
    text: x.mhz === null ? '—' : ghz(x.mhz),
    percent: x.mhz === null || top <= 0 ? 0 : Math.min(100, Math.round((x.mhz / top) * 100)),
  }));
  return {
    cells,
    average: values.length > 0 ? ghz(values.reduce((a, b) => a + b, 0) / values.length) : null,
    highest: values.length > 0 ? ghz(Math.max(...values)) : null,
    badge: polling ? c.live : c.paused,
    paused: !polling,
  };
}

/**
 * One graphics card's lines. An integrated GPU has no PCIe slot to show. `separate` is the main separate card, the only
 * one the Health Check's PCIe and Resizable BAR items look at: only its lines can be flagged, and only it has a
 * Resizable BAR line (another card's aperture, an integrated GPU's above all, can read as on).
 */
function gpuFacts(report: Report, gpu: GpuInfo, separate: boolean): Fact[] {
  const g = h.graphics;
  const facts: Fact[] = [
    // 0 MB is Windows not knowing, not a card with no memory.
    { label: g.vram, value: gpu.vramMb ? vramLabel(gpu.vramMb) : null },
    { label: g.driver, value: gpu.driverVersion ? `${driverLabel(gpu)}${gpu.driverDate ? ` (${gpu.driverDate})` : ''}` : null },
  ];
  if (gpu.isIntegrated) return facts;
  const flag = (id: string) => (separate ? flagged(report, id) : {});
  const { pcie, linkUnderLoad } = gpu;
  const link = (l: PcieLink | null) => (l ? linkLabel(l) : h.notReported);
  if (pcie) {
    facts.push(
      { label: g.pcie, value: g.supports(link(pcie.cardMax), link(pcie.slotMax)) },
      { label: g.now, value: pcie.now ? linkLabel(pcie.now) : null, ...(pcie.now ? { note: g.idle } : {}), ...flag('pcie-lanes') },
    );
  } else {
    facts.push({ label: g.pcie, value: null });
  }
  const day = linkUnderLoad ? (dayMonth(linkUnderLoad.measuredAt) ?? linkUnderLoad.measuredAt) : '';
  facts.push(
    linkUnderLoad
      ? { label: g.underLoad, value: g.fromBenchmark(linkLabel(linkUnderLoad.link), day), ...flag('pcie-speed') }
      : { label: g.underLoad, value: g.runBenchmark, tone: 'muted' },
  );
  if (separate) {
    facts.push({ label: g.resizableBar, value: onOff(gpu.resizableBar), ...(gpu.resizableBar ? { tone: 'good' as const } : {}), ...flagged(report, 'resizable-bar') });
  }
  return facts;
}

/**
 * One card per real graphics card, the main one first; the other adapters only by name, for the dim line: virtual ones
 * (Parsec…), DisplayLink docks and a card without its driver (Microsoft Basic Display Adapter).
 */
export function graphicsView(report: Report): { cards: Card[]; others: string[] } {
  const { gpus } = report.snapshot;
  const main = mainGpu(gpus);
  const separate = separateGpu(gpus);
  const real = gpus.filter((g) => g.vendor !== 'virtual').sort((a, b) => Number(b === main) - Number(a === main));
  return {
    cards: real.map((gpu) => ({ name: cleanName(gpu.name) || null, facts: gpuFacts(report, gpu, gpu === separate) })),
    others: gpus.filter((g) => g.vendor === 'virtual').map((g) => cleanName(g.name)),
  };
}

export interface ModuleRow { slot: string | null; size: string | null; maker: string | null; part: string | null; speed: string | null; rank: string | null }

export function memoryView(report: Report): Card & { modules: ModuleRow[] } {
  const s = report.snapshot;
  const m = h.memory;
  const { totalMb, ddrGeneration, modules, slotsTotal, ratedMts: rated } = s.memory;
  const channels = ramChannels(report);
  const speed = ramSpeed(s);
  return {
    name: totalMb > 0 ? `${Math.round(totalMb / 1024)} GB${ddrGeneration ? ` DDR${ddrGeneration}` : ''}` : null,
    facts: [
      { label: m.channels, value: channels === null ? null : m[channels], ...flagged(report, 'ram-channels') },
      { label: m.slots, value: modules.length > 0 ? m.slotsUsed(modules.length, slotsTotal) : null },
      { label: m.speed, value: speed === null ? null : `${speed} MT/s`, ...flagged(report, 'ram-speed') },
      rated === null ? { label: m.kit, value: m.ratingUnknown, tone: 'muted' } : { label: m.kit, value: `${rated} MT/s` },
    ],
    modules: modules.map((mod) => ({
      slot: slotLabel(mod),
      size: mod.capacityMb > 0 ? `${Math.round(mod.capacityMb / 1024)} GB` : null,
      maker: mod.manufacturer.trim() || null,
      part: mod.partNumber.trim() || null,
      speed: mod.configuredMts ? `${mod.configuredMts} MT/s` : null,
      rank: mod.rank ? h.rank(mod.rank) : null,
    })),
  };
}

/** The board, its BIOS, and "BIOS settings Kestrel can see": On in lime, a Problem in amber, Off with its guide link. */
export function boardView(report: Report): Card & { settings: Fact[] } {
  const { board } = report.snapshot;
  const b = h.board;
  return {
    name: [boardMaker(report), board.product.trim()].filter(Boolean).join(' ') || null,
    facts: [
      { label: b.chipset, value: board.chipset },
      { label: b.biosVendor, value: board.biosVendor.trim() || null },
      { label: b.biosVersion, value: board.biosVersion.trim() || null },
      { label: b.biosDate, value: board.biosDate, ...flagged(report, 'bios-age') },
    ],
    settings: biosSettings(report).map((row) => ({
      label: row.label,
      value: row.value,
      ...(row.problem ? { tone: 'warn' as const } : row.state === 'on' ? { tone: 'good' as const } : {}),
      ...(row.note ? { note: row.note } : {}),
      ...(row.topic ? { topic: row.topic } : {}),
    })),
  };
}

/**
 * One card per monitor; virtual displays only by adapter, for the dim line. A refresh rate below the monitor's top one is
 * amber, as in kestrel-core health::display_refresh (Windows says 0 or 1 Hz for "hardware default", which is no rate).
 */
export function displaysView(s: SystemSnapshot): { cards: (Card & { primary: boolean })[]; virtual: string[] } {
  const d = h.displays;
  return {
    cards: s.displays.filter((x) => !x.isVirtual).map((x, i) => {
      const known = x.currentHz > 1;
      const slow = known && x.maxHz > 0 && x.currentHz + 1 < x.maxHz;
      return {
        name: displayName(x, i + 1),
        primary: x.isPrimary,
        facts: [
          { label: d.resolution, value: x.width > 0 && x.height > 0 ? `${x.width}×${x.height}` : null },
          { label: d.refresh, value: known ? (x.maxHz > 0 ? d.refreshOf(x.currentHz, x.maxHz) : `${x.currentHz} Hz`) : null, ...(slow ? { tone: 'warn' as const } : {}) },
          { label: d.connection, value: x.connection },
          { label: d.gpu, value: x.adapter.trim() ? gpuShortName(x.adapter) : null },
        ],
      };
    }),
    virtual: s.displays.filter((x) => x.isVirtual).map((x) => cleanName(x.adapter) || x.name),
  };
}

export interface VolumeRow { letter: string; space: string; games: string[] }

/** One card per drive, with each lettered volume's free space and the games installed on it. */
export function storageView(s: SystemSnapshot): (Card & { volumes: VolumeRow[] })[] {
  const st = h.storage;
  return s.storage.drives.map((drive) => ({
    name: drive.model.trim() || null,
    facts: [
      { label: st.type, value: mediaLabel(drive.media) },
      { label: st.bus, value: busLabel(drive.bus) },
      { label: st.size, value: drive.sizeGb > 0 ? sizeLabel(drive.sizeGb) : null },
    ],
    volumes: drive.volumes.map((v) => ({
      letter: `${v.letter.replace(':', '')}:`,
      space: v.totalGb > 0 ? st.free(sizeLabel(v.freeGb), sizeLabel(v.totalGb)) : h.notReported,
      games: gamesOn(s, v.letter),
    })),
  }));
}

/**
 * One card per adapter. Amber is a Health Check problem, as in kestrel-core health::link_speed (the first connected cable
 * adapter's link under 1000 Mbps) and health::adapter_power (power saving on a connected adapter).
 */
export function networkView(s: SystemSnapshot): Card[] {
  const n = h.network;
  const cable = s.network.adapters.find((a) => a.connected && a.kind === 'ethernet');
  return s.network.adapters.map((a) => ({
    name: cleanName(a.name) || null,
    facts: [
      { label: n.type, value: n.kinds[a.kind] },
      {
        label: n.link,
        value: a.linkMbps === null ? null : linkSpeedLabel(a.linkMbps),
        ...(a === cable && a.linkMbps !== null && a.linkMbps < 1000 ? { tone: 'warn' as const } : {}),
      },
      { label: n.status, value: a.connected ? n.connected : n.disconnected },
      { label: n.powerSaving, value: onOff(a.powerSaving), ...(a.connected && a.powerSaving ? { tone: 'warn' as const } : {}) },
    ],
  }));
}
