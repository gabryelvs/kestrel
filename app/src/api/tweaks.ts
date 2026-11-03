// What the Tweaks tab decides on its own, without touching the PC: which rows get a tick box and which are ticked, whether
// the measure-first note shows, which way Apply goes at the restore point, what a step of the page locks and says. The page
// (pages/Tweaks.tsx) only wires these to the commands, so the decisions are tested here with plain data.
import { strings } from '../strings';
import { spreadText, variesTooMuch } from './bench';
import type { BenchmarkSummary, Gate, ItemOutcome, PreviewItem, PreviewView, Readiness, RestoreOutcome, Row, TweakId, TweaksView } from './types';

/**
 * What the tab reads from the commands: the view and the report text are read together, each can fail on its own. `failed` is
 * only for a view that was never read. When a view was read and reading it again failed, the last good one is kept as `view` and
 * `stale` says why it may be out of date, so what was just done (a result, a note) stays on screen.
 */
export type ViewLoad =
  | { state: 'loading' }
  | { state: 'failed'; message: string }
  | { state: 'ready'; view: TweaksView; stale?: string };
export type ReportLoad = { state: 'loading' } | { state: 'failed'; message: string } | { state: 'ready'; text: string };

// Measure first.

export type MeasureNote = { kind: 'none' } | { kind: 'spread'; pct: string };

/**
 * The note above the rows, or null when the starting point is fine. It never blocks Apply. No starting point means no baseline
 * among the saved results. A baseline whose average FPS varied by more than 10% (the Benchmark tab's own threshold) can't show
 * a small change.
 */
export function measureNote(list: BenchmarkSummary[]): MeasureNote | null {
  const baseline = list.find((b) => b.isBaseline);
  if (!baseline) return { kind: 'none' };
  const spread = baseline.result.spread.avgFps;
  return variesTooMuch(spread) ? { kind: 'spread', pct: spreadText(spread) } : null;
}

export const measureText = (note: MeasureNote): string =>
  note.kind === 'none' ? strings.tweaks.measureNone : strings.tweaks.measureSpread(note.pct);

// The tick boxes.

/** Only a row Kestrel can apply has a box: Already set, Not needed, Blocked, Applied, flagged and unreadable rows have none. */
export const hasTickBox = (row: Row): boolean => row.state === 'available';

/** What the person ticked (true) or unticked (false) by hand. A row without an entry is ticked by its own flag. */
export type Choices = Partial<Record<TweakId, boolean>>;

export const isTicked = (row: Row, choices: Choices): boolean => hasTickBox(row) && (choices[row.id] ?? row.ticked);

/** The ticked rows' ids, in the rows' order. */
export const tickedIds = (rows: Row[], choices: Choices): TweakId[] => rows.filter((row) => isTicked(row, choices)).map((row) => row.id);

/** The choices without those for `ids`, which were acted on: when they can be ticked again, their own flags decide. */
export function forgetChoices(choices: Choices, ids: TweakId[]): Choices {
  return Object.fromEntries(Object.entries(choices).filter(([id]) => !ids.includes(id as TweakId)));
}

/** A Fix button's tweak ticked on its own, every other box cleared. */
export function onlyTicked(rows: Row[], id: TweakId): Choices {
  return Object.fromEntries(rows.filter(hasTickBox).map((row) => [row.id, row.id === id]));
}

/**
 * Whether a Fix button opens the Preview. A tweak with no box (already set, applied, not needed, blocked, changed since the
 * scan) or no row at all just shows the tab, and so does a closed gate: Apply could not run, and the Preview button is off.
 */
export const canFocus = (view: TweaksView, id: TweakId): boolean => view.gate === 'open' && view.rows.some((row) => row.id === id && hasTickBox(row));

/**
 * The rows Revert everything undoes: Kestrel's changes that still hold, and its plan while another plan is in use. It leaves
 * System Protection on (turning it off deletes the restore points) and rows something else changed.
 */
export const revertRows = (rows: Row[]): Row[] =>
  rows.filter((row) => row.id !== 'system-protection' && (row.state === 'applied' || row.state === 'plan-inactive'));

// The gate.

/** Why Apply and Undo are off, or null while the gate is open. Every gate but `open` is closed. */
export const gateReason = (gate: Gate): string | null => (gate === 'open' ? null : strings.tweaks.gate[gate]);

/**
 * Why nothing that changes the PC may be pressed, or null while it may: a closed gate, or a view that couldn't be read again and
 * so may be out of date (the gate in it is as old as the view). Both are shown beside the buttons and both have a Try again.
 */
export function closedReason(view: TweaksView, stale?: string): string | null {
  if (stale !== undefined) return `${strings.tweaks.rereadFailed} ${stale}`.trim();
  return gateReason(view.gate);
}

// Apply and the restore point.

export type RestoreRoute = 'create' | 'turn-on' | 'blocked';

/** Which way Apply goes at the restore point: straight to Windows when System Protection is on (or Kestrel can't tell), or a question. */
export function restoreRoute(restore: Readiness): RestoreRoute {
  switch (restore) {
    case 'protection-off':
      return 'turn-on';
    case 'blocked-by-policy':
      return 'blocked';
    case 'ready':
    case 'unknown':
      return 'create';
  }
}

/** The reason a restore point failed, or null when the changes may go ahead (created, reused or skipped). A reason may be empty. */
export const restoreFailure = (outcome: RestoreOutcome): string | null => (outcome.result === 'failed' ? outcome.reason : null);

/** Whether to say "Restart VALORANT": a change that took waits for the next launch, by the timing the Preview on screen showed. */
export const needsRestart = (outcomes: ItemOutcome[], items: PreviewItem[]): boolean =>
  outcomes.some((outcome) => outcome.ok && items.some((item) => item.id === outcome.id && item.timing === 'next-launch'));

/** The question Apply puts to the person at the restore point. */
export type RestoreQuestion = { kind: 'protection-off' } | { kind: 'blocked-by-policy' } | { kind: 'failed'; reason: string };

// What the page is doing.

/**
 * The step the page is at below the rows. A step with an `id` is about one row, and that row says it where the person acted.
 * Only the steps that wait for a button or a command lock the rest of the page.
 */
export type Flow =
  | { step: 'idle' }
  | { step: 'busy'; text: string; id?: TweakId }
  | { step: 'preview'; preview: PreviewView }
  | { step: 'restore'; preview: PreviewView; question: RestoreQuestion }
  | { step: 'ask-restore-anyway'; id: TweakId }
  | { step: 'ask-revert' }
  | { step: 'applied'; outcomes: ItemOutcome[]; restart: boolean }
  | { step: 'results'; outcomes: ItemOutcome[] }
  | { step: 'row-result'; id: TweakId; outcome: ItemOutcome }
  /** A command was refused or failed. `unchanged` says no settings were changed, which is only claimed where it is known (see ApplyNext). */
  | { step: 'failed'; message: string; id?: TweakId; unchanged?: boolean };

/** True while a command runs or a question waits: ticking, Preview and every row button are off, so nothing is asked twice. */
export function flowLocks(flow: Flow): boolean {
  switch (flow.step) {
    case 'busy':
    case 'preview':
    case 'restore':
    case 'ask-restore-anyway':
    case 'ask-revert':
      return true;
    case 'idle':
    case 'applied':
    case 'results':
    case 'row-result':
    case 'failed':
      return false;
  }
}

/**
 * Whether coming back to the window reads the page again: while it shows something that is cleared from outside (VALORANT closed,
 * a folder deleted) and no command is under way. It starts no read while the first is still being read.
 */
export function rereadsOnFocus(load: ViewLoad, flow: Flow): boolean {
  if (flow.step === 'busy') return false;
  switch (load.state) {
    case 'loading':
      return false;
    case 'failed':
      return true;
    case 'ready':
      return load.view.gate !== 'open' || load.stale !== undefined;
  }
}

/**
 * The step as it stands against the rows now. A question about a row (Restore anyway) is over once a re-read shows that row no longer
 * changed by something else: it can no longer be answered, and a step that waits for an answer locks the page.
 */
export function currentFlow(flow: Flow, rows: Row[]): Flow {
  if (flow.step !== 'ask-restore-anyway') return flow;
  return rows.some((row) => row.id === flow.id && row.state === 'changed-elsewhere') ? flow : { step: 'idle' };
}

/** The row a step is about, if it is about one. */
export function boundRow(flow: Flow): TweakId | null {
  switch (flow.step) {
    case 'busy':
    case 'failed':
      return flow.id ?? null;
    case 'row-result':
    case 'ask-restore-anyway':
      return flow.id;
    default:
      return null;
  }
}

export type RowNote = { ok: boolean; text: string };

/** What the row `id` says under itself: the step if it is about this row, else nothing. */
export function rowNote(flow: Flow, id: TweakId): RowNote | null {
  if (boundRow(flow) !== id) return null;
  switch (flow.step) {
    case 'busy':
      return { ok: true, text: flow.text };
    case 'row-result':
      return { ok: flow.outcome.ok, text: flow.outcome.message };
    case 'failed':
      return { ok: false, text: flow.message };
    default:
      return null;
  }
}

/** What a rejected command gives: Tauri rejects with the text itself, and the browser preview's fakes may throw an Error. */
export function errorText(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : String(error);
}
