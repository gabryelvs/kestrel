import { afterEach, describe, expect, it, vi } from 'vitest';
import { getCpuClocks, onWindowFocus, tweaksApply, tweaksReport, tweaksView } from './client';
import { sampleTweaks } from './sample-tweaks';
import { watchUnhandledRejections } from './test-support';

/** What Tauri's window answers `onFocusChanged` with, held back until the test says so. */
const tauri = vi.hoisted(() => ({
  handler: null as ((event: { payload: boolean }) => void) | null,
  /** Hands out the way to stop listening, or says listening was refused. */
  answer: null as { resolve: (stop: () => void) => void; reject: (why: unknown) => void } | null,
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onFocusChanged: (handler: (event: { payload: boolean }) => void) =>
      new Promise((resolve, reject) => {
        tauri.handler = handler;
        tauri.answer = { resolve, reject };
      }),
  }),
}));

describe('the browser preview', () => {
  it('makes up 8 cores around 4.8 GHz that move on every call', async () => {
    const first = await getCpuClocks();
    const second = await getCpuClocks();
    for (const clocks of [first, second]) {
      expect(clocks.map((c) => c.core)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
      for (const c of clocks) {
        expect(c.kind).toBeNull();
        expect(c.mhz).toBeGreaterThanOrEqual(4500);
        expect(c.mhz).toBeLessThanOrEqual(5100);
      }
    }
    expect(second.map((c) => c.mhz)).not.toEqual(first.map((c) => c.mhz));
  });
});

describe('the tweaks in the browser preview', () => {
  it('answer like the commands: with a value, or a rejection with the text a closed gate refuses with', async () => {
    const view = await tweaksView();
    expect(view.rows.map((row) => row.id)[0]).toBe('power-plan');
    expect(await tweaksReport()).toContain('No changes yet.');

    sampleTweaks.setGate('game-running');
    // Tauri rejects with the text itself, so the page reads either the same way.
    await expect(tweaksApply(['power-plan'])).rejects.toBe('Close VALORANT first.');
    sampleTweaks.setGate('open');
    expect(sampleTweaks.view().rows[0].state).toBe('available');
  });
});

describe('coming back to the window', () => {
  /** Lets the dynamic import and the promises behind it finish. */
  const settled = async () => {
    for (let i = 0; i < 5; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  };

  const inBrowser = () => {
    const window = new EventTarget();
    vi.stubGlobal('window', window);
    return window;
  };

  const inKestrel = () => {
    const window = new EventTarget();
    vi.stubGlobal('window', Object.assign(window, { __TAURI_INTERNALS__: {} }));
    tauri.handler = null;
    tauri.answer = null;
    return window;
  };

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('is the window’s focus event in the browser preview, until it is stopped, and asks nothing of Tauri', async () => {
    const window = inBrowser();
    tauri.answer = null;
    let reads = 0;
    const stop = onWindowFocus(() => (reads += 1));
    window.dispatchEvent(new Event('focus'));
    expect(reads).toBe(1);
    await settled();
    expect(tauri.answer).toBeNull();
    stop();
    window.dispatchEvent(new Event('focus'));
    expect(reads).toBe(1);
  });

  it('is Tauri’s focus event inside Kestrel too, which WebView2 may not repeat on the DOM, and only for coming back', async () => {
    const window = inKestrel();
    let reads = 0;
    const stop = onWindowFocus(() => (reads += 1));
    await settled();
    tauri.handler?.({ payload: true });
    expect(reads).toBe(1);
    tauri.handler?.({ payload: false });
    expect(reads).toBe(1);
    window.dispatchEvent(new Event('focus'));
    expect(reads).toBe(2);
    stop();
  });

  it('stops listening when it is stopped', async () => {
    inKestrel();
    const stop = onWindowFocus(() => undefined);
    await settled();
    const unlisten = vi.fn(async () => undefined);
    tauri.answer?.resolve(unlisten);
    await settled();
    expect(unlisten).not.toHaveBeenCalled();
    stop();
    expect(unlisten).toHaveBeenCalledTimes(1);
    // Only once, however often it is told.
    stop();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('stops listening as soon as it has started, when the page was gone before that', async () => {
    inKestrel();
    const stop = onWindowFocus(() => undefined);
    await settled();
    stop();
    const unlisten = vi.fn(async () => undefined);
    tauri.answer?.resolve(unlisten);
    await settled();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('calls nothing once it is stopped, whatever Tauri says on its way out', async () => {
    inKestrel();
    let reads = 0;
    const stop = onWindowFocus(() => (reads += 1));
    await settled();
    stop();
    tauri.handler?.({ payload: true });
    expect(reads).toBe(0);
  });

  it('leaves nothing unhandled when Tauri won’t listen, or won’t stop, and the window’s own event still works', async () => {
    const watch = watchUnhandledRejections();
    try {
      const window = inKestrel();
      let reads = 0;
      const refused = onWindowFocus(() => (reads += 1));
      await settled();
      tauri.answer?.reject('core:event:allow-listen not allowed');
      await settled();
      window.dispatchEvent(new Event('focus'));
      expect(reads).toBe(1);
      refused();

      inKestrel();
      const stop = onWindowFocus(() => undefined);
      await settled();
      tauri.answer?.resolve(() => Promise.reject(new Error('the window is gone')));
      await settled();
      stop();
      await settled();
      expect(watch.seen).toEqual([]);
    } finally {
      watch.stop();
    }
  });
});
