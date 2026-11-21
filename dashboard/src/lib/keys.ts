// The service's keys and which relays and proxies each one uses. A relay or a
// proxy lists the keys that may use it; an empty list is every key.
import type { KeyInfo, ProxySettings, Settings } from './schemas';

export const ADMIN = 'admin';
const bare = (url: string) => url.replace(/\/$/, '');

/** The keys (ids) that use something assigned to `assigned`; empty is all of `ids`. */
export const usersOf = (assigned: string[] | undefined, ids: string[]): string[] => (assigned === undefined || assigned.length === 0 ? ids : assigned);

/** Which keys use a relay, by the address it is listed under (a trailing slash does not matter). */
export function relayUsers(relayKeys: Settings['relay_keys'], url: string, ids: string[]): string[] {
  const found = Object.entries(relayKeys).find(([u]) => bare(u) === bare(url));
  return usersOf(found?.[1], ids);
}

/** `assigned` with `id` switched on or off, in key order; everything on is stored as empty ("all keys"). */
export function toggled(assigned: string[], id: string, on: boolean, ids: string[]): string[] {
  const now = usersOf(assigned, ids);
  const next = ids.filter((k) => (k === id ? on : now.includes(k)));
  return next.length === ids.length ? [] : next;
}

/** The relay map as saved: only relays with a restriction, only relays that are listed. */
export function relayKeysFor(relays: string[], relayKeys: Settings['relay_keys'], ids: string[]): Settings['relay_keys'] {
  const out: Settings['relay_keys'] = {};
  for (const url of relays) {
    const users = relayUsers(relayKeys, url, ids);
    if (users.length < ids.length) out[url] = users;
  }
  return out;
}

/** A stable text of a relay map, to tell whether it changed. */
export const relayKeysText = (m: Settings['relay_keys']) =>
  JSON.stringify(
    Object.entries(m)
      .map(([u, k]) => [bare(u), [...k].sort()] as const)
      .sort((a, b) => a[0].localeCompare(b[0])),
  );

/** The relays one key connects to. */
export const relaysOf = (relays: string[], relayKeys: Settings['relay_keys'], id: string, ids: string[]) =>
  relays.filter((r) => relayUsers(relayKeys, r, ids).includes(id));

/** The proxies one key uses. */
export const proxiesOf = (proxies: ProxySettings, id: string, ids: string[]) => proxies.list.filter((p) => usersOf(p.keys, ids).includes(id));

/** Why this assignment cannot be saved, or an empty string. */
export function assignmentProblem(keys: KeyInfo[], form: Settings): string {
  const ids = keys.map((k) => k.id);
  for (const k of keys) {
    if (relaysOf(form.relays, form.relay_keys, k.id, ids).length === 0) return `No relay is assigned to ${k.label}: it could not be reached. Give it one.`;
  }
  if (form.proxies.transparent || form.proxies.list.length === 0) return '';
  for (const k of keys) {
    if (proxiesOf(form.proxies, k.id, ids).length === 0) return `No proxy is assigned to ${k.label}: it would connect without one. Give it a proxy.`;
  }
  return '';
}

const TAGS: Record<string, string> = { invoices: 'Invoices', admin: 'Admin' };
/** A short name for a key id, for badges. */
export const keyTag = (id: string) => TAGS[id] ?? id;
