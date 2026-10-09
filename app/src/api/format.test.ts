import { describe, expect, it } from 'vitest';
import mock from './mock-report.json';
import type { Report } from './types';
import { filledSegments, loadSummary, osLabel, toPoints } from './format';

const report = mock as unknown as Report;

describe('format helpers', () => {
  it('fills one segment per 5 points', () => {
    expect(filledSegments(86)).toBe(17);
    expect(filledSegments(0)).toBe(0);
    expect(filledSegments(100)).toBe(20);
    expect(filledSegments(140)).toBe(20);
  });

  it('labels Windows without the vendor prefix', () => {
    expect(osLabel(report.snapshot)).toBe('Windows 11 Pro 25H2');
  });

  it('right-aligns graph points and flips y', () => {
    expect(toPoints([10, 20], 60)).toBe('58,90 59,80');
    expect(toPoints([], 60)).toBe('');
  });

  it('summarises load, adding GPU and temperature only when known', () => {
    expect(loadSummary({ cpuPercent: 6.24, ramPercent: 41, gpuPercent: 3, gpuTempC: 44 })).toBe('CPU 6% · GPU 3% · 44°C');
    expect(loadSummary({ cpuPercent: 6.24, ramPercent: 41, gpuPercent: null, gpuTempC: null })).toBe('CPU 6% · RAM 41%');
  });
});
