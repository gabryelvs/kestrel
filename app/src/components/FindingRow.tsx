import { openLink } from '../api/client';
import type { BiosTopic, Finding } from '../api/types';
import { strings } from '../strings';
import type { Tab } from './TitleBar';

type Props = { finding: Finding; onTab: (tab: Tab) => void; onGuide: (topic: BiosTopic) => void };

/** One problem on the home screen: severity tag, what's wrong, and the quickest way to fix it. */
export function FindingRow({ finding, onTab, onGuide }: Props) {
  const { fix } = finding;
  return (
    <div className="finding-row">
      <span className={`tag tag-${finding.severity} cut-s`}>{strings.health.severity[finding.severity]}</span>
      <span className="title">{finding.title}</span>
      {fix.kind === 'bios-guide' && (
        <button className="link" onClick={() => onGuide(fix.topic)}>{strings.health.biosGuide}</button>
      )}
      {fix.kind === 'later-stage' && <span className="muted small">{strings.home.stage(fix.stage)}</span>}
      {fix.kind === 'steps' && (
        <button className="link" onClick={() => onTab('health')}>{strings.health.howToFix}</button>
      )}
      {fix.kind === 'link' && (
        <button className="link" onClick={() => void openLink(fix.url)}>{fix.label}</button>
      )}
    </div>
  );
}
