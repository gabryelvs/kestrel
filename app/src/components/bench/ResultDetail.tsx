import { useEffect, useState } from 'react';
import { benchCopyResults, benchDelete, benchGet, benchRename, benchSetBaseline } from '../../api/client';
import { dateText, flagText, METRIC_ORDER, metricText, metricValue, pct, settingsText, spreadText, verdictText, verdictTone } from '../../api/bench';
import type { BenchDetail } from '../../api/types';
import { strings } from '../../strings';
import { FrameGraph } from './FrameGraph';

/** One saved benchmark: its numbers, the comparison with the baseline, the graph, the context and the actions. */
export function ResultDetail({ id, onChanged }: { id: string; onChanged: () => void }) {
  const b = strings.bench;
  const [detail, setDetail] = useState<BenchDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [naming, setNaming] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const load = () => benchGet(id).then(setDetail, (e) => setError(String(e)));
  useEffect(() => {
    void benchGet(id).then(setDetail, (e) => setError(String(e)));
  }, [id]);
  useEffect(() => {
    if (!note) return;
    const timer = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(timer);
  }, [note]);

  if (error) return <p className="notice cut-s">{error}</p>;
  if (!detail) return null;
  const { benchmark: bench, comparison } = detail;
  const m = bench.result.metrics;
  const act = (work: Promise<void>) => void work.then(() => { onChanged(); return load(); }, (e) => setError(String(e)));

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await benchCopyResults(id));
      setNote(b.copied);
    } catch {
      setNote(b.copyFailed);
    }
  };

  const ctx = bench.context;
  const contextRows: [string, string][] = [
    [b.contextRows.powerPlan, ctx.powerPlan ?? '—'],
    [b.contextRows.ram, ctx.ramMts ? `${ctx.ramMts} MT/s` : '—'],
    [b.contextRows.gpu, ctx.gpu ? `${ctx.gpu}${ctx.gpuDriver ? ` · ${ctx.gpuDriver}` : ''}` : '—'],
    [b.contextRows.windows, ctx.windowsBuild ? String(ctx.windowsBuild) : '—'],
    [b.contextRows.display, ctx.displayHz ? `${ctx.displayHz} Hz` : '—'],
    [b.contextRows.game, settingsText(ctx.gameSettings)],
  ];

  return (
    <section className="panel detail">
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
        {METRIC_ORDER.map((id) => (
          <div key={id} className="tile cut-s">
            <div className="label">{b.metrics[id]}</div>
            <div className="num tile-value">{metricText(id, metricValue(m, id))}</div>
            {id === 'avg-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.avgFps))}</div>}
            {id === 'low1-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.low1Fps))}</div>}
            {id === 'low01-fps' && <div className="muted small">{b.spread(spreadText(bench.result.spread.low01Fps))}</div>}
          </div>
        ))}
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
          {bench.runs.map((r, i) => r.kept && r.flags.map((f) => <li key={`${i}-${f.kind}`}>{b.passLabel(i + 1)}: {flagText(f)}</li>))}
        </ul>
      )}

      <FrameGraph graph={detail.graph} />

      <div>
        <div className="label">{b.context}</div>
        <dl className="hw num">
          {contextRows.map(([name, value]) => (
            <div key={name} className="hw-row">
              <dt>{name}</dt>
              <dd title={value}>{value}</dd>
            </div>
          ))}
        </dl>
      </div>

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
          <button className="btn btn-primary cut-s" onClick={() => { setConfirming(false); void benchDelete(id).then(onChanged, (e) => setError(String(e))); }}>{b.yesDelete}</button>
          <button className="btn btn-ghost cut-s" onClick={() => setConfirming(false)}>{b.noKeep}</button>
        </div>
      )}
    </section>
  );
}
