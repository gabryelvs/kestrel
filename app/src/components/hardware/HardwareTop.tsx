import { IconRefresh } from '@tabler/icons-react';
import { dayMonthTime, summaryLine, windowsLine } from '../../api/hardware';
import type { Report } from '../../api/types';
import { strings } from '../../strings';
import { CopySpecs } from './CopySpecs';

const h = strings.hardwarePage;

/** The page head: the PC in one line, Windows, when it was scanned, Scan again and Copy specs. */
export function HardwareTop({ report, onRescan }: { report: Report; onRescan: () => void }) {
  const s = report.snapshot;
  const summary = summaryLine(s);
  const windows = windowsLine(s);
  return (
    <header className="page-head hw-head">
      <div className="hw-title">
        <h1>{h.title}</h1>
        {summary && <p className="num">{summary}</p>}
        {windows && <p className="muted small num">{windows}</p>}
      </div>
      <div className="hw-actions">
        <span className="label">{h.lastScan(dayMonthTime(s.collectedAt) ?? h.notReported)}</span>
        <button className="btn btn-ghost cut-s" onClick={onRescan}>
          <IconRefresh size={14} aria-hidden /> {strings.home.rescan}
        </button>
        <CopySpecs report={report} />
      </div>
    </header>
  );
}
