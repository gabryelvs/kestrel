import { afterEach, describe, expect, it, vi } from 'vitest';
import { firstRunDone, markFirstRunDone } from './flag';

afterEach(() => vi.unstubAllGlobals());

describe('first-run flag', () => {
  it('is false until marked, then true', () => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
    expect(firstRunDone()).toBe(false);
    markFirstRunDone();
    expect(firstRunDone()).toBe(true);
  });

  it('treats unavailable storage as a first run and never throws', () => {
    vi.stubGlobal('localStorage', { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('blocked'); } });
    expect(firstRunDone()).toBe(false);
    expect(() => markFirstRunDone()).not.toThrow();
  });
});
