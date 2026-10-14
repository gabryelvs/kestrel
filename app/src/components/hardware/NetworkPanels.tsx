import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { CardPanels, Facts } from './HwPanel';
import { networkView } from './view';

const h = strings.hardwarePage;

/** One panel per network adapter: kind, link speed, connected, power saving. */
export function NetworkPanels({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  return <CardPanels title={h.panels.network} cards={networkView(snapshot)} failed={failed} body={(card) => <Facts facts={card.facts} />} />;
}
