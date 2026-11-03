import type { Row } from '../../api/types';
import { strings } from '../../strings';

const t = strings.tweaks;

type Props = { rows: Row[]; onRevert: () => void; onCancel: () => void };

/** Revert everything asks once, with the rows it will undo listed. */
export function RevertConfirm({ rows, onRevert, onCancel }: Props) {
  return (
    <section className="panel revert">
      <div className="label">{t.revertEverything}</div>
      <p>{t.revertConfirm}</p>
      <ul className="revert-list">
        {rows.map((row) => (
          <li key={row.id}>{row.title}</li>
        ))}
      </ul>
      <div className="actions">
        <button className="btn btn-primary cut-s" onClick={onRevert}>{t.yesRevert}</button>
        <button className="btn btn-ghost cut-s" onClick={onCancel}>{t.cancel}</button>
      </div>
    </section>
  );
}
