import { panelFailed, type PanelId } from '../api/hardware';
import type { BiosTopic, Report } from '../api/types';
import { BoardPanel } from '../components/hardware/BoardPanel';
import { DisplayPanels } from '../components/hardware/DisplayPanels';
import { GraphicsPanels } from '../components/hardware/GraphicsPanels';
import { HardwareTop } from '../components/hardware/HardwareTop';
import { LiveClocks } from '../components/hardware/LiveClocks';
import { MemoryPanel } from '../components/hardware/MemoryPanel';
import { NetworkPanels } from '../components/hardware/NetworkPanels';
import { ProcessorPanel } from '../components/hardware/ProcessorPanel';
import { StoragePanels } from '../components/hardware/StoragePanels';

type Props = { report: Report; onGuide: (topic: BiosTopic) => void; onRescan: () => void };

/** Every part of the PC Windows reports without a driver (decisions 52 and 74), from the last scan; only the clocks move. */
export function Hardware({ report, onGuide, onRescan }: Props) {
  const s = report.snapshot;
  const failed = (panel: PanelId) => panelFailed(s.probeErrors, panel);
  return (
    <div className="page hardware">
      <HardwareTop report={report} onRescan={onRescan} />
      <div className="hw-grid">
        <ProcessorPanel snapshot={s} failed={failed('processor')} />
        <LiveClocks maxMhz={s.cpu.maxMhz} />
        <GraphicsPanels report={report} failed={failed('graphics')} onGuide={onGuide} />
        <MemoryPanel report={report} failed={failed('memory')} onGuide={onGuide} />
        <BoardPanel report={report} failed={failed('board')} onGuide={onGuide} />
        <DisplayPanels snapshot={s} failed={failed('displays')} />
        <StoragePanels snapshot={s} failed={failed('storage')} />
        <NetworkPanels snapshot={s} failed={failed('network')} />
      </div>
    </div>
  );
}
