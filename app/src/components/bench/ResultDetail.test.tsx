import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Comparison } from '../../api/types';
import { SpreadWarning, TweakChanges } from './ResultDetail';

/** The sentence Copy results prints too (kestrel-bench's text.rs), for a spread of 24.9%. */
const warning = 'These runs vary by 24.9%, so only big changes will count as real. Close background apps and run again.';
const count = (html: string, text: string) => html.split(text).length - 1;

const none: Comparison = {
  baselineId: 'sample-1', currentId: 'sample-2', metrics: [], settingsDiffer: [], routeMismatch: false,
  tweaksAdded: [], tweaksRemoved: [], powerPlanChange: null,
};
const html = (comparison: Partial<Comparison>) => renderToStaticMarkup(<TweakChanges comparison={{ ...none, ...comparison }} />);

describe('SpreadWarning', () => {
  it('warns at a spread of 24.9%, in the words Copy results uses, as a caution like the settings one', () => {
    expect(renderToStaticMarkup(<SpreadWarning spread={0.249} />)).toBe(`<p class="caution">${warning}</p>`);
  });

  it('says nothing at 5%, nor at exactly 10%, only above it', () => {
    expect(renderToStaticMarkup(<SpreadWarning spread={0.05} />)).toBe('');
    expect(renderToStaticMarkup(<SpreadWarning spread={0.1} />)).toBe('');
    expect(renderToStaticMarkup(<SpreadWarning spread={0} />)).toBe('');
    expect(renderToStaticMarkup(<SpreadWarning spread={0.1001} />)).toContain('These runs vary by 10.0%, so only big changes');
  });

  it('never promises a gain', () => {
    expect(renderToStaticMarkup(<SpreadWarning spread={0.249} />)).not.toMatch(/improv|faster|boost/i);
  });
});

describe('TweakChanges', () => {
  it('names an added tweak by its title, not its id', () => {
    const out = html({ tweaksAdded: ['power-plan'] });
    expect(out).toBe('<p class="caution">Tweaks since then: added Kestrel power plan.</p>');
    expect(out).not.toContain('power-plan');
  });

  it('puts what was added and what was removed on one line, each list in the order it came', () => {
    const out = html({ tweaksAdded: ['power-plan', 'game-mode'], tweaksRemoved: ['gpu-preference'] });
    expect(out).toBe('<p class="caution">Tweaks since then: added Kestrel power plan, Game Mode; removed GPU preference.</p>');
    expect(html({ tweaksRemoved: ['pointer-precision'] })).toBe('<p class="caution">Tweaks since then: removed Enhance pointer precision off.</p>');
  });

  it('names the power plan on each side, on a line of its own after the tweaks', () => {
    const out = html({ tweaksAdded: ['power-plan'], powerPlanChange: ['EXM Free Power Plan V8', 'Kestrel'] });
    expect(count(out, '<p class="caution">')).toBe(2);
    expect(out.indexOf('Tweaks since then')).toBeLessThan(out.indexOf('Power plan since then'));
    expect(out).toContain('<p class="caution">Power plan since then: EXM Free Power Plan V8 → Kestrel.</p>');
    // The plan is its own fact: it needs no tweak change beside it.
    expect(html({ powerPlanChange: ['Balanced', 'Kestrel'] })).toBe('<p class="caution">Power plan since then: Balanced → Kestrel.</p>');
  });

  it('says nothing when nothing changed, which is also what a result with no list gives', () => {
    expect(html({})).toBe('');
  });

  it('shows an id it does not know as written, and keeps System Protection like any other', () => {
    expect(html({ tweaksAdded: ['from-the-future'], tweaksRemoved: ['system-protection'] })).toBe(
      '<p class="caution">Tweaks since then: added from-the-future; removed System Protection.</p>',
    );
  });

  it('treats a name an object already has as an unknown id, not as a title', () => {
    // A hand-edited file could say anything; none of these may crash, vanish or print a function.
    expect(html({ tweaksAdded: ['constructor', 'toString', '__proto__', 'hasOwnProperty'] })).toBe(
      '<p class="caution">Tweaks since then: added constructor, toString, __proto__, hasOwnProperty.</p>',
    );
  });

  it('escapes what a file says', () => {
    expect(html({ tweaksAdded: ['<b>x</b>'] })).toBe('<p class="caution">Tweaks since then: added &lt;b&gt;x&lt;/b&gt;.</p>');
  });
});
