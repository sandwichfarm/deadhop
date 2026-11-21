// Warnings an operator dismissed, remembered in this browser by their exact
// text: a warning that changes, or a new one, shows again.
const KEY = 'deadhop.dismissed-warnings';

export function loadDismissed(store: Storage = localStorage): string[] {
  try {
    const value: unknown = JSON.parse(store.getItem(KEY) ?? '[]');
    return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
  } catch {
    return [];
  }
}

export function saveDismissed(list: string[], store: Storage = localStorage): void {
  try {
    store.setItem(KEY, JSON.stringify(list));
  } catch {
    // Storage refused (private mode, quota): the dismissal lasts for this page only.
  }
}
