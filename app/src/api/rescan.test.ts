import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { quietRescan, SCAN_LIMIT_MS, SCAN_WAIT_MS } from './rescan';

/** A scan the test ends by hand, and what the app was shown. */
function rig() {
  const lands: Array<(found: string) => void> = [];
  const shown: string[] = [];
  const scan = () => new Promise<string>((resolve) => lands.push(resolve));
  return { scan, shown, lands, show: (found: string) => shown.push(found) };
}

describe('the bounds on a scan after a change', () => {
  it('are 20 seconds for the page to wait and 60 for the app to let a scan go, so the page gives up first', () => {
    expect(SCAN_WAIT_MS).toBe(20_000);
    expect(SCAN_LIMIT_MS).toBe(60_000);
    expect(SCAN_WAIT_MS).toBeLessThan(SCAN_LIMIT_MS);
  });
});

describe('quietRescan', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows what the scan found, and answers when it did', async () => {
    const r = rig();
    const rescan = quietRescan(r.scan, r.show);
    let answered = false;
    const call = rescan().then(() => {
      answered = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(answered).toBe(false);
    r.lands[0]('picture 1');
    await call;
    expect(r.shown).toEqual(['picture 1']);
  });

  it('runs one scan at a time, and one more for the changes made meanwhile', async () => {
    const r = rig();
    const rescan = quietRescan(r.scan, r.show);
    const first = rescan();
    await vi.advanceTimersByTimeAsync(0);
    const others = [rescan(), rescan()];
    await vi.advanceTimersByTimeAsync(0);
    expect(r.lands).toHaveLength(1);
    r.lands[0]('picture 1');
    await first;
    await vi.advanceTimersByTimeAsync(0);
    expect(r.lands).toHaveLength(2);
    r.lands[1]('picture 2');
    await Promise.all(others);
    expect(r.shown).toEqual(['picture 1', 'picture 2']);
  });

  it('lets a scan that never settles go at the limit, so the next call scans again and shows that', async () => {
    const r = rig();
    const rescan = quietRescan(r.scan, r.show);
    const first = rescan();
    await vi.advanceTimersByTimeAsync(SCAN_LIMIT_MS - 1);
    expect(r.lands).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    await first;
    // A later call is not stuck behind it.
    const second = rescan();
    await vi.advanceTimersByTimeAsync(0);
    expect(r.lands).toHaveLength(2);
    r.lands[1]('picture 2');
    await second;
    expect(r.shown).toEqual(['picture 2']);
  });

  it('leaves out a scan that lands after it was let go, since it is older than the one that came after it', async () => {
    const r = rig();
    const rescan = quietRescan(r.scan, r.show);
    const first = rescan();
    await vi.advanceTimersByTimeAsync(SCAN_LIMIT_MS);
    await first;
    const second = rescan();
    await vi.advanceTimersByTimeAsync(0);
    r.lands[1]('picture 2');
    await second;
    // The hung one lands at last.
    r.lands[0]('picture 1, much older');
    await vi.advanceTimersByTimeAsync(0);
    expect(r.shown).toEqual(['picture 2']);
  });

  it('answers when a scan fails, and shows nothing for it', async () => {
    const shown: unknown[] = [];
    const rescan = quietRescan(
      () => Promise.reject(new Error('The scan failed.')),
      (found) => shown.push(found),
    );
    await expect(rescan()).resolves.toBeUndefined();
    expect(shown).toEqual([]);
  });
});
