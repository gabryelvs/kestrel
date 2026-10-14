import { describe, expect, it } from 'vitest';
import { getCpuClocks } from './client';

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
