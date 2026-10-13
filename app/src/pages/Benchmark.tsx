import { useCallback, useEffect, useState } from 'react';
import { benchDismiss, benchList, benchProfiles, benchStart, benchState, onBenchState } from '../api/client';
import type { BenchmarkSummary, BenchState, GameProfile } from '../api/types';
import { ResultDetail } from '../components/bench/ResultDetail';
import { ResultsList } from '../components/bench/ResultsList';
import { RouteCard } from '../components/bench/RouteCard';
import { SessionPanel } from '../components/bench/SessionPanel';
import { strings } from '../strings';

/** Stage 2: start a benchmark, follow the session, and look through saved results (newest first). */
export function Benchmark() {
  const [profile, setProfile] = useState<GameProfile | null>(null);
  const [items, setItems] = useState<BenchmarkSummary[]>([]);
  const [session, setSession] = useState<BenchState | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [revealId, setRevealId] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async (select?: string | null) => {
    const list = await benchList();
    setItems(list);
    setSelected((current) => select ?? (current && list.some((i) => i.id === current) ? current : list[0]?.id ?? null));
  }, []);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void benchProfiles().then((profiles) => alive && setProfile(profiles[0] ?? null));
    void benchState().then((state) => alive && setSession(state));
    void refresh();
    void onBenchState((state) => {
      setSession(state);
      if (state?.phase === 'done' && state.resultId) void refresh(state.resultId);
    }).then((stop) => (alive ? (unlisten = stop) : stop()));
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [refresh]);

  const start = async () => {
    if (!profile) return;
    setStarting(true);
    setError(null);
    try {
      setSession(await benchStart(profile.id));
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(false);
    }
  };

  // "See the result" brings the new result into view; a plain list click leaves the scroll position alone.
  const close = (resultId: string | null) => {
    void benchDismiss().then(() => {
      setSession(null);
      setRevealId(resultId);
      void refresh(resultId);
    });
  };

  const choose = (id: string) => {
    setRevealId(null);
    setSelected(id);
  };

  return (
    <div className="page bench">
      <header className="page-head">
        <h1>{strings.bench.title}</h1>
        <p className="muted">{strings.bench.intro}</p>
      </header>
      {error && <p className="notice cut-s">{error}</p>}
      {session ? <SessionPanel state={session} onClose={close} /> : profile && <RouteCard profile={profile} starting={starting} onStart={() => void start()} />}
      <ResultsList items={items} selected={selected} onSelect={choose} />
      {selected && <ResultDetail key={selected} id={selected} onChanged={() => void refresh()} revealWhenLoaded={revealId === selected} />}
    </div>
  );
}
