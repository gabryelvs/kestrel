// For tests only: what a run leaves unhandled. Node's `process` isn't in the app's types (tsconfig has the browser's only), so it is
// reached through globalThis. Nothing in the app imports this.
type Emitter = {
  on: (event: 'unhandledRejection', listener: (reason: unknown) => void) => void;
  off: (event: 'unhandledRejection', listener: (reason: unknown) => void) => void;
};

/** Collects the promise rejections nobody handled, until `stop`. */
export function watchUnhandledRejections(): { seen: unknown[]; stop: () => void } {
  const emitter = (globalThis as unknown as { process: Emitter }).process;
  const seen: unknown[] = [];
  const note = (reason: unknown) => {
    seen.push(reason);
  };
  emitter.on('unhandledRejection', note);
  return { seen, stop: () => emitter.off('unhandledRejection', note) };
}
