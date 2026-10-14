import type { BiosTopic, Report } from '../../api/types';
import { strings } from '../../strings';
import { CardPanels, Facts } from './HwPanel';
import { graphicsView } from './view';

const h = strings.hardwarePage;

/** One panel per real graphics card, the main one first; virtual adapters (Parsec…) on one dim line at the end. */
export function GraphicsPanels({ report, failed, onGuide }: { report: Report; failed: boolean; onGuide: (topic: BiosTopic) => void }) {
  const { cards, virtual } = graphicsView(report);
  return (
    <CardPanels
      title={h.panels.graphics}
      cards={cards}
      failed={failed}
      body={(card) => <Facts facts={card.facts} onGuide={onGuide} />}
      footer={virtual.length > 0 && <p className="muted small">{h.graphics.virtual(virtual.join(', '))}</p>}
    />
  );
}
