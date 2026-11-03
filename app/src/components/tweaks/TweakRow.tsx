import { hasTickBox, type RowNote } from '../../api/tweaks';
import type { Row, TweakId } from '../../api/types';
import { strings } from '../../strings';

const t = strings.tweaks;

/** What a row's controls do. The page decides; the row only says which button was pressed. */
export type RowActions = {
  tick: (id: TweakId, ticked: boolean) => void;
  undo: (id: TweakId) => void;
  applyAgain: (id: TweakId) => void;
  /** Restore anyway was pressed: the row asks first. */
  askRestoreAnyway: (id: TweakId) => void;
  /** The question was answered yes. */
  restoreAnyway: (id: TweakId) => void;
  stopAsking: () => void;
  usePlanAgain: () => void;
  removePlan: () => void;
};

type Props = {
  row: Row;
  ticked: boolean;
  /** A command is running or a question is waiting: the box and every button are off. */
  locked: boolean;
  /** The gate is closed, so nothing that changes the PC may be pressed. Ticking changes nothing, so the box stays. */
  closed: boolean;
  /**
   * Restore anyway was pressed, and the row is waiting for the answer. It only asks while the row is still one something else
   * changed: a re-read in between can show otherwise, and Yes would then be an answer to nothing.
   */
  asking: boolean;
  /** What happened to this row's last button, said where the person pressed it. */
  note: RowNote | null;
  on: RowActions;
};

const BUTTON = 'btn btn-ghost btn-small cut-s';

/** One tweak: a tick box when it can be applied, its title, the state and impact tags, why, and the buttons its state has. */
export function TweakRow({ row, ticked, locked, closed, asking, note, on }: Props) {
  const off = locked || closed;
  const boxId = `tick-${row.id}`;
  const question = asking && row.state === 'changed-elsewhere';
  return (
    <li className={`tweak-row tweak-${row.state}`} data-tweak={row.id}>
      <div className="tweak-box">
        {hasTickBox(row) && (
          <input id={boxId} type="checkbox" checked={ticked} disabled={locked} onChange={(event) => on.tick(row.id, event.target.checked)} />
        )}
      </div>
      <div className="tweak-main">
        <div className="tweak-title">
          {hasTickBox(row) ? <label htmlFor={boxId}>{row.title}</label> : <span>{row.title}</span>}
          <span className={`tag tag-state state-${row.state} cut-s`}>{t.states[row.state]}</span>
          <span className="tag tag-info cut-s">{t.impactTag(t.impact[row.impact])}</span>
        </div>
        <p className="tweak-why muted small wrap">{row.why}</p>
        {note && <p className={`tweak-note small wrap${note.ok ? '' : ' tone-warn'}`}>{note.text}</p>}
        {question && <p className="tweak-ask small wrap">{t.restoreAnywayAsk}</p>}
      </div>
      <div className="tweak-actions">
        {question ? (
          <>
            <button className={BUTTON} disabled={closed} onClick={() => on.restoreAnyway(row.id)}>{t.yesRestore}</button>
            <button className={BUTTON} onClick={on.stopAsking}>{t.cancel}</button>
          </>
        ) : (
          <>
            {row.state === 'applied' && (
              <>
                {row.id === 'system-protection' && <span className="tweak-warning muted small wrap">{t.protectionWarning}</span>}
                <button className={BUTTON} disabled={off} onClick={() => on.undo(row.id)}>{t.undo}</button>
              </>
            )}
            {row.state === 'changed-elsewhere' && (
              <>
                <button className={BUTTON} disabled={off} onClick={() => on.applyAgain(row.id)}>{t.applyAgain}</button>
                <button className={BUTTON} disabled={off} onClick={() => on.askRestoreAnyway(row.id)}>{t.restoreAnyway}</button>
              </>
            )}
            {row.state === 'plan-inactive' && (
              <>
                <button className={BUTTON} disabled={off} onClick={on.usePlanAgain}>{t.useAgain}</button>
                <button className={BUTTON} disabled={off} onClick={on.removePlan}>{t.removePlan}</button>
              </>
            )}
          </>
        )}
      </div>
    </li>
  );
}
