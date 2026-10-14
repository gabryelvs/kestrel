import type { BiosTopic, Report } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { memoryView } from './view';

const h = strings.hardwarePage;

/** Total, channels, slots, speed and kit rating, then one row per module. */
export function MemoryPanel({ report, failed, onGuide }: { report: Report; failed: boolean; onGuide: (topic: BiosTopic) => void }) {
  const view = memoryView(report);
  const col = h.memory.columns;
  return (
    <HwPanel title={h.panels.memory} failed={failed}>
      <DeviceName name={view.name} />
      <Facts facts={view.facts} onGuide={onGuide} />
      {view.modules.length > 0 && (
        <table className="modules">
          <thead>
            <tr>
              {[col.slot, col.size, col.maker, col.part, col.speed, col.rank].map((name) => (
                <th key={name}>{name}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {view.modules.map((m, i) => (
              <tr key={`${m.slot}-${i}`}>
                {[m.slot, m.size, m.maker, m.part, m.speed, m.rank].map((value, j) => (
                  <td key={j} className={value === null ? 'tone-muted' : 'num'}>{value ?? h.notReported}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="muted small">{h.memory.note}</p>
    </HwPanel>
  );
}
