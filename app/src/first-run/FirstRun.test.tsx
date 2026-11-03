import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import mock from '../api/mock-report.json';
import { elementsOf } from '../api/test-support';
import type { Report } from '../api/types';
import { strings } from '../strings';
import { FirstRunResult, type FirstRunNext } from './FirstRun';
import { firstRunDone, planNoteWanted } from './flag';

afterEach(() => vi.unstubAllGlobals());

const sample = mock as unknown as Report;

/** The result step with the two buttons pressable by their labels, and every call to `onDone` written down in order. */
function step() {
  const calls: [Report, FirstRunNext][] = [];
  const onDone = (report: Report, next: FirstRunNext) => void calls.push([report, next]);
  const buttons = elementsOf(FirstRunResult({ report: sample, onDone }), 'button');
  const press = (label: string) => {
    const found = buttons.find((b) => b.props.children === label);
    if (!found?.props.onClick) throw new Error(`No button says "${label}".`);
    found.props.onClick();
  };
  return { calls, press, labels: buttons.map((b) => b.props.children) };
}

/** A localStorage that remembers. */
function memory(): void {
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
}

describe('the first run’s result step', () => {
  it('shows what Kestrel found, then the two ways on: the starting-point benchmark first, Later second', () => {
    const html = renderToStaticMarkup(<FirstRunResult report={sample} onDone={() => undefined} />);
    expect(html).toContain(strings.firstRun.resultTitle);
    expect(html).toContain(`<span class="num score-number">${sample.score.value}</span>`);
    expect(html).toContain(strings.firstRun.later);
    expect(step().labels).toEqual([strings.firstRun.benchmarkNow, strings.firstRun.notNow]);
    expect(html.indexOf(strings.firstRun.benchmarkNow)).toBeLessThan(html.indexOf(strings.firstRun.notNow));
  });

  it('sends Later to the Tweaks tab, with the report, and asks for the note there', () => {
    memory();
    const { calls, press } = step();
    press(strings.firstRun.notNow);
    expect(calls).toEqual([[sample, 'tweaks']]);
    expect(planNoteWanted()).toBe(true);
  });

  it('sends the starting-point benchmark to the Benchmark tab, and asks for the note too, for the plan it ends at', () => {
    memory();
    const { calls, press } = step();
    press(strings.firstRun.benchmarkNow);
    expect(calls).toEqual([[sample, 'benchmark']]);
    expect(planNoteWanted()).toBe(true);
  });

  it('asks for the note before it leaves, so the tab that opens finds it asked for', () => {
    memory();
    const seen: boolean[] = [];
    const buttons = elementsOf(FirstRunResult({ report: sample, onDone: () => void seen.push(planNoteWanted()) }), 'button');
    for (const button of buttons) button.props.onClick?.();
    expect(seen).toEqual([true, true]);
  });

  it('never sends anyone straight Home: the first run ends at the plan', () => {
    memory();
    for (const label of [strings.firstRun.benchmarkNow, strings.firstRun.notNow]) {
      const { calls, press } = step();
      press(label);
      expect(calls.map(([, next]) => next), label).not.toContain('home');
    }
  });

  it('still ends the first run when the store throws, and then asks for no note', () => {
    const blocked = () => {
      throw new Error('blocked');
    };
    vi.stubGlobal('localStorage', { getItem: blocked, setItem: blocked, removeItem: blocked });
    for (const [label, next] of [[strings.firstRun.benchmarkNow, 'benchmark'], [strings.firstRun.notNow, 'tweaks']] as const) {
      const { calls, press } = step();
      expect(() => press(label), label).not.toThrow();
      expect(calls, label).toEqual([[sample, next]]);
    }
    expect(planNoteWanted()).toBe(false);
  });

  it('leaves marking the first run done to the app, which owns that flag', () => {
    memory();
    step().press(strings.firstRun.notNow);
    expect(firstRunDone()).toBe(false);
  });
});
