import type { ReportLoad } from '../../api/tweaks';
import { strings } from '../../strings';
import { CopyButton } from '../CopyButton';

const t = strings.tweaks;

type Props = { report: ReportLoad; unreadable: number; onRetry: () => void };

/**
 * The Kestrel report, newest first, with Copy report. The text is Rust's, already laid out (its placeholder lines are
 * indented like entries), so it is shown as it is and never parsed. Copy report copies what is on screen.
 *
 * When part of the journal couldn't be read, the report text itself ends with two lines saying so (kestrel-tweaks' report.rs).
 * The page's own warning is only for while that text isn't here: still being read, or not readable.
 */
export function ReportPanel({ report, unreadable, onRetry }: Props) {
  return (
    <section className="panel report-panel">
      <div className="panel-head">
        <h2 className="label">{t.report}</h2>
        {report.state === 'ready' && <CopyButton label={t.copyReport} getText={() => report.text} copied={t.copied} failed={t.copyFailed} />}
      </div>
      {unreadable > 0 && report.state !== 'ready' && <p className="notice cut-s wrap">{t.unreadable}</p>}
      {report.state === 'loading' && <p className="muted small">{t.reportLoading}</p>}
      {report.state === 'failed' && (
        <>
          <p className="notice cut-s wrap">{`${t.reportFailed} ${report.message}`}</p>
          <div>
            <button className="btn btn-ghost btn-small cut-s" onClick={onRetry}>{t.tryAgain}</button>
          </div>
        </>
      )}
      {report.state === 'ready' && <pre className="report num">{report.text}</pre>}
    </section>
  );
}
