import { describe, expect, it } from 'vitest';
import { shouldPoll } from '../../api/hardware';
import { ownerReport, withFindings } from '../../api/owner-report';
import type { CoreClock, NetAdapter } from '../../api/types';
import { boardView, clockView, displaysView, graphicsView, memoryView, networkView, processorView, storageView, type Fact } from './view';

/** [label, value] of each line, plus its tone and BIOS-guide topic when it has them. */
const lines = (facts: Fact[]) => facts.map((f) => [f.label, f.value, ...(f.tone ? [f.tone] : []), ...(f.topic ? [f.topic] : [])]);

/** The owner's 8 cores, C7 the fastest. */
const ownerClocks: CoreClock[] = [4800, 4850, 4750, 4900, 4800, 4950, 4700, 5200].map((mhz, core) => ({ core, kind: null, mhz }));

describe('Live clocks', () => {
  it('shows one cell per core with Average and Highest while polling', () => {
    const view = clockView(ownerClocks, 5300, shouldPoll(true, true, true));
    expect(view.cells.map((c) => [c.label, c.tag, c.text])).toEqual([
      ['C0', null, '4.80 GHz'], ['C1', null, '4.85 GHz'], ['C2', null, '4.75 GHz'], ['C3', null, '4.90 GHz'],
      ['C4', null, '4.80 GHz'], ['C5', null, '4.95 GHz'], ['C6', null, '4.70 GHz'], ['C7', null, '5.20 GHz'],
    ]);
    expect(view.cells[7].percent).toBe(98); // 5200 of the 5300 MHz max boost
    expect([view.average, view.highest, view.badge, view.paused]).toEqual(['4.87 GHz', '5.20 GHz', '1 Hz', false]);
  });

  it('pauses, dimmed, whenever Kestrel’s window is hidden or not focused', () => {
    for (const [visible, focused] of [[true, false], [false, true], [false, false]]) {
      const view = clockView(ownerClocks, 5300, shouldPoll(true, visible, focused));
      expect([view.badge, view.paused]).toEqual(['Paused', true]);
      // The last values stay on screen, dimmed.
      expect(view.cells[7].text).toBe('5.20 GHz');
    }
  });

  it('tags P- and E-cores, and scales to the fastest core when the max boost is unknown', () => {
    const hybrid: CoreClock[] = [
      { core: 0, kind: 'performance', mhz: 5500 }, { core: 1, kind: 'efficiency', mhz: 4300 }, { core: 2, kind: 'efficiency', mhz: null },
    ];
    const view = clockView(hybrid, null, true);
    expect(view.cells.map((c) => [c.label, c.tag, c.text, c.percent])).toEqual([
      ['C0', 'P', '5.50 GHz', 100], ['C1', 'E', '4.30 GHz', 78], ['C2', 'E', '—', 0],
    ]);
    expect([view.average, view.highest]).toEqual(['4.90 GHz', '5.50 GHz']);
    const none = clockView([{ core: 0, kind: null, mhz: null }], 5300, true);
    expect([none.average, none.highest]).toEqual([null, null]);
  });
});

describe('Hardware panels', () => {
  it('shows the processor, its caches and its instruction sets', () => {
    const view = processorView(ownerReport().snapshot);
    expect(view.name).toBe('Intel Core i9-11900K');
    expect(lines(view.facts)).toEqual([
      ['Codename', 'Rocket Lake-S'], ['Socket', 'LGA1200'], ['Cores', '8 cores · 16 threads'], ['Base', '3.50 GHz'],
      ['Max boost', '5.30 GHz'], ['L1 data', '8 × 48 KB'], ['L1 instruction', '8 × 32 KB'], ['L2', '8 × 512 KB'], ['L3', '16 MB'],
    ]);
    expect(view.chips).toEqual([
      { name: 'SSE4.2', on: true }, { name: 'AVX', on: true }, { name: 'AVX2', on: true }, { name: 'AVX-512', on: true },
      { name: 'AVX-VNNI', on: false }, { name: 'AMX', on: false },
    ]);
  });

  it('says Not reported for processor values Windows didn’t give, never all chips dimmed', () => {
    const s = ownerReport().snapshot;
    const view = processorView({ ...s, cpu: { ...s.cpu, codename: null, socket: null, maxMhz: null, caches: [], instructions: [] } });
    expect(lines(view.facts).filter(([, value]) => value === null).map(([label]) => label)).toEqual(['Codename', 'Socket', 'Max boost', 'Caches']);
    expect(view.chips).toBeNull();
  });

  it('shows each real graphics card with its PCIe link, and the virtual ones by name', () => {
    const view = graphicsView(ownerReport());
    expect(view.cards.map((c) => c.name)).toEqual(['NVIDIA GeForce RTX 3060 Ti']);
    expect(lines(view.cards[0].facts)).toEqual([
      ['VRAM', '8 GB'], ['Driver', '610.74 (2026-09-15)'], ['PCIe link', 'Card 4.0 x16 · Slot 4.0 x16'], ['Now', '1.0 x16'],
      ['Under load', '4.0 x16 · from your benchmark on 13 Oct'], ['Resizable BAR', 'On', 'good'],
    ]);
    expect(view.cards[0].facts[3].note).toBe('The card slows its link when idle.');
    expect(view.virtual).toEqual(['Parsec Virtual Display Adapter']);
  });

  it('asks for a benchmark before any, and flags only the main card’s PCIe problems', () => {
    const owner = ownerReport();
    const [parsec, rtx] = owner.snapshot.gpus;
    const igpu = { ...rtx, name: 'Intel(R) UHD Graphics 750', vendor: 'intel' as const, driverVersion: '31.0.101.2127', vramMb: 128, isIntegrated: true };
    const report = withFindings(
      { ...owner, snapshot: { ...owner.snapshot, gpus: [parsec, igpu, { ...rtx, linkUnderLoad: null }] } },
      { 'pcie-lanes': { status: 'problem', fix: { kind: 'bios-guide', topic: 'pcie-slot' } } },
    );
    const view = graphicsView(report);
    expect(view.cards.map((c) => c.name)).toEqual(['NVIDIA GeForce RTX 3060 Ti', 'Intel UHD Graphics 750']);
    expect(lines(view.cards[0].facts).slice(3)).toEqual([
      ['Now', '1.0 x16', 'warn', 'pcie-slot'], ['Under load', 'Run a benchmark to check the link under load.', 'muted'], ['Resizable BAR', 'On', 'good'],
    ]);
    // Integrated graphics have no PCIe slot or Resizable BAR to show.
    expect(lines(view.cards[1].facts)).toEqual([['VRAM', '128 MB'], ['Driver', '31.0.101.2127 (2026-09-15)']]);
  });

  it('shows Resizable BAR on the main separate card’s panel only, and flags nothing on another card', () => {
    const owner = ownerReport();
    const [parsec, rtx] = owner.snapshot.gpus;
    // A second, smaller card: its PCIe link is real, but Resizable BAR and the Health Check's amber are the main card's.
    const second = { ...rtx, name: 'NVIDIA GeForce GTX 1650', vramMb: 4096 };
    const report = withFindings(
      { ...owner, snapshot: { ...owner.snapshot, gpus: [parsec, second, rtx] } },
      {
        'pcie-lanes': { status: 'problem', fix: { kind: 'bios-guide', topic: 'pcie-slot' } },
        'pcie-speed': { status: 'problem', fix: { kind: 'bios-guide', topic: 'pcie-slot' } },
        'resizable-bar': { status: 'problem', fix: { kind: 'bios-guide', topic: 'resizable-bar' } },
      },
    );
    const [main, other] = graphicsView(report).cards;
    expect([main.name, other.name]).toEqual(['NVIDIA GeForce RTX 3060 Ti', 'NVIDIA GeForce GTX 1650']);
    expect(lines(main.facts).slice(3)).toEqual([
      ['Now', '1.0 x16', 'warn', 'pcie-slot'], ['Under load', '4.0 x16 · from your benchmark on 13 Oct', 'warn', 'pcie-slot'],
      ['Resizable BAR', 'On', 'warn', 'resizable-bar'],
    ]);
    expect(lines(other.facts)).toEqual([
      ['VRAM', '4 GB'], ['Driver', '610.74 (2026-09-15)'], ['PCIe link', 'Card 4.0 x16 · Slot 4.0 x16'], ['Now', '1.0 x16'],
      ['Under load', '4.0 x16 · from your benchmark on 13 Oct'],
    ]);
  });

  it('shows memory per module, with the kit rating from the part numbers', () => {
    const view = memoryView(ownerReport());
    expect(view.name).toBe('32 GB DDR4');
    expect(lines(view.facts)).toEqual([['Channels', 'Dual channel'], ['Slots', '2 of 4'], ['Speed now', '3600 MT/s'], ['Kit rated', '3600 MT/s']]);
    expect(view.modules).toEqual([
      { slot: 'A2', size: '16 GB', maker: 'Corsair', part: 'CMG32GX4M2D3600C18', speed: '3600 MT/s', rank: 'Dual rank' },
      { slot: 'B2', size: '16 GB', maker: 'Corsair', part: 'CMG32GX4M2D3600C18', speed: '3600 MT/s', rank: 'Dual rank' },
    ]);
  });

  it('flags slow RAM with the XMP guide, and says Rating unknown for a kit Kestrel doesn’t know', () => {
    const owner = ownerReport();
    const slow = withFindings(owner, {
      'ram-speed': { status: 'problem', title: 'RAM runs at 2133 MT/s, but the kit is rated 3600', detail: '', fix: { kind: 'bios-guide', topic: 'xmp' } },
    });
    expect(lines(memoryView(slow).facts)[2]).toEqual(['Speed now', '3600 MT/s', 'warn', 'xmp']);
    const s = owner.snapshot;
    const unknown = { ...owner, snapshot: { ...s, memory: { ...s.memory, ratedMts: null } } };
    expect(lines(memoryView(unknown).facts)[3]).toEqual(['Kit rated', 'Rating unknown', 'muted']);
    const bare = memoryView({ ...owner, snapshot: { ...s, memory: { ...s.memory, slotsTotal: null, modules: [{ ...s.memory.modules[0], rank: null, manufacturer: '' }] } } });
    expect(lines(bare.facts)[1]).toEqual(['Slots', '1 used']);
    expect([bare.modules[0].maker, bare.modules[0].rank]).toEqual([null, null]);
  });

  it('says Not reported for a memory module capacity or graphics memory Windows reported as 0, never 0 GB', () => {
    const owner = ownerReport();
    const s = owner.snapshot;
    const memory = memoryView({ ...owner, snapshot: { ...s, memory: { ...s.memory, modules: [{ ...s.memory.modules[0], capacityMb: 0 }] } } });
    expect(memory.modules[0].size).toBeNull();
    const [, rtx] = s.gpus;
    const card = { ...rtx, vramMb: 0, driverVersion: '', driverDate: null, pcie: null, linkUnderLoad: null, resizableBar: null };
    expect(lines(graphicsView({ ...owner, snapshot: { ...s, gpus: [card] } }).cards[0].facts)).toEqual([
      ['VRAM', null], ['Driver', null], ['PCIe link', null], ['Under load', 'Run a benchmark to check the link under load.', 'muted'], ['Resizable BAR', null],
    ]);
  });

  it('shows the board, its BIOS and the BIOS settings Kestrel can see', () => {
    const view = boardView(ownerReport());
    expect(view.name).toBe('ASUS ROG MAXIMUS XIII HERO');
    expect(lines(view.facts)).toEqual([
      ['Chipset', 'Intel Z590'], ['BIOS vendor', 'American Megatrends Inc.'], ['BIOS version', '2103'], ['BIOS date', '2024-05-20', 'warn', 'update-bios'],
    ]);
    expect(lines(view.settings)).toEqual([
      ['XMP', 'On', 'good'], ['Resizable BAR', 'On', 'good'], ['Virtualization (VT-x)', 'On', 'good'], ['IOMMU (VT-d)', 'Off', 'iommu'],
      ['Secure Boot', 'On', 'good'], ['TPM 2.0', 'On', 'good'], ['Boot mode', 'UEFI', 'good'],
    ]);
    expect(view.settings.map((f) => f.note ?? null)).toEqual(['RAM runs at its rated 3600 MT/s', null, null, 'Only for Vanguard On-Demand', null, null, null]);
  });

  it('shows each monitor by name, amber below its top refresh rate', () => {
    const s = ownerReport().snapshot;
    const view = displaysView(s);
    expect(view.cards.map((c) => [c.name, c.primary])).toEqual([['XL2566K', true], ['24G4', false]]);
    expect(lines(view.cards[0].facts)).toEqual([
      ['Resolution', '1920×1080'], ['Refresh rate', '360 Hz of 360 Hz'], ['Connection', 'DisplayPort'], ['Graphics card', 'RTX 3060 Ti'],
    ]);
    const parsec = { ...s.displays[1], name: '\\\\.\\DISPLAY3', adapter: 'Parsec Virtual Display Adapter', isVirtual: true, monitorName: null };
    const slow = displaysView({ ...s, displays: [s.displays[0], { ...s.displays[1], currentHz: 60, monitorName: null }, parsec] });
    expect(slow.cards.map((c) => c.name)).toEqual(['XL2566K', 'Display 2']);
    expect(lines(slow.cards[1].facts)[1]).toEqual(['Refresh rate', '60 Hz of 180 Hz', 'warn']);
    expect(slow.virtual).toEqual(['Parsec Virtual Display Adapter']);
  });

  it('says Not reported for the hardware-default refresh rate, and shows the rate alone when the maximum is unknown', () => {
    const s = ownerReport().snapshot;
    const view = displaysView({ ...s, displays: [{ ...s.displays[0], currentHz: 1 }, { ...s.displays[1], maxHz: 0, currentHz: 144 }] });
    expect(lines(view.cards[0].facts)[1]).toEqual(['Refresh rate', null]);
    expect(lines(view.cards[1].facts)[1]).toEqual(['Refresh rate', '144 Hz']);
  });

  it('shows every drive with its volumes and the games on them', () => {
    const view = storageView(ownerReport().snapshot);
    expect(view.map((d) => [d.name, lines(d.facts)])).toEqual([
      ['Samsung SSD 980 PRO 1TB', [['Type', 'NVMe'], ['Bus', 'NVMe'], ['Size', '931 GB']]],
      ['WDC WD20EZBX-00AYRA0', [['Type', 'HDD'], ['Bus', 'SATA'], ['Size', '1.81 TB']]],
    ]);
    expect(view.map((d) => d.volumes)).toEqual([
      [{ letter: 'C:', space: '210 GB free of 931 GB', games: ['VALORANT'] }],
      [{ letter: 'D:', space: '1.20 TB free of 1.81 TB', games: [] }],
    ]);
  });

  it('shows each network adapter, power saving in amber', () => {
    expect(networkView(ownerReport().snapshot).map((a) => [a.name, lines(a.facts)])).toEqual([
      ['Intel Ethernet Controller I225-V', [['Type', 'Ethernet'], ['Link speed', '2.5 Gbps'], ['Status', 'Connected'], ['Power saving', 'On', 'warn']]],
    ]);
  });

  it('puts amber where the Health Check does: the first connected cable adapter’s slow link, any connected adapter’s power saving', () => {
    const s = ownerReport().snapshot;
    const [lan] = s.network.adapters;
    const slowCable: NetAdapter = { ...lan, linkMbps: 100, powerSaving: false };
    const view = networkView({
      ...s,
      network: {
        ...s.network,
        adapters: [
          slowCable,
          { ...slowCable, name: 'Realtek USB GbE Family Controller', powerSaving: true },
          { ...lan, name: 'Intel(R) Wi-Fi 6 AX200 160MHz', kind: 'wifi', linkMbps: 300, connected: false },
        ],
      },
    });
    // link_speed looks at the first connected cable adapter only; adapter_power at every connected one.
    expect(view.map((a) => lines(a.facts).slice(1))).toEqual([
      [['Link speed', '100 Mbps', 'warn'], ['Status', 'Connected'], ['Power saving', 'Off']],
      [['Link speed', '100 Mbps'], ['Status', 'Connected'], ['Power saving', 'On', 'warn']],
      [['Link speed', '300 Mbps'], ['Status', 'Not connected'], ['Power saving', 'On']],
    ]);
  });
});
