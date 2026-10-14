import { describe, expect, it } from 'vitest';
import mock from './mock-report.json';
import type { Report } from './types';
import { cpuLabel, cpuName, displayLabel, driverLabel, groupByCategory, mainGpu, ramLabel, topProblems } from './findings';

const report = mock as unknown as Report;

describe('findings helpers', () => {
  it('lists problems by severity, keeping check order within a severity', () => {
    expect(topProblems(report.findings).map((f) => f.id)).toEqual(['power-plan', 'bios-age', 'adapter-power']);
    expect(topProblems(report.findings, 1)).toHaveLength(1);
  });

  it('groups checks in a fixed category order and drops empty groups', () => {
    expect(groupByCategory(report.findings).map((g) => g.category)).toEqual([
      'hardware', 'display', 'gpu', 'windows', 'storage', 'background', 'vanguard', 'network',
    ]);
    expect(groupByCategory([])).toEqual([]);
  });

  it('picks the real GPU and shows NVIDIA driver versions the NVIDIA way', () => {
    const gpu = mainGpu(report.snapshot.gpus)!;
    expect(gpu.name).toBe('NVIDIA GeForce RTX 3060 Ti');
    expect(driverLabel(gpu)).toBe('610.74');
    expect(mainGpu([{ ...gpu, vendor: 'virtual' }])).toBeUndefined();
  });

  it('keeps the last of equally big discrete cards, as kestrel-core’s main_gpu does', () => {
    // The Hardware tab and the Health Check must name the same card, so a tie breaks the way Rust's max_by_key does.
    const rtx = mainGpu(report.snapshot.gpus)!;
    for (const vramMb of [8192, null]) {
      const first = { ...rtx, name: 'NVIDIA GeForce RTX 3060 Ti', vramMb };
      const second = { ...rtx, name: 'NVIDIA GeForce RTX 3070', vramMb };
      expect(mainGpu([first, second])?.name, `${vramMb}`).toBe('NVIDIA GeForce RTX 3070');
      expect(mainGpu([second, first])?.name, `${vramMb}`).toBe('NVIDIA GeForce RTX 3060 Ti');
    }
  });

  it('prefers more video memory, then any discrete card, then the first real integrated GPU', () => {
    const rtx = mainGpu(report.snapshot.gpus)!;
    const small = { ...rtx, name: 'NVIDIA GeForce GTX 1650', vramMb: 4096 };
    const unknown = { ...rtx, name: 'NVIDIA GeForce GTX 1050', vramMb: null };
    const igpu = { ...rtx, name: 'Intel(R) UHD Graphics 770', vendor: 'intel' as const, isIntegrated: true, vramMb: 16384 };
    const other = { ...igpu, name: 'AMD Radeon(TM) Graphics', vendor: 'amd' as const };
    for (const order of [[rtx, small], [small, rtx]]) expect(mainGpu(order)?.name).toBe(rtx.name);
    for (const order of [[small, unknown], [unknown, small]]) expect(mainGpu(order)?.name).toBe(small.name);
    // Even an integrated GPU that reports more memory never beats a discrete card.
    for (const order of [[igpu, small], [small, igpu]]) expect(mainGpu(order)?.name).toBe(small.name);
    expect(mainGpu([igpu, other])?.name).toBe(igpu.name);
    expect(mainGpu([other, igpu])?.name).toBe(other.name);
    expect(mainGpu([])).toBeUndefined();
  });

  it('formats hardware labels', () => {
    expect(cpuLabel(report.snapshot)).toBe('Intel Core i9-11900K · 8C/16T');
    expect(cpuName(report.snapshot)).toBe('Intel Core i9-11900K');
    expect(cpuLabel({ ...report.snapshot, cpu: { ...report.snapshot.cpu, name: 'Intel(R) Core(TM) i5-9400F CPU @ 2.90GHz', cores: 6, threads: 6 } })).toBe('Intel Core i5-9400F · 6C/6T');
    expect(ramLabel(report.snapshot)).toBe('32 GB DDR4 · 3600 MT/s');
    expect(displayLabel(report.snapshot)).toBe('360 Hz · 1920×1080');
  });
});
