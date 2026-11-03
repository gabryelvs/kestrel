import { describe, expect, it } from 'vitest';
import { STAGE, strings } from './strings';

describe('stage numbers', () => {
  it('follow the stage order of decision 66', () => {
    expect(strings.home.lockInSoon).toBe('Arrives in stage 7');
    expect(strings.toolbox.later).toBe('Fixes and the VALORANT troubleshooter arrive in stage 8.');
    // What the Lock In page says: App spreads this into ComingSoon, which prints arrives(stage).
    expect(strings.comingSoon.arrives(strings.comingSoon.lockIn.stage)).toBe('Arrives in stage 7');
    // The Tweaks tab is built; the tiers after it are what the page still points ahead to.
    expect(strings.comingSoon.arrives(STAGE.competitive)).toBe('Arrives in stage 6');
  });

  it('no longer has a stage for the Tweaks tab, which exists', () => {
    expect(STAGE).toEqual({ competitive: 6, lockIn: 7, fixes: 8 });
    expect('tweaks' in strings.comingSoon).toBe(false);
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

describe('the Tweaks tab’s words', () => {
  const t = strings.tweaks;

  it('say the spec’s sentences', () => {
    expect(t.intro).toBe('Kestrel picked these for your PC. Nothing changes until you press Apply.');
    expect(t.preview(1)).toBe('Preview 1 change');
    expect(t.preview(0)).toBe('Preview 0 changes');
    expect(t.preview(3)).toBe('Preview 3 changes');
    expect(t.otherAccount).toBe('Kestrel is running as another account. Changes go to the account signed in now.');
    expect(t.otherPending(2)).toBe('Another account has 2 change(s) from Kestrel. Those wait until that account is signed in.');
    expect(t.restoring).toBe('Creating a restore point… this can take a minute.');
    expect(t.protectionOff).toBe(
      'System Protection is off on your Windows drive. Turn it on and create a restore point? Windows will use some disk space for restore points.',
    );
    expect(t.restartValorant).toBe('Restart VALORANT for these to take effect.');
    expect(t.done).toBe('Measure it on your PC: run a benchmark and compare it with your starting point.');
    expect(t.revertConfirm).toBe('Undo every change on this list?');
    expect(t.protectionWarning).toBe('Turning System Protection off deletes this drive’s restore points.');
    expect(t.laterTiers).toEqual(['Competitive', 'Extreme']);
  });

  it('label every row state, impact and timing the tab can show', () => {
    expect(t.states).toEqual({
      available: 'Available', 'already-set': 'Already set', 'not-needed': 'Not needed', 'blocked-by-policy': 'Blocked by policy',
      applied: 'Applied', 'changed-elsewhere': 'Changed by something else', 'plan-inactive': 'Kestrel’s plan isn’t active', 'couldnt-read': 'Couldn’t read',
    });
    expect(t.impact['none-in-game']).toBe('None in game');
    expect(t.timing).toEqual({ now: 'Now', 'next-launch': 'Next time VALORANT opens', 'next-sign-in': 'Next sign-in' });
  });

  it('give every closed gate its reason, the ones a command refuses with included', () => {
    expect(t.gate).toEqual({
      'game-running': 'Close VALORANT first.',
      'benchmark-active': 'Finish the benchmark first.',
      'cant-tell': 'Kestrel couldn’t check whether VALORANT is open. Try again.',
    });
  });

  it('end the restore point question with exactly one full stop, whether or not the reason had one', () => {
    const question = 'Continue with Kestrel’s undo only?';
    expect(t.restoreFailed('System Protection is off.')).toBe(`Couldn’t create a restore point: System Protection is off. ${question}`);
    expect(t.restoreFailed('Windows refused (os error 5)')).toBe(`Couldn’t create a restore point: Windows refused (os error 5). ${question}`);
    // Only one is taken off, and only at the end.
    expect(t.restoreFailed('Timed out after 3 minutes.  ')).toBe(`Couldn’t create a restore point: Timed out after 3 minutes. ${question}`);
    expect(t.restoreFailed('It took 1.5 minutes')).toBe(`Couldn’t create a restore point: It took 1.5 minutes. ${question}`);
    expect(t.restoreFailed('Trailing dots...')).toBe(`Couldn’t create a restore point: Trailing dots... ${question}`);
    for (const reason of ['System Protection is off.', 'Windows refused (os error 5)']) {
      expect(t.restoreFailed(reason)).not.toContain('..');
    }
  });

  it('do not leave a gap where a reason is missing', () => {
    expect(t.restoreFailed('')).toBe('Couldn’t create a restore point. Continue with Kestrel’s undo only?');
    expect(t.restoreFailed('  .')).toBe('Couldn’t create a restore point. Continue with Kestrel’s undo only?');
  });

  it('never promise a gain: the copy says to measure it', () => {
    const texts: string[] = [];
    const collect = (value: unknown): void => {
      if (typeof value === 'string') texts.push(value);
      else if (typeof value === 'function') texts.push((value as (arg: string) => string)('12'));
      else if (value && typeof value === 'object') Object.values(value).forEach(collect);
    };
    collect(t);
    expect(texts.length).toBeGreaterThan(60);
    for (const text of texts) expect(text, text).not.toMatch(/improv|faster|boost|speed ?up|more fps|higher fps|\bgains?\b|increase/i);
  });
});
