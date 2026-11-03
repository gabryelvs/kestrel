import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import {
  benchList, tweaksApply, tweaksApplyAgain, tweaksPreview, tweaksRemovePlan, tweaksReport, tweaksRestoreAnyway, tweaksRestorePoint, tweaksRevertAll,
  tweaksUndo, tweaksUsePlanAgain, tweaksView,
} from '../api/client';
import { continueAt, runApply, settle, type ApplyAt, type ApplyNext, type ApplyStep } from '../api/apply';
import {
  boundRow, canFocus, closedReason, currentFlow, errorText, flowLocks, forgetChoices, isTicked, measureNote, measureText, onlyTicked, rereadsOnFocus,
  revertRows, rowNote, tickedIds, type Choices, type Flow, type MeasureNote, type ReportLoad, type Settings, type ViewLoad,
} from '../api/tweaks';
import type { ItemOutcome, PreviewView, TweakId, TweaksView } from '../api/types';
import { Outcomes } from '../components/tweaks/Outcomes';
import { PreviewPanel } from '../components/tweaks/PreviewPanel';
import { ReportPanel } from '../components/tweaks/ReportPanel';
import { RestoreStep } from '../components/tweaks/RestoreStep';
import { RevertConfirm } from '../components/tweaks/RevertConfirm';
import { TweakRow, type RowActions } from '../components/tweaks/TweakRow';
import type { Tab } from '../components/TitleBar';
import { STAGE, strings } from '../strings';

const t = strings.tweaks;

/** What the page says while Apply waits on a command. */
const STEP_TEXT: Record<ApplyStep, string> = { restoring: t.restoring, applying: t.applying, rescanning: t.rescanning };

/** Everything a button on the page can ask for. The page below decides what each one does. */
export type Handlers = RowActions & {
  preview: () => void;
  apply: () => void;
  /** Turn System Protection on and create the restore point. */
  turnOn: () => void;
  /** Go on with Kestrel's undo only, past the restore point. */
  goOn: () => void;
  /** Leaves the step the page is at (a Preview, a question, a result). */
  cancel: () => void;
  askRevert: () => void;
  revert: () => void;
  benchmark: () => void;
  retry: () => void;
};

type BodyProps = {
  view: TweaksView;
  /** Reading the view again failed, with this reason, and `view` is the last good one: it may be out of date. */
  stale?: string;
  report: ReportLoad;
  /** The note above the rows, if the starting point needs one. */
  measure: MeasureNote | null;
  choices: Choices;
  flow: Flow;
  on: Handlers;
};

/** The step below the buttons, or null when there is none. A step about a row the page shows is said in that row instead. */
function flowPanel(flow: Flow, view: TweaksView, on: Handlers): ReactNode {
  const bound = boundRow(flow);
  const inRow = bound !== null && view.rows.some((row) => row.id === bound);
  switch (flow.step) {
    case 'idle':
    case 'ask-restore-anyway':
      return null;
    case 'busy':
      return inRow ? null : (
        <section className="panel busy">
          <p role="status">{flow.text}</p>
        </section>
      );
    case 'preview':
      return <PreviewPanel preview={flow.preview} onApply={on.apply} onCancel={on.cancel} />;
    case 'restore':
      return <RestoreStep question={flow.question} on={{ turnOn: on.turnOn, goOn: on.goOn, cancel: on.cancel }} />;
    case 'ask-revert':
      return <RevertConfirm rows={revertRows(view.rows)} onRevert={on.revert} onCancel={on.cancel} />;
    case 'applied':
      return <Outcomes outcomes={flow.outcomes} restart={flow.restart} measure={flow.outcomes.some((outcome) => outcome.ok)} onBenchmark={on.benchmark} onClose={on.cancel} />;
    case 'results':
      return <Outcomes outcomes={flow.outcomes} restart={false} measure={false} onBenchmark={on.benchmark} onClose={on.cancel} />;
    case 'row-result':
      // System Protection's row goes with its record, so an Undo there has no row left to answer in.
      return inRow ? null : <Outcomes outcomes={[flow.outcome]} restart={false} measure={false} onBenchmark={on.benchmark} onClose={on.cancel} />;
    case 'failed':
      return inRow ? null : (
        <section className="panel failed-panel">
          <p className="notice cut-s wrap">{flow.message}</p>
          {flow.settings === 'unchanged' && <p className="muted small">{t.noSettingsChanged}</p>}
          {flow.settings === 'protection-may-have-changed' && <p className="small">{t.protectionMayHaveChanged}</p>}
          <div className="actions">
            <button className="btn btn-ghost cut-s" onClick={on.cancel}>{t.close}</button>
          </div>
        </section>
      );
  }
}

/** Opens below the buttons, which can be under the fold of a short window: brings itself into view when the step changes. */
function Slot({ stepKey, children }: { stepKey: string; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.scrollIntoView?.({ block: 'nearest' });
  }, [stepKey]);
  return <div ref={ref} className="tweak-slot">{children}</div>;
}

/**
 * The page once the view is read: notes, the Essential rows, the tiers to come, the buttons with the reason when Apply can't
 * run, the step Apply or an undo is at, and the report. It holds no state of its own, so a test can put any view in it.
 */
export function TweaksBody({ view, stale, report, measure, choices, flow: asked, on }: BodyProps) {
  // A question about a row that is no longer what it asked about is over: it can't be answered, and it would lock the page.
  const flow = currentFlow(asked, view.rows);
  // A closed gate, or rows that may be out of date: nothing that changes the PC may be pressed, and the reason stands beside the buttons.
  const why = closedReason(view, stale);
  const closed = why !== null;
  const locked = flowLocks(flow);
  const ticked = tickedIds(view.rows, choices);
  const asking = flow.step === 'ask-restore-anyway' ? flow.id : null;
  const panel = flowPanel(flow, view, on);
  return (
    <>
      {measure && (
        <div className="notice cut-s measure">
          <p>{measureText(measure)}</p>
          <button className="btn btn-ghost btn-small cut-s" onClick={on.benchmark}>{t.openBenchmark}</button>
        </div>
      )}
      {view.otherAccount && <p className="notice cut-s">{t.otherAccount}</p>}
      {view.pendingOtherAccounts > 0 && <p className="notice cut-s">{t.otherPending(view.pendingOtherAccounts)}</p>}
      {view.interrupted.length > 0 && (
        <ul className="notice cut-s interrupted">
          {view.interrupted.map((line, i) => (
            <li key={`${i}-${line}`}>{line}</li>
          ))}
        </ul>
      )}
      <section className="panel tweak-panel">
        <h2 className="label">{t.essential}</h2>
        <ul className="tweak-list">
          {view.rows.map((row) => (
            <TweakRow
              key={row.id}
              row={row}
              ticked={isTicked(row, choices)}
              locked={locked}
              closed={closed}
              asking={asking === row.id}
              note={rowNote(flow, row.id)}
              on={on}
            />
          ))}
        </ul>
      </section>
      <section className="panel tiers">
        {t.laterTiers.map((name) => (
          <p key={name} className="tier">
            <span>{name}</span> <span className="muted small">{strings.comingSoon.arrives(STAGE.competitive)}</span>
          </p>
        ))}
      </section>
      <div className="tweak-bar">
        <button className="btn btn-primary cut-s" disabled={closed || locked || ticked.length === 0} onClick={on.preview}>{t.preview(ticked.length)}</button>
        {view.anyActive && (
          <button className="btn btn-ghost cut-s" disabled={closed || locked} onClick={on.askRevert}>{t.revertEverything}</button>
        )}
        {why && <span className="tweak-gate wrap">{why}</span>}
        {closed && <button className="btn btn-ghost btn-small cut-s" onClick={on.retry}>{t.tryAgain}</button>}
      </div>
      {panel && <Slot stepKey={`${flow.step}:${boundRow(flow) ?? ''}`}>{panel}</Slot>}
      <ReportPanel report={report} unreadable={view.unreadableLines} onRetry={on.retry} />
    </>
  );
}

type PageProps = Omit<BodyProps, 'view'> & { load: ViewLoad };

/** The page: its title and intro, then the rows, or that they are being read, or why they couldn't be. */
export function TweaksPage({ load, ...rest }: PageProps) {
  return (
    <div className="page tweaks">
      <header className="page-head">
        <h1>{t.title}</h1>
        <p className="muted">{t.intro}</p>
      </header>
      {load.state === 'loading' && <p className="muted">{t.loading}</p>}
      {load.state === 'failed' && (
        <section className="panel load-failed">
          <p>{t.loadFailed}</p>
          <p className="muted small wrap">{load.message}</p>
          <div>
            <button className="btn btn-ghost cut-s" onClick={rest.on.retry}>{t.tryAgain}</button>
          </div>
        </section>
      )}
      {load.state === 'ready' && <TweaksBody view={load.view} stale={load.stale} {...rest} />}
    </div>
  );
}

type Props = {
  /** The tweak a Fix button asked for: its Preview opens, if it can be applied. */
  focus: TweakId | null;
  onFocusUsed: () => void;
  /**
   * Something changed, so the scan is stale: Health, Home and the next benchmark's "PC at the time" need a new one. Answers when a scan
   * that started after the change is done, however it ended, and the page waits for that before it shows what was done.
   */
  onRescan: () => Promise<void>;
  onTab: (tab: Tab) => void;
};

/**
 * The Tweaks tab. Nothing on the PC changes before a button on it is pressed; reading the page writes only to Kestrel's journal.
 * Every command that changes something asks the gate again, so any of them can come back refused, and the page says so where it
 * was pressed and reads the view again. Whatever a step is waiting on, no step is left spinning.
 */
export function Tweaks({ focus, onFocusUsed, onRescan, onTab }: Props) {
  const [load, setLoad] = useState<ViewLoad>({ state: 'loading' });
  const [report, setReport] = useState<ReportLoad>({ state: 'loading' });
  const [measure, setMeasure] = useState<MeasureNote | null>(null);
  const [choices, setChoices] = useState<Choices>({});
  const [flow, setFlow] = useState<Flow>({ step: 'idle' });
  const newest = useRef(0);

  /**
   * Reads the view and the report again. The newest read wins, so a slow old one never shows over a newer one. A view that can't be
   * read again is kept and marked, so what was just done stays on screen; only a view that was never read is a failure.
   */
  const reload = useCallback(async () => {
    newest.current += 1;
    const mine = newest.current;
    const [view, text] = await Promise.allSettled([tweaksView(), tweaksReport()]);
    if (mine !== newest.current) return;
    setLoad((current) =>
      view.status === 'fulfilled'
        ? { state: 'ready', view: view.value }
        : current.state === 'ready'
          ? { ...current, stale: errorText(view.reason) }
          : { state: 'failed', message: errorText(view.reason) },
    );
    setReport(text.status === 'fulfilled' ? { state: 'ready', text: text.value } : { state: 'failed', message: errorText(text.reason) });
  }, []);

  // The saved benchmarks are read once, for the note about the starting point. If they can't be, there is no note.
  useEffect(() => {
    void reload();
    void benchList().then(
      (list) => setMeasure(measureNote(list)),
      () => undefined,
    );
  }, [reload]);

  // What a closed gate (VALORANT open) or an unreadable view waits for happens outside Kestrel, so coming back to the window reads it again.
  const rereads = rereadsOnFocus(load, flow);
  useEffect(() => {
    if (!rereads) return;
    const back = () => void reload();
    window.addEventListener('focus', back);
    return () => window.removeEventListener('focus', back);
  }, [rereads, reload]);

  const rows = load.state === 'ready' ? load.view.rows : [];

  /** A command was refused or failed: say so where the person pressed, read the page again, and leave nothing spinning. */
  const fail = async (error: unknown, where: { id?: TweakId; settings?: Settings } = {}) => {
    setFlow({ step: 'failed', message: errorText(error), ...where });
    await reload();
  };

  const openPreview = async (ids: TweakId[]) => {
    setFlow({ step: 'busy', text: t.previewing });
    try {
      setFlow({ step: 'preview', preview: await tweaksPreview(ids) });
    } catch (error) {
      await fail(error, { settings: 'unchanged' });
    }
  };

  // A Fix button: the Preview of that tweak alone, if it can be applied and Apply could run. Otherwise just the tab.
  useEffect(() => {
    if (focus === null || load.state !== 'ready') return;
    onFocusUsed();
    if (canFocus(load.view, focus)) {
      setChoices(onlyTicked(load.view.rows, focus));
      void openPreview([focus]);
    }
    // Once per Fix: the parent clears `focus` as it is used, so this does not run again for the same one.
  }, [focus, load]);

  /** One go at Apply (the order is in `runApply`), and what the page shows for how it came out. */
  const go = async (preview: PreviewView, at: ApplyAt) => {
    let next: ApplyNext;
    try {
      next = await runApply({ restorePoint: tweaksRestorePoint, apply: tweaksApply, rescan: onRescan }, preview, at, (step) =>
        setFlow({ step: 'busy', text: STEP_TEXT[step] }),
      );
    } catch (error) {
      await fail(error);
      return;
    }
    switch (next.next) {
      case 'ask':
        setFlow({ step: 'restore', preview, question: next.question });
        // Turning System Protection on may have worked before the point failed.
        if (next.question.kind === 'failed') void reload();
        return;
      case 'applied':
        await reload();
        setChoices((current) =>
          forgetChoices(
            current,
            preview.items.map((item) => item.id),
          ),
        );
        setFlow({ step: 'applied', outcomes: next.outcomes, restart: next.restart });
        return;
      case 'stopped':
        setFlow({ step: 'failed', message: next.message, settings: next.settings });
        await reload();
        return;
    }
  };

  /** A row's own button: its outcome is said in that row, once the scan that follows a change is done. */
  const act = async (id: TweakId, command: () => Promise<ItemOutcome>) => {
    setFlow({ step: 'busy', text: t.working, id });
    try {
      const outcome = await command();
      await settle([outcome], onRescan);
      await reload();
      setChoices((current) => forgetChoices(current, [id]));
      setFlow({ step: 'row-result', id, outcome });
    } catch (error) {
      await fail(error, { id });
    }
  };

  const revert = async () => {
    setFlow({ step: 'busy', text: t.undoing });
    try {
      const outcomes = await tweaksRevertAll();
      await settle(outcomes, onRescan);
      await reload();
      setChoices({});
      setFlow({ step: 'results', outcomes });
    } catch (error) {
      await fail(error);
    }
  };

  const cancel = () => setFlow({ step: 'idle' });

  const on: Handlers = {
    tick: (id, ticked) => setChoices((current) => ({ ...current, [id]: ticked })),
    undo: (id) => void act(id, () => tweaksUndo(id)),
    applyAgain: (id) => void act(id, () => tweaksApplyAgain(id)),
    askRestoreAnyway: (id) => setFlow({ step: 'ask-restore-anyway', id }),
    restoreAnyway: (id) => void act(id, () => tweaksRestoreAnyway(id)),
    stopAsking: cancel,
    usePlanAgain: () => void act('power-plan', tweaksUsePlanAgain),
    removePlan: () => void act('power-plan', tweaksRemovePlan),
    preview: () => void openPreview(tickedIds(rows, choices)),
    apply: () => {
      if (flow.step === 'preview') void go(flow.preview, 'pressed');
    },
    turnOn: () => {
      if (flow.step === 'restore') void go(flow.preview, 'turn-on');
    },
    // Continue after a failed point goes on; Continue without (System Protection off, or a policy) is the only skip.
    goOn: () => {
      if (flow.step === 'restore') void go(flow.preview, continueAt(flow.question));
    },
    cancel,
    // With nothing it would undo (only rows something else changed), there is nothing to ask; the results say what was left alone.
    askRevert: () => (revertRows(rows).length > 0 ? setFlow({ step: 'ask-revert' }) : void revert()),
    revert: () => void revert(),
    benchmark: () => onTab('benchmark'),
    retry: () => {
      if (load.state === 'failed') setLoad({ state: 'loading' });
      void reload();
    },
  };

  return <TweaksPage load={load} report={report} measure={measure} choices={choices} flow={flow} on={on} />;
}
