import { dateText, metricText } from '../../api/bench';
import type { BenchmarkSummary } from '../../api/types';
import { strings } from '../../strings';

export function ResultsList({ items, selected, onSelect }: { items: BenchmarkSummary[]; selected: string | null; onSelect: (id: string) => void }) {
  const b = strings.bench;
  return (
    <section className="panel results">
      <div className="label">{b.results}</div>
      {items.length === 0 && <p className="muted small">{b.none}</p>}
      <ul className="results-list">
        {items.map((item) => (
          <li key={item.id}>
            <button className={`result-row${item.id === selected ? ' selected' : ''}`} aria-current={item.id === selected ? 'true' : undefined} onClick={() => onSelect(item.id)}>
              <span className="result-name">
                {item.name}
                {item.isBaseline && <span className="tag tag-info cut-s">{b.baseline}</span>}
                {item.flaggedRunsKept > 0 && <span className="tag tag-high cut-s">{b.kept(item.flaggedRunsKept)}</span>}
              </span>
              <span className="muted small num">{dateText(item.createdAt)}</span>
              <span className="num">{metricText('avg-fps', item.result.metrics.avgFps)} <span className="muted small">FPS</span></span>
              <span className="num">{metricText('low1-fps', item.result.metrics.low1Fps)} <span className="muted small">{b.metrics['low1-fps']}</span></span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
