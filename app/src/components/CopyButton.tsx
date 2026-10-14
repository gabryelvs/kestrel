import { IconClipboard } from '@tabler/icons-react';
import { useEffect, useState } from 'react';

type Props = {
  label: string;
  /** The text to put on the clipboard, read when the button is clicked. */
  getText: () => string | Promise<string>;
  /** The note shown after a copy, and after one that failed. */
  copied: string;
  failed: string;
  variant?: 'primary' | 'ghost';
};

/** A button that copies text to the clipboard and says what happened beside itself for 3 seconds. */
export function CopyButton({ label, getText, copied, failed, variant = 'ghost' }: Props) {
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    if (!note) return;
    const id = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(id);
  }, [note]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await getText());
      setNote(copied);
    } catch {
      setNote(failed);
    }
  };

  return (
    <span className="copy">
      <button className={`btn btn-${variant} cut-s`} onClick={() => void copy()}>
        <IconClipboard size={14} aria-hidden /> {label}
      </button>
      {note && <span className="toast" role="status">{note}</span>}
    </span>
  );
}
