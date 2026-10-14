// Sample benchmarks and a fake session for the browser preview (UI work on the Mac). Never used inside Kestrel.
import type { BenchDetail, BenchState, Benchmark, BenchmarkSummary, GameProfile, RunRecord } from './types';

export const sampleProfile: GameProfile = {
  id: 'valorant', name: 'VALORANT', processName: 'VALORANT-Win64-Shipping.exe',
  route: { version: 1, captureSeconds: 90, switchAtSeconds: 30 },
};

const run = (avg: number, low1: number, low01: number, latency: number): RunRecord => ({
  metrics: { frames: Math.round(avg * 90), durationS: 90, avgFps: avg, low1Fps: low1, low01Fps: low01, stuttersPerMin: 0.6, pcLatencyMs: latency, gpuBusyRatio: 0.93, bottleneck: 'gpu' },
  flags: [],
  kept: false,
});

const context = {
  powerPlan: 'EXM Free Power Plan V8', ramMts: 3600, gpu: 'NVIDIA GeForce RTX 3060 Ti', gpuDriver: '610.74',
  windowsBuild: 26200, displayHz: 360, gameSettings: null,
  gpuPnpDeviceId: 'PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&2B0D3A0E&0&0008', gpuLink: { gen: 4, width: 16 },
};

const benchmarks: Benchmark[] = [
  {
    id: 'sample-1', name: 'Starting point', createdAt: '2026-10-12T21:30:05Z', profileId: 'valorant', routeVersion: 1,
    kestrelVersion: '0.2.0', presentmonVersion: '2.6.0',
    runs: [run(405, 284, 197, 10.1), run(410, 288, 201, 9.8), run(418, 291, 206, 9.6)],
    result: { metrics: run(410, 288, 201, 9.8).metrics, spread: { avgFps: 0.0317, low1Fps: 0.0243, low01Fps: 0.0448 } },
    context,
  },
  {
    id: 'sample-2', name: 'After Essential tweaks (sample)', createdAt: '2026-10-14T19:05:00Z', profileId: 'valorant', routeVersion: 1,
    kestrelVersion: '0.2.0', presentmonVersion: '2.6.0',
    runs: [run(428, 301, 208, 9.2), run(433, 305, 214, 9.1), run(440, 309, 211, 8.9)],
    result: { metrics: run(433, 305, 214, 9.1).metrics, spread: { avgFps: 0.0277, low1Fps: 0.0262, low01Fps: 0.028 } },
    context,
  },
];
let baselineId = 'sample-1';

// Every fake save copies this snapshot of the first sample, taken at load, so deleting samples never changes the copy.
const template: Benchmark = structuredClone(benchmarks[0]);
// Fake saves count up from the highest sample number, so an id is never reused, even after a delete.
let lastSampleNumber = Math.max(...benchmarks.map((b) => Number(b.id.replace('sample-', ''))));

const graph = Array.from({ length: 600 }, (_, i) => (i % 97 === 0 ? 6.5 : 2.3 + ((i * 37) % 11) / 30));

export function sampleList(): BenchmarkSummary[] {
  return [...benchmarks].reverse().map((b) => ({
    id: b.id, name: b.name, createdAt: b.createdAt, profileId: b.profileId, routeVersion: b.routeVersion, result: b.result,
    isBaseline: b.id === baselineId, flaggedRunsKept: b.runs.filter((r) => r.kept).length,
  }));
}

export function sampleDetail(id: string): BenchDetail {
  const benchmark = benchmarks.find((b) => b.id === id);
  if (!benchmark) throw new Error('That benchmark doesn’t exist.');
  const baseline = benchmarks.find((b) => b.id === baselineId && b.id !== id) ?? null;
  const delta = (a: number, b: number) => ((b - a) / a) * 100;
  // Fixed verdicts for the sample, except that a ~0% change (the fake session saves a copy of the starting point) is noise.
  const comparison = baseline && {
    baselineId: baseline.id, currentId: id, settingsDiffer: [], routeMismatch: false,
    metrics: [
      { metric: 'avg-fps' as const, baseline: baseline.result.metrics.avgFps, current: benchmark.result.metrics.avgFps, deltaPct: delta(baseline.result.metrics.avgFps, benchmark.result.metrics.avgFps), verdict: 'real-gain' as const },
      { metric: 'low1-fps' as const, baseline: baseline.result.metrics.low1Fps, current: benchmark.result.metrics.low1Fps, deltaPct: delta(baseline.result.metrics.low1Fps, benchmark.result.metrics.low1Fps), verdict: 'real-gain' as const },
      { metric: 'low01-fps' as const, baseline: baseline.result.metrics.low01Fps, current: benchmark.result.metrics.low01Fps, deltaPct: delta(baseline.result.metrics.low01Fps, benchmark.result.metrics.low01Fps), verdict: 'noise' as const },
    ].map((d) => (Math.abs(d.deltaPct) < 0.05 ? { ...d, verdict: 'noise' as const } : d)),
  };
  return { benchmark, graph, comparison, baselineName: baseline?.name ?? null };
}

export function sampleRename(id: string, name: string): void {
  const b = benchmarks.find((x) => x.id === id);
  if (b && name.trim()) b.name = name.trim().slice(0, 60);
}

export function sampleDelete(id: string): void {
  const at = benchmarks.findIndex((b) => b.id === id);
  if (at >= 0) benchmarks.splice(at, 1);
  if (baselineId === id) baselineId = benchmarks[0]?.id ?? '';
}

export function sampleSetBaseline(id: string): void {
  baselineId = id;
}

// A fake session: each capture takes 3 seconds instead of 90.
let state: BenchState | null = null;
const listeners = new Set<(s: BenchState | null) => void>();
const emit = () => listeners.forEach((l) => l(state));
let timer: number | undefined;

export const fakeSession = {
  state: (): BenchState | null => state,
  subscribe(handler: (s: BenchState | null) => void): () => void {
    listeners.add(handler);
    return () => listeners.delete(handler);
  },
  start(): BenchState {
    state = {
      phase: 'waiting', profileId: 'valorant', pass: 0, gameRunning: true, runs: [null, null, null], resultId: null,
      startMode: 'countdown', startKey: 'F10', captureSeconds: 90, switchAtSeconds: 30, secondsLeft: null, countdown: null,
    };
    emit();
    return state;
  },
  startRun(): void {
    if (!state || state.phase !== 'waiting') return;
    const pass = state.pass;
    state = { ...state, phase: 'capturing', secondsLeft: 90 };
    emit();
    timer = window.setInterval(() => {
      if (!state || state.phase !== 'capturing') return window.clearInterval(timer);
      const left = Math.max(0, (state.secondsLeft ?? 90) - 3);
      if (left > 0) {
        state = { ...state, secondsLeft: left };
      } else {
        window.clearInterval(timer);
        const runs = [...state.runs];
        if (pass > 0) runs[pass - 1] = run(410 + pass, 288, 201, 9.8);
        if (pass < 3) {
          state = { ...state, phase: 'waiting', pass: pass + 1, runs, secondsLeft: null };
        } else {
          lastSampleNumber += 1;
          const id = `sample-${lastSampleNumber}`;
          benchmarks.push({ ...structuredClone(template), id, name: `Benchmark ${new Date().toISOString().slice(0, 16).replace('T', ' ')}`, createdAt: new Date().toISOString() });
          state = { ...state, phase: 'done', runs, secondsLeft: null, resultId: id };
        }
      }
      emit();
    }, 100);
  },
  cancel(): void {
    window.clearInterval(timer);
    if (state && state.phase !== 'done') state = { ...state, phase: 'cancelled', secondsLeft: null };
    emit();
  },
  dismiss(): void {
    state = null;
    emit();
  },
};
