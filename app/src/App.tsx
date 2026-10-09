import { useCallback, useEffect, useRef, useState } from 'react';
import { getReport, inTauri, isElevated, rescan } from './api/client';
import type { BiosTopic, Report } from './api/types';
import { Banner } from './components/Banner';
import { TitleBar, type Tab } from './components/TitleBar';
import { FirstRun } from './first-run/FirstRun';
import { firstRunDone, markFirstRunDone } from './first-run/flag';
import { ComingSoon } from './pages/ComingSoon';
import { Health } from './pages/Health';
import { Home } from './pages/Home';
import { Toolbox } from './pages/Toolbox';
import { strings } from './strings';

type Scan = { state: 'loading' } | { state: 'ready'; report: Report } | { state: 'failed'; message: string };

export function App() {
  const [tab, setTab] = useState<Tab>('home');
  const [scan, setScan] = useState<Scan>({ state: 'loading' });
  const [admin, setAdmin] = useState<boolean | null>(null);
  const [guideTopic, setGuideTopic] = useState<BiosTopic | null>(null);
  const [firstRun, setFirstRun] = useState(() => !firstRunDone());
  const skippedInitialScan = useRef(firstRun);

  const run = useCallback(async (fresh: boolean) => {
    setScan({ state: 'loading' });
    try {
      setScan({ state: 'ready', report: await (fresh ? rescan() : getReport()) });
    } catch (error) {
      setScan({ state: 'failed', message: String(error) });
    }
  }, []);

  useEffect(() => {
    if (!skippedInitialScan.current) void run(false);
    void isElevated().then(setAdmin, () => setAdmin(null));
  }, [run]);

  const openGuide = (topic: BiosTopic) => {
    setGuideTopic(topic);
    setTab('toolbox');
  };

  const chooseTab = (next: Tab) => {
    if (next !== 'toolbox') setGuideTopic(null);
    setTab(next);
  };

  const finishFirstRun = (report: Report) => {
    markFirstRunDone();
    setScan({ state: 'ready', report });
    setFirstRun(false);
  };

  const report = scan.state === 'ready' ? scan.report : null;
  const build = report?.snapshot.os.build ?? 0;

  return (
    <div className="app">
      <TitleBar tab={tab} onTab={chooseTab} admin={admin} />
      {!inTauri() && <Banner>{strings.banners.sample}</Banner>}
      {build > 0 && build < 22000 && <Banner>{strings.banners.windows10}</Banner>}
      {firstRun ? (
        <FirstRun onDone={finishFirstRun} />
      ) : (
        <main className="content scroll">
          {scan.state === 'loading' && <p className="center muted">{strings.loading.scanning}</p>}
          {scan.state === 'failed' && (
            <div className="center">
              <p>{strings.loading.failed}</p>
              <p className="muted">{scan.message}</p>
              <button className="btn btn-ghost cut-s" onClick={() => void run(true)}>{strings.loading.retry}</button>
            </div>
          )}
          {report && tab === 'home' && <Home report={report} onTab={chooseTab} onGuide={openGuide} onRescan={() => void run(true)} />}
          {report && tab === 'health' && <Health report={report} onGuide={openGuide} />}
          {report && tab === 'toolbox' && <Toolbox guide={report.guide} focus={guideTopic} />}
          {tab === 'tweaks' && <ComingSoon {...strings.comingSoon.tweaks} />}
          {tab === 'lockIn' && <ComingSoon {...strings.comingSoon.lockIn} />}
          {tab === 'benchmark' && <ComingSoon {...strings.comingSoon.benchmark} />}
        </main>
      )}
    </div>
  );
}
