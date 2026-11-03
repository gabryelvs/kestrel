import { describe, expect, it } from 'vitest';
import { STAGE, strings } from './strings';

describe('stage numbers', () => {
  it('follow the stage order of decision 66', () => {
    expect(strings.home.lockInSoon).toBe('Arrives in stage 7');
    expect(strings.toolbox.later).toBe('Fixes and the VALORANT troubleshooter arrive in stage 8.');
    // What the Tweaks and Lock In pages say: App spreads these into ComingSoon, which prints arrives(stage).
    expect(strings.comingSoon.arrives(strings.comingSoon.tweaks.stage)).toBe('Arrives in stage 5');
    expect(strings.comingSoon.arrives(strings.comingSoon.lockIn.stage)).toBe('Arrives in stage 7');
  });

  it('give every tweak the title kestrel-tweaks’ catalog gives it', () => {
    // catalog.rs pins the same seven titles, so the page and the report in Rust use the same words. toEqual also
    // fails on a name that isn't a tweak; tsc fails on a TweakId with no name.
    expect(strings.tweaks.names).toEqual({
      'power-plan': 'Kestrel power plan',
      'game-mode': 'Game Mode',
      'background-recording': 'Background recording off',
      'windowed-optimizations': 'Optimizations for windowed games',
      'gpu-preference': 'GPU preference',
      'pointer-precision': 'Enhance pointer precision off',
      'system-protection': 'System Protection',
    });
  });

  it('are only ever one of STAGE’s', () => {
    const named = [...JSON.stringify(strings).matchAll(/stage (\d+)/gi)].map((m) => Number(m[1]));
    expect(named.length).toBeGreaterThan(0);
    for (const n of named) expect(Object.values(STAGE)).toContain(n);
  });
});
