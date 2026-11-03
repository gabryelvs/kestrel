import { useEffect, useState } from 'react';
import { rescan } from '../api/client';
import { topProblems } from '../api/findings';
import type { Report } from '../api/types';
import { Logo } from '../components/Logo';
import { strings } from '../strings';
import { askForPlanNote } from './flag';

type Phase = { name: 'welcome'; error?: string } | { name: 'scanning' } | { name: 'result'; report: Report };

/** Where the first run goes on to. The result step's two buttons lead to the benchmark or to the plan; neither goes straight Home. */
export type FirstRunNext = 'home' | 'benchmark' | 'tweaks';

/**
 * What Kestrel found, and the two ways on. Either way the first run ends at the plan: the starting-point benchmark goes to the
 * Benchmark tab, whose first saved result leads to the Tweaks tab, and Later (decision 22's Not now) goes there at once. Each asks for
 * the Tweaks tab's one-time note before it leaves, so the tab finds it asked for whichever way the person came.
 */
export function FirstRunResult({ report, onDone }: { report: Report; onDone: (report: Report, next: FirstRunNext) => void }) {
  const leave = (next: FirstRunNext) => {
    askForPlanNote();
    onDone(report, next);
  };
  return (
    <>
      <h1>{strings.firstRun.resultTitle}</h1>
      <p>
        <span className="num score-number">{report.score.value}</span>
        <span className="num muted">/100</span> <span className="fixes">{strings.home.fixes(report.score.problems)}</span>
      </p>
      <ul className="steps">
        {topProblems(report.findings).map((f) => (
          <li key={f.id}>{f.title}</li>
        ))}
      </ul>
      <p className="muted small">{strings.firstRun.later}</p>
      <div className="actions">
        <button className="btn btn-primary cut-s" onClick={() => leave('benchmark')}>{strings.firstRun.benchmarkNow}</button>
        <button className="btn btn-ghost cut-s" onClick={() => leave('tweaks')}>{strings.firstRun.notNow}</button>
      </div>
    </>
  );
}

/** Spec decision 22: welcome → scan → what Kestrel found → the benchmark to measure the starting point, or the plan. */
export function FirstRun({ onDone }: { onDone: (report: Report, next: FirstRunNext) => void }) {
  const [phase, setPhase] = useState<Phase>({ name: 'welcome' });
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (phase.name !== 'scanning') return;
    const id = window.setInterval(() => setStep((s) => Math.min(s + 1, strings.firstRun.steps.length - 1)), 450);
    return () => window.clearInterval(id);
  }, [phase.name]);

  const start = async () => {
    setStep(0);
    setPhase({ name: 'scanning' });
    try {
      setPhase({ name: 'result', report: await rescan() });
    } catch (error) {
      setPhase({ name: 'welcome', error: String(error) });
    }
  };

  return (
    <div className="first-run">
      <section className="panel">
        <Logo size={40} />
        {phase.name === 'welcome' && (
          <>
            <h1>{strings.firstRun.title}</h1>
            <p className="muted">{strings.firstRun.body}</p>
            {phase.error && <p className="caution">{strings.loading.failed} {phase.error}</p>}
            <div>
              <button className="btn btn-primary cut-s" onClick={() => void start()}>{strings.firstRun.start}</button>
            </div>
          </>
        )}
        {phase.name === 'scanning' && (
          <>
            <h1>{strings.firstRun.scanning}</h1>
            <p className="scan-step" aria-live="polite">{strings.firstRun.steps[step]}…</p>
          </>
        )}
        {phase.name === 'result' && <FirstRunResult report={phase.report} onDone={onDone} />}
      </section>
    </div>
  );
}
