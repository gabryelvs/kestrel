import { describe, expect, it } from 'vitest';
import { createSampleTweaks, sampleTweaks } from './sample-tweaks';
import { revertRows, tickedIds } from './tweaks';
import type { RowState, TweakId } from './types';

/** A fresh PC every time, with a clock the test moves by hand: 2026-11-03 04:50 local time to begin with. */
function owner() {
  let now = new Date(2026, 10, 3, 4, 50);
  const pc = createSampleTweaks({ clock: () => now });
  return { pc, minutesLater: (n: number) => (now = new Date(now.getTime() + n * 60_000)) };
}

const states = (pc: ReturnType<typeof owner>['pc']): Record<string, RowState> =>
  Object.fromEntries(pc.view().rows.map((row) => [row.id, row.state]));

const HEADER = 'Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-03 04:50';

describe('the owner’s PC, as the Tweaks tab finds it', () => {
  it('offers the plan unticked and names the EXM plan, calls Game Mode and recording already set, and has no GPU choice to make', () => {
    const { pc } = owner();
    const view = pc.view();
    expect(view.rows.map((row) => row.id)).toEqual([
      'power-plan', 'game-mode', 'background-recording', 'windowed-optimizations', 'gpu-preference', 'pointer-precision',
    ]);
    expect(states(pc)).toEqual({
      'power-plan': 'available', 'game-mode': 'already-set', 'background-recording': 'already-set',
      'windowed-optimizations': 'available', 'gpu-preference': 'not-needed', 'pointer-precision': 'available',
    });
    const plan = view.rows[0];
    expect(plan.ticked).toBe(false);
    expect(plan.why).toContain('EXM Free Power Plan V8');
    expect(tickedIds(view.rows, {})).toEqual([]);
    for (const row of view.rows) expect(row.title).toBeTruthy();
  });

  it('is quiet: the gate is open, nobody else is signed in, nothing was interrupted, nothing is active', () => {
    const view = owner().pc.view();
    expect(view).toMatchObject({ gate: 'open', otherAccount: false, pendingOtherAccounts: 0, interrupted: [], unreadableLines: 0, anyActive: false });
  });

  it('has no changes yet in its report', () => {
    expect(owner().pc.report()).toBe(`${HEADER}\nNo changes yet.`);
  });

  it('reads the tags the catalog gives each tweak, and None in game for the GPU preference, since the owner has one graphics card', () => {
    const rows = Object.fromEntries(owner().pc.view().rows.map((row) => [row.id, [row.impact, row.timing]]));
    expect(rows).toEqual({
      'power-plan': ['low', 'now'], 'game-mode': ['medium', 'now'], 'background-recording': ['medium', 'now'],
      'windowed-optimizations': ['situational', 'next-launch'], 'gpu-preference': ['none-in-game', 'next-launch'], 'pointer-precision': ['none-in-game', 'now'],
    });
  });
});

describe('Preview', () => {
  it('shows the plan’s three settings beside Kestrel’s and says what else changes, and writes nothing', () => {
    const { pc } = owner();
    const { items, restore } = pc.preview(['power-plan']);
    expect(restore).toBe('ready');
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ id: 'power-plan', title: 'Kestrel power plan', timing: 'now', location: 'Power plan' });
    expect(items[0].changes.map((c) => c.label)).toEqual(['Core parking minimum cores', 'USB selective suspend', 'PCIe link-state power management']);
    expect(items[0].note).toBe(
      'Switching plans also replaces everything else in EXM Free Power Plan V8 with the settings of High performance. Undo returns to your plan as it was.',
    );
    expect(states(pc)['power-plan']).toBe('available');
    expect(pc.report()).toBe(`${HEADER}\nNo changes yet.`);
  });

  it('leaves out a row that is not available, and says when the setting is read: windowed on the next launch', () => {
    const { pc } = owner();
    const { items } = pc.preview(['game-mode', 'windowed-optimizations', 'gpu-preference', 'pointer-precision']);
    expect(items.map((i) => i.id)).toEqual(['windowed-optimizations', 'pointer-precision']);
    expect(items[0]).toMatchObject({ timing: 'next-launch', note: null });
    expect(items[0].location).toContain('DirectXUserGlobalSettings');
    expect(items[0].changes).toEqual([{ label: 'Optimizations for windowed games', now: 'not set', after: 'on' }]);
    expect(items[1].location).toContain('MouseSpeed');
    expect(items[1].changes).toEqual([{ label: 'Enhance pointer precision off', now: 'on', after: 'off' }]);
  });
});

describe('Apply and Undo', () => {
  it('walks apply and undo back to where it started', () => {
    const { pc, minutesLater } = owner();
    const first = pc.view();

    const applied = pc.apply(['power-plan']);
    expect(applied).toEqual([{ id: 'power-plan', title: 'Kestrel power plan', ok: true, message: 'Applied.' }]);
    expect(states(pc)['power-plan']).toBe('applied');
    expect(pc.view().anyActive).toBe(true);
    expect(pc.report()).toBe(
      [
        HEADER,
        'Active',
        '  Kestrel power plan: EXM Free Power Plan V8 → Kestrel (2026-11-03 04:50)',
        'History',
        '  2026-11-03 04:50 Applied Kestrel power plan: EXM Free Power Plan V8 → Kestrel',
      ].join('\n'),
    );

    minutesLater(1);
    const undone = pc.undo('power-plan');
    expect(undone).toEqual({ id: 'power-plan', title: 'Kestrel power plan', ok: true, message: 'Back to EXM Free Power Plan V8.' });
    expect(pc.view()).toEqual(first);
    expect(pc.report().split('\n').slice(1)).toEqual([
      'Active',
      '  Nothing is active.',
      'History',
      '  2026-11-03 04:51 Undid Kestrel power plan: Back to EXM Free Power Plan V8.',
      '  2026-11-03 04:50 Applied Kestrel power plan: EXM Free Power Plan V8 → Kestrel',
    ]);
  });

  it('applies the registry settings and puts them back one by one, with their values before and after in the report', () => {
    const { pc, minutesLater } = owner();
    expect(pc.apply(['windowed-optimizations', 'pointer-precision']).map((i) => i.ok)).toEqual([true, true]);
    expect(states(pc)).toMatchObject({ 'windowed-optimizations': 'applied', 'pointer-precision': 'applied' });
    minutesLater(2);
    expect(pc.undo('windowed-optimizations')).toMatchObject({ ok: true, message: 'Undone.' });
    expect(states(pc)).toMatchObject({ 'windowed-optimizations': 'available', 'pointer-precision': 'applied' });
    expect(pc.report().split('\n').slice(1)).toEqual([
      'Active',
      '  Enhance pointer precision off: on → off (2026-11-03 04:50)',
      'History',
      '  2026-11-03 04:52 Undid Optimizations for windowed games: back to not set',
      '  2026-11-03 04:50 Applied Enhance pointer precision off: on → off',
      '  2026-11-03 04:50 Applied Optimizations for windowed games: not set → on',
    ]);
    // The row's flag is what ticks it again: it was not ticked before.
    expect(pc.view().rows.find((row) => row.id === 'windowed-optimizations')?.ticked).toBe(false);
  });

  it('answers a row that is not available with its own reason and changes nothing', () => {
    const { pc } = owner();
    const items = pc.apply(['game-mode', 'gpu-preference', 'system-protection']);
    expect(items.map((i) => i.ok)).toEqual([false, false, false]);
    expect(items[0].message).toBe('Game Mode is already on.');
    expect(items[1].message).toBe('One graphics card: nothing to choose.');
    expect(pc.view().anyActive).toBe(false);
    expect(pc.report()).toBe(`${HEADER}\nNo changes yet.`);
  });

  it('has nothing to undo for what Kestrel never changed', () => {
    const { pc } = owner();
    const item = pc.undo('game-mode');
    expect(item).toMatchObject({ id: 'game-mode', ok: false });
    expect(item.message).toContain('nothing to undo');
  });
});

describe('the gate', () => {
  it('shows in the view, turns every command that changes something away with its message, and changes nothing', () => {
    const { pc } = owner();
    const cases = [['game-running', 'Close VALORANT first.'], ['benchmark-active', 'Finish the benchmark first.'], ['cant-tell', 'Kestrel couldn’t check whether VALORANT is open. Try again.']] as const;
    for (const [gate, message] of cases) {
      pc.setGate(gate);
      expect(pc.view().gate).toBe(gate);
      const commands = [
        () => pc.apply(['power-plan']), () => pc.undo('power-plan'), () => pc.applyAgain('pointer-precision'), () => pc.restoreAnyway('pointer-precision'),
        () => pc.usePlanAgain(), () => pc.removePlan(), () => pc.revertAll(), () => pc.restorePoint('create'),
      ];
      for (const command of commands) expect(command).toThrow(message);
      // A thrown text, as a Tauri command rejects with one, not an Error.
      try {
        pc.apply(['power-plan']);
      } catch (thrown) {
        expect(thrown).toBe(message);
      }
    }
    pc.setGate('open');
    expect(pc.report()).toBe(`${HEADER}\nNo changes yet.`);
    expect(states(pc)['power-plan']).toBe('available');
  });

  it('lets the tab look while it is closed', () => {
    const { pc } = owner();
    pc.setGate('game-running');
    expect(pc.preview(['power-plan']).items).toHaveLength(1);
    expect(pc.report()).toContain('No changes yet.');
  });
});

describe('Revert everything', () => {
  it('undoes the changes newest first and leaves the PC as it started', () => {
    const { pc } = owner();
    const first = pc.view();
    pc.apply(['windowed-optimizations']);
    pc.apply(['power-plan']);
    pc.apply(['pointer-precision']);
    expect(revertRows(pc.view().rows).map((row) => row.id)).toEqual(['power-plan', 'windowed-optimizations', 'pointer-precision']);

    const items = pc.revertAll();
    expect(items.map((i) => [i.id, i.ok])).toEqual([['pointer-precision', true], ['power-plan', true], ['windowed-optimizations', true]]);
    expect(pc.view()).toEqual(first);
  });

  it('says plainly what it left alone: a row something else changed', () => {
    const { pc } = owner();
    pc.apply(['windowed-optimizations', 'pointer-precision']);
    pc.changeBehindItsBack('windowed-optimizations');
    const items = pc.revertAll();
    expect(items.find((i) => i.id === 'pointer-precision')).toMatchObject({ ok: true });
    const left = items.find((i) => i.id === 'windowed-optimizations');
    expect(left).toMatchObject({ ok: false, message: 'Something else changed this after Kestrel did. Kestrel left it alone.' });
    expect(states(pc)['windowed-optimizations']).toBe('changed-elsewhere');
    expect(pc.view().anyActive).toBe(true);
  });
});

describe('a value something else changed', () => {
  it('is flagged, and Undo leaves it alone', () => {
    const { pc } = owner();
    pc.apply(['pointer-precision']);
    pc.changeBehindItsBack('pointer-precision');
    const row = pc.view().rows.find((r) => r.id === 'pointer-precision');
    expect(row).toMatchObject({ state: 'changed-elsewhere', ticked: false, why: 'Something else changed this after Kestrel did.' });
    expect(pc.undo('pointer-precision')).toMatchObject({ ok: false, message: 'Something else changed this after Kestrel did. Kestrel left it alone.' });
    expect(states(pc)['pointer-precision']).toBe('changed-elsewhere');
  });

  it('is written over by Apply again, which the report calls that', () => {
    const { pc } = owner();
    pc.apply(['pointer-precision']);
    pc.changeBehindItsBack('pointer-precision');
    expect(pc.applyAgain('pointer-precision')).toMatchObject({ ok: true, message: 'Applied.' });
    expect(states(pc)['pointer-precision']).toBe('applied');
    expect(pc.report()).toContain('Applied Enhance pointer precision off again');
  });

  it('is written over by Restore anyway, which puts the value from before Kestrel back', () => {
    const { pc } = owner();
    pc.apply(['pointer-precision']);
    pc.changeBehindItsBack('pointer-precision');
    expect(pc.restoreAnyway('pointer-precision')).toMatchObject({ ok: true, message: 'Undone.' });
    expect(states(pc)['pointer-precision']).toBe('available');
    expect(pc.view().anyActive).toBe(false);
  });

  it('is turned away by Apply again when nothing is flagged, and Restore anyway then just undoes it', () => {
    const { pc } = owner();
    pc.apply(['pointer-precision']);
    expect(pc.applyAgain('pointer-precision')).toMatchObject({ ok: false, message: 'Already set.' });
    expect(states(pc)['pointer-precision']).toBe('applied');
    expect(pc.restoreAnyway('pointer-precision')).toMatchObject({ ok: true, message: 'Undone.' });
    expect(states(pc)['pointer-precision']).toBe('available');
  });

  it('has no Apply again or Restore anyway for what Kestrel never changed', () => {
    const { pc } = owner();
    expect(pc.applyAgain('pointer-precision').message).toBe("Kestrel hasn't changed this, so there is nothing to apply again.");
    expect(pc.restoreAnyway('pointer-precision').message).toBe("Kestrel hasn't changed this, so there is nothing to undo.");
  });
});

describe('Kestrel’s plan when another plan is picked', () => {
  it('says Kestrel’s plan isn’t active, and Use it again switches back to it', () => {
    const { pc } = owner();
    pc.apply(['power-plan']);
    pc.pickPlan('Balanced');
    const row = pc.view().rows[0];
    expect(row).toMatchObject({ state: 'plan-inactive', why: 'Another plan is active: Balanced.' });
    expect(pc.view().anyActive).toBe(true);

    expect(pc.usePlanAgain()).toMatchObject({ ok: true });
    expect(states(pc)['power-plan']).toBe('applied');
  });

  it('is deleted by Remove it, which keeps the plan the person chose', () => {
    const { pc } = owner();
    pc.apply(['power-plan']);
    pc.pickPlan('Balanced');
    expect(pc.removePlan()).toMatchObject({ ok: true, message: 'Undone.' });
    const row = pc.view().rows[0];
    // Balanced is the plan in use now, so Kestrel's plan is offered again, ticked, for it.
    expect(row).toMatchObject({ state: 'available', ticked: true });
    expect(row.why).toContain('Balanced');
    expect(pc.view().anyActive).toBe(false);
    expect(pc.report()).toContain('removed Kestrel\'s plan and left the plan in use alone');
  });

  it('is not removed while it is the plan in use, and cannot be switched to when it is', () => {
    const { pc } = owner();
    pc.apply(['power-plan']);
    expect(pc.removePlan()).toMatchObject({ ok: false });
    expect(pc.usePlanAgain()).toMatchObject({ ok: false });
    expect(states(pc)['power-plan']).toBe('applied');
  });

  it('is undone by Revert everything like Remove it', () => {
    const { pc } = owner();
    pc.apply(['power-plan']);
    pc.pickPlan('Balanced');
    expect(pc.revertAll()).toEqual([{ id: 'power-plan', title: 'Kestrel power plan', ok: true, message: 'Undone.' }]);
    expect(states(pc)['power-plan']).toBe('available');
  });

  it('offers a slow built-in plan ticked, in words that promise nothing', () => {
    const { pc } = owner();
    pc.pickPlan('Balanced');
    expect(pc.view().rows[0]).toMatchObject({
      state: 'available', ticked: true,
      why: 'Balanced lets the CPU slow down between frames. The difference may be small or none: measure it on your PC.',
    });
    pc.pickPlan('Power saver');
    expect(pc.view().rows[0].why).toBe('Power saver lets the CPU slow down between frames. The difference may be small or none: measure it on your PC.');
  });

  it('offers a fast plan unticked, and any other plan with a nudge to measure', () => {
    const { pc } = owner();
    pc.pickPlan('High performance');
    expect(pc.view().rows[0]).toMatchObject({ ticked: false, why: 'You\'re already on a fast plan. Kestrel\'s also turns off USB and PCIe power saving.' });
    pc.pickPlan('My own plan');
    expect(pc.view().rows[0]).toMatchObject({ ticked: false, why: 'You\'re on My own plan. Measure before and after to see which is better.' });
  });
});

describe('the restore point', () => {
  it('is created with the next number, or skipped, and the report says so', () => {
    const { pc, minutesLater } = owner();
    expect(pc.restorePoint('create')).toEqual({ result: 'created', sequence: 57 });
    minutesLater(1);
    expect(pc.restorePoint('skip')).toEqual({ result: 'skipped' });
    expect(pc.restorePoint('create')).toEqual({ result: 'created', sequence: 58 });
    expect(pc.report().split('\n').slice(1)).toEqual([
      'Active',
      '  Nothing is active.',
      'History',
      '  2026-11-03 04:51 Restore point created (#58)',
      '  2026-11-03 04:51 Restore point skipped',
      '  2026-11-03 04:50 Restore point created (#57)',
    ]);
  });

  it('is asked about when System Protection is off, and turning it on is a change with a row of its own', () => {
    const { pc } = owner();
    pc.setRestore('protection-off');
    expect(pc.preview(['power-plan']).restore).toBe('protection-off');
    expect(pc.restorePoint('turn-on-and-create')).toEqual({ result: 'created', sequence: 57 });
    expect(pc.preview(['power-plan']).restore).toBe('ready');

    const view = pc.view();
    const row = view.rows.find((r) => r.id === 'system-protection');
    expect(row).toMatchObject({ state: 'applied', ticked: false, impact: 'none-in-game', timing: 'now' });
    // The warning about the restore points is the page's, beside Undo, so the row's own sentence doesn't say it too.
    expect(row?.why).toBe('System Protection is on because Kestrel turned it on.');
    expect(view.rows[view.rows.length - 1].id).toBe('system-protection');
    // It is never part of Revert everything: no button for it, and nothing for the button to do.
    expect(view.anyActive).toBe(false);
    expect(revertRows(view.rows)).toEqual([]);
    expect(pc.revertAll()).toEqual([{ id: 'system-protection', title: 'System Protection', ok: false, message: 'System Protection stays on: turning it off deletes this drive\'s restore points.' }]);

    expect(pc.undo('system-protection')).toMatchObject({ ok: true, message: 'Undone.' });
    expect(pc.view().rows.some((r) => r.id === 'system-protection')).toBe(false);
    expect(pc.preview(['power-plan']).restore).toBe('protection-off');
  });

  it('cannot be made under a policy, and the readiness says so', () => {
    const { pc } = owner();
    pc.setRestore('blocked-by-policy');
    expect(pc.preview(['power-plan']).restore).toBe('blocked-by-policy');
    expect(pc.restorePoint('create')).toMatchObject({ result: 'failed' });
    expect(pc.restorePoint('turn-on-and-create')).toMatchObject({ result: 'failed' });
    expect(pc.view().rows.some((r) => r.id === 'system-protection')).toBe(false);
    expect(pc.restorePoint('skip')).toEqual({ result: 'skipped' });
  });
});

describe('a journal with lines that couldn’t be read', () => {
  const CAUSE = 'An interruption while Kestrel was writing is the usual cause of an unreadable line.';
  const WARNING = 'Part of Kestrel\'s history couldn\'t be read.';

  it('is counted in the view, and the report ends with its own two warning lines, as kestrel-tweaks’ report does', () => {
    const { pc } = owner();
    pc.setUnreadable(2);
    expect(pc.view().unreadableLines).toBe(2);
    expect(pc.report()).toBe(`${HEADER}\nNo changes yet.\n${CAUSE}\n${WARNING}`);
  });

  it('puts them after the History of a report that has one', () => {
    const { pc } = owner();
    pc.apply(['power-plan']);
    pc.setUnreadable(1);
    const lines = pc.report().split('\n');
    expect(lines.slice(-2)).toEqual([CAUSE, WARNING]);
    expect(lines[lines.length - 3]).toContain('Applied Kestrel power plan');
    expect(lines.filter((line) => line === WARNING)).toHaveLength(1);
  });

  it('says nothing more once there are none', () => {
    const { pc } = owner();
    pc.setUnreadable(3);
    pc.setUnreadable(0);
    expect(pc.view().unreadableLines).toBe(0);
    expect(pc.report()).toBe(`${HEADER}\nNo changes yet.`);
  });
});

describe('the singleton the browser preview uses', () => {
  it('starts as the owner’s PC too', () => {
    const states = sampleTweaks.view().rows.map((row) => row.state);
    expect(states).toEqual(['available', 'already-set', 'already-set', 'available', 'not-needed', 'available']);
    const ids: TweakId[] = sampleTweaks.view().rows.map((row) => row.id);
    expect(ids[0]).toBe('power-plan');
  });
});
