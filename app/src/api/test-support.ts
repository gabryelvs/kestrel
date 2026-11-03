// For tests only: what a run leaves unhandled, and a way to press a button where there is no DOM. Node's `process` isn't in the app's
// types (tsconfig has the browser's only), so it is reached through globalThis. Nothing in the app imports this.
import { isValidElement, type ReactElement, type ReactNode } from 'react';

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

/** The props of an element a test looks for: its words and what it does when pressed. */
export type ElementProps = { children?: ReactNode; onClick?: () => void };

/**
 * Every element of one kind (`'button'`) in what a component function returned, in order. The tests run without a DOM, so a press is the
 * element's own `onClick` called: only components with no hooks can be called this way, and a component inside the tree is not opened.
 */
export function elementsOf(node: ReactNode, type: string, found: ReactElement<ElementProps>[] = []): ReactElement<ElementProps>[] {
  if (Array.isArray(node)) node.forEach((child) => elementsOf(child, type, found));
  else if (isValidElement<ElementProps>(node)) {
    if (node.type === type) found.push(node);
    elementsOf(node.props.children, type, found);
  }
  return found;
}
