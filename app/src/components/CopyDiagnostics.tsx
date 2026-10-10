import { IconClipboard } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { getDiagnostics } from '../api/client';
import { strings } from '../strings';

/** Copies the full report as JSON so a friend can paste it to the owner (spec decision 10). */
export function CopyDiagnostics() {
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    if (!note) return;
    const id = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(id);
  }, [note]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await getDiagnostics());
      setNote(strings.diagnostics.copied);
    } catch {
      setNote(strings.diagnostics.failed);
    }
  };

  return (
    <span className="copy">
      <button className="btn btn-ghost cut-s" onClick={() => void copy()}>
        <IconClipboard size={14} aria-hidden /> {strings.diagnostics.copy}
      </button>
      {note && <span className="toast" role="status">{note}</span>}
    </span>
  );
}
