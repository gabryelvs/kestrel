import { describe, expect, it } from 'vitest';
import {
  biosSettings, boardMaker, busLabel, cacheLabel, cacheName, cacheRows, coresLabel, dayMonth, dayMonthTime, displayName, gamesOn,
  genLabel, ghz, gpuShortName, kbLabel, linkLabel, linkReported, linkSpeedLabel, mediaLabel, memoryProfile, memorySummary, panelFailed,
  ramChannels, ramSpeed, separateGpu, shouldPoll, sizeLabel, slotLabel, summaryLine, vramLabel, windowsLine,
} from './hardware';
import { ownerReport, withFindings } from './owner-report';
import type { CacheInfo, GpuInfo, Report } from './types';

describe('hardware formatting', () => {
  it('formats clocks and PCIe links', () => {
    expect(ghz(4800)).toBe('4.80 GHz');
    expect(ghz(5312)).toBe('5.31 GHz');
    expect(genLabel(4)).toBe('4.0');
    expect(linkLabel({ gen: 4, width: 16 })).toBe('4.0 x16');
    expect(linkLabel({ gen: 1, width: 8 })).toBe('1.0 x8');
  });

  it('says Not reported for a link with a zero in it, never x0', () => {
    for (const link of [{ gen: 0, width: 16 }, { gen: 4, width: 0 }, { gen: 0, width: 0 }]) {
      expect(linkLabel(link), JSON.stringify(link)).toBe('Not reported');
      expect(linkReported(link), JSON.stringify(link)).toBe(false);
    }
    expect(linkReported({ gen: 4, width: 16 })).toBe(true);
    expect(linkReported(null)).toBe(false);
    expect(linkReported(undefined)).toBe(false);
  });

  it('names and sizes caches, one row per name', () => {
    expect(kbLabel(48)).toBe('48 KB');
    expect(kbLabel(16384)).toBe('16 MB');
    expect(kbLabel(1280)).toBe('1.25 MB');
    expect(cacheLabel({ level: 2, kind: 'unified', sizeKb: 512, count: 8 })).toBe('8 × 512 KB');
    expect(cacheLabel({ level: 3, kind: 'unified', sizeKb: 16384, count: 1 })).toBe('16 MB');
    expect(cacheName({ level: 1, kind: 'data', sizeKb: 48, count: 8 })).toBe('L1 data');
    expect(cacheName({ level: 1, kind: 'instruction', sizeKb: 32, count: 8 })).toBe('L1 instruction');
    expect(cacheName({ level: 3, kind: 'unified', sizeKb: 16384, count: 1 })).toBe('L3');
    expect(cacheRows(ownerReport().snapshot.cpu.caches)).toEqual([
      ['L1 data', '8 × 48 KB'], ['L1 instruction', '8 × 32 KB'], ['L2', '8 × 512 KB'], ['L3', '16 MB'],
    ]);
    // A Core i9-13900K in kestrel-core's order: 8 P-cores, and 16 E-cores whose L2 is shared by each cluster of four.
    const hybrid: CacheInfo[] = [
      { level: 1, kind: 'data', sizeKb: 48, count: 8 }, { level: 1, kind: 'data', sizeKb: 32, count: 16 },
      { level: 1, kind: 'instruction', sizeKb: 64, count: 16 }, { level: 1, kind: 'instruction', sizeKb: 32, count: 8 },
      { level: 2, kind: 'unified', sizeKb: 4096, count: 4 }, { level: 2, kind: 'unified', sizeKb: 2048, count: 8 },
      { level: 3, kind: 'unified', sizeKb: 36864, count: 1 },
    ];
    expect(cacheRows(hybrid)).toEqual([
      ['L1 data', '8 × 48 KB + 16 × 32 KB'], ['L1 instruction', '16 × 64 KB + 8 × 32 KB'], ['L2', '4 × 4 MB + 8 × 2 MB'], ['L3', '36 MB'],
    ]);
    expect(cacheRows([])).toEqual([]);
  });

  it('sizes drives the way Windows Explorer does', () => {
    expect(sizeLabel(931.5)).toBe('931 GB');
    expect(sizeLabel(210.5)).toBe('210 GB');
    expect(sizeLabel(1863)).toBe('1.81 TB');
    expect(sizeLabel(1228.8)).toBe('1.20 TB');
    expect(sizeLabel(57.36)).toBe('57.3 GB');
    expect(sizeLabel(7.459)).toBe('7.45 GB');
    expect(sizeLabel(0)).toBe('0.00 GB');
  });

  it('counts cores, a hybrid CPU’s by kind', () => {
    const cpu = ownerReport().snapshot.cpu;
    expect(coresLabel(cpu)).toBe('8 cores · 16 threads');
    expect(coresLabel({ ...cpu, cores: 24, threads: 32, pCores: 8, eCores: 16 })).toBe('8 P-cores + 16 E-cores · 32 threads');
    expect(coresLabel({ ...cpu, threads: 0 })).toBe('8 cores');
  });

  it('sums up memory at the slowest module’s speed', () => {
    const s = ownerReport().snapshot;
    expect(ramSpeed(s)).toBe(3600);
    expect(memorySummary(s)).toBe('32 GB DDR4-3600');
    const slower = { ...s, memory: { ...s.memory, modules: [s.memory.modules[0], { ...s.memory.modules[1], configuredMts: 3200 }] } };
    expect(memorySummary(slower)).toBe('32 GB DDR4-3200');
    expect(memorySummary({ ...s, memory: { ...s.memory, ddrGeneration: null } })).toBe('32 GB 3600 MT/s');
    const noSpeed = { ...s, memory: { ...s.memory, modules: s.memory.modules.map((m) => ({ ...m, configuredMts: null })) } };
    expect(ramSpeed(noSpeed)).toBeNull();
    expect(memorySummary(noSpeed)).toBe('32 GB DDR4');
    expect(memorySummary({ ...s, memory: { ...s.memory, totalMb: 0 } })).toBeNull();
  });

  it('reads the channels from the RAM channel check', () => {
    const owner = ownerReport();
    expect(ramChannels(owner)).toBe('dual');
    expect(ramChannels(withFindings(owner, { 'ram-channels': { status: 'problem' } }))).toBe('single');
    expect(ramChannels(withFindings(owner, { 'ram-channels': { status: 'unknown' } }))).toBeNull();
  });

  it('names the memory profile and the board maker the way the BIOS does', () => {
    const owner = ownerReport();
    expect(memoryProfile(owner)).toBe('XMP');
    const amd = (ddr: number | null, vendor: Report['guide']['vendor']): Report => ({
      ...owner,
      snapshot: { ...owner.snapshot, cpu: { ...owner.snapshot.cpu, vendor: 'amd' }, memory: { ...owner.snapshot.memory, ddrGeneration: ddr } },
      guide: { ...owner.guide, vendor },
    });
    expect(memoryProfile(amd(5, 'asus'))).toBe('EXPO');
    expect(memoryProfile(amd(4, 'asus'))).toBe('D.O.C.P.');
    expect(memoryProfile(amd(null, 'msi'))).toBe('A-XMP');
    expect(memoryProfile(amd(4, 'gigabyte'))).toBe('XMP');
    expect(boardMaker(owner)).toBe('ASUS');
    const dell = { ...owner, guide: { ...owner.guide, vendor: 'other' as const }, snapshot: { ...owner.snapshot, board: { ...owner.snapshot.board, manufacturer: ' Dell Inc. ' } } };
    expect(boardMaker(dell)).toBe('Dell Inc.');
  });

  it('shortens graphics card names and finds the separate card', () => {
    expect(gpuShortName('NVIDIA GeForce RTX 3060 Ti')).toBe('RTX 3060 Ti');
    expect(gpuShortName('AMD Radeon RX 7800 XT')).toBe('RX 7800 XT');
    expect(gpuShortName('Intel(R) Arc(TM) A770 Graphics')).toBe('Arc A770 Graphics');
    expect(gpuShortName('AMD Radeon(TM) Graphics')).toBe('Radeon Graphics');
    const gpus = ownerReport().snapshot.gpus;
    expect(separateGpu(gpus)?.name).toBe('NVIDIA GeForce RTX 3060 Ti');
    const integrated: GpuInfo = { ...gpus[1], name: 'Intel(R) UHD Graphics 770', vendor: 'intel', isIntegrated: true };
    expect(separateGpu([integrated])).toBeUndefined();
    expect(separateGpu([gpus[0]])).toBeUndefined();
    // A laptop lists its integrated GPU first; the separate card is still the discrete one.
    expect(separateGpu([integrated, gpus[1]])?.name).toBe('NVIDIA GeForce RTX 3060 Ti');
  });

  it('names the card the checks name when two separate cards are equally big', () => {
    const [, rtx] = ownerReport().snapshot.gpus;
    for (const vramMb of [8192, null]) {
      const first: GpuInfo = { ...rtx, name: 'NVIDIA GeForce RTX 3060 Ti', vramMb };
      const second: GpuInfo = { ...rtx, name: 'NVIDIA GeForce RTX 3070', vramMb };
      expect(separateGpu([first, second])?.name, `${vramMb}`).toBe('NVIDIA GeForce RTX 3070');
      expect(separateGpu([second, first])?.name, `${vramMb}`).toBe('NVIDIA GeForce RTX 3060 Ti');
    }
  });

  it('sums up the PC and Windows in one line each', () => {
    const s = ownerReport().snapshot;
    expect(summaryLine(s)).toBe('Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO');
    expect(summaryLine({ ...s, gpus: [], board: { ...s.board, product: '' } })).toBe('Core i9-11900K · 32 GB DDR4-3600');
    expect(windowsLine(s)).toBe('Windows 11 Pro 25H2 · build 26200');
    expect(windowsLine({ ...s, os: { caption: '', build: 0, displayVersion: null } })).toBe('');
  });

  it('writes dates in local time, day first', () => {
    expect(dayMonth('2026-10-13T21:40:00')).toBe('13 Oct');
    expect(dayMonthTime('2026-10-13T21:04:00')).toBe('13 Oct 21:04');
    expect(dayMonthTime('2026-01-02T03:05:00')).toBe('2 Jan 03:05');
    expect(dayMonth('')).toBeNull();
    expect(dayMonthTime('not a date')).toBeNull();
  });

  it('labels VRAM, link speeds, drives, displays and memory slots', () => {
    expect(vramLabel(8192)).toBe('8 GB');
    expect(vramLabel(512)).toBe('512 MB');
    expect(linkSpeedLabel(2500)).toBe('2.5 Gbps');
    expect(linkSpeedLabel(1000)).toBe('1 Gbps');
    expect(linkSpeedLabel(100)).toBe('100 Mbps');
    expect(mediaLabel('nvme')).toBe('NVMe');
    expect(mediaLabel('hdd')).toBe('HDD');
    expect(mediaLabel('unknown')).toBeNull();
    expect(busLabel('sata')).toBe('SATA');
    expect(busLabel('usb')).toBe('USB');
    const [monitor] = ownerReport().snapshot.displays;
    expect(displayName(monitor, 1)).toBe('XL2566K');
    expect(displayName({ ...monitor, monitorName: null }, 2)).toBe('Display 2');
    expect(displayName({ ...monitor, monitorName: ' ' }, 3)).toBe('Display 3');
    const [module] = ownerReport().snapshot.memory.modules;
    expect(slotLabel(module)).toBe('A2');
    expect(slotLabel({ ...module, deviceLocator: 'DIMM_B1', bankLabel: '' })).toBe('B1');
    expect(slotLabel({ ...module, deviceLocator: 'CPU1 DIMM A1', bankLabel: 'BANK 0' })).toBe('A1');
    expect(slotLabel({ ...module, deviceLocator: 'P0_ChannelA', bankLabel: 'BANK 0' })).toBe('P0_ChannelA');
    expect(slotLabel({ ...module, deviceLocator: '', bankLabel: '' })).toBeNull();
  });

  it('tags a volume with the games installed on it', () => {
    const s = ownerReport().snapshot;
    expect(gamesOn(s, 'C')).toEqual(['VALORANT']);
    expect(gamesOn(s, 'c:')).toEqual(['VALORANT']);
    expect(gamesOn(s, 'D')).toEqual([]);
    expect(gamesOn({ ...s, storage: { ...s.storage, valorant: null } }, 'C')).toEqual([]);
  });

  it('points each failed probe at its panel', () => {
    const failed = (probe: string) => [{ probe, message: 'x' }];
    expect(panelFailed(failed('cpu-id'), 'processor')).toBe(true);
    expect(panelFailed(failed('cpu-id'), 'graphics')).toBe(false);
    expect(panelFailed(failed('pcie'), 'graphics')).toBe(true);
    expect(panelFailed(failed('firmware'), 'board')).toBe(true);
    expect(panelFailed(failed('drives'), 'storage')).toBe(true);
    expect(panelFailed(failed('displays'), 'displays')).toBe(true);
    // The Motherboard and BIOS panel's TPM 2.0 and Virtualization rows read these two WMI queries.
    for (const probe of ['board', 'chipset', 'firmware', 'tpm', 'device-guard']) {
      expect(panelFailed(failed(probe), 'board'), probe).toBe(true);
      expect(panelFailed(failed(probe), 'processor'), probe).toBe(false);
    }
    for (const panel of ['processor', 'graphics', 'memory', 'board', 'displays', 'storage', 'network'] as const) {
      expect(panelFailed(failed('wmi'), panel)).toBe(true);
      expect(panelFailed(failed('probe'), panel)).toBe(true);
      expect(panelFailed(failed('mock'), panel)).toBe(false);
      expect(panelFailed([], panel)).toBe(false);
    }
  });

  it('polls the clocks only while the tab is shown and the window is visible and focused', () => {
    // [tab shown, window visible, window focused, polls]
    const table: [boolean, boolean, boolean, boolean][] = [
      [true, true, true, true],
      [true, true, false, false], // a game in front
      [true, false, true, false], // minimised
      [true, false, false, false],
      [false, true, true, false], // another tab
      [false, true, false, false],
      [false, false, true, false],
      [false, false, false, false],
    ];
    for (const [tab, visible, focused, polls] of table) {
      expect(shouldPoll(tab, visible, focused), `${tab} ${visible} ${focused}`).toBe(polls);
    }
  });
});

describe('BIOS settings Kestrel can see', () => {
  it('lists the owner’s settings, linking the guide only for one that is off', () => {
    expect(biosSettings(ownerReport())).toEqual([
      { label: 'XMP', value: 'On', state: 'on', problem: false, note: 'RAM runs at its rated 3600 MT/s', topic: null },
      { label: 'Resizable BAR', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'Virtualization (VT-x)', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'IOMMU (VT-d)', value: 'Off', state: 'off', problem: false, note: 'Only for Vanguard On-Demand', topic: 'iommu' },
      { label: 'Secure Boot', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'TPM 2.0', value: 'On', state: 'on', problem: false, note: null, topic: null },
      { label: 'Boot mode', value: 'UEFI', state: 'on', problem: false, note: null, topic: null },
    ]);
  });

  it('gives XMP its rated-speed note only when the part numbers name a rating', () => {
    const owner = ownerReport();
    const unrated: Report = { ...owner, snapshot: { ...owner.snapshot, memory: { ...owner.snapshot.memory, ratedMts: null } } };
    expect(biosSettings(unrated)[0]).toEqual({ label: 'XMP', value: 'On', state: 'on', problem: false, note: null, topic: null });
  });

  it('marks settings that are off, in amber when their check is a Problem', () => {
    const owner = ownerReport();
    const s = owner.snapshot;
    const report = withFindings({
      ...owner,
      snapshot: {
        ...s,
        cpu: { ...s.cpu, virtualization: false },
        gpus: s.gpus.map((g) => ({ ...g, resizableBar: g.vendor === 'nvidia' ? false : null })),
        board: { ...s.board, bootMode: 'legacy' },
        security: { ...s.security, iommu: true, secureBoot: false, tpm2: false },
      },
    }, {
      'ram-speed': { status: 'problem', title: 'RAM runs at 2133 MT/s, but the kit is rated 3600', fix: { kind: 'bios-guide', topic: 'xmp' } },
      'resizable-bar': { status: 'info', title: 'Resizable BAR is off', fix: { kind: 'bios-guide', topic: 'resizable-bar' } },
      virtualization: { status: 'info', title: 'Virtualization is off', fix: { kind: 'bios-guide', topic: 'virtualization' } },
      iommu: { status: 'info', title: 'IOMMU is on', fix: { kind: 'nothing' } },
      'secure-boot': { status: 'problem', title: 'Secure Boot is off', fix: { kind: 'steps', steps: ['Restart into the BIOS.'] } },
      tpm: { status: 'problem', title: 'TPM 2.0 is off or missing', fix: { kind: 'steps', steps: ['Restart into the BIOS.'] } },
    });
    expect(biosSettings(report).map((r) => [r.label, r.value, r.state, r.problem, r.note, r.topic])).toEqual([
      ['XMP', 'Off', 'off', true, 'RAM runs at 2133 MT/s, but the kit is rated 3600', 'xmp'],
      ['Resizable BAR', 'Off', 'off', false, null, 'resizable-bar'],
      ['Virtualization (VT-x)', 'Off', 'off', false, null, 'virtualization'],
      ['IOMMU (VT-d)', 'On', 'on', false, null, null],
      ['Secure Boot', 'Off', 'off', true, null, null],
      ['TPM 2.0', 'Off', 'off', true, null, null],
      ['Boot mode', 'Legacy (CSM)', 'off', false, null, null],
    ]);
  });

  it('says Not reported for what Windows didn’t report, names AMD’s settings, and skips Resizable BAR without a separate card', () => {
    const owner = ownerReport();
    const s = owner.snapshot;
    const report = withFindings({
      ...owner,
      snapshot: {
        ...s,
        cpu: { ...s.cpu, vendor: 'amd', virtualization: null },
        gpus: [{ ...s.gpus[1], name: 'AMD Radeon(TM) Graphics', vendor: 'amd', isIntegrated: true }],
        board: { ...s.board, bootMode: 'unknown' },
        security: { ...s.security, iommu: null, secureBoot: null, tpm2: null },
      },
    }, { 'ram-speed': { status: 'unknown', title: 'Couldn’t tell your RAM’s rated speed', fix: { kind: 'bios-guide', topic: 'xmp' } } });
    expect(biosSettings(report).map((r) => [r.label, r.value, r.state, r.note, r.topic])).toEqual([
      ['D.O.C.P.', null, 'unknown', null, null],
      ['Virtualization (SVM)', null, 'unknown', null, null],
      ['IOMMU (AMD-Vi)', null, 'unknown', null, null],
      ['Secure Boot', null, 'unknown', null, null],
      ['TPM 2.0', null, 'unknown', null, null],
      ['Boot mode', null, 'unknown', null, null],
    ]);
  });

  it('shows Resizable BAR for the main separate card, never an integrated GPU whose aperture reads on', () => {
    const owner = ownerReport();
    const s = owner.snapshot;
    const [, rtx] = s.gpus;
    const integrated: GpuInfo = { ...rtx, name: 'Intel(R) UHD Graphics 770', vendor: 'intel', isIntegrated: true, resizableBar: true };
    const row = (gpus: GpuInfo[]) => biosSettings({ ...owner, snapshot: { ...s, gpus } }).filter((r) => r.label === 'Resizable BAR');
    // A laptop's discrete card is off while its integrated GPU reads on: the row is the discrete card's, in either order.
    for (const gpus of [[integrated, { ...rtx, resizableBar: false }], [{ ...rtx, resizableBar: false }, integrated]]) {
      expect(row(gpus).map((r) => [r.value, r.state])).toEqual([['Off', 'off']]);
    }
    expect(row([integrated, { ...rtx, resizableBar: null }]).map((r) => [r.value, r.state])).toEqual([[null, 'unknown']]);
    // No separate card: no row, however the integrated GPU reads.
    expect(row([integrated])).toEqual([]);
    expect(row([s.gpus[0]])).toEqual([]);
  });
});
