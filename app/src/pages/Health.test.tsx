import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ownerReport } from '../api/owner-report';
import type { Finding } from '../api/types';
import { strings } from '../strings';
import { Health } from './Health';

const count = (html: string, text: string) => html.split(text).length - 1;
const nothing = () => undefined;

// The owner's report, plus a Game Mode check whose fix is a tweak. Its own fixes: BIOS guides for bios-age and iommu,
// and "stage 6" for adapter-power.
const gameMode: Finding = {
  id: 'game-mode', category: 'windows', severity: 'medium', status: 'problem', title: 'Game Mode is off',
  detail: 'Game Mode tells Windows to favour the game you’re playing.', fix: { kind: 'tweak', tweak: 'game-mode' },
};

describe('Health', () => {
  it('gives a check whose fix is a tweak a Fix button, and the other fixes theirs', () => {
    const report = ownerReport();
    const html = renderToStaticMarkup(<Health report={{ ...report, findings: [...report.findings, gameMode] }} onGuide={nothing} onTweak={nothing} />);
    // The status icon's tooltip also says "Fix", so count the buttons.
    expect(count(html, `<button class="link">${strings.health.fix}</button>`)).toBe(1);
    expect(count(html, `>${strings.health.biosGuide}</button>`)).toBe(2);
    expect(html).toContain(strings.health.laterStage(6));
  });

  it('shows no Fix button when nothing is fixed by a tweak', () => {
    const html = renderToStaticMarkup(<Health report={ownerReport()} onGuide={nothing} onTweak={nothing} />);
    expect(count(html, `<button class="link">${strings.health.fix}</button>`)).toBe(0);
  });
});
