import type { ReactNode } from 'react';
import type { BiosTopic } from '../../api/types';
import { strings } from '../../strings';
import type { Card, Fact } from './view';

const h = strings.hardwarePage;

type PanelProps = { title: string; failed?: boolean; aside?: ReactNode; className?: string; children: ReactNode };

/** One Hardware panel: its label (and whatever sits at the right of it), the notice when its probe failed, its content. */
export function HwPanel({ title, failed = false, aside, className, children }: PanelProps) {
  return (
    <section className={`panel hw-panel${className ? ` ${className}` : ''}`}>
      <div className="panel-head">
        <h2 className="label">{title}</h2>
        {aside}
      </div>
      {failed && <p className="notice cut-s">{h.panelFailed}</p>}
      {children}
    </section>
  );
}

/** A device's name at the top of its panel; Not reported, in grey, when Windows gave none. */
export function DeviceName({ name, children }: { name: string | null; children?: ReactNode }) {
  return (
    <h3 className={`hw-name${name === null ? ' tone-muted' : ''}`}>
      {name ?? h.notReported}
      {children}
    </h3>
  );
}

/** A panel's label–value lines. A missing value says Not reported, in grey; a flagged line links its BIOS-guide topic. */
export function Facts({ facts, onGuide }: { facts: Fact[]; onGuide?: (topic: BiosTopic) => void }) {
  return (
    <dl className="facts">
      {facts.map(({ label, value, tone, note, topic }) => (
        <div key={label} className="fact">
          <dt>{label}</dt>
          <dd>
            <span className={value === null || tone === 'muted' ? 'tone-muted' : `num${tone ? ` tone-${tone}` : ''}`}>{value ?? h.notReported}</span>
            {note && <span className="muted small">{note}</span>}
            {topic && onGuide && <button className="link" onClick={() => onGuide(topic)}>{h.biosGuide}</button>}
          </dd>
        </div>
      ))}
    </dl>
  );
}

type CardPanelsProps<C extends Card> = {
  title: string;
  cards: C[];
  failed: boolean;
  /** What a panel holds below its device's name. */
  body: (card: C) => ReactNode;
  /** Sits beside a device's name (the Primary tag). */
  badge?: (card: C) => ReactNode;
  /** Goes under the last panel: the dim line naming the virtual adapters. */
  footer?: ReactNode;
};

/**
 * A panel per device (a graphics card, a monitor, a drive, a network adapter), with the notice when its probe failed on
 * the first one only. With no device at all, one panel that says Not reported keeps the failure notice and the footer.
 */
export function CardPanels<C extends Card>({ title, cards, failed, body, badge, footer }: CardPanelsProps<C>) {
  if (cards.length === 0) {
    return (
      <HwPanel title={title} failed={failed}>
        <DeviceName name={null} />
        {footer}
      </HwPanel>
    );
  }
  return (
    <>
      {cards.map((card, i) => (
        <HwPanel key={`${card.name}-${i}`} title={title} failed={failed && i === 0}>
          <DeviceName name={card.name}>{badge?.(card)}</DeviceName>
          {body(card)}
          {i === cards.length - 1 && footer}
        </HwPanel>
      ))}
    </>
  );
}
