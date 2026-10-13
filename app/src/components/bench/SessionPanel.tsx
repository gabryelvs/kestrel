import { benchCancel, benchKeep, benchRedo, benchStartRun } from '../../api/client';
import { flagText, sessionHeadline } from '../../api/bench';
import type { BenchState } from '../../api/types';
import { strings } from '../../strings';
import { CopyDiagnostics } from '../CopyDiagnostics';

/** The running session: what to do now, the countdown, each pass's status, and the session's buttons. */
export function SessionPanel({ state, onClose }: { state: BenchState; onClose: (resultId: string | null) => void }) {
  const b = strings.bench;
  const over = state.phase === 'done' || state.phase === 'failed' || state.phase === 'cancelled';
  const big = state.countdown ?? (state.phase === 'capturing' ? state.secondsLeft : null);
  return (
    <section className="panel session">
      <div className="session-head">
        <h2 aria-live="polite">{sessionHeadline(state)}</h2>
        {big !== null && <span className="num session-clock">{b.secondsLeft(big)}</span>}
      </div>
      <ol className="pass-chips">
        {[0, 1, 2, 3].map((pass) => {
          const record = pass > 0 ? state.runs[pass - 1] : null;
          const finished = pass < state.pass || state.phase === 'done';
          return (
            <li key={pass} className={`cut-s${pass === state.pass && !over ? ' current' : ''}${finished ? ' finished' : ''}`}>
              <span>{b.passChip(pass)}</span>
              {finished && <span className="muted small"> · {record?.kept ? b.runKept : b.runOk}</span>}
            </li>
          );
        })}
      </ol>
      {state.phase === 'review' && (
        <ul className="flags">
          {state.flags.map((flag) => (
            <li key={flag.kind}>{flagText(flag)}</li>
          ))}
        </ul>
      )}
      {state.phase === 'failed' && <p className="caution">{state.message}</p>}
      {state.phase === 'failed' && <CopyDiagnostics />}
      <div className="actions">
        {state.phase === 'waiting' && state.startMode === 'countdown' && state.countdown === null && state.gameRunning && (
          <button className="btn btn-primary cut-s" onClick={() => void benchStartRun()}>{b.startRun}</button>
        )}
        {state.phase === 'review' && (
          <>
            <button className="btn btn-primary cut-s" onClick={() => void benchRedo()}>{b.redo(state.pass)}</button>
            <button className="btn btn-ghost cut-s" onClick={() => void benchKeep()}>{b.keep}</button>
          </>
        )}
        {!over && <button className="btn btn-ghost cut-s" onClick={() => void benchCancel()}>{b.cancel}</button>}
        {state.phase === 'done' && <button className="btn btn-primary cut-s" onClick={() => onClose(state.resultId)}>{b.seeResult}</button>}
        {(state.phase === 'failed' || state.phase === 'cancelled') && <button className="btn btn-ghost cut-s" onClick={() => onClose(null)}>{b.close}</button>}
      </div>
    </section>
  );
}
