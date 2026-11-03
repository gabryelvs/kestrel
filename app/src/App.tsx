import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getReport, inTauri, isElevated, rescan } from './api/client';
import { quietRescan } from './api/rescan';
import type { BiosTopic, Report, TweakId } from './api/types';
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
import { Tweaks } from './pages/Tweaks';
import { strings } from './strings';

type Scan = { state: 'loading' } | { state: 'ready'; report: Report } | { state: 'failed'; message: string };

export function App() {
  const [tab, setTab] = useState<Tab>('home');
  const [scan, setScan] = useState<Scan>({ state: 'loading' });
  const [admin, setAdmin] = useState<boolean | null>(null);
  const [guideTopic, setGuideTopic] = useState<BiosTopic | null>(null);
  // The tweak a Fix button asked for. The Tweaks page opens its Preview for it, if it can be applied, and clears this.
  const [tweakFocus, setTweakFocus] = useState<TweakId | null>(null);
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

  // After an Apply or an undo, the scan is stale: Health, Home and the next benchmark's "PC at the time" must see what changed. The
  // Tweaks page keeps its result on screen, and the loading screen of Scan again would take the page away, so this scans quietly.
  // The scans never overlap (two could land out of order, in the UI and in the app's own copy of the report): one runs at a time, and
  // changes made meanwhile share the one more that follows. The page waits for its scan (for a while, see api/rescan.ts), so a
  // benchmark started right after records the PC as it is now; a scan that fails leaves the old report and answers all the same,
  // and one that never ends is let go after a minute instead of holding up every later one.
  const rescanQuietly = useMemo(
    () => quietRescan(rescan, (report) => setScan((current) => (current.state === 'ready' ? { state: 'ready', report } : current))),
    [],
  );

  const openGuide = (topic: BiosTopic) => {
    refresh();
    setGuideTopic(topic);
    setTab('toolbox');
  };

  const openTweak = (tweak: TweakId) => {
    refresh();
    setTweakFocus(tweak);
    setTab('tweaks');
  };

  const chooseTab = (next: Tab) => {
    if (firstRun) return;
    if (next !== 'toolbox') setGuideTopic(null);
    if (next !== 'tweaks') setTweakFocus(null);
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
          {report && tab === 'home' && <Home report={report} onTab={chooseTab} onGuide={openGuide} onTweak={openTweak} onRescan={() => void run(true)} />}
          {report && tab === 'health' && <Health report={report} onGuide={openGuide} onTweak={openTweak} />}
          {report && tab === 'hardware' && <Hardware report={report} onGuide={openGuide} onRescan={() => void run(true)} />}
          {report && tab === 'toolbox' && <Toolbox guide={report.guide} focus={guideTopic} appVersion={report.appVersion} />}
          {report && tab === 'tweaks' && <Tweaks focus={tweakFocus} onFocusUsed={() => setTweakFocus(null)} onRescan={rescanQuietly} onTab={chooseTab} />}
          {report && tab === 'lockIn' && <ComingSoon {...strings.comingSoon.lockIn} />}
          {report && tab === 'benchmark' && <Benchmark />}
        </main>
      )}
    </div>
  );
}
