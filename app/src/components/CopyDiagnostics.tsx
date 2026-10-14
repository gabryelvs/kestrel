import { getDiagnostics } from '../api/client';
import { strings } from '../strings';
import { CopyButton } from './CopyButton';

/** Copies the full report as JSON so a friend can paste it to the owner (spec decision 10). */
export function CopyDiagnostics() {
  const t = strings.diagnostics;
  return <CopyButton label={t.copy} getText={getDiagnostics} copied={t.copied} failed={t.failed} />;
}
