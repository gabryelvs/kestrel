import { describe, expect, it } from 'vitest';
import { coalesce } from './coalesce';

/** A task the test ends by hand, and a count of how many of its runs began and how many were under way at once. */
function rig() {
  const ends: Array<(failed: boolean) => void> = [];
  let running = 0;
  const seen = { started: 0, most: 0 };
  const task = () =>
    new Promise<void>((resolve, reject) => {
      seen.started += 1;
      running += 1;
      seen.most = Math.max(seen.most, running);
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
