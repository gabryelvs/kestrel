import { strings } from '../strings';
import type { BenchState, Flag, MetricId, RunMetrics, ValorantSettings, Verdict } from './types';

/** The order metrics appear in tiles and comparisons. */
export const METRIC_ORDER: MetricId[] = ['avg-fps', 'low1-fps', 'low01-fps', 'stutters-per-min', 'pc-latency-ms'];

export function metricValue(m: RunMetrics, id: MetricId): number | null {
  switch (id) {
    case 'avg-fps': return m.avgFps;
    case 'low1-fps': return m.low1Fps;
    case 'low01-fps': return m.low01Fps;
    case 'stutters-per-min': return m.stuttersPerMin;
    case 'pc-latency-ms': return m.pcLatencyMs;
  }
}

export function metricText(id: MetricId, value: number | null): string {
  if (value === null) return strings.bench.notReported;
  if (id === 'pc-latency-ms') return strings.bench.milliseconds(value.toFixed(1));
  if (id === 'stutters-per-min') return strings.bench.perMinute(value.toFixed(1));
  return Math.round(value).toString();
}

/** +4.9% / -1.0% / 0.0%. The sign follows the rounded digits, so a value that rounds to zero shows no sign. */
export const pct = (n: number): string => {
  const text = n.toFixed(1);
  if (Number(text) === 0) return '0.0%';
  return `${text.startsWith('-') ? '' : '+'}${text}%`;
};

/** A spread ratio (0.0317) as a percentage: "3.2%". */
export const spreadText = (ratio: number): string => `${(ratio * 100).toFixed(1)}%`;

export const verdictTone = (v: Verdict): 'good' | 'bad' | 'neutral' => (v === 'real-gain' ? 'good' : v === 'real-loss' ? 'bad' : 'neutral');
export const verdictText = (v: Verdict): string => strings.bench.verdict[v];

export function flagText(flag: Flag): string {
  return flag.kind === 'busy-background' ? strings.bench.flags.busy(flag.name) : strings.bench.flags[flag.kind];
}

export const passLabel = (pass: number): string => strings.bench.passLabel(pass);

/**
 * Whether the Start run button is offered: a pass is waiting, the game runs and no countdown is under way. It's offered
 * in both start modes, not only when F10 couldn't be claimed, because a game can swallow F10 even though Kestrel holds it.
 */
export const canStartRun = (s: BenchState): boolean => s.phase === 'waiting' && s.countdown === null && s.gameRunning;

/** One sentence telling the player what happens now or what to do next. */
export function sessionHeadline(s: BenchState): string {
  const b = strings.bench;
  switch (s.phase) {
    case 'waiting':
      if (s.countdown !== null) return b.countdown(s.countdown);
      if (!s.gameRunning) return b.waitingForGame;
      return s.startMode === 'hotkey' ? b.pressKey(s.startKey, b.begins(s.pass)) : b.pressButton;
    case 'capturing': {
      // The controller clears secondsLeft when PresentMon stops, before the results are read.
      if (s.secondsLeft === null) return b.reading;
      const elapsed = s.captureSeconds - s.secondsLeft;
      return elapsed < s.switchAtSeconds ? b.walk(passLabel(s.pass)) : b.shoot(passLabel(s.pass));
    }
    case 'review':
      return b.review(passLabel(s.pass));
    case 'done':
      return b.done;
    case 'failed':
      return b.failed;
    case 'cancelled':
      return b.cancelled;
  }
}

/** SVG points for frame times in a 600 × 100 box: x across, y scaled so the slowest frame touches the top. */
export function graphShape(graph: number[]): { points: string; maxMs: number; medianMs: number } {
  if (graph.length === 0) return { points: '', maxMs: 0, medianMs: 0 };
  const values = graph.length === 1 ? [graph[0], graph[0]] : graph;
  const maxMs = Math.max(...values);
  const sorted = [...values].sort((a, b) => a - b);
  const medianMs = sorted[Math.max(0, Math.ceil(sorted.length / 2) - 1)];
  const round = (n: number) => Math.round(n * 100) / 100;
  const points = values
    .map((ms, i) => `${round((i * 600) / (values.length - 1))},${round(maxMs > 0 ? 100 - (ms / maxMs) * 100 : 100)}`)
    .join(' ');
  return { points, maxMs, medianMs };
}

/** "2026-10-12T21:30:05Z" → local "2026-10-12 21:30". */
export function dateText(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const two = (n: number) => n.toString().padStart(2, '0');
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

/** VALORANT's recorded settings in one line: "1920×1080 · V-Sync off · No FPS limit". */
export function settingsText(s: ValorantSettings | null): string {
  const g = strings.bench.gameSettings;
  if (!s) return g.notFound;
  const parts: string[] = [];
  if (s.resolution) parts.push(`${s.resolution[0]}×${s.resolution[1]}`);
  if (s.vsync !== null) parts.push(g.vsync(s.vsync));
  if (s.frameRateLimit !== null) parts.push(g.limit(s.frameRateLimit));
  return parts.length > 0 ? parts.join(' · ') : '—';
}
