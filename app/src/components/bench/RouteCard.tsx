import type { GameProfile } from '../../api/types';
import { strings } from '../../strings';

/** Route v1 and the button that arms a session (spec "What the owner sees", 1). */
export function RouteCard({ profile, starting, onStart }: { profile: GameProfile; starting: boolean; onStart: () => void }) {
  const b = strings.bench;
  return (
    <section className="panel route">
      <div className="label">{b.route(profile.route.version)}</div>
      <p>{b.setup}</p>
      <ol className="steps">
        {b.steps.map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
      <p className="muted small">{b.passes}</p>
      <p className="muted small">{b.readOnly}</p>
      <div>
        <button className="btn btn-primary cut-s" disabled={starting} onClick={onStart}>
          {starting ? b.starting : b.start}
        </button>
      </div>
    </section>
  );
}
