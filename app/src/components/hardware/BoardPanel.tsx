import type { BiosTopic, Report } from '../../api/types';
import { strings } from '../../strings';
import { DeviceName, Facts, HwPanel } from './HwPanel';
import { boardView } from './view';

const h = strings.hardwarePage;

/** The board, chipset and BIOS, then the BIOS settings Kestrel can see, each off one linking its BIOS-guide topic. */
export function BoardPanel({ report, failed, onGuide }: { report: Report; failed: boolean; onGuide: (topic: BiosTopic) => void }) {
  const view = boardView(report);
  return (
    <HwPanel title={h.panels.board} failed={failed}>
      <DeviceName name={view.name} />
      <Facts facts={view.facts} onGuide={onGuide} />
      <h4 className="label">{h.board.settings}</h4>
      <Facts facts={view.settings} onGuide={onGuide} />
    </HwPanel>
  );
}
