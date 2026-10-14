import { specsText } from '../../api/specs';
import type { Report } from '../../api/types';
import { strings } from '../../strings';
import { CopyButton } from '../CopyButton';

/** Copies a short plain-text summary of the PC, to paste in a chat or a forum (decision 77). */
export function CopySpecs({ report }: { report: Report }) {
  const t = strings.specs;
  return <CopyButton variant="primary" label={t.copy} getText={() => specsText(report)} copied={t.copied} failed={t.failed} />;
}
