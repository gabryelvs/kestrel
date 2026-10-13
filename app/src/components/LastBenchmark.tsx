import { useEffect, useState } from 'react';
import { benchGet, benchList } from '../api/client';
import { metricText, metricValue, pct, verdictTone } from '../api/bench';
import type { BenchDetail } from '../api/types';
import { strings } from '../strings';
import type { Tab } from './TitleBar';

/** A load in progress, no benchmarks yet, a failed load, or the newest benchmark. */
type Load = { state: 'loading' } | { state: 'none' } | { state: 'failed' } | { state: 'ready'; detail: BenchDetail };

/** The latest benchmark against the baseline, or a nudge to measure the starting point. */
export function LastBenchmark({ onTab }: { onTab: (tab: Tab) => void }) {
  const [load, setLoad] = useState<Load>({ state: 'loading' });

  useEffect(() => {
    let alive = true;
    void benchList()
      .then((list) => (list[0] ? benchGet(list[0].id) : null))
      .then(
        (d) => alive && setLoad(d ? { state: 'ready', detail: d } : { state: 'none' }),
        () => alive && setLoad({ state: 'failed' }),
      );
    return () => {
      alive = false;
    };
  }, []);

  const h = strings.home;
  if (load.state === 'loading') return <section className="panel home-bench" />;
  if (load.state === 'none') {
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
  if (load.state === 'failed') {
    return (
      <section className="panel home-bench">
        <div className="label">{h.lastBenchmark}</div>
        <p className="muted small">{h.benchmarksFailed}</p>
        <div>
          <button className="link" onClick={() => onTab('benchmark')}>{h.seeBenchmarks}</button>
        </div>
      </section>
    );
  }
  const { detail } = load;
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
      {detail.comparison && detail.baselineName !== null && (
        <p className="muted small">{strings.bench.versus(detail.baselineName)}</p>
      )}
    </section>
  );
}
