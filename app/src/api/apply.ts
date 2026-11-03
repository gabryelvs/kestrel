// What Apply does and in what order: the restore point first, the changes only after it is accepted, then a scan so the rest of
// the app sees what changed. The commands come in as arguments, so the order and every way out of it are tested with the
// browser preview's fake PC and with stand-ins that refuse, and the page (pages/Tweaks.tsx) only shows what comes back.
import { SCAN_WAIT_MS } from './rescan';
import { errorText, needsRestart, restoreFailure, restoreRoute, type RestoreQuestion, type Settings } from './tweaks';
import type { ItemOutcome, PreviewView, RestoreChoice, RestoreOutcome, TweakId } from './types';

/** The commands Apply is made of. `rescan` answers when a scan that started after the change is done, however it ended. */
export type ApplyCommands = {
  restorePoint: (choice: RestoreChoice) => Promise<RestoreOutcome>;
  apply: (ids: TweakId[]) => Promise<ItemOutcome[]>;
  rescan: () => Promise<void>;
};

/**
 * Where Apply is asked to go on from: the button was pressed, or the person answered the question it put. Skipping the restore
 * point is only ever `without`, which is Continue without. `go-on` is Continue after a point that failed, which is already in the
 * journal and needs nothing more asked of Windows; `go-on-after-turn-on` is the same after a point that was asked to turn System
 * Protection on first, which may have worked, so that what Apply says if it is then refused takes it into account.
 */
export type ApplyAt = 'pressed' | 'turn-on' | 'without' | 'go-on' | 'go-on-after-turn-on';

/** What Continue means at a question: Continue without at the ones about System Protection or a policy, going on past a failed point. */
export function continueAt(question: RestoreQuestion): ApplyAt {
  if (question.kind !== 'failed') return 'without';
  return question.turnOnAsked ? 'go-on-after-turn-on' : 'go-on';
}

/** What the page says while it waits. */
export type ApplyStep = 'restoring' | 'applying' | 'rescanning';

/**
 * What the page shows next: a question, the outcome of each change, or the reason Apply stopped. `settings` says what is known of
 * the PC then: no setting was changed, unless a turn-on was asked for, which can have turned System Protection on before whatever
 * was refused (a restore point that was made is not a setting, so it doesn't count).
 */
export type ApplyNext =
  | { next: 'ask'; question: RestoreQuestion }
  | { next: 'applied'; outcomes: ItemOutcome[]; restart: boolean }
  | { next: 'stopped'; message: string; settings: Settings };

/** Whether something took, which is when the scan is out of date. */
export const changedAnything = (outcomes: ItemOutcome[]): boolean => outcomes.some((outcome) => outcome.ok);

/**
 * After a command that changed something: scans again, and waits for it, so that a benchmark started next records the PC as it
 * is now. However the scan ends, what was done is still shown: the old scan stays, and that is all a failed one costs. A scan that
 * never ends is waited for `SCAN_WAIT_MS` and no longer, since nothing else puts a limit on it.
 */
export async function settle(outcomes: ItemOutcome[], rescan: () => Promise<void>): Promise<void> {
  if (!changedAnything(outcomes)) return;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      rescan(),
      new Promise<void>((resolve) => {
        timer = setTimeout(resolve, SCAN_WAIT_MS);
      }),
    ]);
  } catch {
    // The result is shown all the same.
  } finally {
    clearTimeout(timer);
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

  // `turnOnAsked`: the step before this one was asked to turn System Protection on, so a refusal can't say that nothing changed.
  const makeChanges = async (turnOnAsked: boolean): Promise<ApplyNext> => {
    say('applying');
    let outcomes: ItemOutcome[];
    try {
      outcomes = await commands.apply(preview.items.map((item) => item.id));
    } catch (error) {
      // The engine changes no setting when it refuses, and the gate and the scan come before the engine.
      return { next: 'stopped', message: errorText(error), settings: turnOnAsked ? 'protection-may-have-changed' : 'unchanged' };
    }
    if (changedAnything(outcomes)) say('rescanning');
    await settle(outcomes, commands.rescan);
    return { next: 'applied', outcomes, restart: needsRestart(outcomes, preview.items) };
  };

  const withRestorePoint = async (choice: RestoreChoice): Promise<ApplyNext> => {
    // A skip is recorded in a moment; asking Windows is what takes the time.
    say(choice === 'skip' ? 'applying' : 'restoring');
    const turnOnAsked = choice === 'turn-on-and-create';
    let outcome: RestoreOutcome;
    try {
      outcome = await commands.restorePoint(choice);
    } catch (error) {
      return { next: 'stopped', message: errorText(error), settings: turnOnAsked ? 'protection-may-have-changed' : 'unchanged' };
    }
    const reason = restoreFailure(outcome);
    return reason === null ? makeChanges(turnOnAsked) : { next: 'ask', question: { kind: 'failed', reason, turnOnAsked } };
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
      return makeChanges(false);
    case 'go-on-after-turn-on':
      return makeChanges(true);
  }
}
