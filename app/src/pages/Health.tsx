import { IconAlertTriangle, IconCheck, IconHelpCircle, IconInfoCircle } from '@tabler/icons-react';
import { useState, type ReactElement } from 'react';
import { openLink } from '../api/client';
import { groupByCategory } from '../api/findings';
import type { BiosTopic, Finding, Report, Status } from '../api/types';
import { strings } from '../strings';

const ICONS: Record<Status, ReactElement> = {
  pass: <IconCheck size={16} color="var(--accent)" aria-hidden />,
  problem: <IconAlertTriangle size={16} color="var(--warn)" aria-hidden />,
  info: <IconInfoCircle size={16} color="var(--muted)" aria-hidden />,
  unknown: <IconHelpCircle size={16} color="var(--muted)" aria-hidden />,
};

export function Health({ report, onGuide }: { report: Report; onGuide: (topic: BiosTopic) => void }) {
  const errors = report.snapshot.probeErrors;
  return (
    <div className="page">
      <header className="page-head">
        <h1>{strings.health.title}</h1>
        <p className="muted">{strings.health.intro}</p>
      </header>
      {errors.length > 0 && <p className="notice cut-s">{strings.health.probeErrors(errors.length)}</p>}
      {groupByCategory(report.findings).map((group) => (
        <section key={group.category} className="panel group">
          <h2 className="label">{strings.health.categories[group.category]}</h2>
          {group.findings.map((finding) => (
            <CheckRow key={finding.id} finding={finding} onGuide={onGuide} />
          ))}
        </section>
      ))}
    </div>
  );
}

function CheckRow({ finding, onGuide }: { finding: Finding; onGuide: (topic: BiosTopic) => void }) {
  const [open, setOpen] = useState(false);
  const { fix } = finding;
  return (
    <div className={`check check-${finding.status}`}>
      <span className="status-icon" title={strings.health.status[finding.status]}>
        {ICONS[finding.status]}
      </span>
      <div>
        <div className="check-title">
          {finding.title}
          {finding.status === 'problem' && (
            <span className={`tag tag-${finding.severity} cut-s`}>{strings.health.severity[finding.severity]}</span>
          )}
        </div>
        {finding.detail && <p className="muted small">{finding.detail}</p>}
        {fix.kind === 'steps' && open && (
          <ol className="steps">
            {fix.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        )}
      </div>
      <div className="check-action">
        {fix.kind === 'bios-guide' && <button className="link" onClick={() => onGuide(fix.topic)}>{strings.health.biosGuide}</button>}
        {fix.kind === 'later-stage' && <span className="muted small">{strings.health.laterStage(fix.stage)}</span>}
        {fix.kind === 'steps' && (
          <button className="link" aria-expanded={open} onClick={() => setOpen(!open)}>{strings.health.howToFix}</button>
        )}
        {fix.kind === 'link' && <button className="link" onClick={() => void openLink(fix.url)}>{fix.label}</button>}
      </div>
    </div>
  );
}
