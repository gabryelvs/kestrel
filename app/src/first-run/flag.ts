const KEY = 'kestrel.firstRunDone';
const PLAN_NOTE_KEY = 'kestrel.planNote';

export function firstRunDone(): boolean {
  try {
    return localStorage.getItem(KEY) === '1';
  } catch {
    return false;
  }
}

export function markFirstRunDone(): void {
  try {
    localStorage.setItem(KEY, '1');
  } catch {
    // Storage can be unavailable; the guided scan will simply show again next time.
  }
}

/**
 * Whether the Tweaks tab owes this person its one-time note ("Here's the plan Kestrel picked"). Only the first run's two ways on ask for
 * it, so a person whose first run was done before it existed never has it, and neither does anyone whose storage can't be read.
 */
export function planNoteWanted(): boolean {
  try {
    return localStorage.getItem(PLAN_NOTE_KEY) === '1';
  } catch {
    return false;
  }
}

/** The first run ended: the Tweaks tab shows its note, until it is seen. */
export function askForPlanNote(): void {
  try {
    localStorage.setItem(PLAN_NOTE_KEY, '1');
  } catch {
    // If this can't be remembered the note is never shown, rather than shown every time with no way to be done with it.
  }
}

/** The note was answered (Not now) or the plan was applied: it isn't asked for again. */
export function planNoteSeen(): void {
  try {
    localStorage.removeItem(PLAN_NOTE_KEY);
  } catch {
    // Storage can be unavailable. The Tweaks tab hides the note for this visit on its own, whatever happens here.
  }
}
