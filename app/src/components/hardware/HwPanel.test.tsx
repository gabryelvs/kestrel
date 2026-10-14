import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { strings } from '../../strings';
import { CardPanels, Facts } from './HwPanel';
import type { Card } from './view';

const h = strings.hardwarePage;
const count = (html: string, text: string) => html.split(text).length - 1;
const cards: Card[] = [{ name: 'Samsung SSD 980 PRO 1TB', facts: [] }, { name: 'WDC WD20EZBX-00AYRA0', facts: [] }];

describe('CardPanels', () => {
  it('makes one panel per card, with the failure notice on the first and the footer after the last', () => {
    const html = renderToStaticMarkup(
      <CardPanels title="Storage" cards={cards} failed footer={<p>Dim line</p>} body={(card) => <p>{`Body of ${card.name}`}</p>} />,
    );
    expect(count(html, '<section')).toBe(2);
    expect(count(html, '>Storage</h2>')).toBe(2);
    expect(count(html, h.panelFailed)).toBe(1);
    expect(html.indexOf(h.panelFailed)).toBeLessThan(html.indexOf('WDC WD20EZBX-00AYRA0'));
    expect(count(html, 'Dim line')).toBe(1);
    expect(html.indexOf('Dim line')).toBeGreaterThan(html.indexOf('Body of WDC WD20EZBX-00AYRA0'));
  });

  it('shows no notice while nothing failed, and the badge beside each name', () => {
    const html = renderToStaticMarkup(
      <CardPanels title="Displays" cards={cards} failed={false} badge={(card) => <b>{`Tag ${card.name}`}</b>} body={() => null} />,
    );
    expect(count(html, h.panelFailed)).toBe(0);
    expect(count(html, '<b>Tag ')).toBe(2);
  });

  it('shows one panel that says Not reported when there are no cards, still with the notice and the footer', () => {
    const html = renderToStaticMarkup(<CardPanels title="Network" cards={[]} failed footer={<p>Dim line</p>} body={() => null} />);
    expect(count(html, '<section')).toBe(1);
    expect(html).toContain(`<h3 class="hw-name tone-muted">${h.notReported}</h3>`);
    expect(count(html, h.panelFailed)).toBe(1);
    expect(count(html, 'Dim line')).toBe(1);
  });
});

describe('Facts', () => {
  const facts = [
    { label: 'Driver', value: null },
    { label: 'Now', value: '1.0 x16', tone: 'warn' as const, note: 'The card slows its link when idle.', topic: 'pcie-slot' as const },
    { label: 'XMP', value: 'On', tone: 'good' as const },
    { label: 'Under load', value: 'Run a benchmark to check the link under load.', tone: 'muted' as const },
  ];

  it('says Not reported in grey for a missing value, and colours the others by tone', () => {
    const html = renderToStaticMarkup(<Facts facts={facts} />);
    expect(html).toContain(`<dt>Driver</dt><dd><span class="tone-muted">${h.notReported}</span></dd>`);
    expect(html).toContain('<span class="num tone-warn">1.0 x16</span>');
    expect(html).toContain('<span class="num tone-good">On</span>');
    // A hint is grey text, not a mono value.
    expect(html).toContain('<span class="tone-muted">Run a benchmark to check the link under load.</span>');
    expect(html).toContain('The card slows its link when idle.');
  });

  it('links the BIOS guide only for a line with a topic, and only when it can open one', () => {
    expect(count(renderToStaticMarkup(<Facts facts={facts} onGuide={() => undefined} />), `>${h.biosGuide}</button>`)).toBe(1);
    expect(count(renderToStaticMarkup(<Facts facts={facts} />), h.biosGuide)).toBe(0);
  });
});
