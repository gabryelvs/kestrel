import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { createSampleTweaks } from '../api/sample-tweaks';
import type { Flow, ReportLoad } from '../api/tweaks';
import type { ItemOutcome, PreviewView, Row, RowState, TweakId, TweaksView } from '../api/types';
import { Outcomes } from '../components/tweaks/Outcomes';
import { PreviewPanel } from '../components/tweaks/PreviewPanel';
import { ReportPanel } from '../components/tweaks/ReportPanel';
import { RestoreStep } from '../components/tweaks/RestoreStep';
import { RevertConfirm } from '../components/tweaks/RevertConfirm';
import { TweakRow, type RowActions } from '../components/tweaks/TweakRow';
import { strings } from '../strings';
import { TweaksBody, TweaksPage, type Handlers } from './Tweaks';

const t = strings.tweaks;
const nothing = () => undefined;
const count = (html: string, text: string) => html.split(text).length - 1;
const plain = (markup: string) => markup.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();

type Button = { text: string; disabled: boolean };
/** Every button in the markup, with its words and whether it is off. */
const buttons = (html: string): Button[] =>
  [...html.matchAll(/<button([^>]*)>([\s\S]*?)<\/button>/g)].map(([, attrs, inner]) => ({ text: plain(inner), disabled: /\sdisabled=""/.test(attrs) }));
const texts = (html: string) => buttons(html).map((b) => b.text);
const button = (html: string, text: string) => buttons(html).find((b) => b.text === text);
const boxes = (html: string) => [...html.matchAll(/<input[^>]*type="checkbox"[^>]*>/g)].map((m) => m[0]);

const row = (id: TweakId, state: RowState, over: Partial<Row> = {}): Row => ({
  id, title: t.names[id], state, ticked: false, why: `Why ${id}.`, impact: 'medium', timing: 'now', ...over,
});

const rowActions: RowActions = {
  tick: nothing, undo: nothing, applyAgain: nothing, askRestoreAnyway: nothing, restoreAnyway: nothing, stopAsking: nothing, usePlanAgain: nothing, removePlan: nothing,
};
const on: Handlers = {
  ...rowActions, preview: nothing, apply: nothing, turnOn: nothing, goOn: nothing, cancel: nothing, askRevert: nothing, revert: nothing, benchmark: nothing,
  retry: nothing,
};

const renderRow = (r: Row, over: Partial<Parameters<typeof TweakRow>[0]> = {}) =>
  renderToStaticMarkup(<TweakRow row={r} ticked={false} locked={false} closed={false} asking={false} note={null} on={rowActions} {...over} />);

describe('TweakRow', () => {
  it('shows an Undo on an applied row and no tick box', () => {
    const html = renderRow(row('game-mode', 'applied'));
    expect(texts(html)).toEqual([t.undo]);
    expect(boxes(html)).toEqual([]);
    expect(html).toContain(t.states.applied);
  });

  it('shows Apply again and Restore anyway on a changed row, and neither Undo nor a tick box', () => {
    const html = renderRow(row('pointer-precision', 'changed-elsewhere'));
    expect(texts(html)).toEqual([t.applyAgain, t.restoreAnyway]);
    expect(boxes(html)).toEqual([]);
    expect(html).toContain('Changed by something else');
  });

  it('shows Use it again and Remove it on an inactive plan', () => {
    const html = renderRow(row('power-plan', 'plan-inactive', { why: 'Another plan is active: Balanced.' }));
    expect(texts(html)).toEqual([t.useAgain, t.removePlan]);
    expect(boxes(html)).toEqual([]);
    expect(html).toContain('Kestrel’s plan isn’t active');
    expect(html).toContain('Another plan is active: Balanced.');
  });

  it('never shows a tick box, or any button, on Already set, Not needed, Blocked by policy or Couldn’t read', () => {
    for (const state of ['already-set', 'not-needed', 'blocked-by-policy', 'couldnt-read'] as const) {
      // Even a row whose flag says ticked, and a choice made for it before, gets no box.
      const html = renderRow(row('gpu-preference', state, { ticked: true }), { ticked: true });
      expect(boxes(html), state).toEqual([]);
      expect(buttons(html), state).toEqual([]);
      expect(html, state).toContain(t.states[state]);
    }
  });

  it('shows a tick box on an available row, named by its title and ticked as told', () => {
    const off = renderRow(row('windowed-optimizations', 'available'));
    const on_ = renderRow(row('windowed-optimizations', 'available'), { ticked: true });
    expect(boxes(off)).toHaveLength(1);
    expect(boxes(off)[0]).not.toContain('checked');
    expect(boxes(on_)[0]).toContain('checked=""');
    // A real label, so the whole title ticks the box.
    expect(off).toMatch(/<label for="tick-windowed-optimizations"[^>]*>Optimizations for windowed games<\/label>/);
    expect(off).toContain('id="tick-windowed-optimizations"');
    expect(buttons(off)).toEqual([]);
  });

  it('tags the row with its state and its impact, and says why in the sentence it was given', () => {
    const html = renderRow(row('pointer-precision', 'available', { impact: 'none-in-game', why: 'Only the cursor on the desktop changes.' }));
    expect(html).toContain(t.states.available);
    expect(html).toContain('Impact: None in game');
    expect(html).toContain('Only the cursor on the desktop changes.');
    expect(plain(renderRow(row('game-mode', 'applied', { impact: 'medium' })))).toContain('Impact: Medium');
  });

  it('puts the System Protection warning beside its Undo, and on no other row', () => {
    const protection = renderRow(row('system-protection', 'applied'));
    const actions = protection.match(/<div class="tweak-actions">([\s\S]*?)<\/div>/)?.[1] ?? '';
    expect(plain(actions)).toContain(t.protectionWarning);
    expect(texts(actions)).toEqual([t.undo]);
    expect(renderRow(row('game-mode', 'applied'))).not.toContain(t.protectionWarning);
    expect(renderRow(row('system-protection', 'applied'))).toContain(t.protectionWarning);
  });

  it('keeps a long plan name whole and lets it wrap', () => {
    const name = `Plan-${'x'.repeat(90)}`;
    const html = renderRow(row('power-plan', 'available', { why: `You're on ${name}. Measure before and after to see which is better.` }));
    expect(html).toContain(name);
    expect(html).toMatch(/<p class="[^"]*\bwrap\b[^"]*">You&#x27;re on Plan-x+\./);
  });

  it('turns every button off while a step is under way or a closed gate refuses, and the box only for the step', () => {
    const applied = row('game-mode', 'applied');
    expect(buttons(renderRow(applied, { locked: true })).every((b) => b.disabled)).toBe(true);
    expect(buttons(renderRow(applied, { closed: true })).every((b) => b.disabled)).toBe(true);
    expect(button(renderRow(applied), t.undo)?.disabled).toBe(false);
    expect(buttons(renderRow(row('pointer-precision', 'changed-elsewhere'), { closed: true })).map((b) => b.disabled)).toEqual([true, true]);
    expect(buttons(renderRow(row('power-plan', 'plan-inactive'), { locked: true })).map((b) => b.disabled)).toEqual([true, true]);
    // Ticking changes nothing on the PC, so a closed gate leaves the box alone; a step under way does not.
    const available = row('windowed-optimizations', 'available');
    expect(boxes(renderRow(available, { closed: true }))[0]).not.toContain('disabled');
    expect(boxes(renderRow(available, { locked: true }))[0]).toContain('disabled=""');
  });

  it('asks first before Restore anyway: the question and its answers take the place of the two buttons', () => {
    const html = renderRow(row('pointer-precision', 'changed-elsewhere'), { asking: true });
    expect(html).toContain(t.restoreAnywayAsk);
    expect(texts(html)).toEqual([t.yesRestore, t.cancel]);
    expect(button(html, t.yesRestore)?.disabled).toBe(false);
    // The question can be answered while the rest of the page waits on it.
    expect(buttons(renderRow(row('pointer-precision', 'changed-elsewhere'), { asking: true, locked: true })).map((b) => b.disabled)).toEqual([false, false]);
    expect(renderRow(row('pointer-precision', 'changed-elsewhere'))).not.toContain(t.restoreAnywayAsk);
  });

  it('asks nothing about a row that a re-read shows is no longer one something else changed, and shows its own buttons', () => {
    // Restore anyway was pressed, and the view was read again before the answer: Yes and Cancel would act on the wrong thing.
    for (const state of ['available', 'applied', 'already-set', 'plan-inactive', 'couldnt-read'] as const) {
      const html = renderRow(row('pointer-precision', state), { asking: true });
      expect(html, state).not.toContain(t.restoreAnywayAsk);
      expect(texts(html), state).not.toContain(t.yesRestore);
    }
    expect(texts(renderRow(row('pointer-precision', 'applied'), { asking: true }))).toEqual([t.undo]);
    expect(texts(renderRow(row('pointer-precision', 'plan-inactive'), { asking: true }))).toEqual([t.useAgain, t.removePlan]);
    expect(texts(renderRow(row('pointer-precision', 'changed-elsewhere'), { asking: true }))).toEqual([t.yesRestore, t.cancel]);
  });

  it('says what happened to the row where the person acted: a result plainly, a refusal in the warning colour', () => {
    const done = renderRow(row('game-mode', 'available'), { note: { ok: true, text: 'Undone.' } });
    expect(done).toMatch(/<p class="tweak-note[^"]*">Undone\.<\/p>/);
    expect(done).not.toContain('tone-warn');
    const refused = renderRow(row('game-mode', 'applied'), { note: { ok: false, text: 'Close VALORANT first.' } });
    expect(refused).toMatch(/<p class="tweak-note[^"]*tone-warn[^"]*">Close VALORANT first\.<\/p>/);
    expect(renderRow(row('game-mode', 'applied'))).not.toContain('tweak-note');
  });

  it('shows what Rust sent as text, never as markup', () => {
    const html = renderRow(row('power-plan', 'available', { why: "You're on <b>Plan</b>." }));
    expect(html).not.toContain('<b>');
    expect(html).toContain('&lt;b&gt;Plan&lt;/b&gt;');
  });
});

const planItem = {
  id: 'power-plan' as const, title: 'Kestrel power plan', location: 'Power plan', timing: 'now' as const,
  changes: [
    { label: 'Core parking minimum cores', now: '5%', after: '100%' },
    { label: 'USB selective suspend', now: 'On', after: 'Off' },
    { label: 'PCIe link-state power management', now: 'Maximum', after: 'Off' },
  ],
  note: 'Switching plans also replaces everything else in EXM Free Power Plan V8 with the settings of High performance. Undo returns to your plan as it was.',
};
const windowedItem = {
  id: 'windowed-optimizations' as const, title: 'Optimizations for windowed games', timing: 'next-launch' as const, note: null,
  location: String.raw`HKCU\Software\Microsoft\DirectX\UserGpuPreferences, DirectXUserGlobalSettings`,
  changes: [{ label: 'Optimizations for windowed games', now: 'not set', after: 'on' }],
};
const pointerItem = {
  id: 'pointer-precision' as const, title: 'Enhance pointer precision off', timing: 'next-sign-in' as const, note: null,
  location: String.raw`HKCU\Control Panel\Mouse, MouseSpeed, MouseThreshold1, MouseThreshold2`,
  changes: [{ label: 'Enhance pointer precision off', now: 'on', after: 'off' }],
};

describe('PreviewPanel', () => {
  const preview: PreviewView = { items: [planItem, windowedItem, pointerItem], restore: 'ready' };
  const html = renderToStaticMarkup(<PreviewPanel preview={preview} onApply={nothing} onCancel={nothing} />);

  it('lists each change: what it is, where, the value now and after, and when it takes effect', () => {
    for (const item of preview.items) {
      expect(html).toContain(item.title);
      expect(html).toContain(item.location.replace(/&/g, '&amp;'));
      for (const change of item.changes) {
        expect(html).toContain(`<td>${change.label}</td>`);
        expect(html).toContain(`>${change.now}</td>`);
        expect(html).toContain(`>${change.after}</td>`);
      }
    }
    expect(plain(html)).toContain(`${t.takesEffect} ${t.timing.now}`);
    expect(plain(html)).toContain(`${t.takesEffect} Next time VALORANT opens`);
    expect(plain(html)).toContain(`${t.takesEffect} Next sign-in`);
  });

  it('shows the plan’s three settings with Kestrel’s beside the current plan’s, and the sentence about everything else', () => {
    expect(html).toContain('<td>Core parking minimum cores</td>');
    expect(html).toContain(planItem.note);
    // The values sit in their own cells, in the order of the columns: setting, now, after.
    expect(html).toMatch(/<td>USB selective suspend<\/td><td[^>]*>On<\/td><td[^>]*>Off<\/td>/);
    expect(html).toContain(`<th scope="col">${t.columns.now}</th>`);
    expect(html).toContain(`<th scope="col">${t.columns.after}</th>`);
  });

  it('says nothing has changed yet, and offers Apply and Cancel', () => {
    expect(html).toContain(t.previewIntro);
    expect(buttons(html)).toEqual([{ text: t.apply, disabled: false }, { text: t.cancel, disabled: false }]);
    // No note, no note paragraph.
    expect(count(html, planItem.note)).toBe(1);
  });

  it('has nothing to apply when the Preview came back empty, and says so', () => {
    const empty = renderToStaticMarkup(<PreviewPanel preview={{ items: [], restore: 'ready' }} onApply={nothing} onCancel={nothing} />);
    expect(empty).toContain(t.previewEmpty);
    expect(empty).not.toContain(t.previewIntro);
    expect(buttons(empty)).toEqual([{ text: t.apply, disabled: true }, { text: t.cancel, disabled: false }]);
  });

  it('never promises a gain', () => {
    expect(plain(html)).not.toMatch(/improv|faster|boost|\bgains?\b/i);
  });
});

describe('RestoreStep', () => {
  const step = (question: Parameters<typeof RestoreStep>[0]['question']) =>
    renderToStaticMarkup(<RestoreStep question={question} on={{ turnOn: nothing, goOn: nothing, cancel: nothing }} />);

  it('asks about System Protection when it is off, with the three answers in order', () => {
    const html = step({ kind: 'protection-off' });
    expect(html).toContain(t.protectionOff);
    expect(texts(html)).toEqual(['Turn on and create', 'Continue without', 'Cancel']);
    expect(buttons(html).some((b) => b.disabled)).toBe(false);
  });

  it('under a policy offers only Continue without and Cancel, and names the policy', () => {
    const html = step({ kind: 'blocked-by-policy' });
    expect(html).toContain('An organisation policy');
    expect(html).toContain(t.restoreBlocked);
    expect(texts(html)).toEqual(['Continue without', 'Cancel']);
    expect(html).not.toContain('Turn on and create');
  });

  it('asks whether to go on when the restore point failed, with one full stop whichever way the reason ends', () => {
    const stop = step({ kind: 'failed', reason: 'System Protection is off.', turnOnAsked: false });
    expect(stop).toContain('Couldn’t create a restore point: System Protection is off. Continue with Kestrel’s undo only?');
    expect(texts(stop)).toEqual(['Continue', 'Cancel']);
    const none = step({ kind: 'failed', reason: 'Windows refused the request (os error 5)', turnOnAsked: true });
    expect(none).toContain('Couldn’t create a restore point: Windows refused the request (os error 5). Continue with Kestrel’s undo only?');
    expect(stop).not.toContain('..');
    expect(none).not.toContain('..');
  });
});

describe('Outcomes', () => {
  const done: ItemOutcome[] = [
    { id: 'power-plan', title: 'Kestrel power plan', ok: true, message: 'Applied.' },
    { id: 'windowed-optimizations', title: 'Optimizations for windowed games', ok: true, message: 'Applied.' },
    { id: 'pointer-precision', title: 'Enhance pointer precision off', ok: false, message: 'Windows refused the change: Access is denied.' },
  ];
  const render = (over: Partial<Parameters<typeof Outcomes>[0]> = {}) =>
    renderToStaticMarkup(<Outcomes outcomes={done} restart={false} measure onBenchmark={nothing} onClose={nothing} {...over} />);

  it('shows each outcome with its title and message, a failure as one', () => {
    const html = render();
    for (const item of done) {
      expect(html).toContain(item.title);
      expect(html).toContain(item.message.replace(/'/g, '&#x27;'));
    }
    expect(count(html, 'class="outcome ok"')).toBe(2);
    expect(count(html, 'class="outcome failed"')).toBe(1);
  });

  it('says to restart VALORANT only when a change waits for the next launch', () => {
    expect(render({ restart: true })).toContain(t.restartValorant);
    expect(render({ restart: false })).not.toContain(t.restartValorant);
  });

  it('suggests measuring, with a button to the Benchmark tab, in the words “Measure it on your PC”', () => {
    const html = render();
    expect(html).toContain(t.done);
    expect(texts(html)).toEqual([t.runBenchmark, t.close]);
    expect(t.done.startsWith('Measure it on your PC')).toBe(true);
  });

  it('suggests nothing to measure when nothing changed', () => {
    const none = render({ measure: false });
    expect(none).not.toContain(t.done);
    expect(texts(none)).toEqual([t.close]);
  });
});

describe('RevertConfirm', () => {
  it('asks once, lists the rows it will undo, and offers the answers', () => {
    const html = renderToStaticMarkup(
      <RevertConfirm rows={[row('power-plan', 'plan-inactive'), row('game-mode', 'applied')]} onRevert={nothing} onCancel={nothing} />,
    );
    expect(count(html, t.revertConfirm)).toBe(1);
    expect(html).toContain('<li>Kestrel power plan</li>');
    expect(html).toContain('<li>Game Mode</li>');
    expect(html.indexOf(t.revertConfirm)).toBeLessThan(html.indexOf('<li>Kestrel power plan</li>'));
    expect(texts(html)).toEqual([t.yesRevert, t.cancel]);
  });
});

describe('ReportPanel', () => {
  const text = [
    'Kestrel report · Kestrel 0.5.0 · Windows build 26200 · 2026-11-02 18:20',
    'Active',
    '  Nothing is active.',
    'History',
    '  2026-11-02 18:15 Undid Game Mode: back to off',
  ].join('\n');
  const render = (report: ReportLoad, unreadable = 0) => renderToStaticMarkup(<ReportPanel report={report} unreadable={unreadable} onRetry={nothing} />);

  it('shows the report text with Copy report', () => {
    const html = render({ state: 'ready', text });
    expect(html).toContain(t.report);
    expect(texts(html)).toEqual([t.copyReport]);
    // As it came: the indented placeholder line keeps its two spaces, and nothing is parsed.
    expect(html).toContain('<pre class="report num">');
    expect(html).toContain('\n  Nothing is active.\n');
    expect(html).toContain('Kestrel report · Kestrel 0.5.0');
  });

  it('shows what the report is made of as text', () => {
    expect(render({ state: 'ready', text: 'Applied <b>x</b>' })).toContain('Applied &lt;b&gt;x&lt;/b&gt;');
  });

  it('leaves the warning about unreadable history to the report itself, which ends with it', () => {
    // kestrel-tweaks' report appends its own two lines when part of the journal couldn't be read; saying it twice would be noise.
    const pc = createSampleTweaks();
    pc.setUnreadable(2);
    const html = render({ state: 'ready', text: pc.report() }, 2);
    expect(count(html, 'Part of Kestrel&#x27;s history couldn&#x27;t be read.')).toBe(1);
    expect(html).toContain('An interruption while Kestrel was writing is the usual cause of an unreadable line.');
    expect(html).not.toContain(t.unreadable);
    expect(render({ state: 'ready', text }, 2)).not.toContain(t.unreadable);
  });

  it('warns about unreadable history itself while there is no report text to say it, and only then', () => {
    expect(render({ state: 'loading' }, 2)).toContain(t.unreadable);
    expect(render({ state: 'failed', message: 'x' }, 2)).toContain(t.unreadable);
    expect(render({ state: 'loading' }, 0)).not.toContain(t.unreadable);
    expect(render({ state: 'failed', message: 'x' }, 0)).not.toContain(t.unreadable);
  });

  it('says it is reading, or why it couldn’t, with a way to try again and nothing to copy', () => {
    expect(render({ state: 'loading' })).toContain(t.reportLoading);
    const failed = render({ state: 'failed', message: 'C:\\ProgramData\\Kestrel belongs to another account.' });
    expect(failed).toContain(t.reportFailed);
    expect(failed).toContain('belongs to another account.');
    expect(texts(failed)).toEqual([t.tryAgain]);
    expect(texts(render({ state: 'loading' }))).toEqual([]);
  });
});

/** The owner's PC from the browser preview's fake, so the views have the shapes the commands give. */
const ownerView = (): TweaksView => createSampleTweaks().view();
const ready = (text = 'Kestrel report · test\nNo changes yet.'): ReportLoad => ({ state: 'ready', text });
const idle: Flow = { step: 'idle' };
const body = (over: Partial<Parameters<typeof TweaksBody>[0]> = {}) =>
  renderToStaticMarkup(<TweaksBody view={ownerView()} report={ready()} measure={null} choices={{}} flow={idle} on={on} {...over} />);

describe('the page’s body', () => {
  it('shows the rows in order, in a panel, with the later tiers below', () => {
    const html = body();
    const at = (text: string) => html.indexOf(text);
    const titles = ['Kestrel power plan', 'Game Mode', 'Background recording off', 'Optimizations for windowed games', 'GPU preference', 'Enhance pointer precision off'];
    const positions = titles.map((title) => at(`>${title}<`));
    expect(positions.every((p) => p > 0)).toBe(true);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
    expect(html).toContain(t.essential);
    expect(plain(html)).toContain('Competitive Arrives in stage 6');
    expect(plain(html)).toContain('Extreme Arrives in stage 6');
    expect(at('Arrives in stage 6')).toBeGreaterThan(positions[5]);
  });

  it('gives a tick box to the rows that can be applied and none to the others, on the owner’s PC', () => {
    const html = body();
    expect(boxes(html).map((box) => box.match(/id="tick-([a-z-]+)"/)?.[1])).toEqual(['power-plan', 'windowed-optimizations', 'pointer-precision']);
    expect(texts(html)).not.toContain(t.undo);
  });

  it('shows Preview with the number of ticked rows, from the rows’ own flags until the person chooses', () => {
    const view = ownerView();
    const ticked = { ...view, rows: view.rows.map((r) => (r.id === 'windowed-optimizations' ? { ...r, ticked: true } : r)) };
    expect(texts(body())).toContain('Preview 0 changes');
    expect(texts(body({ view: ticked }))).toContain('Preview 1 change');
    expect(button(body({ view: ticked }), 'Preview 1 change')?.disabled).toBe(false);
    // A choice by hand wins over the flag, in both directions.
    expect(texts(body({ view: ticked, choices: { 'power-plan': true, 'pointer-precision': true } }))).toContain('Preview 3 changes');
    expect(texts(body({ view: ticked, choices: { 'windowed-optimizations': false } }))).toContain('Preview 0 changes');
    expect(boxes(body({ view: ticked }))[1]).toContain('checked=""');
    expect(boxes(body({ view: ticked, choices: { 'power-plan': true } }))[0]).toContain('checked=""');
  });

  it('turns Preview off with nothing ticked, but says nothing about a reason', () => {
    const html = body({ choices: {} });
    expect(button(html, 'Preview 0 changes')?.disabled).toBe(true);
    for (const reason of Object.values(t.gate)) expect(html).not.toContain(reason);
  });

  it('shows the gate reason instead of an enabled Preview when VALORANT is open', () => {
    const view = { ...ownerView(), gate: 'game-running' as const };
    const html = body({ view, choices: { 'power-plan': true } });
    expect(button(html, 'Preview 1 change')?.disabled).toBe(true);
    expect(html).toContain('Close VALORANT first.');
    // The reason is text beside the button, not only a tooltip.
    expect(html).toMatch(/Preview 1 change<\/button><span[^>]*>Close VALORANT first\.<\/span>/);
    // And a way to read the page again once VALORANT is closed, beside the reason.
    expect(html).toMatch(/Close VALORANT first\.<\/span><button[^>]*>Try again<\/button>/);
    expect(button(html, t.tryAgain)?.disabled).toBe(false);
  });

  it('lets the reason beside the buttons wrap, since it can carry a command’s error as Rust wrote it', () => {
    const closed = body({ view: { ...ownerView(), gate: 'game-running' } });
    expect(closed).toMatch(/<span class="tweak-gate wrap">Close VALORANT first\.<\/span>/);
    const stale = renderToStaticMarkup(
      <TweaksPage load={{ state: 'ready', view: ownerView(), stale: `C:\\${'ProgramData\\'.repeat(12)}Kestrel belongs to another account.` }} report={ready()} measure={null} choices={{}} flow={idle} on={on} />,
    );
    expect(stale).toMatch(/<span class="tweak-gate wrap">Kestrel couldn’t read the tweaks again/);
  });

  it('offers Try again for every closed gate, and for none while it is open', () => {
    for (const gate of ['game-running', 'benchmark-active', 'cant-tell'] as const) {
      const html = body({ view: { ...ownerView(), gate } });
      expect(texts(html).filter((text) => text === t.tryAgain), gate).toHaveLength(1);
    }
    expect(button(body(), t.tryAgain)).toBeUndefined();
  });

  it('says to finish the benchmark when one is armed, and Preview is off too', () => {
    const html = body({ view: { ...ownerView(), gate: 'benchmark-active' }, choices: { 'power-plan': true } });
    expect(button(html, 'Preview 1 change')?.disabled).toBe(true);
    expect(html).toContain('Finish the benchmark first.');
    expect(button(html, t.tryAgain)?.disabled).toBe(false);
  });

  it('treats a gate that can’t tell like a closed one, with a small Try again that reads the view again', () => {
    const html = body({ view: { ...ownerView(), gate: 'cant-tell' }, choices: { 'power-plan': true } });
    expect(button(html, 'Preview 1 change')?.disabled).toBe(true);
    expect(html).toContain('Kestrel couldn’t check whether VALORANT is open. Try again.');
    expect(button(html, t.tryAgain)?.disabled).toBe(false);
  });

  it('turns the row buttons off under a closed gate, so Undo is refused here too', () => {
    const applied = createSampleTweaks();
    applied.apply(['power-plan']);
    for (const gate of ['game-running', 'benchmark-active', 'cant-tell'] as const) {
      const html = body({ view: { ...applied.view(), gate } });
      expect(button(html, t.undo)?.disabled, gate).toBe(true);
      expect(button(html, t.revertEverything)?.disabled, gate).toBe(true);
    }
    const open = body({ view: applied.view() });
    expect(button(open, t.undo)?.disabled).toBe(false);
    expect(button(open, t.revertEverything)?.disabled).toBe(false);
  });

  it('shows Revert everything only when something is active', () => {
    expect(texts(body())).not.toContain(t.revertEverything);
    const pc = createSampleTweaks();
    pc.apply(['pointer-precision']);
    expect(texts(body({ view: pc.view() }))).toContain(t.revertEverything);
  });

  it('puts each button on its own row: Undo on the applied one, Apply again and Restore anyway on the changed one', () => {
    const pc = createSampleTweaks();
    pc.apply(['power-plan', 'windowed-optimizations', 'pointer-precision']);
    pc.changeBehindItsBack('pointer-precision');
    const html = body({ view: pc.view() });
    const rowHtml = (id: TweakId) => html.match(new RegExp(`<li class="tweak-row[^"]*"[^>]*data-tweak="${id}">[\\s\\S]*?</li>`))?.[0] ?? '';
    expect(texts(rowHtml('power-plan'))).toEqual([t.undo]);
    expect(texts(rowHtml('windowed-optimizations'))).toEqual([t.undo]);
    expect(texts(rowHtml('pointer-precision'))).toEqual([t.applyAgain, t.restoreAnyway]);
    expect(texts(rowHtml('game-mode'))).toEqual([]);
    expect(texts(rowHtml('gpu-preference'))).toEqual([]);
    pc.pickPlan('Balanced');
    expect(texts(body({ view: pc.view() }).match(/<li class="tweak-row[^"]*"[^>]*data-tweak="power-plan">[\s\S]*?<\/li>/)?.[0] ?? '')).toEqual([t.useAgain, t.removePlan]);
  });

  it('shows the measure-first note with a button to the Benchmark tab, above the rows, and never blocks Preview', () => {
    const none = body({ measure: { kind: 'none' }, choices: { 'power-plan': true } });
    expect(none).toContain(t.measureNone);
    expect(button(none, t.openBenchmark)?.disabled).toBe(false);
    expect(none.indexOf(t.measureNone)).toBeLessThan(none.indexOf('>Kestrel power plan<'));
    expect(button(none, 'Preview 1 change')?.disabled).toBe(false);

    const spread = body({ measure: { kind: 'spread', pct: '24.9%' } });
    expect(spread).toContain('Your starting point’s runs vary by 24.9%, so small changes won’t show. Run a clean one before applying.');
    expect(button(spread, t.openBenchmark)).toBeDefined();

    const quiet = body({ measure: null });
    expect(quiet).not.toContain(t.measureNone);
    expect(quiet).not.toContain('runs vary by');
    expect(button(quiet, t.openBenchmark)).toBeUndefined();
  });

  it('says when Kestrel runs as another account, and how many changes wait for an account that isn’t signed in', () => {
    const view = { ...ownerView(), otherAccount: true, pendingOtherAccounts: 2 };
    const html = body({ view });
    expect(html).toContain('Kestrel is running as another account. Changes go to the account signed in now.');
    expect(html).toContain('Another account has 2 changes from Kestrel. Those wait until that account is signed in.');
    expect(html.indexOf(t.otherAccount)).toBeLessThan(html.indexOf('>Kestrel power plan<'));
    expect(body({ view: { ...view, pendingOtherAccounts: 1 } })).toContain('Another account has 1 change from Kestrel. It waits until that account is signed in.');
    const quiet = body();
    expect(quiet).not.toContain('another account');
    expect(quiet).not.toContain('Another account has');
  });

  it('lists what a crash cut short above the rows, one line each, so it is seen without scrolling past them', () => {
    const lines = ['Kestrel was interrupted while changing Game Mode. It finished.', 'Kestrel was interrupted while changing Kestrel power plan. Its plan isn\'t active.'];
    const html = body({ view: { ...ownerView(), interrupted: lines } });
    for (const line of lines) {
      expect(html).toContain(line.replace(/'/g, '&#x27;'));
      expect(html.indexOf(line.replace(/'/g, '&#x27;'))).toBeLessThan(html.indexOf('>Kestrel power plan<'));
    }
    expect(count(html, '<li>Kestrel was interrupted')).toBe(2);
    expect(body()).not.toContain('was interrupted');
  });

  it('shows the report’s own warning about unreadable history, once, in the report panel', () => {
    const pc = createSampleTweaks();
    pc.setUnreadable(3);
    const html = body({ view: pc.view(), report: ready(pc.report()) });
    expect(count(html, 'Part of Kestrel&#x27;s history couldn&#x27;t be read.')).toBe(1);
    expect(html.indexOf('Part of Kestrel&#x27;s history')).toBeGreaterThan(html.indexOf(t.preview(0)));
    expect(html).not.toContain(t.unreadable);
    expect(body()).not.toContain('history couldn');
  });

  it('says it itself while the report text isn’t there, and still has the report panel to say it in', () => {
    const view = { ...ownerView(), unreadableLines: 3 };
    for (const report of [{ state: 'loading' }, { state: 'failed', message: 'x' }] as ReportLoad[]) {
      const html = body({ view, report });
      expect(html, report.state).toContain(t.unreadable);
      expect(html.indexOf(t.unreadable), report.state).toBeGreaterThan(html.indexOf(t.preview(0)));
    }
  });

  it('shows the report last, below the buttons, with Copy report', () => {
    const html = body();
    expect(html.indexOf(t.report)).toBeGreaterThan(html.indexOf(t.preview(0)));
    expect(texts(html)).toContain(t.copyReport);
    expect(html).toContain('No changes yet.');
  });
});

describe('the page’s body, mid-flow', () => {
  const preview: PreviewView = { items: [planItem], restore: 'ready' };

  it('shows the Preview below the buttons and above the report, and locks the rows and buttons meanwhile', () => {
    const html = body({ flow: { step: 'preview', preview }, choices: { 'power-plan': true } });
    const at = (text: string) => html.indexOf(text);
    expect(at('Preview 1 change')).toBeLessThan(at(t.previewIntro));
    expect(at(t.previewIntro)).toBeLessThan(at(t.report));
    expect(button(html, t.apply)?.disabled).toBe(false);
    expect(button(html, 'Preview 1 change')?.disabled).toBe(true);
    for (const box of boxes(html)) expect(box).toContain('disabled=""');
  });

  it('shows the restore point question, and the status line while it works', () => {
    const asking = body({ flow: { step: 'restore', preview: { ...preview, restore: 'protection-off' }, question: { kind: 'protection-off' } } });
    expect(asking).toContain(t.protectionOff);
    expect(texts(asking)).toContain('Turn on and create');
    const working = body({ flow: { step: 'busy', text: t.restoring } });
    expect(working).toContain('Creating a restore point… this can take a minute.');
    expect(working).toMatch(/role="status"[^>]*>[^<]*Creating a restore point/);
    expect(buttons(working).filter((b) => b.text.startsWith('Preview'))[0].disabled).toBe(true);
  });

  it('shows the result of Apply, then what to do next', () => {
    const outcomes: ItemOutcome[] = [{ id: 'windowed-optimizations', title: 'Optimizations for windowed games', ok: true, message: 'Applied.' }];
    const html = body({ flow: { step: 'applied', outcomes, restart: true } });
    expect(html).toContain('Optimizations for windowed games');
    expect(html).toContain(t.restartValorant);
    expect(html).toContain(t.done);
    expect(texts(html)).toContain(t.runBenchmark);
    // The rows are free again to carry on.
    expect(button(html, 'Preview 0 changes')?.disabled).toBe(true);
    expect(boxes(html)[0]).not.toContain('disabled');
  });

  it('asks once before Revert everything, listing the rows', () => {
    const pc = createSampleTweaks();
    pc.apply(['power-plan', 'pointer-precision']);
    pc.changeBehindItsBack('pointer-precision');
    pc.apply(['windowed-optimizations']);
    const html = body({ view: pc.view(), flow: { step: 'ask-revert' } });
    expect(count(html, t.revertConfirm)).toBe(1);
    // The plan and the window setting hold; the pointer was changed by something else and stays as it is.
    const list = html.match(/<ul class="revert-list">([\s\S]*?)<\/ul>/)?.[1] ?? '';
    expect(list).toContain('<li>Kestrel power plan</li>');
    expect(list).toContain('<li>Optimizations for windowed games</li>');
    expect(list).not.toContain('Enhance pointer precision off');
    expect(texts(html)).toEqual(expect.arrayContaining([t.yesRevert, t.cancel]));
    expect(button(html, t.revertEverything)?.disabled).toBe(true);
  });

  it('shows what Revert everything did, plainly, including what it left alone', () => {
    const outcomes: ItemOutcome[] = [
      { id: 'power-plan', title: 'Kestrel power plan', ok: true, message: 'Undone.' },
      { id: 'pointer-precision', title: 'Enhance pointer precision off', ok: false, message: 'Something else changed this after Kestrel did. Kestrel left it alone.' },
      { id: 'system-protection', title: 'System Protection', ok: false, message: 'System Protection stays on: turning it off deletes this drive\'s restore points.' },
    ];
    const html = body({ flow: { step: 'results', outcomes } });
    expect(html).toContain('Kestrel left it alone.');
    expect(html).toContain('System Protection stays on');
    expect(count(html, 'class="outcome failed"')).toBe(2);
    expect(html).not.toContain(t.done);
  });

  it('says a single Undo’s result in its row, and not again below', () => {
    const pc = createSampleTweaks();
    pc.apply(['pointer-precision']);
    const item = pc.undo('pointer-precision');
    const html = body({ view: pc.view(), flow: { step: 'row-result', id: 'pointer-precision', outcome: item } });
    expect(html).toMatch(/<li class="tweak-row[^"]*"[^>]*data-tweak="pointer-precision">[\s\S]*?class="tweak-note[^"]*">Undone\.<\/p>[\s\S]*?<\/li>/);
    expect(count(html, 'Undone.')).toBe(1);
  });

  it('says a refusal where the person pressed the button, and the gate’s message with it', () => {
    const pc = createSampleTweaks();
    pc.apply(['pointer-precision']);
    const html = body({ view: { ...pc.view(), gate: 'game-running' }, flow: { step: 'failed', id: 'pointer-precision', message: 'Close VALORANT first.' } });
    expect(count(html, 'class="tweak-note')).toBe(1);
    // The same words stand once beside Preview and once in the row.
    expect(count(html, 'Close VALORANT first.')).toBe(2);
    expect(html).not.toContain(`${t.close}</button>`);
  });

  it('says it in the panel below when the row it is about is no longer on the page', () => {
    // System Protection's row goes with its record, so its Undo has no row to answer in.
    const pc = createSampleTweaks();
    pc.setRestore('protection-off');
    pc.restorePoint('turn-on-and-create');
    const item = pc.undo('system-protection');
    const html = body({ view: pc.view(), flow: { step: 'row-result', id: 'system-protection', outcome: item } });
    expect(html).toContain('System Protection');
    expect(html).toContain('class="outcome ok"');
    expect(html).not.toContain('class="tweak-note');
  });

  it('shows a refused Apply with its message, that no settings were changed and a way to close it, and never a spinner', () => {
    const html = body({ flow: { step: 'failed', message: 'Close VALORANT first.', settings: 'unchanged' } });
    expect(html).toContain('Close VALORANT first.');
    expect(html).toContain('No settings were changed.');
    expect(html).not.toContain('Nothing was changed');
    expect(html).not.toContain(t.protectionMayHaveChanged);
    expect(texts(html)).toContain(t.close);
    expect(html).not.toContain(t.applying);
    // The page is free again, not locked.
    expect(boxes(html)[0]).not.toContain('disabled');
  });

  it('says plainly that System Protection may have been turned on when a turn-on was asked for, and not that no settings changed', () => {
    const html = body({ flow: { step: 'failed', message: 'Close VALORANT first.', settings: 'protection-may-have-changed' } });
    expect(html).toContain('Close VALORANT first.');
    expect(html).toContain('System Protection may have been turned on: check its row above.');
    expect(html).not.toContain(t.noSettingsChanged);
    expect(texts(html)).toContain(t.close);
    // The row it points at is in the list above the panel, where the person reads it.
    const pc = createSampleTweaks();
    pc.setRestore('protection-off');
    pc.restorePoint('turn-on-and-create');
    const shown = body({ view: pc.view(), flow: { step: 'failed', message: 'Close VALORANT first.', settings: 'protection-may-have-changed' } });
    expect(shown.indexOf('>System Protection<')).toBeGreaterThan(0);
    expect(shown.indexOf('>System Protection<')).toBeLessThan(shown.indexOf(t.protectionMayHaveChanged));
  });

  it('does not claim nothing was changed for a failure that did not say so', () => {
    const html = body({ flow: { step: 'failed', message: 'Kestrel is already changing settings in another window.' } });
    expect(html).not.toContain(t.noSettingsChanged);
    expect(html).not.toContain(t.protectionMayHaveChanged);
  });

  it('keeps the page free when a re-read shows that the row it was asking about changed back', () => {
    // Restore anyway was pressed on the pointer row, and the view was read again: it is available now, so the question is over.
    const html = body({ flow: { step: 'ask-restore-anyway', id: 'pointer-precision' }, choices: { 'power-plan': true } });
    expect(html).not.toContain(t.restoreAnywayAsk);
    expect(texts(html)).not.toContain(t.yesRestore);
    expect(button(html, 'Preview 1 change')?.disabled).toBe(false);
    for (const box of boxes(html)) expect(box).not.toContain('disabled');
  });

  it('keeps the question, and locks the rest, while the row is still one something else changed', () => {
    const pc = createSampleTweaks();
    pc.apply(['pointer-precision']);
    pc.changeBehindItsBack('pointer-precision');
    const html = body({ view: pc.view(), flow: { step: 'ask-restore-anyway', id: 'pointer-precision' }, choices: { 'power-plan': true } });
    expect(html).toContain(t.restoreAnywayAsk);
    expect(texts(html)).toEqual(expect.arrayContaining([t.yesRestore, t.cancel]));
    expect(button(html, 'Preview 1 change')?.disabled).toBe(true);
  });

  it('says System Protection’s warning once, beside its Undo, and not again in the row’s own sentence', () => {
    const pc = createSampleTweaks();
    pc.setRestore('protection-off');
    pc.restorePoint('turn-on-and-create');
    const html = body({ view: pc.view() });
    expect(count(plain(html), t.protectionWarning)).toBe(1);
    expect(count(plain(html), 'deletes this drive')).toBe(1);
    expect(plain(html)).toContain('System Protection is on because Kestrel turned it on.');
  });
});

describe('the page’s body when the view couldn’t be read again', () => {
  const REASON = 'C:\\ProgramData\\Kestrel belongs to another account. Delete it, then try again.';
  const outcomes: ItemOutcome[] = [{ id: 'windowed-optimizations', title: 'Optimizations for windowed games', ok: true, message: 'Applied.' }];
  const page = (over: Partial<Parameters<typeof TweaksPage>[0]> = {}) =>
    renderToStaticMarkup(
      <TweaksPage load={{ state: 'ready', view: ownerView(), stale: REASON }} report={ready()} measure={null} choices={{}} flow={{ step: 'applied', outcomes, restart: true }} on={on} {...over} />,
    );

  it('keeps the rows and the result on screen, with the reason beside the buttons and a way to try again', () => {
    const html = page();
    expect(html).toContain('Optimizations for windowed games');
    expect(html).toContain(t.restartValorant);
    expect(texts(html)).toContain(t.runBenchmark);
    expect(html).toContain('class="tweak-row');
    expect(html).toContain(`${t.rereadFailed} ${REASON}`);
    expect(texts(html).filter((text) => text === t.tryAgain)).toHaveLength(1);
    expect(html).not.toContain(t.loadFailed);
  });

  it('turns the buttons that change things off, since the rows may be out of date, but leaves ticking alone', () => {
    const pc = createSampleTweaks();
    pc.apply(['power-plan']);
    const html = page({ load: { state: 'ready', view: pc.view(), stale: REASON } });
    expect(button(html, t.undo)?.disabled).toBe(true);
    expect(button(html, t.revertEverything)?.disabled).toBe(true);
    expect(button(html, 'Preview 0 changes')?.disabled).toBe(true);
    expect(boxes(html)[0]).not.toContain('disabled');
    // The result's own buttons are not about the rows.
    expect(button(html, t.runBenchmark)?.disabled).toBe(false);
    expect(button(html, t.close)?.disabled).toBe(false);
  });

  it('shows the full-page failure only when there is no view at all', () => {
    const html = page({ load: { state: 'failed', message: REASON } });
    expect(html).toContain(t.loadFailed);
    expect(html).not.toContain('tweak-row');
    expect(html).not.toContain(t.restartValorant);
  });

  it('says nothing of it for a view that was read', () => {
    const html = page({ load: { state: 'ready', view: ownerView() } });
    expect(html).not.toContain(t.rereadFailed);
    expect(button(html, t.tryAgain)).toBeUndefined();
  });
});

describe('the whole page', () => {
  const page = (over: Partial<Parameters<typeof TweaksPage>[0]> = {}) =>
    renderToStaticMarkup(<TweaksPage load={{ state: 'ready', view: ownerView() }} report={ready()} measure={null} choices={{}} flow={idle} on={on} {...over} />);

  it('opens with the title and the sentence that nothing changes until Apply', () => {
    const html = page();
    expect(html).toContain('<h1>Tweaks</h1>');
    expect(html).toContain('Kestrel picked these for your PC. Nothing changes until you press Apply.');
    expect(html.indexOf(t.intro)).toBeLessThan(html.indexOf(t.essential));
  });

  it('says it is reading while it loads, with no rows and no buttons', () => {
    const html = page({ load: { state: 'loading' } });
    expect(html).toContain(t.loading);
    expect(html).not.toContain('tweak-row');
    expect(buttons(html)).toEqual([]);
  });

  it('shows the reason instead of rows when the view can’t be read, with a way to try again', () => {
    const reason = 'C:\\ProgramData\\Kestrel belongs to another account. Delete it, then try again.';
    const html = page({ load: { state: 'failed', message: reason } });
    expect(html).toContain(t.loadFailed);
    expect(html).toContain('C:\\ProgramData\\Kestrel belongs to another account. Delete it, then try again.');
    expect(html).not.toContain('tweak-row');
    expect(html).not.toContain(t.essential);
    expect(texts(html)).toEqual([t.tryAgain]);
    expect(html).toMatch(/<p class="[^"]*\bwrap\b[^"]*">C:\\ProgramData/);
  });

  it('never promises a gain, whatever state it is in', () => {
    const pc = createSampleTweaks();
    pc.apply(['power-plan']);
    for (const html of [page(), page({ load: { state: 'ready', view: pc.view() } }), page({ measure: { kind: 'none' } })]) {
      expect(plain(html)).not.toMatch(/improv|faster|boost|speed up/i);
    }
  });
});
