import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { CardPanels, Facts } from './HwPanel';
import { storageView } from './view';

const h = strings.hardwarePage;

/** One panel per drive: type, bus and size, then each volume with its free space and the games installed there. */
export function StoragePanels({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  return (
    <CardPanels
      title={h.panels.storage}
      cards={storageView(snapshot)}
      failed={failed}
      body={(card) => (
        <>
          <Facts facts={card.facts} />
          {card.volumes.length > 0 && (
            <ul className="volumes">
              {card.volumes.map((v) => (
                <li key={v.letter}>
                  <span className="num">{v.letter}</span>
                  <span className="num">{v.space}</span>
                  {v.games.map((game) => (
                    <span key={game} className="tag tag-info cut-s">{game}</span>
                  ))}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    />
  );
}
