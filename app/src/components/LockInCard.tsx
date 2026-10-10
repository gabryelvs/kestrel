import { strings } from '../strings';

/** Placeholder until stage 5: shows where Lock In will live, switched off. */
export function LockInCard() {
  return (
    <section className="panel lock-in">
      <div>
        <div className="label">{strings.home.lockIn}</div>
        <div className="muted small">{strings.home.lockInSoon}</div>
      </div>
      <span className="toggle cut-s" aria-hidden="true" />
    </section>
  );
}
