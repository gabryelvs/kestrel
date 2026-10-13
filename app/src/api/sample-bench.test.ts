import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { benchCopyResults } from './client';
import { fakeSession, sampleDelete, sampleDetail, sampleList } from './sample-bench';

/** Runs the fake session from start to a saved result, skipping the real 90-second captures. Returns the saved id. */
function runFakeSession(): string {
  fakeSession.start();
  for (let pass = 0; pass < 4; pass += 1) {
    fakeSession.startRun();
    for (let tick = 0; tick < 100 && fakeSession.state()?.phase === 'capturing'; tick += 1) vi.advanceTimersByTime(100);
  }
  const resultId = fakeSession.state()?.resultId;
  fakeSession.dismiss();
  if (!resultId) throw new Error('The fake session saved nothing.');
  return resultId;
}

beforeEach(() => {
  // The fake session times its captures with window.setInterval, and this test runs in node, so window is stubbed.
  vi.useFakeTimers();
  vi.stubGlobal('window', globalThis);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it('gives each fake save a new id and a complete benchmark, and rejects copies of unknown ids', async () => {
  const used = ['sample-1', 'sample-2'];

  // Delete sample-1, then save: the new id must not repeat any id used so far.
  sampleDelete('sample-1');
  const first = runFakeSession();
  expect(used).not.toContain(first);
  used.push(first);
  const ids = sampleList().map((b) => b.id);
  expect(new Set(ids).size).toBe(ids.length);
  expect(ids).toContain(first);

  // Delete everything, then save: the list holds one benchmark with a fresh id and its full data.
  for (const id of ids) sampleDelete(id);
  expect(sampleList()).toEqual([]);
  const second = runFakeSession();
  expect(used).not.toContain(second);
  expect(sampleList().map((b) => b.id)).toEqual([second]);
  const saved = sampleDetail(second).benchmark;
  expect(saved.runs).toHaveLength(3);
  expect(saved.result.metrics.avgFps).toBeGreaterThan(0);
  expect(saved.context.gpu).toBeTruthy();

  // Copying an unknown benchmark rejects, as the Tauri command does.
  await expect(benchCopyResults('no-such-id')).rejects.toThrow();
});
