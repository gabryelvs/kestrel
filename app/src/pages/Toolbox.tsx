import { useEffect, useRef } from 'react';
import { openLink } from '../api/client';
import type { BiosGuide, BiosTopic } from '../api/types';
import { strings } from '../strings';

export function Toolbox({ guide, focus }: { guide: BiosGuide; focus: BiosTopic | null }) {
  const sections = useRef(new Map<BiosTopic, HTMLElement>());

  useEffect(() => {
    if (focus) sections.current.get(focus)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, [focus]);

  return (
    <div className="page">
      <header className="page-head">
        <h1>{strings.toolbox.title}</h1>
        <p className="muted">{strings.toolbox.intro(guide.board)}</p>
        <p>
          {strings.toolbox.enterKey(guide.enterKey)} <span className="muted">{guide.tip}</span>
        </p>
      </header>
      {guide.sections.map((section) => (
        <section
          key={section.topic}
          ref={(el) => {
            if (el) sections.current.set(section.topic, el);
          }}
          className={`panel guide${section.topic === focus ? ' guide-focus' : ''}`}
        >
          <h2>{section.title}</h2>
          <p className="muted">
            <span className="label">{strings.toolbox.why}</span> {section.why}
          </p>
          <ol className="steps">
            {section.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
          {section.caution && (
            <p className="caution">
              <span className="label">{strings.toolbox.careful}</span> {section.caution}
            </p>
          )}
          {section.link && (
            <button className="link" onClick={() => void openLink(section.link!.url)}>{section.link.label}</button>
          )}
        </section>
      ))}
      <p className="muted small">{strings.toolbox.later}</p>
    </div>
  );
}
