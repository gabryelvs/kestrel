import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { strings } from '../strings';
import { TABS, TitleBar, tabsHeld } from './TitleBar';

it('puts the Hardware tab between Health and Tweaks', () => {
  expect(TABS.map((tab) => strings.tabs[tab])).toEqual(['Home', 'Health', 'Hardware', 'Tweaks', 'Lock In', 'Benchmark', 'Toolbox']);
});

describe('while a Tweaks step waits on a command', () => {
  const render = (busy: boolean) => renderToStaticMarkup(createElement(TitleBar, { tab: 'tweaks', onTab: () => undefined, admin: true, busy }));
  /** Each button in the title bar: its class and whether it is off. */
  const buttons = (html: string) => [...html.matchAll(/<button([^>]*)>/g)].map(([, attrs]) => ({ tab: /class="tab /.test(attrs), off: /\sdisabled=""/.test(attrs) }));

  it('ignores a tab change, since a tab change remounts the page and loses what the command comes back with', () => {
    expect(tabsHeld(false, true)).toBe(true);
    expect(tabsHeld(false, false)).toBe(false);
  });

  it('ignores one during the first run too, busy or not', () => {
    expect(tabsHeld(true, false)).toBe(true);
    expect(tabsHeld(true, true)).toBe(true);
  });

  it('turns every tab off and says why, where the tabs are', () => {
    const html = render(true);
    const tabs = buttons(html).filter((button) => button.tab);
    expect(tabs).toHaveLength(TABS.length);
    expect(tabs.every((tab) => tab.off)).toBe(true);
    expect(html).toContain(strings.titleBar.busy);
    expect(html.indexOf(strings.titleBar.busy)).toBeGreaterThan(html.indexOf('</nav>'));
    // The tab the person is on still says so.
    expect(html).toContain('aria-current="page"');
  });

  it('leaves the window’s own buttons on, so Kestrel can always be closed', () => {
    const others = buttons(render(true)).filter((button) => !button.tab);
    expect(others).toHaveLength(3);
    expect(others.some((button) => button.off)).toBe(false);
  });

  it('turns no tab off and says nothing otherwise', () => {
    const html = render(false);
    expect(buttons(html).some((button) => button.off)).toBe(false);
    expect(html).not.toContain(strings.titleBar.busy);
  });
});
