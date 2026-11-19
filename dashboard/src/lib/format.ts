// Presentation helpers. Pure functions only.
export { formatBtc, formatSats, receivedSummary, statusLabel, timeLeft } from '@cvmpay/client';

/** Unix seconds -> `2026-11-18 12:30 UTC`. */
export function formatTime(unix: number): string {
  return `${new Date(unix * 1000).toISOString().slice(0, 16).replace('T', ' ')} UTC`;
}

/** `abcdef…123456` */
export function shortKey(hex: string): string {
  return hex.length <= 16 ? hex : `${hex.slice(0, 8)}…${hex.slice(-6)}`;
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
