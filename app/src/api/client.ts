import mock from './mock-report.json';
import type { LoadSample, Report } from './types';

const sample = mock as unknown as Report;

/** True inside the Tauri window; false in a plain browser (UI development on the Mac). */
export const inTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(command: string): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command);
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
