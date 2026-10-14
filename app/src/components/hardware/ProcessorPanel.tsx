import type { SystemSnapshot } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { processorView } from './view';

const h = strings.hardwarePage;

/** Name, codename, socket, cores, clocks, caches, and a chip per instruction set (dimmed when the CPU lacks it). */
export function ProcessorPanel({ snapshot, failed }: { snapshot: SystemSnapshot; failed: boolean }) {
  const view = processorView(snapshot);
  return (
    <HwPanel title={h.panels.processor} failed={failed}>
      <DeviceName name={view.name} />
      <Facts facts={view.facts} />
      <div>
        <h4 className="label">{h.processor.instructions}</h4>
        {view.chips ? (
          <ul className="chips">
            {view.chips.map((chip) => (
              <li key={chip.name} className={`chip cut-s${chip.on ? '' : ' chip-off'}`}>{chip.name}</li>
            ))}
          </ul>
        ) : (
          <p className="tone-muted small">{h.notReported}</p>
        )}
      </div>
    </HwPanel>
  );
}
