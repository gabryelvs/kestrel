import { useEffect, useState } from 'react';
import { getCpuClocks } from '../../api/client';
import { shouldPoll } from '../../api/hardware';
import type { CoreClock } from '../../api/types';
import { strings } from '../../strings';
import { HwPanel } from './HwPanel';
import { clockView } from './view';

const h = strings.hardwarePage;

/**
 * Each physical core's clock, read once a second, but only while Kestrel's window is visible and focused (a game in
 * front means it isn't); otherwise the last values stay, dimmed, under Paused. The page is mounted only while the
 * Hardware tab is shown, so leaving the tab stops the reads as well.
 */
export function LiveClocks({ maxMhz }: { maxMhz: number | null }) {
  const [clocks, setClocks] = useState<CoreClock[] | null>(null);
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');
  const [focused, setFocused] = useState(() => document.hasFocus());
  const polling = shouldPoll(true, visible, focused);

  useEffect(() => {
    const onVisibility = () => setVisible(document.visibilityState === 'visible');
    const onFocus = () => setFocused(true);
    const onBlur = () => setFocused(false);
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('focus', onFocus);
    window.addEventListener('blur', onBlur);
    return () => {
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('blur', onBlur);
    };
  }, []);

  useEffect(() => {
    if (!polling) return;
    let alive = true;
    const tick = async () => {
      try {
        const next = await getCpuClocks();
        if (alive) setClocks(next);
      } catch {
        // Keep the last values; the next tick tries again.
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [polling]);

  const view = clockView(clocks ?? [], maxMhz, polling);
  const c = h.clocks;
  return (
    <HwPanel
      title={h.panels.clocks}
      className={view.paused ? 'clocks-paused' : undefined}
      aside={<span className={`small num ${view.paused ? 'muted' : 'live'}`}>{view.badge}</span>}
    >
      {clocks === null ? (
        <p className="muted small num">…</p>
      ) : view.highest === null ? (
        <p className="tone-muted small">{h.notReported}</p>
      ) : (
        <>
          <ol className="clock-cells">
            {view.cells.map((cell) => (
              <li key={cell.label} className="clock-cell cut-s">
                <span className="muted small">
                  {cell.label}
                  {cell.tag && <span className="clock-kind"> {cell.tag}</span>}
                </span>
                <span className="num">{cell.text}</span>
                <span className="clock-bar" aria-hidden="true">
                  <span style={{ width: `${cell.percent}%` }} />
                </span>
              </li>
            ))}
          </ol>
          <p className="small num">
            <span className="muted">{c.average}</span> {view.average} · <span className="muted">{c.highest}</span> {view.highest}
          </p>
        </>
      )}
    </HwPanel>
  );
}
