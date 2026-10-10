import { useEffect, useState } from 'react';
import { getLoad } from '../api/client';
import { loadSummary, toPoints } from '../api/format';
import type { LoadSample } from '../api/types';
import { strings } from '../strings';

const WINDOW = 60;

/** Polls live load once a second (skipping while the window is hidden) and draws the last minute. */
export function LoadGraph() {
  const [samples, setSamples] = useState<LoadSample[]>([]);

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      if (document.hidden) return;
      try {
        const sample = await getLoad();
        if (alive) setSamples((previous) => [...previous.slice(-(WINDOW - 1)), sample]);
      } catch {
        // Keep the last values; the next tick tries again.
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  const last: LoadSample | undefined = samples[samples.length - 1];
  const hasGpu = samples.some((s) => s.gpuPercent !== null);
  return (
    <section className="panel">
      <div className="graph-head">
        <span className="label">{strings.home.liveLoad}</span>
        <span className="num muted small">{last ? loadSummary(last) : '…'}</span>
      </div>
      <svg className="graph" viewBox={`0 0 ${WINDOW - 1} 100`} preserveAspectRatio="none" aria-hidden="true">
        {[25, 50, 75].map((y) => (
          <line key={y} x1={0} x2={WINDOW - 1} y1={y} y2={y} stroke="var(--line)" strokeWidth={1} vectorEffect="non-scaling-stroke" />
        ))}
        {hasGpu && (
          <polyline points={toPoints(samples.map((s) => s.gpuPercent ?? 0), WINDOW)} fill="none" stroke="var(--muted)" strokeWidth={1.25} vectorEffect="non-scaling-stroke" />
        )}
        <polyline points={toPoints(samples.map((s) => s.cpuPercent), WINDOW)} fill="none" stroke="var(--accent)" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
      </svg>
    </section>
  );
}
