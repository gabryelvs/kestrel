import { IconAlertTriangle, IconCheck } from '@tabler/icons-react';
import type { ItemOutcome } from '../../api/types';
import { strings } from '../../strings';

const t = strings.tweaks;

type Props = {
  outcomes: ItemOutcome[];
  /** A change that took waits for the next launch of the game. */
  restart: boolean;
  /** Something changed, so measuring is worth suggesting. */
  measure: boolean;
  onBenchmark: () => void;
  onClose: () => void;
};

/** How each change went, said plainly: a change left alone or refused shows its reason as it is. */
export function Outcomes({ outcomes, restart, measure, onBenchmark, onClose }: Props) {
  return (
    <section className="panel outcomes">
      <div className="label">{t.result}</div>
      <ul className="outcome-list">
        {outcomes.map((outcome, i) => (
          <li key={`${i}-${outcome.id}`} className={`outcome ${outcome.ok ? 'ok' : 'failed'}`}>
            {outcome.ok ? <IconCheck size={16} color="var(--accent)" aria-hidden /> : <IconAlertTriangle size={16} color="var(--warn)" aria-hidden />}
            <span className="outcome-title">{outcome.title}</span>
            <span className="outcome-message muted wrap">{outcome.message}</span>
          </li>
        ))}
      </ul>
      {restart && <p>{t.restartValorant}</p>}
      {measure && <p className="muted">{t.done}</p>}
      <div className="actions">
        {measure && <button className="btn btn-primary cut-s" onClick={onBenchmark}>{t.runBenchmark}</button>}
        <button className="btn btn-ghost cut-s" onClick={onClose}>{t.close}</button>
      </div>
    </section>
  );
}
