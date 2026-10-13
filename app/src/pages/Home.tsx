import { IconRefresh } from '@tabler/icons-react';
import { openLink } from '../api/client';
import type { BiosTopic, Report } from '../api/types';
import { CopyDiagnostics } from '../components/CopyDiagnostics';
import { HardwarePanel } from '../components/HardwarePanel';
import { LastBenchmark } from '../components/LastBenchmark';
import { LoadGraph } from '../components/LoadGraph';
import { LockInCard } from '../components/LockInCard';
import { ScoreCard } from '../components/ScoreCard';
import type { Tab } from '../components/TitleBar';
import { strings } from '../strings';

type Props = { report: Report; onTab: (tab: Tab) => void; onGuide: (topic: BiosTopic) => void; onRescan: () => void };

export function Home({ report, onTab, onGuide, onRescan }: Props) {
  return (
    <div className="home">
      <ScoreCard report={report} onTab={onTab} onGuide={onGuide} />
      <div className="home-side">
        <HardwarePanel snapshot={report.snapshot} />
        <LockInCard />
        <LoadGraph />
      </div>
      <LastBenchmark onTab={onTab} />
      <footer className="home-footer">
        <CopyDiagnostics />
        <button className="btn btn-ghost cut-s" onClick={onRescan}>
          <IconRefresh size={14} aria-hidden /> {strings.home.rescan}
        </button>
        <button className="link" onClick={() => void openLink(strings.links.github)}>{strings.home.help}</button>
        <span className="muted disclaimer">{strings.about.disclaimer}</span>
      </footer>
    </div>
  );
}
