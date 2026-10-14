import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ownerReport } from '../../api/owner-report';
import { GraphicsPanels } from './GraphicsPanels';

describe('GraphicsPanels', () => {
  it('names the adapters that aren’t graphics cards on one dim line: virtual ones, DisplayLink docks, a card without its driver', () => {
    const owner = ownerReport();
    const [parsec, rtx] = owner.snapshot.gpus;
    const displayLink = { ...parsec, name: 'DisplayLink USB Device', pnpDeviceId: 'USB\\VID_17E9&PID_4301\\0001' };
    const basic = { ...parsec, name: 'Microsoft Basic Display Adapter', pnpDeviceId: 'PCI\\VEN_1002&DEV_73BF\\6&1B3F2C1A&0&00000019' };
    const report = { ...owner, snapshot: { ...owner.snapshot, gpus: [parsec, rtx, displayLink, basic] } };
    const html = renderToStaticMarkup(<GraphicsPanels report={report} failed={false} onGuide={() => undefined} />);
    expect(html).toContain(
      '<p class="muted small">Other adapters: Parsec Virtual Display Adapter, DisplayLink USB Device, Microsoft Basic Display Adapter</p>',
    );
    expect(html).not.toContain('Virtual adapters');
  });
});
