// Presentation helpers. Pure functions only.
import type { Status } from './schemas';

/** `123456789` -> `1.23456789 BTC`. */
export function formatBtc(sat: number): string {
  const whole = Math.floor(sat / 100_000_000);
  const frac = String(sat % 100_000_000).padStart(8, '0');
  return `${whole}.${frac} BTC`;
}

/** `1234567` -> `1,234,567 sats`. */
export function formatSats(sat: number): string {
  return `${String(sat).replace(/\B(?=(\d{3})+$)/g, ',')} sats`;
}

/** Unix seconds -> `2026-11-18 12:30 UTC`. */
export function formatTime(unix: number): string {
  return `${new Date(unix * 1000).toISOString().slice(0, 16).replace('T', ' ')} UTC`;
}

/** Remaining time until `expiresAt`, e.g. `59m 10s`; empty once passed. */
export function timeLeft(expiresAt: number, now: number): string {
  const s = expiresAt - now;
  if (s <= 0) return '';
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m ${s % 60}s`;
}

/** `abcdef…123456` */
export function shortKey(hex: string): string {
  return hex.length <= 16 ? hex : `${hex.slice(0, 8)}…${hex.slice(-6)}`;
}

const LABELS: Record<Status, string> = {
  pending: 'Awaiting payment',
  detected: 'Payment seen, confirming',
  confirmed: 'Paid',
  expired: 'Expired',
};

export function statusLabel(status: Status): string {
  return LABELS[status];
}

/** One-line summary of what has arrived for an invoice. */
export function receivedSummary(inv: { received_sat: number; confirmed_sat: number; confirmations_required: number }): string {
  const n = inv.confirmations_required;
  return `Received ${formatSats(inv.received_sat)}, confirmed ${formatSats(inv.confirmed_sat)} (needs ${n} confirmation${n === 1 ? '' : 's'}).`;
}

/** `93784` -> `1d 2h`, `3700` -> `1h 1m`, `59` -> `59s`. */
export function formatDuration(secs: number): string {
  const d = Math.floor(secs / 86_400);
  const h = Math.floor((secs % 86_400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${secs}s`;
}

/** `1536` -> `1.5 KB`. */
export function formatBytes(n: number): string {
  const units = ['B', 'KB', 'MB', 'GB'];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return i === 0 ? `${n} B` : `${v.toFixed(1)} ${units[i]}`;
}
