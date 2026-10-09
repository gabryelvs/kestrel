import { describe, expect, it } from 'vitest';
import mock from './mock-report.json';
import type { Report } from './types';
import { cpuLabel, displayLabel, driverLabel, groupByCategory, mainGpu, ramLabel, topProblems } from './findings';

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

  it('formats hardware labels', () => {
    expect(cpuLabel(report.snapshot)).toBe('Intel Core i9-11900K · 8C/16T');
    expect(ramLabel(report.snapshot)).toBe('32 GB DDR4 · 3600 MT/s');
    expect(displayLabel(report.snapshot)).toBe('360 Hz · 1920×1080');
  });
});
