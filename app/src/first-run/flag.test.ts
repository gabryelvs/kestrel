import { afterEach, describe, expect, it, vi } from 'vitest';
import { askForPlanNote, firstRunDone, markFirstRunDone, planNoteSeen, planNoteWanted } from './flag';

afterEach(() => vi.unstubAllGlobals());

describe('first-run flag', () => {
  it('is false until marked, then true', () => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
    expect(firstRunDone()).toBe(false);
    markFirstRunDone();
    expect(firstRunDone()).toBe(true);
  });

  it('treats unavailable storage as a first run and never throws', () => {
    vi.stubGlobal('localStorage', { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('blocked'); } });
    expect(firstRunDone()).toBe(false);
    expect(() => markFirstRunDone()).not.toThrow();
  });
});

/** A localStorage that remembers, with its contents in hand so a test can look at what was written. */
function memory(): Map<string, string> {
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
  return store;
}
const blocked = () => {
  throw new Error('blocked');
};

describe('plan note flag', () => {
  it('wants the note only after askForPlanNote, until it is seen', () => {
    memory();
    expect(planNoteWanted()).toBe(false);
    askForPlanNote();
    expect(planNoteWanted()).toBe(true);
    planNoteSeen();
    expect(planNoteWanted()).toBe(false);
  });

  it('is stored under kestrel.planNote, and leaves nothing behind once seen', () => {
    const store = memory();
    askForPlanNote();
    expect([...store]).toEqual([['kestrel.planNote', '1']]);
    planNoteSeen();
    expect([...store]).toEqual([]);
  });

  it('wants nothing from an empty store, which is every person whose first run was done before this existed', () => {
    const store = memory();
    // Their first run is marked and nothing else is: only the first-run buttons ask for the note.
    markFirstRunDone();
    expect(firstRunDone()).toBe(true);
    expect(planNoteWanted()).toBe(false);
    expect([...store.keys()]).toEqual(['kestrel.firstRunDone']);
  });

  it('is its own flag: asking for the note or seeing it never touches the first-run one', () => {
    memory();
    askForPlanNote();
    expect(firstRunDone()).toBe(false);
    markFirstRunDone();
    planNoteSeen();
    expect(firstRunDone()).toBe(true);
    askForPlanNote();
    expect(planNoteWanted()).toBe(true);
  });

  it('wants the note only for the value it wrote', () => {
    const store = memory();
    for (const other of ['', '0', 'true', 'yes', '2', 'null']) {
      store.set('kestrel.planNote', other);
      expect(planNoteWanted(), JSON.stringify(other)).toBe(false);
    }
  });

  it('stays asked for after any number of looks, until it is seen', () => {
    memory();
    askForPlanNote();
    askForPlanNote();
    expect(planNoteWanted()).toBe(true);
    expect(planNoteWanted()).toBe(true);
    planNoteSeen();
    planNoteSeen();
    expect(planNoteWanted()).toBe(false);
  });

  it('wants nothing, and never throws, when the store throws', () => {
    vi.stubGlobal('localStorage', { getItem: blocked, setItem: blocked, removeItem: blocked });
    expect(planNoteWanted()).toBe(false);
    expect(() => askForPlanNote()).not.toThrow();
    expect(() => planNoteSeen()).not.toThrow();
    expect(planNoteWanted()).toBe(false);
  });

  it('asks for no note when it cannot be remembered, so nothing is shown that could never be dismissed', () => {
    // Readable but not writable: the request is lost, and no note is the safe side of that.
    vi.stubGlobal('localStorage', { getItem: () => null, setItem: blocked, removeItem: blocked });
    expect(() => askForPlanNote()).not.toThrow();
    expect(planNoteWanted()).toBe(false);
  });

  it('wants nothing, and never throws, when there is no store at all', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(planNoteWanted()).toBe(false);
    expect(() => askForPlanNote()).not.toThrow();
    expect(() => planNoteSeen()).not.toThrow();
  });
});
