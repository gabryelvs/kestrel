import { graphShape } from '../../api/bench';
import { strings } from '../../strings';

/** The middle run's frame times: spikes are hitches. */
export function FrameGraph({ graph }: { graph: number[] }) {
  const { points, maxMs, medianMs } = graphShape(graph);
  if (!points) return null;
  const medianY = maxMs > 0 ? 100 - (medianMs / maxMs) * 100 : 100;
  return (
    <figure className="frame-graph">
      <figcaption className="label">{strings.bench.graph}</figcaption>
      <div className="frame-graph-box">
        <span className="num muted small axis-top">{maxMs.toFixed(1)} ms</span>
        <svg viewBox="0 0 600 100" preserveAspectRatio="none" role="img" aria-label={strings.bench.graph}>
          <line x1={0} x2={600} y1={medianY} y2={medianY} stroke="var(--line-strong)" strokeDasharray="4 4" vectorEffect="non-scaling-stroke" />
          <polyline points={points} fill="none" stroke="var(--accent)" strokeWidth={1.25} vectorEffect="non-scaling-stroke" />
        </svg>
        <span className="num muted small axis-bottom">0 ms</span>
      </div>
      <p className="muted small">{strings.bench.graphAxis}</p>
    </figure>
  );
}
