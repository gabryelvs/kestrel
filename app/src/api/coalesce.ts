/**
 * Wraps `task` so its runs never overlap and never pile up: a call made while a run is under way waits for it and is answered by
 * one more run that starts after it, and every call made meanwhile shares that one more run. A call is only ever answered by a run
 * that began after the call, which is what a scan after a change needs: one that began before could have read the old state.
 *
 * The answer never rejects, because a run that failed only ends itself: the next call (or the one that waited) runs again.
 */
export function coalesce(task: () => Promise<unknown>): () => Promise<void> {
  type Run = { answered: Promise<void>; answer: () => void };
  const newRun = (): Run => {
    let answer!: () => void;
    const answered = new Promise<void>((resolve) => {
      answer = resolve;
    });
    return { answered, answer };
  };

  /** The run under way, and the one that waits for it. */
  let current: Run | null = null;
  let following: Run | null = null;

  const start = (run: Run) => {
    current = run;
    void (async () => {
      try {
        await task();
      } catch {
        // A failed run only ends itself.
      }
      current = null;
      const next = following;
      following = null;
      // Nobody can get in between this run and the next: while a caller waits, a run is always under way.
      if (next) start(next);
      run.answer();
    })();
  };

  return () => {
    if (current === null) {
      const run = newRun();
      start(run);
      return run.answered;
    }
    following ??= newRun();
    return following.answered;
  };
}
