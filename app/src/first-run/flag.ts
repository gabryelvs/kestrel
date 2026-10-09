const KEY = 'kestrel.firstRunDone';

export function firstRunDone(): boolean {
  try {
    return localStorage.getItem(KEY) === '1';
  } catch {
    return false;
  }
}

export function markFirstRunDone(): void {
  try {
    localStorage.setItem(KEY, '1');
  } catch {
    // Storage can be unavailable; the guided scan will simply show again next time.
  }
}
