import { useEffect, useState } from 'react';
import { rescan } from '../api/client';
import { topProblems } from '../api/findings';
import type { Report } from '../api/types';
import { Logo } from '../components/Logo';
import { strings } from '../strings';

type Phase = { name: 'welcome'; error?: string } | { name: 'scanning' } | { name: 'result'; report: Report };

/** Spec decision 22: welcome → scan → what Kestrel found → home, or the Benchmark tab to measure the starting point. */
export function FirstRun({ onDone }: { onDone: (report: Report, next: 'home' | 'benchmark') => void }) {
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
        {phase.name === 'result' && (
          <>
            <h1>{strings.firstRun.resultTitle}</h1>
            <p>
              <span className="num score-number">{phase.report.score.value}</span>
              <span className="num muted">/100</span> <span className="fixes">{strings.home.fixes(phase.report.score.problems)}</span>
            </p>
            <ul className="steps">
              {topProblems(phase.report.findings).map((f) => (
                <li key={f.id}>{f.title}</li>
              ))}
            </ul>
            <p className="muted small">{strings.firstRun.later}</p>
            <div className="actions">
              <button className="btn btn-primary cut-s" onClick={() => onDone(phase.report, 'benchmark')}>{strings.firstRun.benchmarkNow}</button>
              <button className="btn btn-ghost cut-s" onClick={() => onDone(phase.report, 'home')}>{strings.firstRun.notNow}</button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
