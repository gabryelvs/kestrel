import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { coalesce } from './coalesce';
import { watchUnhandledRejections } from './test-support';

/** A task the test ends by hand, how many of its runs began and how many were under way at once, and what each run was told. */
function rig() {
  const ends: Array<(failed: boolean) => void> = [];
  let running = 0;
  const seen = { started: 0, most: 0, told: [] as AbortSignal[] };
  const task = (letGo?: AbortSignal) =>
    new Promise<void>((resolve, reject) => {
      seen.started += 1;
      running += 1;
      seen.most = Math.max(seen.most, running);
      if (letGo) seen.told.push(letGo);
      ends.push((failed) => {
        running -= 1;
        if (failed) reject(new Error('The scan failed.'));
        else resolve();
      });
    });
  return { task, seen, end: (failed = false) => ends.shift()?.(failed) };
}

/** Lets every promise that can settle settle. */
const flush = async () => {
  for (let i = 0; i < 30; i += 1) await Promise.resolve();
};

describe('coalesce', () => {
  it('runs the task for a call, and answers when the run ends', async () => {
    const r = rig();
    const rescan = coalesce(r.task);
    let answered = false;
    const call = rescan().then(() => {
      answered = true;
    });
    await flush();
    expect(r.seen.started).toBe(1);
    expect(answered).toBe(false);
    r.end();
    await call;
    expect(answered).toBe(true);
  });

  it('never runs two at once: a call made during a run waits for it, then runs once more', async () => {
    const r = rig();
    const rescan = coalesce(r.task);
    const first = rescan();
    await flush();
    const second = rescan();
    await flush();
    expect(r.seen.started).toBe(1);
    r.end();
    await first;
    await flush();
    expect(r.seen.started).toBe(2);
    r.end();
    await second;
    expect(r.seen.most).toBe(1);
  });

  it('gives every call made during a run the same one more run, so at most one waits', async () => {
    const r = rig();
    const rescan = coalesce(r.task);
    const first = rescan();
    await flush();
    const others = [rescan(), rescan(), rescan()];
    await flush();
    r.end();
    await first;
    await flush();
    expect(r.seen.started).toBe(2);
    r.end();
    await Promise.all(others);
    expect(r.seen.started).toBe(2);
    expect(r.seen.most).toBe(1);
  });

  it('does not answer a call with a run that began before it: the scan must have started after the change', async () => {
    const r = rig();
    const rescan = coalesce(r.task);
    const first = rescan();
    await flush();
    let answered = false;
    const second = rescan().then(() => {
      answered = true;
    });
    r.end();
    await first;
    await flush();
    expect(answered).toBe(false);
    r.end();
    await second;
    expect(answered).toBe(true);
  });

  it('starts afresh when nothing is running', async () => {
    const r = rig();
    const rescan = coalesce(r.task);
    const first = rescan();
    r.end();
    await first;
    const second = rescan();
    await flush();
    expect(r.seen.started).toBe(2);
    r.end();
    await second;
    expect(r.seen.most).toBe(1);
  });

  it('answers when a run fails, never rejects, and still runs the one that waited', async () => {
    const r = rig();
    const rescan = coalesce(r.task);
    const first = rescan();
    await flush();
    const second = rescan();
    r.end(true);
    await expect(first).resolves.toBeUndefined();
    await flush();
    expect(r.seen.started).toBe(2);
    r.end(true);
    await expect(second).resolves.toBeUndefined();
  });

  it('answers when the task throws before it returns a promise', async () => {
    const rescan = coalesce(() => {
      throw new Error('Not even started.');
    });
    await expect(rescan()).resolves.toBeUndefined();
    await expect(rescan()).resolves.toBeUndefined();
  });

  it('keeps the one-at-a-time rule when a caller asks again the moment its run ends', async () => {
    const r = rig();
    const rescan = coalesce(r.task);
    const first = rescan();
    await flush();
    // Waiting for the run that is under way.
    const second = rescan();
    let third: Promise<void> | undefined;
    // The first caller goes on as its run ends, and asks again at once, while the one more run is starting.
    void first.then(() => {
      third = rescan();
    });
    r.end();
    await flush();
    expect(r.seen.started).toBe(2);
    expect(r.seen.most).toBe(1);
    r.end();
    await second;
    await flush();
    expect(r.seen.started).toBe(3);
    r.end();
    await third;
    expect(r.seen.most).toBe(1);
  });
});

describe('a run that never ends', () => {
  const LIMIT = 60_000;
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('is let go at the limit, answering its caller, so the next call starts a new run instead of waiting for ever', async () => {
    const r = rig();
    const rescan = coalesce(r.task, LIMIT);
    let answered = false;
    const first = rescan().then(() => {
      answered = true;
    });
    await vi.advanceTimersByTimeAsync(LIMIT - 1);
    expect(answered).toBe(false);
    expect(r.seen.started).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    await first;
    expect(answered).toBe(true);
    void rescan();
    await vi.advanceTimersByTimeAsync(0);
    expect(r.seen.started).toBe(2);
  });

  it('starts the run that waited the moment the hung one is let go, and not before', async () => {
    const r = rig();
    const rescan = coalesce(r.task, LIMIT);
    void rescan();
    await vi.advanceTimersByTimeAsync(1_000);
    let answered = false;
    const second = rescan().then(() => {
      answered = true;
    });
    await vi.advanceTimersByTimeAsync(LIMIT - 1_001);
    expect(r.seen.started).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(r.seen.started).toBe(2);
    expect(answered).toBe(false);
    // The run that waited is held to the limit too.
    await vi.advanceTimersByTimeAsync(LIMIT);
    await second;
    expect(answered).toBe(true);
  });

  it('never wedges: every call still gets its run, however many in a row hang', async () => {
    const r = rig();
    const rescan = coalesce(r.task, LIMIT);
    for (let i = 1; i <= 4; i += 1) {
      const call = rescan();
      await vi.advanceTimersByTimeAsync(LIMIT);
      await call;
      expect(r.seen.started).toBe(i);
    }
  });

  it('tells a run that was let go, so what it finds later can be left out, and tells no run that ended in time', async () => {
    const r = rig();
    const rescan = coalesce(r.task, LIMIT);
    void rescan();
    await vi.advanceTimersByTimeAsync(LIMIT);
    expect(r.seen.told[0].aborted).toBe(true);
    const second = rescan();
    await vi.advanceTimersByTimeAsync(0);
    // The hung run lands at last, and the second ends in time.
    r.end();
    r.end();
    await second;
    expect(r.seen.told[0].aborted).toBe(true);
    expect(r.seen.told[1].aborted).toBe(false);
  });

  it('clears the timer of a run that ends in time, and has none at all without a limit', async () => {
    const r = rig();
    const bounded = coalesce(r.task, LIMIT);
    const call = bounded();
    expect(vi.getTimerCount()).toBe(1);
    r.end();
    await call;
    expect(vi.getTimerCount()).toBe(0);
    const unbounded = coalesce(r.task);
    const other = unbounded();
    expect(vi.getTimerCount()).toBe(0);
    r.end();
    await other;
  });
});

describe('a run that was let go and fails later', () => {
  it('is no unhandled rejection: nobody is waiting for it any more', async () => {
    const watch = watchUnhandledRejections();
    try {
      const r = rig();
      const rescan = coalesce(r.task, 10);
      await rescan();
      r.end(true);
      await new Promise((resolve) => setTimeout(resolve, 30));
      expect(watch.seen).toEqual([]);
    } finally {
      watch.stop();
    }
  });
});
