import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import {
  benchList, tweaksApply, tweaksApplyAgain, tweaksPreview, tweaksRemovePlan, tweaksReport, tweaksRestoreAnyway, tweaksRestorePoint, tweaksRevertAll,
  tweaksUndo, tweaksUsePlanAgain, tweaksView,
} from '../api/client';
import {
  boundRow, canFocus, errorText, flowLocks, forgetChoices, gateReason, isTicked, measureNote, measureText, needsRestart, onlyTicked, restoreFailure,
  restoreRoute, revertRows, rowNote, tickedIds, type Choices, type Flow, type MeasureNote, type ReportLoad, type ViewLoad,
} from '../api/tweaks';
import type { ItemOutcome, PreviewView, RestoreChoice, TweakId, TweaksView } from '../api/types';
import { Outcomes } from '../components/tweaks/Outcomes';
import { PreviewPanel } from '../components/tweaks/PreviewPanel';
import { ReportPanel } from '../components/tweaks/ReportPanel';
import { RestoreStep } from '../components/tweaks/RestoreStep';
import { RevertConfirm } from '../components/tweaks/RevertConfirm';
import { TweakRow, type RowActions } from '../components/tweaks/TweakRow';
import type { Tab } from '../components/TitleBar';
import { STAGE, strings } from '../strings';

const t = strings.tweaks;

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
          {flow.unchanged && <p className="muted small">{t.nothingChanged}</p>}
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
export function TweaksBody({ view, report, measure, choices, flow, on }: BodyProps) {
  const closed = view.gate !== 'open';
  const locked = flowLocks(flow);
  const ticked = tickedIds(view.rows, choices);
  const why = gateReason(view.gate);
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
        {why && <span className="tweak-gate">{why}</span>}
        {view.gate === 'cant-tell' && <button className="btn btn-ghost btn-small cut-s" onClick={on.retry}>{t.tryAgain}</button>}
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
      {load.state === 'ready' && <TweaksBody view={load.view} {...rest} />}
    </div>
  );
}

type Props = {
  /** The tweak a Fix button asked for: its Preview opens, if it can be applied. */
  focus: TweakId | null;
  onFocusUsed: () => void;
  /** Something changed, so the scan is stale: Health, Home and the next benchmark's "PC at the time" need a new one. */
  onRescan: () => void;
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

  /** Reads the view and the report again. The newest read wins, so a slow old one never shows over a newer one. */
  const reload = useCallback(async () => {
    newest.current += 1;
    const mine = newest.current;
    const [view, text] = await Promise.allSettled([tweaksView(), tweaksReport()]);
    if (mine !== newest.current) return;
    setLoad(view.status === 'fulfilled' ? { state: 'ready', view: view.value } : { state: 'failed', message: errorText(view.reason) });
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

  const rows = load.state === 'ready' ? load.view.rows : [];

  /** A command was refused or failed: say so where the person pressed, read the page again, and leave nothing spinning. */
  const fail = async (error: unknown, where: { id?: TweakId; unchanged?: boolean } = {}) => {
    setFlow({ step: 'failed', message: errorText(error), ...where });
    await reload();
  };

  const openPreview = async (ids: TweakId[]) => {
    setFlow({ step: 'busy', text: t.previewing });
    try {
      setFlow({ step: 'preview', preview: await tweaksPreview(ids) });
    } catch (error) {
      await fail(error, { unchanged: true });
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

  /** The changes of a Preview, after the restore point step. */
  const applyChanges = async (preview: PreviewView) => {
    setFlow({ step: 'busy', text: t.applying });
    try {
      const ids = preview.items.map((item) => item.id);
      const outcomes = await tweaksApply(ids);
      if (outcomes.some((outcome) => outcome.ok)) onRescan();
      await reload();
      setChoices((current) => forgetChoices(current, ids));
      setFlow({ step: 'applied', outcomes, restart: needsRestart(outcomes, preview.items) });
    } catch (error) {
      // The restore point may have been made, but the command that changes the settings refused: nothing was changed.
      await fail(error, { unchanged: true });
    }
  };

  /** The restore point step, then the changes. A failed point is a question (go on without?), not an error; an error stops here. */
  const makeRestorePoint = async (choice: RestoreChoice, preview: PreviewView) => {
    setFlow({ step: 'busy', text: choice === 'skip' ? t.applying : t.restoring });
    try {
      const why = restoreFailure(await tweaksRestorePoint(choice));
      if (why !== null) {
        setFlow({ step: 'restore', preview, question: { kind: 'failed', reason: why } });
        // Turning System Protection on may have worked before the point failed.
        void reload();
        return;
      }
    } catch (error) {
      await fail(error, { unchanged: true });
      return;
    }
    await applyChanges(preview);
  };

  /** A row's own button: its outcome is said in that row. */
  const act = async (id: TweakId, command: () => Promise<ItemOutcome>) => {
    setFlow({ step: 'busy', text: t.working, id });
    try {
      const outcome = await command();
      if (outcome.ok) onRescan();
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
      if (outcomes.some((outcome) => outcome.ok)) onRescan();
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
      if (flow.step !== 'preview') return;
      const route = restoreRoute(flow.preview.restore);
      if (route === 'create') void makeRestorePoint('create', flow.preview);
      else setFlow({ step: 'restore', preview: flow.preview, question: { kind: route === 'turn-on' ? 'protection-off' : 'blocked-by-policy' } });
    },
    turnOn: () => {
      if (flow.step === 'restore') void makeRestorePoint('turn-on-and-create', flow.preview);
    },
    goOn: () => {
      if (flow.step !== 'restore') return;
      // A failed point is already in the journal. Going on without a question asked of Windows records that it was skipped.
      if (flow.question.kind === 'failed') void applyChanges(flow.preview);
      else void makeRestorePoint('skip', flow.preview);
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
