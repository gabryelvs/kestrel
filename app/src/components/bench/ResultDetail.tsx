import { useEffect, useRef, useState } from 'react';
import { benchCopyResults, benchDelete, benchGet, benchRename, benchSetBaseline } from '../../api/client';
import { contextRows, dateText, flagText, METRIC_ORDER, metricText, metricValue, pct, spreadText, verdictText, verdictTone } from '../../api/bench';
import type { BenchDetail } from '../../api/types';
import { strings } from '../../strings';
import { FrameGraph } from './FrameGraph';

/** One saved benchmark: its numbers, the comparison with the baseline, the graph, the context and the actions. */
export function ResultDetail({ id, onChanged, revealWhenLoaded = false }: { id: string; onChanged: () => void; revealWhenLoaded?: boolean }) {
  const b = strings.bench;
  const [detail, setDetail] = useState<BenchDetail | null>(null);
  // A failed first load replaces the panel; a failed action only adds a notice, so the result stays on screen.
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [naming, setNaming] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const sectionRef = useRef<HTMLElement>(null);
  const revealed = useRef(false);

  const reload = () => benchGet(id).then(setDetail, (e) => setActionError(String(e)));
  useEffect(() => {
    void benchGet(id).then(setDetail, (e) => setLoadError(String(e)));
  }, [id]);
  useEffect(() => {
    if (!note) return;
    const timer = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(timer);
  }, [note]);
  // Brings the result into view once it has rendered after "See the result"; never on a plain list click.
  useEffect(() => {
    if (!detail || !revealWhenLoaded || revealed.current) return;
    revealed.current = true;
    sectionRef.current?.scrollIntoView({ block: 'start' });
  }, [detail, revealWhenLoaded]);

  if (!detail) return loadError ? <p className="notice cut-s" role="alert">{loadError}</p> : null;
  const { benchmark: bench, comparison } = detail;
  const m = bench.result.metrics;
  const act = (work: Promise<void>) => {
    setActionError(null);
    void work.then(() => { onChanged(); return reload(); }, (e) => setActionError(String(e)));
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await benchCopyResults(id));
      setNote(b.copied);
    } catch {
      setNote(b.copyFailed);
    }
  };

  return (
    <section ref={sectionRef} className="panel detail">
      <header className="detail-head">
        {naming === null ? (
          <h2>{bench.name}</h2>
        ) : (
          <form className="rename" onSubmit={(e) => { e.preventDefault(); act(benchRename(id, naming)); setNaming(null); }}>
            <input id="bench-name" className="cut-s" value={naming} maxLength={60} onChange={(e) => setNaming(e.target.value)} aria-label={b.rename} autoFocus />
            <button className="btn btn-primary cut-s" type="submit" disabled={!naming.trim()}>{b.save}</button>
          </form>
        )}
        <span className="muted small num">{dateText(bench.createdAt)} · {b.route(bench.routeVersion)}</span>
      </header>

      <div className="tiles">
        {METRIC_ORDER.map((id) => {
          const value = metricValue(m, id);
          return (
            <div key={id} className="tile cut-s">
              <div className="label">{b.metrics[id]}</div>
              <div className={`num tile-value${value === null ? ' tile-word' : ''}`}>{metricText(id, value)}</div>
              {id === 'avg-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.avgFps))}</div>}
              {id === 'low1-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.low1Fps))}</div>}
              {id === 'low01-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.low01Fps))}</div>}
            </div>
          );
        })}
        <div className="tile cut-s">
          <div className="label">{b.bottleneckLabel}</div>
          <div className="tile-value tile-word">{b.bottleneck[m.bottleneck]}</div>
        </div>
      </div>

      {comparison && detail.baselineName && (
        <div className="comparison">
          <div className="label">{b.versus(detail.baselineName)}</div>
          {comparison.routeMismatch ? (
            <p className="muted small">{b.routeMismatch}</p>
          ) : (
            <ul>
              {comparison.metrics.map((d) => (
                <li key={d.metric}>
                  <span>{b.metrics[d.metric]}</span>
                  <span className="num">{metricText(d.metric, d.baseline)} → {metricText(d.metric, d.current)}</span>
                  <span className={`num verdict-${verdictTone(d.verdict)}`}>{d.deltaPct !== null && `${pct(d.deltaPct)} · `}{verdictText(d.verdict)}</span>
                </li>
              ))}
            </ul>
          )}
          {comparison.settingsDiffer.length > 0 && <p className="caution">{b.settingsDiffer(comparison.settingsDiffer.join(', '))}</p>}
        </div>
      )}

      {bench.runs.some((r) => r.kept) && (
        <ul className="flags">
          {bench.runs.map((r, i) => r.kept && r.flags.map((f) => <li key={`${i}-${f.kind}`}>{b.flagLine(b.passLabel(i + 1), flagText(f))}</li>))}
        </ul>
      )}

      <FrameGraph graph={detail.graph} />

      <div>
        <div className="label">{b.context}</div>
        <dl className="hw num">
          {contextRows(bench.context).map(([name, value]) => (
            <div key={name} className="hw-row">
              <dt>{name}</dt>
              <dd title={value}>{value}</dd>
            </div>
          ))}
        </dl>
      </div>

      {actionError && <p className="notice cut-s" role="alert">{actionError}</p>}
      <div className="actions">
        <button className="btn btn-primary cut-s" onClick={() => void copy()}>{b.copy}</button>
        <button className="btn btn-ghost cut-s" onClick={() => setNaming(bench.name)}>{b.rename}</button>
        <button className="btn btn-ghost cut-s" onClick={() => act(benchSetBaseline(id))}>{b.setBaseline}</button>
        {!confirming && <button className="btn btn-ghost cut-s" onClick={() => setConfirming(true)}>{b.delete}</button>}
        {note && <span className="toast" role="status">{note}</span>}
      </div>
      {confirming && (
        <div className="confirm">
          <p>{b.confirmDelete}</p>
          <button className="btn btn-primary cut-s" onClick={() => { setConfirming(false); setActionError(null); void benchDelete(id).then(onChanged, (e) => setActionError(String(e))); }}>{b.yesDelete}</button>
          <button className="btn btn-ghost cut-s" onClick={() => setConfirming(false)}>{b.noKeep}</button>
        </div>
      )}
    </section>
  );
}
