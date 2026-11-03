import type { PreviewItem, PreviewView } from '../../api/types';
import { strings } from '../../strings';

const t = strings.tweaks;

function Item({ item }: { item: PreviewItem }) {
  return (
    <article className="preview-item">
      <div className="preview-head">
        <h3>{item.title}</h3>
        <span className="preview-when">
          <span className="muted small">{t.takesEffect}</span> <span className="tag tag-info cut-s">{t.timing[item.timing]}</span>
        </span>
      </div>
      <p className="preview-where muted small num wrap">{item.location}</p>
      <table className="changes">
        <thead>
          <tr>
            <th scope="col">{t.columns.setting}</th>
            <th scope="col">{t.columns.now}</th>
            <th scope="col">{t.columns.after}</th>
          </tr>
        </thead>
        <tbody>
          {item.changes.map((change, i) => (
            <tr key={`${i}-${change.label}`}>
              <td>{change.label}</td>
              <td className="num">{change.now}</td>
              <td className="num">{change.after}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {item.note && <p className="preview-note muted small wrap">{item.note}</p>}
    </article>
  );
}

type Props = { preview: PreviewView; onApply: () => void; onCancel: () => void };

/** What Apply would change, before it changes anything: each ticked tweak with where, the value now, the value after, and when. */
export function PreviewPanel({ preview, onApply, onCancel }: Props) {
  const empty = preview.items.length === 0;
  return (
    <section className="panel preview">
      <div className="label">{t.previewLabel}</div>
      <p className="muted small">{empty ? t.previewEmpty : t.previewIntro}</p>
      {preview.items.map((item) => (
        <Item key={item.id} item={item} />
      ))}
      <div className="actions">
        <button className="btn btn-primary cut-s" disabled={empty} onClick={onApply}>{t.apply}</button>
        <button className="btn btn-ghost cut-s" onClick={onCancel}>{t.cancel}</button>
      </div>
    </section>
  );
}
