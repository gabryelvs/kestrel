import { describe, expect, it } from 'vitest';
import { getCpuClocks, tweaksApply, tweaksReport, tweaksView } from './client';
import { sampleTweaks } from './sample-tweaks';

describe('the browser preview', () => {
  it('makes up 8 cores around 4.8 GHz that move on every call', async () => {
    const first = await getCpuClocks();
    const second = await getCpuClocks();
    for (const clocks of [first, second]) {
      expect(clocks.map((c) => c.core)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
      for (const c of clocks) {
        expect(c.kind).toBeNull();
        expect(c.mhz).toBeGreaterThanOrEqual(4500);
        expect(c.mhz).toBeLessThanOrEqual(5100);
      }
    }
    expect(second.map((c) => c.mhz)).not.toEqual(first.map((c) => c.mhz));
  });
});

describe('the tweaks in the browser preview', () => {
  it('answer like the commands: with a value, or a rejection with the text a closed gate refuses with', async () => {
    const view = await tweaksView();
    expect(view.rows.map((row) => row.id)[0]).toBe('power-plan');
    expect(await tweaksReport()).toContain('No changes yet.');

    sampleTweaks.setGate('game-running');
    // Tauri rejects with the text itself, so the page reads either the same way.
    await expect(tweaksApply(['power-plan'])).rejects.toBe('Close VALORANT first.');
    sampleTweaks.setGate('open');
    expect(sampleTweaks.view().rows[0].state).toBe('available');
  });
});
