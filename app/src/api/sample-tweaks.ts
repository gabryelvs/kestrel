// A pretend PC for the browser preview (UI work on the Mac): the owner's, as the Tweaks tab finds it, answering the tweaks
// commands and remembering what was changed, so tick, Preview, Apply, Undo and Revert everything can be walked in a plain
// browser. The words are the ones kestrel-tweaks writes (its rows' reasons, its Preview, its report). Never used inside Kestrel.
//
// Its commands throw the text a Tauri command rejects with, so the page treats a refusal the same either way. The PC's own
// side (the gate, System Protection, another tool changing a value, another plan picked) is moved with the methods at the end.
import { strings } from '../strings';
import type {
  Gate, Impact, ItemOutcome, PreviewItem, PreviewView, Readiness, RestoreChoice, RestoreOutcome, Row, RowState, Timing, TweakId, TweaksView,
} from './types';

const OWNER_PLAN = 'EXM Free Power Plan V8';
const KESTREL_PLAN = 'Kestrel';
const KESTREL_VERSION = '0.5.0';
const WINDOWS_BUILD = 26200;

const ESSENTIAL: TweakId[] = ['power-plan', 'game-mode', 'background-recording', 'windowed-optimizations', 'gpu-preference', 'pointer-precision'];

// The catalog's tags (kestrel-tweaks' catalog.rs).
const TAGS: Record<TweakId, { impact: Impact; timing: Timing }> = {
  'power-plan': { impact: 'low', timing: 'now' },
  'game-mode': { impact: 'medium', timing: 'now' },
  'background-recording': { impact: 'medium', timing: 'now' },
  'windowed-optimizations': { impact: 'situational', timing: 'next-launch' },
  'gpu-preference': { impact: 'high', timing: 'next-launch' },
  'pointer-precision': { impact: 'none-in-game', timing: 'now' },
  'system-protection': { impact: 'none-in-game', timing: 'now' },
};

// What the engine says (engine.rs).
const NOTHING_TO_UNDO = "Kestrel hasn't changed this, so there is nothing to undo.";
const NOTHING_TO_APPLY_AGAIN = "Kestrel hasn't changed this, so there is nothing to apply again.";
const ONLY_THE_SETTINGS = 'Restore anyway is only for a setting something else changed. Kestrel changed nothing.';
const LEFT_ALONE = 'Something else changed this after Kestrel did. Kestrel left it alone.';
const NOT_ON_THIS_PC = "Kestrel doesn't change this on this PC.";
const ALREADY_SET = 'Already set.';
const PROTECTION_STAYS = "System Protection stays on: turning it off deletes this drive's restore points.";
const PROTECTION_ON = 'System Protection is on because Kestrel turned it on.';
const PLAN_IN_USE = "Kestrel's plan is in use, so it can't be removed. Switch to another plan first.";
// The two lines kestrel-tweaks' report ends with when part of the journal couldn't be read (report.rs).
const UNREADABLE_CAUSE = 'An interruption while Kestrel was writing is the usual cause of an unreadable line.';
const UNREADABLE = "Part of Kestrel's history couldn't be read.";

// What a plan row says for the plan in use (power.rs).
const SLOW_PLANS = ['Balanced', 'Power saver'];
const FAST_PLANS = ['High performance', 'Ultimate Performance'];

type Verdict = { state: RowState; ticked: boolean; why: string };

/** The plan row while Kestrel hasn't changed the plan. */
function planVerdict(plan: string): Verdict {
  if (SLOW_PLANS.includes(plan)) {
    return { state: 'available', ticked: true, why: `${plan} lets the CPU slow down between frames. The difference may be small or none: measure it on your PC.` };
  }
  if (FAST_PLANS.includes(plan)) {
    return { state: 'available', ticked: false, why: "You're already on a fast plan. Kestrel's also turns off USB and PCIe power saving." };
  }
  return { state: 'available', ticked: false, why: `You're on ${plan}. Measure before and after to see which is better.` };
}

/** The owner's rows before Kestrel touches anything: Game Mode and recording are already as Kestrel would leave them, there is one graphics card. */
function untouched(id: TweakId, plan: string): Verdict {
  switch (id) {
    case 'power-plan':
      return planVerdict(plan);
    case 'game-mode':
      return { state: 'already-set', ticked: false, why: 'Game Mode is already on.' };
    case 'background-recording':
      return { state: 'already-set', ticked: false, why: 'Background recording is already off.' };
    case 'windowed-optimizations':
      return { state: 'available', ticked: false, why: 'Only matters in windowed modes.' };
    case 'gpu-preference':
      return { state: 'not-needed', ticked: false, why: 'One graphics card: nothing to choose.' };
    case 'pointer-precision':
      return {
        state: 'available', ticked: false,
        why: "Doesn't change aim or FPS in VALORANT, which reads the mouse directly. Only the cursor on the desktop changes.",
      };
    case 'system-protection':
      throw new Error('System Protection has a row only while Kestrel has turned it on.');
  }
}

/** Where a change is made, and the values before and after, for the rows the owner can apply (the Preview, kestrel-tweaks' engine.rs and power.rs). */
function previewOf(id: TweakId, plan: string): PreviewItem {
  const title = strings.tweaks.names[id];
  const { timing } = TAGS[id];
  switch (id) {
    case 'power-plan':
      return {
        id, title, location: 'Power plan', timing,
        changes: [
          { label: 'Core parking minimum cores', now: '5%', after: '100%' },
          { label: 'USB selective suspend', now: 'On', after: 'Off' },
          { label: 'PCIe link-state power management', now: 'Maximum', after: 'Off' },
        ],
        note: `Switching plans also replaces everything else in ${plan} with the settings of High performance. Undo returns to your plan as it was.`,
      };
    case 'windowed-optimizations':
      return {
        id, title, timing, note: null,
        location: String.raw`HKCU\Software\Microsoft\DirectX\UserGpuPreferences, DirectXUserGlobalSettings`,
        changes: [{ label: title, now: 'not set', after: 'on' }],
      };
    case 'pointer-precision':
      return {
        id, title, timing, note: null,
        location: String.raw`HKCU\Control Panel\Mouse, MouseSpeed, MouseThreshold1, MouseThreshold2`,
        changes: [{ label: title, now: 'on', after: 'off' }],
      };
    default:
      throw new Error(`The owner's PC offers no Preview for ${id}.`);
  }
}

/** A record Kestrel has open: its setting holds Kestrel's value, something else changed it, or (the plan only) another plan is in use. */
type Standing = 'applied' | 'changed' | 'inactive';
type OpenRecord = { standing: Standing; before: string; after: string; at: string };

const success = (id: TweakId, message: string): ItemOutcome => ({ id, title: strings.tweaks.names[id], ok: true, message });
const failure = (id: TweakId, message: string): ItemOutcome => ({ id, title: strings.tweaks.names[id], ok: false, message });

const two = (n: number) => String(n).padStart(2, '0');
/** A time as the report shows it: 2026-11-03 04:50. */
const stamp = (date: Date) => `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())} ${two(date.getHours())}:${two(date.getMinutes())}`;

function restoreLine(outcome: RestoreOutcome): string {
  switch (outcome.result) {
    case 'created':
      return `Restore point created (#${outcome.sequence})`;
    case 'reused':
      return `Restore point reused from ${outcome.time.slice(0, 16).replace('T', ' ')} (#${outcome.sequence})`;
    case 'skipped':
      return 'Restore point skipped';
    case 'failed':
      return `Restore point failed: ${outcome.reason}`;
  }
}

/** The owner's PC with nothing changed. `clock` says what time the report shows. */
export function createSampleTweaks({ clock = () => new Date() }: { clock?: () => Date } = {}) {
  let gate: Gate = 'open';
  let readiness: Readiness = 'ready';
  let activePlan = OWNER_PLAN;
  let lastPoint = 56;
  let unreadable = 0;
  /** Kestrel's open records, oldest first. */
  const open = new Map<TweakId, OpenRecord>();
  /** The report's History, newest first. */
  const history: string[] = [];

  const now = () => stamp(clock());
  const told = (words: string) => history.unshift(`${now()} ${words}`);

  const requireOpen = () => {
    // Thrown as text, as a Tauri command rejects.
    if (gate !== 'open') throw strings.tweaks.gate[gate];
  };

  /** The row for `id`, or null where there is none: System Protection has one only while its record is open. */
  function rowOf(id: TweakId): Row | null {
    const record = open.get(id);
    if (id === 'system-protection' && !record) return null;
    let verdict: Verdict;
    if (!record) {
      verdict = untouched(id, activePlan);
    } else if (record.standing === 'changed') {
      verdict = { state: 'changed-elsewhere', ticked: false, why: 'Something else changed this after Kestrel did.' };
    } else if (record.standing === 'inactive') {
      verdict = { state: 'plan-inactive', ticked: false, why: `Another plan is active: ${activePlan}.` };
    } else {
      const why = id === 'power-plan' ? "Kestrel's plan is active." : id === 'system-protection' ? PROTECTION_ON : 'Kestrel changed this.';
      verdict = { state: 'applied', ticked: false, why };
    }
    return { id, title: strings.tweaks.names[id], ...verdict, ...TAGS[id] };
  }

  /** Starts a change: Kestrel's record, the plan switched, and the report's line. */
  function change(id: TweakId, before: string, after: string): ItemOutcome {
    open.set(id, { standing: 'applied', before, after, at: now() });
    if (id === 'power-plan') activePlan = KESTREL_PLAN;
    told(`Applied ${strings.tweaks.names[id]}: ${before} → ${after}`);
    return success(id, 'Applied.');
  }

  /** Ends a record: what undoing it does to the PC, what the report says, and what the person is told. */
  function undoRecord(id: TweakId, record: OpenRecord): ItemOutcome {
    const title = strings.tweaks.names[id];
    if (record.standing === 'changed') return failure(id, LEFT_ALONE);
    if (id === 'power-plan' && record.standing === 'applied') {
      activePlan = record.before;
      open.delete(id);
      told(`Undid ${title}: Back to ${record.before}.`);
      return success(id, `Back to ${record.before}.`);
    }
    if (id === 'power-plan') {
      open.delete(id);
      told(`Undid ${title}: removed Kestrel's plan and left the plan in use alone`);
      return success(id, 'Undone.');
    }
    open.delete(id);
    if (id === 'system-protection') readiness = 'protection-off';
    told(`Undid ${title}: back to ${record.before}`);
    return success(id, 'Undone.');
  }

  return {
    view(): TweaksView {
      const ids: TweakId[] = open.has('system-protection') ? [...ESSENTIAL, 'system-protection'] : ESSENTIAL;
      return {
        rows: ids.map((id) => rowOf(id)).filter((row): row is Row => row !== null),
        gate,
        otherAccount: false,
        pendingOtherAccounts: 0,
        interrupted: [],
        unreadableLines: unreadable,
        anyActive: [...open.keys()].some((id) => id !== 'system-protection'),
      };
    },

    preview(ids: TweakId[]): PreviewView {
      return { items: ids.filter((id) => rowOf(id)?.state === 'available').map((id) => previewOf(id, activePlan)), restore: readiness };
    },

    restorePoint(choice: RestoreChoice): RestoreOutcome {
      requireOpen();
      let outcome: RestoreOutcome;
      if (choice === 'skip') {
        outcome = { result: 'skipped' };
      } else if (readiness === 'blocked-by-policy') {
        outcome = { result: 'failed', reason: 'An organisation policy turns System Protection off.' };
      } else if (readiness === 'protection-off' && choice === 'create') {
        outcome = { result: 'failed', reason: 'System Protection is off.' };
      } else {
        if (readiness === 'protection-off') {
          readiness = 'ready';
          open.set('system-protection', { standing: 'applied', before: 'off', after: 'on', at: now() });
          told('Applied System Protection: off → on');
        }
        lastPoint += 1;
        outcome = { result: 'created', sequence: lastPoint };
      }
      told(restoreLine(outcome));
      return outcome;
    },

    apply(ids: TweakId[]): ItemOutcome[] {
      requireOpen();
      return ids.map((id) => {
        const row = rowOf(id);
        if (!row) return failure(id, NOT_ON_THIS_PC);
        if (row.state !== 'available') return failure(id, row.why);
        const [before, after] =
          id === 'power-plan' ? [activePlan, KESTREL_PLAN] : id === 'windowed-optimizations' ? ['not set', 'on'] : ['on', 'off'];
        return change(id, before, after);
      });
    },

    undo(id: TweakId): ItemOutcome {
      requireOpen();
      const record = open.get(id);
      return record ? undoRecord(id, record) : failure(id, NOTHING_TO_UNDO);
    },

    applyAgain(id: TweakId): ItemOutcome {
      requireOpen();
      const record = open.get(id);
      if (!record) return failure(id, NOTHING_TO_APPLY_AGAIN);
      if (id === 'power-plan' || id === 'system-protection') return failure(id, NOT_ON_THIS_PC);
      if (record.standing !== 'changed') return failure(id, ALREADY_SET);
      record.standing = 'applied';
      told(`Applied ${strings.tweaks.names[id]} again`);
      return success(id, 'Applied.');
    },

    restoreAnyway(id: TweakId): ItemOutcome {
      requireOpen();
      const record = open.get(id);
      if (!record) return failure(id, NOTHING_TO_UNDO);
      if (id === 'power-plan' || id === 'system-protection') return failure(id, ONLY_THE_SETTINGS);
      // A value that still holds what Kestrel wrote is simply undone; only a flagged one is written over.
      return undoRecord(id, { ...record, standing: 'applied' });
    },

    usePlanAgain(): ItemOutcome {
      requireOpen();
      const record = open.get('power-plan');
      if (!record) return failure('power-plan', NOTHING_TO_APPLY_AGAIN);
      if (record.standing === 'applied') return failure('power-plan', ALREADY_SET);
      activePlan = KESTREL_PLAN;
      record.standing = 'applied';
      told('Applied Kestrel power plan again');
      return success('power-plan', 'Applied.');
    },

    removePlan(): ItemOutcome {
      requireOpen();
      const record = open.get('power-plan');
      if (!record) return failure('power-plan', NOTHING_TO_UNDO);
      if (record.standing === 'applied') return failure('power-plan', PLAN_IN_USE);
      return undoRecord('power-plan', record);
    },

    revertAll(): ItemOutcome[] {
      requireOpen();
      return [...open].reverse().map(([id, record]) => (id === 'system-protection' ? failure(id, PROTECTION_STAYS) : undoRecord(id, record)));
    },

    /** The Kestrel report: the spec's example, for the owner's PC. */
    report(): string {
      const header = `Kestrel report · Kestrel ${KESTREL_VERSION} · Windows build ${WINDOWS_BUILD} · ${now()}`;
      const active = [...open]
        .reverse()
        .filter(([, record]) => record.standing === 'applied')
        .map(([id, record]) => `  ${strings.tweaks.names[id]}: ${record.before} → ${record.after} (${record.at})`);
      const finished = history.map((line) => `  ${line}`);
      const story =
        active.length === 0 && history.length === 0
          ? ['No changes yet.']
          : ['Active', ...(active.length > 0 ? active : ['  Nothing is active.']), 'History', ...(finished.length > 0 ? finished : ['  No finished changes yet.'])];
      return [header, ...story, ...(unreadable > 0 ? [UNREADABLE_CAUSE, UNREADABLE] : [])].join('\n');
    },

    // The PC's own side: what the person doesn't do from the tab.
    /** VALORANT opens, a benchmark is armed, or the process list can't be read. */
    setGate(next: Gate): void {
      gate = next;
    },
    /** System Protection is on (`ready`), off, or decided by a policy. */
    setRestore(next: Readiness): void {
      readiness = next;
    },
    /** Lines of the journal that couldn't be read, which the view counts and the report ends by saying. */
    setUnreadable(count: number): void {
      unreadable = count;
    },
    /** Another tool changes a setting Kestrel changed. Only the five settings can be: the plan and System Protection have their own rows. */
    changeBehindItsBack(id: TweakId): void {
      const record = open.get(id);
      if (record?.standing === 'applied' && id !== 'power-plan' && id !== 'system-protection') record.standing = 'changed';
    },
    /** The person picks a plan in Power Options by hand. */
    pickPlan(name: string): void {
      activePlan = name;
      const record = open.get('power-plan');
      if (record) record.standing = name === KESTREL_PLAN ? 'applied' : 'inactive';
    },
  };
}

/** The one PC the browser preview shares between its tabs. */
export const sampleTweaks = createSampleTweaks();
