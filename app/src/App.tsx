import { useCallback, useEffect, useRef, useState } from 'react';
import { getReport, inTauri, isElevated, rescan } from './api/client';
import type { BiosTopic, Report } from './api/types';
import { Banner } from './components/Banner';
import { TitleBar, type Tab } from './components/TitleBar';
import { FirstRun } from './first-run/FirstRun';
import { firstRunDone, markFirstRunDone } from './first-run/flag';
import { Benchmark } from './pages/Benchmark';
import { ComingSoon } from './pages/ComingSoon';
import { Hardware } from './pages/Hardware';
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

  // A benchmark saved since the last look changes the report (its PCIe link under load), so each tab change fetches it
  // again, quietly: no loading screen. Only once a scan has finished: while one is under way, or after a failed one,
  // get_report could start another full scan whose result nobody wants.
  const refresh = () => {
    if (scan.state !== 'ready') return;
    void getReport().then(
      (report) => setScan((current) => (current.state === 'ready' ? { state: 'ready', report } : current)),
      () => undefined,
    );
  };

  const openGuide = (topic: BiosTopic) => {
    refresh();
    setGuideTopic(topic);
    setTab('toolbox');
  };

  const chooseTab = (next: Tab) => {
    if (firstRun) return;
    if (next !== 'toolbox') setGuideTopic(null);
    if (next !== tab) refresh();
    setTab(next);
  };

  const finishFirstRun = (report: Report, next: 'home' | 'benchmark') => {
    markFirstRunDone();
    setScan({ state: 'ready', report });
    setFirstRun(false);
    setTab(next);
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
        <main key={tab} className="content scroll">
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
          {report && tab === 'hardware' && <Hardware report={report} onGuide={openGuide} onRescan={() => void run(true)} />}
          {report && tab === 'toolbox' && <Toolbox guide={report.guide} focus={guideTopic} appVersion={report.appVersion} />}
          {report && tab === 'tweaks' && <ComingSoon {...strings.comingSoon.tweaks} />}
          {report && tab === 'lockIn' && <ComingSoon {...strings.comingSoon.lockIn} />}
          {report && tab === 'benchmark' && <Benchmark />}
        </main>
      )}
    </div>
  );
}
