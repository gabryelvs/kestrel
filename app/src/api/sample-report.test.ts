import { describe, expect, it } from 'vitest';
import { STAGE } from '../strings';
import { summaryLine } from './hardware';
import mock from './mock-report.json';
import { specsText } from './specs';
import type { Report } from './types';

// The browser preview's sample, printed by `npm run mock` from the owner's fixture (crates/kestrel-core/fixtures/owner-pc.json).
const sample = mock as unknown as Report;

describe('the sample report', () => {
  it('is schema 2 with all 31 checks, its later-stage fixes in the new stage order', () => {
    expect(sample.snapshot.schemaVersion).toBe(2);
    expect(sample.findings).toHaveLength(31);
    const stage = (id: string) => {
      const fix = sample.findings.find((f) => f.id === id)?.fix;
      return fix?.kind === 'later-stage' ? fix.stage : null;
    };
    expect(stage('power-plan')).toBe(STAGE.tweaks);
    expect(stage('adapter-power')).toBe(STAGE.competitive);
  });

  it('fills the Hardware tab and Copy specs from what the probe prints', () => {
    expect(summaryLine(sample.snapshot)).toBe('Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO');
    // The probe prints the kit's rating from the part numbers; the Memory panel reads it from there.
    expect(sample.snapshot.memory.ratedMts).toBe(3600);
    // Only the app fills the link under load (from benchmarks), never the probe, so the GPU line gives the lanes in use.
    expect(specsText(sample)).toBe([
      'CPU: Intel Core i9-11900K (8 cores, 16 threads)',
      'GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe x16',
      'RAM: 32 GB DDR4-3600 · dual channel · 2 × 16 GB Corsair CMG32GX4M2D3600C18',
      'Board: ASUS ROG MAXIMUS XIII HERO · BIOS 2103 (2024-05-20)',
      'Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz',
      'Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX-00AYRA0 (HDD)',
      'OS: Windows 11 Pro 25H2 (build 26200)',
    ].join('\n'));
  });
});
