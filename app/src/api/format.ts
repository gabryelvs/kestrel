import { strings } from '../strings';
import type { LoadSample, SystemSnapshot } from './types';

export const filledSegments = (score: number): number => Math.max(0, Math.min(20, Math.round(score / 5)));

export const osLabel = (s: SystemSnapshot): string =>
  `${s.os.caption.replace(/^Microsoft\s+/, '')} ${s.os.displayVersion ?? ''}`.trim();

/** SVG polyline points for 0–100 values, newest at the right edge of a `window`-wide graph. */
export function toPoints(values: number[], window: number): string {
  const offset = window - values.length;
  return values.map((v, i) => `${offset + i},${100 - Math.max(0, Math.min(100, v))}`).join(' ');
}

export function loadSummary(s: LoadSample): string {
  const parts = [`${strings.hardware.cpu} ${Math.round(s.cpuPercent)}%`];
  if (s.gpuPercent !== null) parts.push(`${strings.hardware.gpu} ${Math.round(s.gpuPercent)}%`);
  if (s.gpuTempC !== null) parts.push(`${Math.round(s.gpuTempC)}°C`);
  if (s.gpuPercent === null && s.gpuTempC === null) parts.push(`${strings.hardware.ram} ${Math.round(s.ramPercent)}%`);
  return parts.join(' · ');
}
