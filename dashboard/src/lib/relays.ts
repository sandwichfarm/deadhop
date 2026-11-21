// Helpers for the relay history: windows, matching a relay by address, error rates.
import type { RelayHistory, RelayStats } from './schemas';

export type HistoryWindow = RelayHistory['window'];

export const WINDOWS: { value: HistoryWindow; label: string }[] = [
  { value: '15m', label: '15 min' },
  { value: '1h', label: '1 hour' },
  { value: '12h', label: '12 hours' },
  { value: '1d', label: '1 day' },
  { value: '1w', label: '1 week' },
];

const bare = (url: string) => url.replace(/\/$/, '');

/** One relay's results in a history, matched by address (a trailing slash does not matter). */
export function statsFor(history: RelayHistory | null, url: string): RelayStats | null {
  return history?.relays.find((s) => bare(s.relay) === bare(url)) ?? null;
}

export interface ErrorRate {
  failed: number;
  total: number;
  /** `0%`, `2.5%`, `100%`, or a dash when nothing was recorded. */
  text: string;
  /** Green with no failures, orange with a few, red with many; none without data. */
  tone: 'ok' | 'warn' | 'bad' | 'none';
}

/** Failed health checks and failed replies, together, out of everything recorded. */
export function errorRate(s: RelayStats): ErrorRate {
  const total = s.checks + s.sends;
  const failed = s.check_failures + s.send_failures;
  if (total === 0) return { failed, total, text: '—', tone: 'none' };
  const share = (100 * failed) / total;
  const text = `${failed > 0 && share < 10 ? share.toFixed(1) : Math.round(share)}%`;
  return { failed, total, text, tone: failed === 0 ? 'ok' : share < 20 ? 'warn' : 'bad' };
}
