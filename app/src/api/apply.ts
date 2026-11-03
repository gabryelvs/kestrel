// What Apply does and in what order: the restore point first, the changes only after it is accepted, then a scan so the rest of
// the app sees what changed. The commands come in as arguments, so the order and every way out of it are tested with the
// browser preview's fake PC and with stand-ins that refuse, and the page (pages/Tweaks.tsx) only shows what comes back.
import { errorText, needsRestart, restoreFailure, restoreRoute, type RestoreQuestion } from './tweaks';
import type { ItemOutcome, PreviewView, RestoreChoice, RestoreOutcome, TweakId } from './types';

/** The commands Apply is made of. `rescan` answers when a scan that started after the change is done, however it ended. */
export type ApplyCommands = {
  restorePoint: (choice: RestoreChoice) => Promise<RestoreOutcome>;
  apply: (ids: TweakId[]) => Promise<ItemOutcome[]>;
  rescan: () => Promise<void>;
};

/**
 * Where Apply is asked to go on from: the button was pressed, or the person answered the question it put. Skipping the restore
 * point is only ever `without`, which is Continue without; `go-on` is Continue after a point that failed, which is already in the
 * journal and needs nothing more asked of Windows.
 */
export type ApplyAt = 'pressed' | 'turn-on' | 'without' | 'go-on';

/** What the page says while it waits. */
export type ApplyStep = 'restoring' | 'applying' | 'rescanning';

/**
 * What the page shows next: a question, the outcome of each change, or the reason Apply stopped. `unchanged` says that no
 * setting was changed, which is all that is known: a restore point may have been made, and turning System Protection on, which
 * comes with `turn-on`, can have worked before the command that did it was refused.
 */
export type ApplyNext =
  | { next: 'ask'; question: RestoreQuestion }
  | { next: 'applied'; outcomes: ItemOutcome[]; restart: boolean }
  | { next: 'stopped'; message: string; unchanged: boolean };

/** Whether something took, which is when the scan is out of date. */
export const changedAnything = (outcomes: ItemOutcome[]): boolean => outcomes.some((outcome) => outcome.ok);

/**
 * After a command that changed something: scans again, and waits for it, so that a benchmark started next records the PC as it
 * is now. However the scan ends, what was done is still shown: the old scan stays, and that is all a failed one costs.
 */
export async function settle(outcomes: ItemOutcome[], rescan: () => Promise<void>): Promise<void> {
  if (!changedAnything(outcomes)) return;
  try {
    await rescan();
  } catch {
    // The result is shown all the same.
  }
}

/**
 * One go at Apply for a Preview. Nothing is applied before the restore point step has come to an accepted outcome (created,
 * reused or skipped); a failed one is a question for the person, and Cancel is simply not calling this again. Each command asks
 * the gate for itself, so either can be refused, and a refusal is the answer, not an exception.
 */
export async function runApply(commands: ApplyCommands, preview: PreviewView, at: ApplyAt, progress: (step: ApplyStep) => void): Promise<ApplyNext> {
  let shown: ApplyStep | null = null;
  const say = (step: ApplyStep) => {
    if (step === shown) return;
    shown = step;
    progress(step);
  };

  const makeChanges = async (): Promise<ApplyNext> => {
    say('applying');
    let outcomes: ItemOutcome[];
    try {
      outcomes = await commands.apply(preview.items.map((item) => item.id));
    } catch (error) {
      // The engine changes nothing when it refuses, and the gate and the scan come before the engine.
      return { next: 'stopped', message: errorText(error), unchanged: true };
    }
    if (changedAnything(outcomes)) say('rescanning');
    await settle(outcomes, commands.rescan);
    return { next: 'applied', outcomes, restart: needsRestart(outcomes, preview.items) };
  };

  const withRestorePoint = async (choice: RestoreChoice): Promise<ApplyNext> => {
    // A skip is recorded in a moment; asking Windows is what takes the time.
    say(choice === 'skip' ? 'applying' : 'restoring');
    let outcome: RestoreOutcome;
    try {
      outcome = await commands.restorePoint(choice);
    } catch (error) {
      return { next: 'stopped', message: errorText(error), unchanged: choice !== 'turn-on-and-create' };
    }
    const reason = restoreFailure(outcome);
    return reason === null ? makeChanges() : { next: 'ask', question: { kind: 'failed', reason } };
  };

  switch (at) {
    case 'pressed': {
      const route = restoreRoute(preview.restore);
      if (route === 'turn-on') return { next: 'ask', question: { kind: 'protection-off' } };
      if (route === 'blocked') return { next: 'ask', question: { kind: 'blocked-by-policy' } };
      return withRestorePoint('create');
    }
    case 'turn-on':
      return withRestorePoint('turn-on-and-create');
    case 'without':
      return withRestorePoint('skip');
    case 'go-on':
      return makeChanges();
  }
}
