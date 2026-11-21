import { describe, expect, it } from 'vitest';
import { assignmentProblem, keyTag, proxiesOf, relayKeysFor, relayKeysText, relayUsers, relaysOf, toggled, usersOf } from './keys';
import { keyInfos, settings } from '../test-helpers';

const ids = ['invoices', 'admin'];

describe('who uses what', () => {
  it('reads an empty or missing assignment as every key', () => {
    expect(usersOf(undefined, ids)).toEqual(ids);
    expect(usersOf([], ids)).toEqual(ids);
    expect(usersOf(['admin'], ids)).toEqual(['admin']);
  });

  it('finds a relay by its address, ignoring a trailing slash', () => {
    const map = { 'wss://a.example/': ['admin'] };
    expect(relayUsers(map, 'wss://a.example', ids)).toEqual(['admin']);
    expect(relayUsers(map, 'wss://b.example', ids)).toEqual(ids);
  });

  it('switches one key on or off and stores everything-on as empty', () => {
    expect(toggled([], 'admin', false, ids)).toEqual(['invoices']);
    expect(toggled(['invoices'], 'admin', true, ids)).toEqual([]);
    expect(toggled(['invoices'], 'invoices', true, ids)).toEqual(['invoices']);
    expect(toggled(['admin'], 'invoices', true, ids)).toEqual([]);
  });

  it('keeps only restricted relays that are listed, and compares maps without regard to order', () => {
    const map = { 'wss://a.example': ['admin', 'invoices'], 'wss://b.example': ['invoices'], 'wss://gone.example': ['admin'] };
    const kept = relayKeysFor(['wss://a.example', 'wss://b.example'], map, ids);
    expect(kept).toEqual({ 'wss://b.example': ['invoices'] });
    expect(relayKeysText({ 'wss://b.example/': ['admin', 'invoices'], 'wss://a.example': ['invoices'] })).toBe(relayKeysText({ 'wss://a.example': ['invoices'], 'wss://b.example': ['invoices', 'admin'] }));
  });

  it("lists one key's relays and proxies", () => {
    const relays = ['wss://a.example', 'wss://b.example'];
    expect(relaysOf(relays, { 'wss://b.example': ['invoices'] }, 'admin', ids)).toEqual(['wss://a.example']);
    const proxies = { ...settings().proxies, list: [{ id: 'a', name: '', url: 'socks5://h:1', keys: ['invoices'] }, { id: 'b', name: '', url: 'socks5://h:2', keys: [] }] };
    expect(proxiesOf(proxies, 'admin', ids).map((p) => p.id)).toEqual(['b']);
  });

  it('names what cannot be saved: a key with no relay, or no proxy beside a list', () => {
    const keys = keyInfos();
    const ok = settings({ relays: ['wss://a.example', 'wss://b.example'], relay_keys: { 'wss://b.example': ['invoices'] } });
    expect(assignmentProblem(keys, ok)).toBe('');
    const noRelay = settings({ relays: ['wss://a.example'], relay_keys: { 'wss://a.example': ['invoices'] } });
    expect(assignmentProblem(keys, noRelay)).toContain('No relay is assigned to Administration');
    const only = (keysOf: string[], transparent = false) => settings({ proxies: { ...settings().proxies, transparent, list: [{ id: 'a', name: '', url: 'socks5://h:1', keys: keysOf }] } });
    expect(assignmentProblem(keys, only(['invoices']))).toContain('No proxy is assigned to Administration');
    expect(assignmentProblem(keys, only([]))).toBe('');
    // Nothing to assign when the proxy is the operator's own, or there is none.
    expect(assignmentProblem(keys, only(['invoices'], true))).toBe('');
  });

  it('shortens key ids for badges', () => {
    expect(keyTag('invoices')).toBe('Invoices');
    expect(keyTag('admin')).toBe('Admin');
    expect(keyTag('reports')).toBe('reports');
  });
});
