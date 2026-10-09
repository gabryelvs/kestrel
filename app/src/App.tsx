import { useCallback, useEffect, useState } from 'react';
import { getReport, inTauri, isElevated, rescan } from './api/client';
import type { Report } from './api/types';
import { Banner } from './components/Banner';
import { TitleBar, type Tab } from './components/TitleBar';
import { ComingSoon } from './pages/ComingSoon';
import { Home } from './pages/Home';
import { strings } from './strings';

type Scan = { state: 'loading' } | { state: 'ready'; report: Report } | { state: 'failed'; message: string };

export function App() {
  const [tab, setTab] = useState<Tab>('home');
  const [scan, setScan] = useState<Scan>({ state: 'loading' });
  const [admin, setAdmin] = useState(false);

  const run = useCallback(async (fresh: boolean) => {
    setScan({ state: 'loading' });
    try {
      setScan({ state: 'ready', report: await (fresh ? rescan() : getReport()) });
    } catch (error) {
      setScan({ state: 'failed', message: String(error) });
    }
  }, []);

  useEffect(() => {
    void run(false);
    void isElevated().then(setAdmin);
  }, [run]);

  const report = scan.state === 'ready' ? scan.report : null;
  const build = report?.snapshot.os.build ?? 0;

  return (
    <div className="app">
      <TitleBar tab={tab} onTab={setTab} admin={admin} />
      {!inTauri() && <Banner>{strings.banners.sample}</Banner>}
      {build > 0 && build < 22000 && <Banner>{strings.banners.windows10}</Banner>}
      <main className="content scroll">
        {scan.state === 'loading' && <p className="center muted">{strings.loading.scanning}</p>}
        {scan.state === 'failed' && (
          <div className="center">
            <p>{strings.loading.failed}</p>
            <p className="muted">{scan.message}</p>
            <button className="btn btn-ghost cut-s" onClick={() => void run(true)}>
              {strings.loading.retry}
            </button>
          </div>
        )}
        {report && <Page tab={tab} report={report} onTab={setTab} onRescan={() => void run(true)} />}
      </main>
    </div>
  );
}

type PageProps = { tab: Tab; report: Report; onTab: (tab: Tab) => void; onRescan: () => void };

function Page({ tab, report, onTab, onRescan }: PageProps) {
  switch (tab) {
    case 'tweaks':
      return <ComingSoon {...strings.comingSoon.tweaks} />;
    case 'lockIn':
      return <ComingSoon {...strings.comingSoon.lockIn} />;
    case 'benchmark':
      return <ComingSoon {...strings.comingSoon.benchmark} />;
    default:
      return <Home report={report} onTab={onTab} onGuide={() => onTab('toolbox')} onRescan={onRescan} />;
  }
}
