import { describe, expect, it } from 'vitest';
import { ownerReport } from './owner-report';
import { specsText } from './specs';

const lines = (text: string) => text.split('\n');

describe('Copy specs', () => {
  it('writes the owner’s PC in seven lines', () => {
    expect(specsText(ownerReport())).toBe([
      'CPU: Intel Core i9-11900K (8 cores, 16 threads)',
      'GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe 4.0 x16',
      'RAM: 32 GB DDR4-3600 · dual channel · 2 × 16 GB Corsair CMG32GX4M2D3600C18',
      'Board: ASUS ROG MAXIMUS XIII HERO · BIOS 2103 (2024-05-20)',
      'Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz',
      'Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX-00AYRA0 (HDD)',
      'OS: Windows 11 Pro 25H2 (build 26200)',
    ].join('\n'));
  });

  it('counts P-cores and E-cores on a hybrid CPU', () => {
    const r = ownerReport();
    r.snapshot.cpu = { ...r.snapshot.cpu, name: '13th Gen Intel(R) Core(TM) i9-13900K', cores: 24, threads: 32, pCores: 8, eCores: 16 };
    expect(lines(specsText(r))[0]).toBe('CPU: Intel Core i9-13900K (8 P-cores + 16 E-cores, 32 threads)');
  });

  it('lists modules that differ one by one', () => {
    const r = ownerReport();
    const [a] = r.snapshot.memory.modules;
    const b = { ...a, capacityMb: 8192, manufacturer: 'Kingston', partNumber: 'KF3600C17D4/8GX', configuredMts: 3200 };
    r.snapshot.memory = { ...r.snapshot.memory, totalMb: 24576, modules: [a, b] };
    r.findings = r.findings.map((f) => (f.id === 'ram-channels' ? { ...f, status: 'problem' } : f));
    expect(lines(specsText(r))[2]).toBe('RAM: 24 GB DDR4-3200 · single channel · 16 GB Corsair CMG32GX4M2D3600C18 + 8 GB Kingston KF3600C17D4/8GX');
  });

  it('gives only the lanes in use before any benchmark, and no PCIe part without a link', () => {
    const r = ownerReport();
    const gpu = r.snapshot.gpus[1];
    gpu.linkUnderLoad = null;
    expect(lines(specsText(r))[1]).toBe('GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe x16');
    gpu.pcie = null;
    expect(lines(specsText(r))[1]).toBe('GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74');
  });

  it('never writes a PCIe link with a zero in it, as Not reported or x0', () => {
    const r = ownerReport();
    const gpu = r.snapshot.gpus[1];
    gpu.linkUnderLoad = { link: { gen: 0, width: 0 }, measuredAt: '2026-10-13T21:40:00', benchmarkId: 'sample-1' };
    // The lanes in use stand in for a link under load that Windows didn't report.
    expect(lines(specsText(r))[1]).toBe('GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe x16');
    gpu.pcie = { cardMax: null, slotMax: null, now: { gen: 1, width: 0 } };
    expect(lines(specsText(r))[1]).toBe('GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74');
  });

  it('leaves out every line Kestrel can’t fill', () => {
    const r = ownerReport();
    const s = r.snapshot;
    r.snapshot = {
      ...s,
      cpu: { ...s.cpu, name: '' },
      gpus: [s.gpus[0]], // only the Parsec adapter, which isn't a real card
      memory: { ...s.memory, totalMb: 0 },
      board: { ...s.board, product: '' },
      displays: [{ ...s.displays[0], isVirtual: true }],
      storage: { ...s.storage, drives: [] },
    };
    expect(specsText(r)).toBe('OS: Windows 11 Pro 25H2 (build 26200)');
    r.snapshot.os = { caption: '', build: 0, displayVersion: null };
    expect(specsText(r)).toBe('');
  });

  it('leaves out the parts of a line Windows didn’t report', () => {
    const r = ownerReport();
    const s = r.snapshot;
    r.snapshot = {
      ...s,
      cpu: { ...s.cpu, cores: 0, threads: 0 },
      gpus: [{ ...s.gpus[1], vramMb: null, driverVersion: '', pcie: null, linkUnderLoad: null }],
      memory: { ...s.memory, ddrGeneration: null, modules: [] },
      board: { ...s.board, manufacturer: '', biosVersion: '' },
      os: { ...s.os, build: 0 },
    };
    r.guide = { ...r.guide, vendor: 'other' };
    r.findings = r.findings.filter((f) => f.id !== 'ram-channels');
    expect(lines(specsText(r))).toEqual([
      'CPU: Intel Core i9-11900K',
      'GPU: NVIDIA GeForce RTX 3060 Ti',
      'RAM: 32 GB',
      'Board: ROG MAXIMUS XIII HERO',
      'Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz',
      'Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX-00AYRA0 (HDD)',
      'OS: Windows 11 Pro 25H2',
    ]);
  });

  it('writes a drive without parentheses when its type is unknown', () => {
    const r = ownerReport();
    r.snapshot.storage.drives[1] = { ...r.snapshot.storage.drives[1], model: 'Generic USB Flash Disk', media: 'unknown', bus: 'usb' };
    expect(lines(specsText(r))[5]).toBe('Storage: Samsung SSD 980 PRO 1TB (NVMe) · Generic USB Flash Disk');
  });
});
