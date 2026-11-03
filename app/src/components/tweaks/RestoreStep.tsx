import type { RestoreQuestion } from '../../api/tweaks';
import { strings } from '../../strings';

const t = strings.tweaks;

type Props = {
  question: RestoreQuestion;
  on: {
    /** Turn System Protection on and create the restore point. */
    turnOn: () => void;
    /** Go on with Kestrel's undo only. */
    goOn: () => void;
    cancel: () => void;
  };
};

/**
 * The question Apply puts at the restore point. System Protection off: turn it on and create, go on without, or cancel. Under a
 * policy: go on without, or cancel. A restore point that failed: go on, or cancel.
 */
export function RestoreStep({ question, on }: Props) {
  return (
    <section className="panel restore">
      <p className="wrap">
        {question.kind === 'protection-off' && t.protectionOff}
        {question.kind === 'blocked-by-policy' && t.restoreBlocked}
        {question.kind === 'failed' && t.restoreFailed(question.reason)}
      </p>
      <div className="actions">
        {question.kind === 'protection-off' && <button className="btn btn-primary cut-s" onClick={on.turnOn}>{t.turnOnAndCreate}</button>}
        {question.kind === 'failed' ? (
          <button className="btn btn-primary cut-s" onClick={on.goOn}>{t.continue}</button>
        ) : (
          <button className="btn btn-ghost cut-s" onClick={on.goOn}>{t.continueWithout}</button>
        )}
        <button className="btn btn-ghost cut-s" onClick={on.cancel}>{t.cancel}</button>
      </div>
    </section>
  );
}
