import mock from './mock-report.json';
import { fakeSession, sampleDelete, sampleDetail, sampleList, sampleProfile, sampleRename, sampleSetBaseline } from './sample-bench';
import type { BenchDetail, BenchState, BenchmarkSummary, GameProfile, LoadSample, Report } from './types';

const sample = mock as unknown as Report;

/** True inside the Tauri window; false in a plain browser (UI development on the Mac). */
export const inTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

const later = <T,>(value: T, ms: number) => new Promise<T>((resolve) => setTimeout(() => resolve(value), ms));

export const getReport = (): Promise<Report> => (inTauri() ? call<Report>('get_report') : later(sample, 600));
export const rescan = (): Promise<Report> => (inTauri() ? call<Report>('rescan') : later(sample, 1800));
export const getDiagnostics = (): Promise<string> => (inTauri() ? call<string>('get_diagnostics') : Promise.resolve(JSON.stringify(sample, null, 2)));
export const isElevated = (): Promise<boolean> => (inTauri() ? call<boolean>('is_elevated') : Promise.resolve(false));

let tick = 0;
export const getLoad = (): Promise<LoadSample> => {
  if (inTauri()) return call<LoadSample>('get_load');
  tick += 1;
  const wave = (period: number, base: number, swing: number) => Math.round((base + swing * Math.sin(tick / period) + Math.random() * 3) * 10) / 10;
  return Promise.resolve({ cpuPercent: wave(3, 9, 5), ramPercent: 41, gpuPercent: wave(5, 4, 2), gpuTempC: 44 });
};

export async function openLink(url: string): Promise<void> {
  if (!inTauri()) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  const { openUrl } = await import('@tauri-apps/plugin-opener');
  await openUrl(url);
}

// Stage 2: the benchmark. In the browser preview these use sample data and a fake session.
export const benchProfiles = (): Promise<GameProfile[]> => (inTauri() ? call('bench_profiles') : Promise.resolve([sampleProfile]));
export const benchState = (): Promise<BenchState | null> => (inTauri() ? call('bench_state') : Promise.resolve(fakeSession.state()));
export const benchStart = (profile: string): Promise<BenchState> => (inTauri() ? call('bench_start', { profile }) : later(fakeSession.start(), 300));
export const benchStartRun = (): Promise<void> => (inTauri() ? call('bench_start_run') : Promise.resolve(fakeSession.startRun()));
export const benchCancel = (): Promise<void> => (inTauri() ? call('bench_cancel') : Promise.resolve(fakeSession.cancel()));
export const benchRedo = (): Promise<void> => (inTauri() ? call('bench_redo') : Promise.resolve());
export const benchKeep = (): Promise<void> => (inTauri() ? call('bench_keep') : Promise.resolve());
export const benchDismiss = (): Promise<void> => (inTauri() ? call('bench_dismiss') : Promise.resolve(fakeSession.dismiss()));
export const benchList = (): Promise<BenchmarkSummary[]> => (inTauri() ? call('bench_list') : Promise.resolve(sampleList()));
export const benchGet = (id: string): Promise<BenchDetail> => (inTauri() ? call('bench_get', { id }) : Promise.resolve().then(() => sampleDetail(id)));
export const benchRename = (id: string, name: string): Promise<void> => (inTauri() ? call('bench_rename', { id, name }) : Promise.resolve(sampleRename(id, name)));
export const benchDelete = (id: string): Promise<void> => (inTauri() ? call('bench_delete', { id }) : Promise.resolve(sampleDelete(id)));
export const benchSetBaseline = (id: string): Promise<void> => (inTauri() ? call('bench_set_baseline', { id }) : Promise.resolve(sampleSetBaseline(id)));
export const benchCopyResults = (id: string): Promise<string> =>
  inTauri() ? call('bench_copy_results', { id }) : Promise.resolve().then(() => `Kestrel benchmark (sample): ${sampleDetail(id).benchmark.name}`);

/** Calls `handler` whenever the benchmark session changes; resolves to an unsubscribe function. */
export async function onBenchState(handler: (state: BenchState | null) => void): Promise<() => void> {
  if (!inTauri()) return fakeSession.subscribe(handler);
  const { listen } = await import('@tauri-apps/api/event');
  return listen<BenchState | null>('bench-state', (event) => handler(event.payload));
}
