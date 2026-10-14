import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { CardPanels, Facts } from './HwPanel';
import { displaysView } from './view';

const h = strings.hardwarePage;

/** One panel per monitor; virtual displays on one dim line at the end. */
export function DisplayPanels({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  const { cards, virtual } = displaysView(snapshot);
  return (
    <CardPanels
      title={h.panels.displays}
      cards={cards}
      failed={failed}
      badge={(card) => card.primary && <span className="tag tag-info cut-s">{h.displays.primary}</span>}
      body={(card) => <Facts facts={card.facts} />}
      footer={virtual.length > 0 && <p className="muted small">{h.displays.virtual(virtual.join(', '))}</p>}
    />
  );
}
