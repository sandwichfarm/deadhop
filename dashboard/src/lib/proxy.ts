// Proxies as form fields and back. The service stores each URL and shows its
// password as `***`; the password stays with the proxy's id unless re-entered.
import type { ProxiesPatch, ProxyInput } from './api';
import type { ProxyEntry, ProxySettings } from './schemas';
export type ProxyKind = 'none' | 'socks5' | 'https' | 'http';

export interface ProxyFields {
  kind: ProxyKind;
  host: string;
  port: string;
  username: string;
  password: string;
  /** The service already holds a password for this proxy. */
  hasPassword: boolean;
}

export const NO_PROXY: ProxyFields = { kind: 'none', host: '', port: '', username: '', password: '', hasPassword: false };

const SCHEMES: Record<string, ProxyKind> = { 'socks5:': 'socks5', 'socks5h:': 'socks5', 'https:': 'https', 'http:': 'http' };
const DEFAULT_PORT: Record<string, string> = { 'https:': '443', 'http:': '80' };

export function parseProxy(url: string): ProxyFields {
  if (url === '') return { ...NO_PROXY };
  let u: URL;
  try {
    u = new URL(url);
  } catch {
    return { ...NO_PROXY };
  }
  return {
    kind: SCHEMES[u.protocol] ?? 'none',
    host: u.hostname.replace(/^\[|\]$/g, ''),
    port: u.port || (DEFAULT_PORT[u.protocol] ?? ''),
    username: decodeURIComponent(u.username),
    password: '',
    hasPassword: u.password !== '',
  };
}

/** The URL for these fields, or an error message. */
export function buildProxy(f: ProxyFields): { url: string } | { error: string } {
  if (f.kind === 'none') return { url: '' };
  const host = f.host.trim();
  if (!/^[A-Za-z0-9.\-:]+$/.test(host)) return { error: 'Enter the proxy host (name or IP address).' };
  const port = Number(f.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return { error: 'Enter a proxy port between 1 and 65535.' };
  if (f.hasPassword && f.password === '' && f.username !== '') return { error: 'Re-enter the proxy password to change the proxy.' };
  const shownHost = host.includes(':') ? `[${host}]` : host;
  const user = f.username === '' ? '' : encodeURIComponent(f.username);
  const pass = f.password === '' ? '' : `:${encodeURIComponent(f.password)}`;
  const auth = user === '' ? '' : `${user}${pass}@`;
  return { url: `${f.kind}://${auth}${shownHost}:${port}` };
}

/** Most proxies in the list. */
export const MAX_PROXIES = 8;
/** Shortest and longest interval the service accepts, in seconds. */
export const MIN_SECS = 60;
export const MAX_SECS = 604_800;
/** Ids of proxies that have not been saved yet start with this. */
export const NEW_PREFIX = 'new:';

export const isNew = (e: ProxyEntry) => e.id.startsWith(NEW_PREFIX);

let counter = 0;
/** A proxy that is not saved yet. */
export function newEntry(name: string, url: string): ProxyEntry {
  counter += 1;
  return { id: `${NEW_PREFIX}${counter}`, name, url };
}

/** `list` with the item at `from` moved to position `to`. */
export function move<T>(list: T[], from: number, to: number): T[] {
  if (from === to || to < 0 || to >= list.length) return list;
  const out = [...list];
  out.splice(to, 0, out.splice(from, 1)[0]);
  return out;
}

/**
 * The URL after editing a saved proxy. With the password hidden and nothing
 * else about the address changed, the hidden form is kept (the service restores
 * the password from the id); anything else needs the password again.
 */
export function editedUrl(old: string, f: ProxyFields): { url: string } | { error: string } {
  const before = parseProxy(old);
  const same = (['kind', 'host', 'port', 'username'] as const).every((k) => f[k] === before[k]);
  if (same && f.password === '' && before.hasPassword) return { url: old };
  return buildProxy(f);
}

export type Unit = 's' | 'm' | 'h' | 'd';
const UNIT_SECS: Record<Unit, number> = { s: 1, m: 60, h: 3600, d: 86_400 };

/** A number of seconds as the largest unit that divides it evenly. */
export function splitDuration(secs: number): { n: number; unit: Unit } {
  const unit = (['d', 'h', 'm'] as const).find((u) => secs > 0 && secs % UNIT_SECS[u] === 0) ?? 's';
  return { n: secs / UNIT_SECS[unit], unit };
}

export const toSecs = (n: number, unit: Unit) => n * UNIT_SECS[unit];

/** Seconds as short text: `15m`, `1h 30m`. */
export function shortDuration(secs: number): string {
  const { n, unit } = splitDuration(secs);
  if (unit !== 's' || secs < 60) return `${n}${unit}`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

const entryKey = (e: ProxyEntry) => `${e.id}\n${e.name}\n${e.url}`;

/** What differs from the live proxy settings, or the problem with the form. */
export function proxiesPatch(live: ProxySettings, form: ProxySettings): ProxiesPatch | string {
  const patch: ProxiesPatch = {};
  if (form.list.map(entryKey).join('\u0000') !== live.list.map(entryKey).join('\u0000')) {
    patch.list = form.list.map((e): ProxyInput => (isNew(e) ? { name: e.name, url: e.url } : { id: e.id, name: e.name, url: e.url }));
  }
  if (form.mode !== live.mode) patch.mode = form.mode;
  if (form.offline !== live.offline) patch.offline = form.offline;
  if (form.transparent !== live.transparent) patch.transparent = form.transparent;
  for (const k of ['rotate_secs', 'chaos_min_secs', 'chaos_max_secs'] as const) {
    const v = form[k];
    if (!Number.isInteger(v)) return 'Fill in every interval.';
    if (v < MIN_SECS || v > MAX_SECS) return 'Intervals run from 1 minute to 7 days.';
    if (v !== live[k]) patch[k] = v;
  }
  if (form.chaos_min_secs > form.chaos_max_secs) return 'The shortest chaos interval is longer than the longest.';
  return patch;
}
