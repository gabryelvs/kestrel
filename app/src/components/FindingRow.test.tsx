import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Finding } from '../api/types';
import { strings } from '../strings';
import { FindingRow } from './FindingRow';

const finding = (fix: Finding['fix']): Finding => ({
  id: 'game-mode', category: 'windows', severity: 'medium', status: 'problem', title: 'Game Mode is off', detail: '', fix,
});
const nothing = () => undefined;

type ElementProps = { children?: ReactNode; onClick?: () => void };
/** Every element under a node. FindingRow has no hooks, so calling it as a function gives its tree without rendering. */
function elements(node: ReactNode): ReactElement<ElementProps>[] {
  if (Array.isArray(node)) return node.flatMap((child) => elements(child));
  if (!isValidElement<ElementProps>(node)) return [];
  return [node, ...elements(node.props.children)];
}

describe('FindingRow', () => {
  it('offers a Fix button for a tweak that opens that tweak, and nothing else', () => {
    const calls: string[] = [];
    const row = FindingRow({
      finding: finding({ kind: 'tweak', tweak: 'game-mode' }),
      onTab: (tab) => calls.push(`tab ${tab}`),
      onGuide: (topic) => calls.push(`guide ${topic}`),
      onTweak: (tweak) => calls.push(`tweak ${tweak}`),
    });
    const buttons = elements(row).filter((element) => element.type === 'button');
    expect(buttons.map((button) => button.props.children)).toEqual([strings.health.fix]);
    buttons[0].props.onClick?.();
    expect(calls).toEqual(['tweak game-mode']);
  });

  it('draws the Fix button as a link, with no stage note beside it', () => {
    const html = renderToStaticMarkup(
      <FindingRow finding={finding({ kind: 'tweak', tweak: 'power-plan' })} onTab={nothing} onGuide={nothing} onTweak={nothing} />,
    );
    expect(html).toContain('<button class="link">Fix</button>');
    expect(html).not.toContain('Stage');
  });

  it('still names the stage of a fix that arrives later, and gives it no Fix button', () => {
    const html = renderToStaticMarkup(
      <FindingRow finding={finding({ kind: 'later-stage', stage: 6 })} onTab={nothing} onGuide={nothing} onTweak={nothing} />,
    );
    expect(html).toContain('Stage 6');
    expect(html).not.toContain('<button');
  });
});
