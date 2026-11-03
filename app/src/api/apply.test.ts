import { describe, expect, it } from 'vitest';
import { runApply, settle, type ApplyAt, type ApplyCommands, type ApplyStep } from './apply';
import { createSampleTweaks } from './sample-tweaks';
import type { ItemOutcome, Readiness, RestoreOutcome } from './types';

const flush = async () => {
  for (let i = 0; i < 30; i += 1) await Promise.resolve();
};

/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

const created: RestoreOutcome = { result: 'created', sequence: 57 };
const failedPoint = (reason: string): RestoreOutcome => ({ result: 'failed', reason });

/**
 * The owner's fake PC behind the commands, every call written down in order, and a Preview of the plan and the windowed
 * setting. `over` swaps a command for a stand-in.
 */
function rig(readiness: Readiness = 'ready', over: Partial<ApplyCommands> = {}) {
  const pc = createSampleTweaks({ clock: () => new Date(2026, 10, 3, 4, 50) });
  pc.setRestore(readiness);
  const calls: string[] = [];
  const steps: ApplyStep[] = [];
  const commands: ApplyCommands = {
    restorePoint: async (choice) => {
      calls.push(`restore:${choice}`);
      return pc.restorePoint(choice);
    },
    apply: async (ids) => {
      calls.push(`apply:${ids.join(',')}`);
      return pc.apply(ids);
    },
    rescan: async () => {
      calls.push('rescan');
    },
    ...over,
  };
  const preview = pc.preview(['power-plan', 'windowed-optimizations']);
  return { pc, calls, steps, preview, run: (at: ApplyAt) => runApply(commands, preview, at, (step) => steps.push(step)) };
}

const APPLY = 'apply:power-plan,windowed-optimizations';

describe('Apply, in order', () => {
  it('asks Windows for the restore point first, and changes nothing until it is answered', async () => {
    const answer = deferred<RestoreOutcome>();
    const calls: string[] = [];
    const r = rig('ready', {
      restorePoint: async (choice) => {
        calls.push(`restore:${choice}`);
        return answer.promise;
      },
      apply: async (ids) => {
        calls.push(`apply:${ids.join(',')}`);
        return [];
      },
    });
    const running = r.run('pressed');
    await flush();
    expect(calls).toEqual(['restore:create']);
    answer.resolve(created);
    await running;
    expect(calls).toEqual(['restore:create', APPLY]);
  });

  it('then applies, scans again, and says so in that order', async () => {
    const r = rig();
    const next = await r.run('pressed');
    expect(r.calls).toEqual(['restore:create', APPLY, 'rescan']);
    expect(r.steps).toEqual(['restoring', 'applying', 'rescanning']);
    expect(next).toMatchObject({ next: 'applied' });
    if (next.next !== 'applied') throw new Error('Expected the changes to be made.');
    expect(next.outcomes.map((o) => [o.id, o.ok])).toEqual([['power-plan', true], ['windowed-optimizations', true]]);
    // The windowed setting is read at the game's next launch, by the Preview's timing.
    expect(next.restart).toBe(true);
    expect(r.pc.report()).toContain('Restore point created (#57)');
  });

  it('says to restart only for a change that waits for the next launch, as the Preview said', async () => {
    const r = rig();
    const onlyPlan = r.pc.preview(['power-plan']);
    const next = await runApply({ restorePoint: async () => created, apply: async (ids) => r.pc.apply(ids), rescan: async () => undefined }, onlyPlan, 'pressed', () => undefined);
    expect(next).toMatchObject({ next: 'applied', restart: false });
  });

  it('goes on after a restore point that was created, reused or skipped', async () => {
    const accepted: RestoreOutcome[] = [created, { result: 'reused', sequence: 56, time: '2026-11-02T09:00:00+00:00' }, { result: 'skipped' }];
    for (const outcome of accepted) {
      const r = rig('ready', { restorePoint: async () => outcome });
      const next = await r.run('pressed');
      expect(next.next, outcome.result).toBe('applied');
    }
  });

  it('does not go on after a failed restore point: it asks, and applies nothing', async () => {
    const calls: string[] = [];
    const r = rig('ready', {
      restorePoint: async (choice) => {
        calls.push(`restore:${choice}`);
        return failedPoint('System Protection is off.');
      },
      apply: async (ids) => {
        calls.push(`apply:${ids.join(',')}`);
        return [];
      },
    });
    const next = await r.run('pressed');
    expect(next).toEqual({ next: 'ask', question: { kind: 'failed', reason: 'System Protection is off.' } });
    expect(calls).toEqual(['restore:create']);
    expect(r.steps).toEqual(['restoring']);
  });

  it('applies nothing for a failed point that is answered with Cancel, which is no call at all', async () => {
    const r = rig('ready', { restorePoint: async () => failedPoint('Timed out.') });
    await r.run('pressed');
    // Cancel asks Apply for nothing more, however long the page waits.
    await flush();
    expect(r.calls).toEqual([]);
    expect(r.pc.view().anyActive).toBe(false);
    expect(r.pc.report()).toBe('Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-03 04:50\nNo changes yet.');
  });

  it('goes on after a failed point only when told to, and then asks Windows for nothing more', async () => {
    const r = rig();
    const next = await r.run('go-on');
    expect(r.calls).toEqual([APPLY, 'rescan']);
    expect(next).toMatchObject({ next: 'applied' });
  });

  it('asks before any command when System Protection is off or a policy decides', async () => {
    const off = rig('protection-off');
    expect(await off.run('pressed')).toEqual({ next: 'ask', question: { kind: 'protection-off' } });
    const policy = rig('blocked-by-policy');
    expect(await policy.run('pressed')).toEqual({ next: 'ask', question: { kind: 'blocked-by-policy' } });
    for (const r of [off, policy]) {
      expect(r.calls).toEqual([]);
      expect(r.steps).toEqual([]);
    }
  });

  it('turns System Protection on and creates the point when asked to, then applies', async () => {
    const r = rig('protection-off');
    const next = await r.run('turn-on');
    expect(r.calls).toEqual(['restore:turn-on-and-create', APPLY, 'rescan']);
    expect(next.next).toBe('applied');
    expect(r.pc.view().rows.some((row) => row.id === 'system-protection')).toBe(true);
  });

  it('skips the restore point only when told to continue without one', async () => {
    // Pressing Apply never skips, whatever the readiness was.
    for (const readiness of ['ready', 'unknown', 'protection-off', 'blocked-by-policy'] as const) {
      const r = rig(readiness);
      await r.run('pressed');
      expect(r.calls, readiness).not.toContain('restore:skip');
    }
    // Neither does turning on, or going on after a failure.
    const turnedOn = rig('protection-off');
    await turnedOn.run('turn-on');
    expect(turnedOn.calls).not.toContain('restore:skip');
    const wentOn = rig();
    await wentOn.run('go-on');
    expect(wentOn.calls).not.toContain('restore:skip');
    // Continue without records the skip, and then applies.
    for (const readiness of ['protection-off', 'blocked-by-policy'] as const) {
      const r = rig(readiness);
      const next = await r.run('without');
      expect(r.calls, readiness).toEqual(['restore:skip', APPLY, 'rescan']);
      expect(r.steps).toEqual(['applying', 'rescanning']);
      expect(next.next).toBe('applied');
      expect(r.pc.report()).toContain('Restore point skipped');
    }
  });
});

describe('Apply, when something is refused', () => {
  it('stops at a restore point command that is rejected, applies nothing, and says no settings were changed', async () => {
    const calls: string[] = [];
    const r = rig('ready', {
      restorePoint: async (choice) => {
        calls.push(`restore:${choice}`);
        throw new Error('Kestrel is already changing settings in another window.');
      },
      apply: async (ids) => {
        calls.push(`apply:${ids.join(',')}`);
        return [];
      },
    });
    const next = await r.run('pressed');
    expect(next).toEqual({ next: 'stopped', message: 'Kestrel is already changing settings in another window.', unchanged: true });
    expect(calls).toEqual(['restore:create']);
  });

  it('says the same of a skip that is rejected, but not of a turn-on, which may have changed System Protection', async () => {
    const reject = async () => {
      throw 'The journal is held by another process.';
    };
    const skipped = rig('blocked-by-policy', { restorePoint: reject });
    expect(await skipped.run('without')).toEqual({ next: 'stopped', message: 'The journal is held by another process.', unchanged: true });
    const turnedOn = rig('protection-off', { restorePoint: reject });
    expect(await turnedOn.run('turn-on')).toEqual({ next: 'stopped', message: 'The journal is held by another process.', unchanged: false });
  });

  it('is turned away by a closed gate at the restore point, before anything is applied', async () => {
    for (const [gate, said] of [['game-running', 'Close VALORANT first.'], ['benchmark-active', 'Finish the benchmark first.']] as const) {
      const r = rig();
      r.pc.setGate(gate);
      const next = await r.run('pressed');
      expect(next, gate).toEqual({ next: 'stopped', message: said, unchanged: true });
      expect(r.calls, gate).toEqual(['restore:create']);
    }
  });

  it('reports an apply that is refused after the restore point was made, and keeps the point on record', async () => {
    const calls: string[] = [];
    const r = rig();
    const pc = r.pc;
    // VALORANT starts between the two commands, which each ask the gate again.
    const next = await runApply(
      {
        restorePoint: async (choice) => {
          calls.push(`restore:${choice}`);
          const outcome = pc.restorePoint(choice);
          pc.setGate('game-running');
          return outcome;
        },
        apply: async (ids) => {
          calls.push(`apply:${ids.join(',')}`);
          return pc.apply(ids);
        },
        rescan: async () => {
          calls.push('rescan');
        },
      },
      r.preview,
      'pressed',
      () => undefined,
    );
    expect(next).toEqual({ next: 'stopped', message: 'Close VALORANT first.', unchanged: true });
    expect(calls).toEqual(['restore:create', APPLY]);
    pc.setGate('open');
    expect(pc.report()).toContain('Restore point created (#57)');
    expect(pc.report()).not.toContain('Applied');
    expect(pc.view().anyActive).toBe(false);
  });

  it('shows what each item came to when some are refused and some are not', async () => {
    const r = rig();
    const next = await runApply(
      { restorePoint: async () => created, apply: async () => [{ id: 'power-plan', title: 'Kestrel power plan', ok: false, message: 'Windows refused the change.' }], rescan: async () => undefined },
      r.preview,
      'pressed',
      () => undefined,
    );
    expect(next).toMatchObject({ next: 'applied', restart: false });
  });
});

describe('scanning again after a change', () => {
  it('waits for the scan before the result is ready, so what is measured next is the PC as it is now', async () => {
    const scan = deferred<void>();
    const calls: string[] = [];
    const r = rig('ready', {
      rescan: async () => {
        calls.push('rescan');
        return scan.promise;
      },
    });
    let finished = false;
    const running = r.run('pressed').then((next) => {
      finished = true;
      return next;
    });
    await flush();
    expect(finished).toBe(false);
    expect(r.steps).toEqual(['restoring', 'applying', 'rescanning']);
    scan.resolve();
    expect((await running).next).toBe('applied');
  });

  it('still shows the result when the scan fails, and when it throws before it starts', async () => {
    const failing = rig('ready', {
      rescan: async () => {
        throw new Error('The scan failed.');
      },
    });
    expect((await failing.run('pressed')).next).toBe('applied');
    const throwing = rig('ready', {
      rescan: () => {
        throw new Error('Not even started.');
      },
    });
    expect((await throwing.run('pressed')).next).toBe('applied');
  });

  it('does not scan, or say it is scanning, when nothing took', async () => {
    const calls: string[] = [];
    const r = rig('ready', {
      apply: async () => [{ id: 'power-plan', title: 'Kestrel power plan', ok: false, message: 'Windows refused the change.' }],
      rescan: async () => {
        calls.push('rescan');
      },
    });
    const next = await r.run('pressed');
    expect(next.next).toBe('applied');
    expect(calls).toEqual([]);
    expect(r.steps).toEqual(['restoring', 'applying']);
  });
});

describe('settle', () => {
  const outcome = (ok: boolean): ItemOutcome => ({ id: 'game-mode', title: 'Game Mode', ok, message: ok ? 'Undone.' : 'No.' });

  it('scans after a change that took, and waits for it', async () => {
    const scan = deferred<void>();
    let scans = 0;
    let done = false;
    const settled = settle([outcome(false), outcome(true)], async () => {
      scans += 1;
      return scan.promise;
    }).then(() => {
      done = true;
    });
    await flush();
    expect(scans).toBe(1);
    expect(done).toBe(false);
    scan.resolve();
    await settled;
    expect(done).toBe(true);
  });

  it('does not scan when nothing took', async () => {
    let scans = 0;
    await settle([outcome(false)], async () => {
      scans += 1;
    });
    await settle([], async () => {
      scans += 1;
    });
    expect(scans).toBe(0);
  });

  it('never fails, whatever the scan comes to', async () => {
    await expect(
      settle([outcome(true)], async () => {
        throw new Error('The scan failed.');
      }),
    ).resolves.toBeUndefined();
  });
});
