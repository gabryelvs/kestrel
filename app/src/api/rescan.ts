// How the scan that follows a change is bounded. A scan takes about 2 seconds on a PC like the owner's, and nothing in Kestrel or in
// the probe puts a limit on one, so a scan that never ended would keep the page on "Checking your PC again…" for good and every
// later change would wait behind it. These bounds stop that, and what they cost is small: the page shows its result a little before
// the scan that follows it is done, and a scan that lands after it was let go is left out.
import { coalesce } from './coalesce';

/** The longest the page waits for the scan that follows a change before it shows what was done. */
export const SCAN_WAIT_MS = 20_000;

/** How long the app lets a scan run before letting it go, so that a scan that hangs doesn't keep the next one from starting. */
export const SCAN_LIMIT_MS = 60_000;

/**
 * The scan that follows a change, made to run without the loading screen. `scan` runs one at a time (see `coalesce`) and `show` gets
 * what it found, unless that scan was let go as hung: it landed after the ones that came after it began, so what it found is older.
 * The answer is when a scan that began after the call is done or let go, and it never rejects.
 */
export function quietRescan<T>(scan: () => Promise<T>, show: (found: T) => void): () => Promise<void> {
  return coalesce(async (letGo) => {
    const found = await scan();
    if (!letGo.aborted) show(found);
  }, SCAN_LIMIT_MS);
}
