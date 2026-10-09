import { topProblems } from '../api/findings';
import { filledSegments } from '../api/format';
import type { BiosTopic, Report } from '../api/types';
import { strings } from '../strings';
import { FindingRow } from './FindingRow';
import type { Tab } from './TitleBar';

type Props = { report: Report; onTab: (tab: Tab) => void; onGuide: (topic: BiosTopic) => void };

export function ScoreCard({ report, onTab, onGuide }: Props) {
  const { score } = report;
  const filled = filledSegments(score.value);
  return (
    <section className="panel score-card">
      <div className="label">{strings.home.score}</div>
      <div className="score-line">
        <span className="num score-number">{score.value}</span>
        <span className="num muted">/100</span>
        <span className="fixes">{strings.home.fixes(score.problems)}</span>
      </div>
      <div className="segments" aria-hidden="true">
        {Array.from({ length: 20 }, (_, i) => (
          <span key={i} className={i < filled ? 'on' : undefined} />
        ))}
      </div>
      {score.potentialGain > 0 && <p className="muted small">{strings.home.gain(score.potentialGain)}</p>}
      <div className="score-findings">
        {topProblems(report.findings).map((f) => (
          <FindingRow key={f.id} finding={f} onTab={onTab} onGuide={onGuide} />
        ))}
      </div>
      <div className="actions">
        <button className="btn btn-primary cut-s" onClick={() => onTab('tweaks')}>{strings.home.applyPlan}</button>
        <button className="btn btn-ghost cut-s" onClick={() => onTab('health')}>{strings.home.seeAll}</button>
      </div>
    </section>
  );
}
