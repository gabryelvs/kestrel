import { useEffect, useState } from 'react';
import { benchGet, benchList } from '../api/client';
import { metricText, metricValue, pct, verdictTone } from '../api/bench';
import type { BenchDetail } from '../api/types';
import { strings } from '../strings';
import type { Tab } from './TitleBar';

/** The latest benchmark against the baseline, or a nudge to measure the starting point. */
export function LastBenchmark({ onTab }: { onTab: (tab: Tab) => void }) {
  const [detail, setDetail] = useState<BenchDetail | null | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    void benchList()
      .then((list) => (list[0] ? benchGet(list[0].id) : null))
      .then((d) => alive && setDetail(d), () => alive && setDetail(null));
    return () => {
      alive = false;
    };
  }, []);

  const h = strings.home;
  if (detail === undefined) return <section className="panel home-bench" />;
  if (detail === null) {
    return (
      <section className="panel home-bench">
        <div className="label">{h.lastBenchmark}</div>
        <p className="muted small">{h.noBenchmark}</p>
        <div>
          <button className="btn btn-primary cut-s" onClick={() => onTab('benchmark')}>{h.runStartingPoint}</button>
        </div>
      </section>
    );
  }
  const m = detail.benchmark.result.metrics;
  const delta = (id: 'avg-fps' | 'low1-fps') => detail.comparison?.metrics.find((d) => d.metric === id);
  return (
    <section className="panel home-bench">
      <div className="label">{h.lastBenchmark}</div>
      <div className="last-bench">
        <span>{detail.benchmark.name}</span>
        {(['avg-fps', 'low1-fps'] as const).map((id) => {
          const d = delta(id);
          return (
            <span key={id} className="num">
              {metricText(id, metricValue(m, id))} <span className="muted small">{strings.bench.metrics[id]}</span>
              {d && d.deltaPct !== null && <span className={`small verdict-${verdictTone(d.verdict)}`}> {pct(d.deltaPct)}</span>}
            </span>
          );
        })}
        <button className="link" onClick={() => onTab('benchmark')}>{h.seeBenchmarks}</button>
      </div>
    </section>
  );
}
