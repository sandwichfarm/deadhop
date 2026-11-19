// Presentation helpers. Pure functions only.
import type { InvoiceStatus } from './schemas.ts';

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

/** Remaining time until `expiresAt`, e.g. `59m 10s`; empty once passed. */
export function timeLeft(expiresAt: number, now: number): string {
  const s = expiresAt - now;
  if (s <= 0) return '';
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m ${s % 60}s`;
}

const LABELS: Record<InvoiceStatus, string> = {
  pending: 'Awaiting payment',
  detected: 'Payment seen, confirming',
  confirmed: 'Paid',
  expired: 'Expired',
};

/** Human label for an invoice status. */
export function statusLabel(status: InvoiceStatus): string {
  return LABELS[status];
}

/** One-line summary of what has arrived for an invoice. */
export function receivedSummary(inv: { received_sat: number; confirmed_sat: number; confirmations_required: number }): string {
  const n = inv.confirmations_required;
  return `Received ${formatSats(inv.received_sat)}, confirmed ${formatSats(inv.confirmed_sat)} (needs ${n} confirmation${n === 1 ? '' : 's'}).`;
}
