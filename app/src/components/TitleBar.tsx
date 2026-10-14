import { IconMinus, IconShieldCheck, IconShieldOff, IconSquare, IconX } from '@tabler/icons-react';
import { inTauri } from '../api/client';
import { strings } from '../strings';
import { Logo } from './Logo';

export type Tab = 'home' | 'health' | 'hardware' | 'tweaks' | 'lockIn' | 'benchmark' | 'toolbox';
export const TABS: Tab[] = ['home', 'health', 'hardware', 'tweaks', 'lockIn', 'benchmark', 'toolbox'];

async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  if (!inTauri()) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow()[action]();
}

export function TitleBar({ tab, onTab, admin }: { tab: Tab; onTab: (tab: Tab) => void; admin: boolean | null }) {
  return (
    <header className="titlebar" data-tauri-drag-region="deep">
      <div className="brand">
        <Logo size={16} />
        <span>{strings.app.name}</span>
      </div>
      <nav className="tabs" aria-label={strings.titleBar.sections}>
        {TABS.map((t) => (
          <button key={t} className={`tab cut-s${t === tab ? ' tab-active' : ''}`} aria-current={t === tab ? 'page' : undefined} onClick={() => onTab(t)}>
            {strings.tabs[t]}
          </button>
        ))}
      </nav>
      <span className="admin num">
        {admin !== null && (
          <>
            {admin ? <IconShieldCheck size={14} color="var(--accent)" aria-hidden /> : <IconShieldOff size={14} aria-hidden />}
            {admin ? strings.titleBar.admin : strings.titleBar.notAdmin}
          </>
        )}
      </span>
      <div className="window-buttons">
        <button aria-label={strings.titleBar.minimize} onClick={() => void windowAction('minimize')}>
          <IconMinus size={14} />
        </button>
        <button aria-label={strings.titleBar.maximize} onClick={() => void windowAction('toggleMaximize')}>
          <IconSquare size={12} />
        </button>
        <button aria-label={strings.titleBar.close} className="close" onClick={() => void windowAction('close')}>
          <IconX size={14} />
        </button>
      </div>
    </header>
  );
}
