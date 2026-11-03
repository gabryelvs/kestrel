import { describe, expect, it } from 'vitest';
import {
  boundRow, canFocus, closedReason, currentFlow, errorText, flowLocks, forgetChoices, gateReason, hasTickBox, isTicked, measureNote, measureText,
  needsRestart, onlyTicked, restoreFailure, restoreRoute, revertRows, rereadsOnFocus, rowNote, tickedIds, type Flow, type ViewLoad,
} from './tweaks';
import type { BenchmarkSummary, ItemOutcome, PreviewItem, PreviewView, Row, RowState, TweakId, TweaksView } from './types';

/** A saved benchmark whose runs vary by `spread` (a ratio, 0.249 is 24.9%) in average FPS. */
const summary = (over: { id?: string; isBaseline?: boolean; spread?: number; low1Spread?: number } = {}): BenchmarkSummary => ({
  id: over.id ?? 'sample-1', name: 'Starting point', createdAt: '2026-10-12T21:30:05Z', profileId: 'valorant', routeVersion: 1,
  result: {
    metrics: { frames: 36900, durationS: 90, avgFps: 410, low1Fps: 288, low01Fps: 201, stuttersPerMin: 0.6, pcLatencyMs: 9.8, gpuBusyRatio: 0.93, bottleneck: 'gpu' },
    spread: { avgFps: over.spread ?? 0.03, low1Fps: over.low1Spread ?? 0.03, low01Fps: 0.03 },
  },
  isBaseline: over.isBaseline ?? false, flaggedRunsKept: 0,
});

const row = (id: TweakId, state: RowState, ticked = false): Row => ({
  id, title: id, state, ticked, why: 'Because.', impact: 'medium', timing: 'now',
});

const view = (rows: Row[], gate: TweaksView['gate'] = 'open'): TweaksView => ({
  rows, gate, otherAccount: false, pendingOtherAccounts: 0, interrupted: [], unreadableLines: 0, anyActive: false,
});

const outcome = (id: TweakId, ok: boolean, message = 'Applied.'): ItemOutcome => ({ id, title: id, ok, message });

const item = (id: TweakId, timing: PreviewItem['timing']): PreviewItem => ({ id, title: id, location: 'Somewhere', changes: [], timing, note: null });

describe('measureNote', () => {
  it('asks for a benchmark when there is no baseline', () => expect(measureNote([])).toEqual({ kind: 'none' }));

  it('asks for one too when results exist and none is the baseline', () => {
    expect(measureNote([summary({ spread: 0.5 })])).toEqual({ kind: 'none' });
  });

  it('flags a baseline whose runs vary over 10%', () => {
    expect(measureNote([summary({ isBaseline: true, spread: 0.249 })])).toEqual({ kind: 'spread', pct: '24.9%' });
  });

  it('stays quiet for a clean baseline', () => expect(measureNote([summary({ isBaseline: true, spread: 0.05 })])).toBeNull());

  it('flags strictly above 10%, as the benchmark tab does', () => {
    expect(measureNote([summary({ isBaseline: true, spread: 0.1 })])).toBeNull();
    expect(measureNote([summary({ isBaseline: true, spread: 0.1001 })])).toEqual({ kind: 'spread', pct: '10.0%' });
  });

  it('looks at the baseline only, and at its average FPS', () => {
    const noisyOther = summary({ id: 'sample-2', spread: 0.4 });
    const baseline = summary({ id: 'sample-1', isBaseline: true, spread: 0.02, low1Spread: 0.4 });
    expect(measureNote([noisyOther, baseline])).toBeNull();
  });

  it('says it in the words of the spec', () => {
    expect(measureText({ kind: 'none' })).toBe('Run a benchmark first, so you can compare afterwards.');
    expect(measureText({ kind: 'spread', pct: '24.9%' })).toBe(
      'Your starting point’s runs vary by 24.9%, so small changes won’t show. Run a clean one before applying.',
    );
  });
});

describe('the tick boxes', () => {
  const rows = [
    row('power-plan', 'available', false),
    row('game-mode', 'already-set', false),
    row('background-recording', 'not-needed', false),
    row('windowed-optimizations', 'available', true),
    row('gpu-preference', 'blocked-by-policy', false),
    row('pointer-precision', 'applied', false),
    row('system-protection', 'couldnt-read', false),
  ];

  it('gives a box to a row that can be applied, and to no other', () => {
    expect(rows.filter(hasTickBox).map((r) => r.id)).toEqual(['power-plan', 'windowed-optimizations']);
    for (const state of ['already-set', 'not-needed', 'blocked-by-policy', 'applied', 'changed-elsewhere', 'plan-inactive', 'couldnt-read'] as const) {
      expect(hasTickBox(row('game-mode', state)), state).toBe(false);
    }
  });

  it('starts from the rows’ own flags', () => {
    expect(tickedIds(rows, {})).toEqual(['windowed-optimizations']);
    expect(isTicked(rows[0], {})).toBe(false);
    expect(isTicked(rows[3], {})).toBe(true);
  });

  it('keeps what the person ticked or unticked by hand over the flags', () => {
    expect(tickedIds(rows, { 'power-plan': true, 'windowed-optimizations': false })).toEqual(['power-plan']);
    expect(tickedIds(rows, { 'power-plan': true })).toEqual(['power-plan', 'windowed-optimizations']);
  });

  it('never ticks a row without a box, whatever was chosen for it before', () => {
    expect(tickedIds(rows, { 'game-mode': true, 'pointer-precision': true, 'system-protection': true })).toEqual(['windowed-optimizations']);
    expect(isTicked(row('game-mode', 'already-set', true), { 'game-mode': true })).toBe(false);
  });

  it('lists the ticked rows in the rows’ order', () => {
    expect(tickedIds([...rows].reverse(), { 'power-plan': true })).toEqual(['windowed-optimizations', 'power-plan']);
  });

  it('forgets the choices for the rows that were acted on, and only those', () => {
    const choices = { 'power-plan': true, 'windowed-optimizations': false };
    expect(forgetChoices(choices, ['power-plan'])).toEqual({ 'windowed-optimizations': false });
    expect(forgetChoices(choices, [])).toEqual(choices);
    expect(choices).toEqual({ 'power-plan': true, 'windowed-optimizations': false });
  });
});

describe('a Fix button’s tweak', () => {
  const rows = [row('power-plan', 'available', false), row('game-mode', 'already-set'), row('windowed-optimizations', 'available', true), row('pointer-precision', 'applied')];

  it('is ticked alone: every other box is cleared', () => {
    const choices = onlyTicked(rows, 'power-plan');
    expect(tickedIds(rows, choices)).toEqual(['power-plan']);
    expect(tickedIds(rows, onlyTicked(rows, 'windowed-optimizations'))).toEqual(['windowed-optimizations']);
  });

  it('opens its Preview only for a row that can be applied, with the gate open', () => {
    expect(canFocus(view(rows), 'power-plan')).toBe(true);
    expect(canFocus(view(rows), 'windowed-optimizations')).toBe(true);
    // Already set, Applied, Not needed, Blocked, changed since the scan, and a tweak with no row at all: just show the tab.
    expect(canFocus(view(rows), 'game-mode')).toBe(false);
    expect(canFocus(view(rows), 'pointer-precision')).toBe(false);
    expect(canFocus(view(rows), 'gpu-preference')).toBe(false);
    for (const state of ['not-needed', 'blocked-by-policy', 'changed-elsewhere', 'plan-inactive', 'couldnt-read'] as const) {
      expect(canFocus(view([row('power-plan', state)]), 'power-plan'), state).toBe(false);
    }
  });

  it('opens no Preview while the gate is closed: Apply could not run', () => {
    for (const gate of ['game-running', 'benchmark-active', 'cant-tell'] as const) {
      expect(canFocus(view(rows, gate), 'power-plan'), gate).toBe(false);
    }
  });
});

describe('Revert everything', () => {
  it('lists Kestrel’s changes that hold and its plan while another is in use', () => {
    const rows = [
      row('power-plan', 'plan-inactive'), row('game-mode', 'applied'), row('background-recording', 'already-set'),
      row('windowed-optimizations', 'available', true), row('gpu-preference', 'not-needed'), row('pointer-precision', 'applied'),
    ];
    expect(revertRows(rows).map((r) => r.id)).toEqual(['power-plan', 'game-mode', 'pointer-precision']);
  });

  it('leaves out System Protection, which stays on, and rows something else changed', () => {
    const rows = [row('game-mode', 'applied'), row('pointer-precision', 'changed-elsewhere'), row('system-protection', 'applied'), row('power-plan', 'couldnt-read')];
    expect(revertRows(rows).map((r) => r.id)).toEqual(['game-mode']);
  });

  it('lists nothing when nothing is active', () => {
    expect(revertRows([row('game-mode', 'already-set'), row('power-plan', 'available')])).toEqual([]);
  });
});

describe('the restore point step', () => {
  it('goes ahead and asks Windows when it can, or when it can’t tell', () => {
    expect(restoreRoute('ready')).toBe('create');
    expect(restoreRoute('unknown')).toBe('create');
  });

  it('asks first when System Protection is off, and offers no turning on under a policy', () => {
    expect(restoreRoute('protection-off')).toBe('turn-on');
    expect(restoreRoute('blocked-by-policy')).toBe('blocked');
  });

  it('goes on with the changes after every outcome but a failed one', () => {
    expect(restoreFailure({ result: 'created', sequence: 57 })).toBeNull();
    expect(restoreFailure({ result: 'reused', sequence: 56, time: '2026-11-02T09:00:00+00:00' })).toBeNull();
    expect(restoreFailure({ result: 'skipped' })).toBeNull();
  });

  it('stops at a failed one, with its reason, even an empty one', () => {
    expect(restoreFailure({ result: 'failed', reason: 'System Protection is off.' })).toBe('System Protection is off.');
    expect(restoreFailure({ result: 'failed', reason: '' })).toBe('');
  });
});

describe('Restart VALORANT', () => {
  const items = [item('power-plan', 'now'), item('windowed-optimizations', 'next-launch'), item('pointer-precision', 'next-sign-in')];

  it('is needed when a change that took waits for the next launch, by the timing the Preview showed', () => {
    expect(needsRestart([outcome('power-plan', true), outcome('windowed-optimizations', true)], items)).toBe(true);
  });

  it('is not needed for changes that took effect now or at the next sign-in', () => {
    expect(needsRestart([outcome('power-plan', true), outcome('pointer-precision', true)], items)).toBe(false);
  });

  it('is not needed for a change that failed: nothing waits for the launch', () => {
    expect(needsRestart([outcome('windowed-optimizations', false, 'Windows refused the change.')], items)).toBe(false);
  });

  it('takes the timing from the Preview only, and an item it never showed has none', () => {
    expect(needsRestart([outcome('gpu-preference', true)], items)).toBe(false);
    expect(needsRestart([], items)).toBe(false);
  });
});

describe('the gate', () => {
  it('says nothing while it is open, and why otherwise', () => {
    expect(gateReason('open')).toBeNull();
    expect(gateReason('game-running')).toBe('Close VALORANT first.');
    expect(gateReason('benchmark-active')).toBe('Finish the benchmark first.');
    expect(gateReason('cant-tell')).toBe('Kestrel couldn’t check whether VALORANT is open. Try again.');
  });
});

describe('what the page is doing', () => {
  const preview: PreviewView = { items: [], restore: 'ready' };
  const idle: Flow = { step: 'idle' };
  const locking: Flow[] = [
    { step: 'busy', text: 'Applying…' },
    { step: 'busy', text: 'Working…', id: 'game-mode' },
    { step: 'preview', preview },
    { step: 'restore', preview, question: { kind: 'protection-off' } },
    { step: 'ask-restore-anyway', id: 'game-mode' },
    { step: 'ask-revert' },
  ];
  const resting: Flow[] = [
    idle,
    { step: 'applied', outcomes: [], restart: false },
    { step: 'results', outcomes: [] },
    { step: 'row-result', id: 'game-mode', outcome: outcome('game-mode', true, 'Undone.') },
    { step: 'failed', message: 'Close VALORANT first.' },
  ];

  it('locks the rows and buttons while a step is under way or waits for an answer', () => {
    for (const flow of locking) expect(flowLocks(flow), flow.step).toBe(true);
  });

  it('leaves them free when it is idle or only showing a result', () => {
    for (const flow of resting) expect(flowLocks(flow), flow.step).toBe(false);
  });

  it('tells a row what happened to it, where the person acted', () => {
    expect(rowNote({ step: 'busy', text: 'Working…', id: 'game-mode' }, 'game-mode')).toEqual({ ok: true, text: 'Working…' });
    expect(rowNote({ step: 'row-result', id: 'game-mode', outcome: outcome('game-mode', true, 'Undone.') }, 'game-mode')).toEqual({ ok: true, text: 'Undone.' });
    const refused = outcome('game-mode', false, 'Something else changed this after Kestrel did. Kestrel left it alone.');
    expect(rowNote({ step: 'row-result', id: 'game-mode', outcome: refused }, 'game-mode')).toEqual({ ok: false, text: refused.message });
    expect(rowNote({ step: 'failed', id: 'game-mode', message: 'Close VALORANT first.' }, 'game-mode')).toEqual({ ok: false, text: 'Close VALORANT first.' });
  });

  it('tells no other row, and nothing for a step that is not about one row', () => {
    expect(rowNote({ step: 'busy', text: 'Working…', id: 'game-mode' }, 'power-plan')).toBeNull();
    expect(rowNote({ step: 'failed', id: 'game-mode', message: 'x' }, 'power-plan')).toBeNull();
    for (const flow of [...locking.filter((f) => f.step !== 'busy' || f.id === undefined), ...resting.filter((f) => f.step !== 'row-result' && f.step !== 'failed')]) {
      expect(rowNote(flow, 'game-mode'), flow.step).toBeNull();
    }
    expect(rowNote({ step: 'failed', message: 'Close VALORANT first.' }, 'game-mode')).toBeNull();
  });

  it('names the row a step is about, if it is about one', () => {
    expect(boundRow({ step: 'busy', text: 'Working…', id: 'power-plan' })).toBe('power-plan');
    expect(boundRow({ step: 'row-result', id: 'power-plan', outcome: outcome('power-plan', true) })).toBe('power-plan');
    expect(boundRow({ step: 'failed', id: 'power-plan', message: 'x' })).toBe('power-plan');
    expect(boundRow({ step: 'ask-restore-anyway', id: 'power-plan' })).toBe('power-plan');
    expect(boundRow({ step: 'busy', text: 'Applying…' })).toBeNull();
    expect(boundRow({ step: 'failed', message: 'x' })).toBeNull();
    expect(boundRow(idle)).toBeNull();
    expect(boundRow({ step: 'ask-revert' })).toBeNull();
  });
});

describe('the text an error gives', () => {
  it('is the text a command refused with', () => expect(errorText('Close VALORANT first.')).toBe('Close VALORANT first.'));
  it('is the message of an Error', () => expect(errorText(new Error('The scan failed.'))).toBe('The scan failed.'));
  it('is something readable for anything else', () => {
    expect(errorText(42)).toBe('42');
    expect(errorText(undefined)).toBe('undefined');
    expect(errorText({ toString: () => 'odd' })).toBe('odd');
  });
});

describe('why nothing that changes the PC may be pressed', () => {
  const STALE = 'Kestrel couldn’t read the tweaks again, so these rows may be out of date.';

  it('is the gate’s reason while it is closed, and nothing while it is open', () => {
    expect(closedReason(view([]))).toBeNull();
    expect(closedReason(view([], 'game-running'))).toBe('Close VALORANT first.');
    expect(closedReason(view([], 'benchmark-active'))).toBe('Finish the benchmark first.');
    expect(closedReason(view([], 'cant-tell'))).toBe('Kestrel couldn’t check whether VALORANT is open. Try again.');
  });

  it('is that the view couldn’t be read again, with the reason, whatever the gate said when it was last read', () => {
    expect(closedReason(view([]), 'Access is denied (os error 5)')).toBe(`${STALE} Access is denied (os error 5)`);
    expect(closedReason(view([], 'game-running'), 'The journal is held.')).toBe(`${STALE} The journal is held.`);
    // An error with no words is still an error.
    expect(closedReason(view([]), '')).toBe(`${STALE} `.trim());
  });
});

describe('coming back to the window', () => {
  const ready = (gate: TweaksView['gate'], stale?: string): ViewLoad => ({ state: 'ready', view: view([], gate), ...(stale === undefined ? {} : { stale }) });
  const preview: PreviewView = { items: [], restore: 'ready' };
  const waiting: Flow[] = [
    { step: 'idle' },
    { step: 'preview', preview },
    { step: 'restore', preview, question: { kind: 'blocked-by-policy' } },
    { step: 'ask-restore-anyway', id: 'game-mode' },
    { step: 'ask-revert' },
    { step: 'applied', outcomes: [], restart: false },
    { step: 'results', outcomes: [] },
    { step: 'row-result', id: 'game-mode', outcome: outcome('game-mode', true, 'Undone.') },
    { step: 'failed', message: 'Close VALORANT first.' },
  ];
  const running: Flow[] = [{ step: 'busy', text: 'Applying…' }, { step: 'busy', text: 'Working…', id: 'game-mode' }];

  it('reads the page again while a gate is closed, which is the one thing the person can clear from outside', () => {
    for (const gate of ['game-running', 'benchmark-active', 'cant-tell'] as const) {
      for (const flow of waiting) expect(rereadsOnFocus(ready(gate), flow), `${gate} ${flow.step}`).toBe(true);
    }
  });

  it('leaves an open gate alone: there is nothing to clear', () => {
    for (const flow of [...waiting, ...running]) expect(rereadsOnFocus(ready('open'), flow), flow.step).toBe(false);
  });

  it('never reads it again while a command is under way, whatever the gate says', () => {
    for (const gate of ['open', 'game-running', 'benchmark-active', 'cant-tell'] as const) {
      for (const flow of running) expect(rereadsOnFocus(ready(gate), flow), gate).toBe(false);
    }
    expect(rereadsOnFocus({ state: 'failed', message: 'x' }, running[0])).toBe(false);
    expect(rereadsOnFocus(ready('open', 'x'), running[1])).toBe(false);
  });

  it('also reads it again when the view couldn’t be read, or couldn’t be read again, since that is cleared from outside too', () => {
    expect(rereadsOnFocus({ state: 'failed', message: 'x' }, { step: 'idle' })).toBe(true);
    expect(rereadsOnFocus(ready('open', 'x'), { step: 'idle' })).toBe(true);
  });

  it('does not start a read while the first is still under way', () => {
    expect(rereadsOnFocus({ state: 'loading' }, { step: 'idle' })).toBe(false);
  });
});

describe('a question that no longer applies', () => {
  const asking: Flow = { step: 'ask-restore-anyway', id: 'pointer-precision' };

  it('stays while its row is still one something else changed', () => {
    expect(currentFlow(asking, [row('pointer-precision', 'changed-elsewhere')])).toBe(asking);
  });

  it('is over once the row went back, was undone or applied again by a re-read, or is gone, so it cannot lock the page', () => {
    for (const state of ['available', 'applied', 'already-set', 'not-needed', 'blocked-by-policy', 'plan-inactive', 'couldnt-read'] as const) {
      expect(currentFlow(asking, [row('pointer-precision', state)]), state).toEqual({ step: 'idle' });
    }
    expect(currentFlow(asking, [row('game-mode', 'changed-elsewhere')])).toEqual({ step: 'idle' });
    expect(currentFlow(asking, [])).toEqual({ step: 'idle' });
  });

  it('leaves every other step as it is', () => {
    const others: Flow[] = [
      { step: 'idle' }, { step: 'busy', text: 'Working…', id: 'pointer-precision' }, { step: 'ask-revert' },
      { step: 'row-result', id: 'pointer-precision', outcome: outcome('pointer-precision', true) }, { step: 'failed', message: 'x', id: 'pointer-precision' },
    ];
    for (const flow of others) expect(currentFlow(flow, []), flow.step).toBe(flow);
  });
});
