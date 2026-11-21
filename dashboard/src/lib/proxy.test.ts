import { describe, expect, it } from 'vitest';
import { NO_PROXY, buildProxy, editedUrl, isNew, move, newEntry, parseProxy, proxiesPatch, shortDuration, splitDuration, toSecs } from './proxy';
import { settings } from '../test-helpers';

describe('proxy fields', () => {
  it('parses every kind of proxy url', () => {
    expect(parseProxy('')).toEqual(NO_PROXY);
    expect(parseProxy('not a url')).toEqual(NO_PROXY);
    expect(parseProxy('ftp://h:1').kind).toBe('none');
    expect(parseProxy('socks5://127.0.0.1:9050')).toEqual({ ...NO_PROXY, kind: 'socks5', host: '127.0.0.1', port: '9050' });
    expect(parseProxy('socks5h://tor:9050').kind).toBe('socks5');
    expect(parseProxy('https://proxy.example')).toMatchObject({ kind: 'https', port: '443' });
    expect(parseProxy('http://proxy.lan')).toMatchObject({ kind: 'http', port: '80' });
    expect(parseProxy('socks5://u%40x:***@[::1]:1080')).toMatchObject({ host: '::1', username: 'u@x', hasPassword: true, password: '' });
    expect(parseProxy('socks5://h')).toMatchObject({ port: '' });
  });

  it('builds urls and explains what is missing', () => {
    expect(buildProxy(NO_PROXY)).toEqual({ url: '' });
    const base = { ...NO_PROXY, kind: 'socks5' as const, host: ' 127.0.0.1 ', port: '9050' };
    expect(buildProxy(base)).toEqual({ url: 'socks5://127.0.0.1:9050' });
    expect(buildProxy({ ...base, host: '::1' })).toEqual({ url: 'socks5://[::1]:9050' });
    expect(buildProxy({ ...base, username: 'u@x', password: 'p:w' })).toEqual({ url: 'socks5://u%40x:p%3Aw@127.0.0.1:9050' });
    expect(buildProxy({ ...base, username: 'u' })).toEqual({ url: 'socks5://u@127.0.0.1:9050' });
    expect(buildProxy({ ...base, host: 'a b' })).toEqual({ error: expect.stringContaining('host') });
    expect(buildProxy({ ...base, port: '0' })).toEqual({ error: expect.stringContaining('port') });
    expect(buildProxy({ ...base, port: '70000' })).toEqual({ error: expect.stringContaining('port') });
    expect(buildProxy({ ...base, username: 'u', hasPassword: true })).toEqual({ error: expect.stringContaining('Re-enter') });
    expect(buildProxy({ ...base, hasPassword: true })).toEqual({ url: 'socks5://127.0.0.1:9050' });
  });

  it('keeps a hidden password only while the rest of the address is unchanged', () => {
    const old = 'socks5://me:***@10.0.0.1:1080';
    const f = parseProxy(old);
    expect(editedUrl(old, f)).toEqual({ url: old });
    expect(editedUrl(old, { ...f, port: '1081' })).toEqual({ error: expect.stringContaining('Re-enter') });
    expect(editedUrl(old, { ...f, port: '1081', password: 'x' })).toEqual({ url: 'socks5://me:x@10.0.0.1:1081' });
    expect(editedUrl('socks5://10.0.0.1:1080', parseProxy('socks5://10.0.0.1:1080'))).toEqual({ url: 'socks5://10.0.0.1:1080' });
  });
});

describe('the proxy list', () => {
  it('moves entries and ignores moves off the ends', () => {
    expect(move([1, 2, 3], 0, 1)).toEqual([2, 1, 3]);
    expect(move([1, 2, 3], 2, 0)).toEqual([3, 1, 2]);
    const same = [1, 2];
    expect(move(same, 1, 1)).toBe(same);
    expect(move(same, 0, -1)).toBe(same);
    expect(move(same, 0, 2)).toBe(same);
  });

  it('numbers new entries apart from saved ones', () => {
    const a = newEntry('x', 'socks5://h:1');
    expect(isNew(a)).toBe(true);
    expect(newEntry('', 'socks5://h:2').id).not.toBe(a.id);
    expect(isNew({ id: 'p1', name: '', url: '', keys: [] })).toBe(false);
  });

  it('splits seconds into the biggest even unit and back', () => {
    expect(splitDuration(3600)).toEqual({ n: 1, unit: 'h' });
    expect(splitDuration(900)).toEqual({ n: 15, unit: 'm' });
    expect(splitDuration(172_800)).toEqual({ n: 2, unit: 'd' });
    expect(splitDuration(90)).toEqual({ n: 90, unit: 's' });
    expect(splitDuration(0)).toEqual({ n: 0, unit: 's' });
    expect(toSecs(15, 'm')).toBe(900);
    expect(shortDuration(900)).toBe('15m');
    expect(shortDuration(5400)).toBe('90m');
    expect(shortDuration(90)).toBe('1m 30s');
    expect(shortDuration(30)).toBe('30s');
  });
});

describe('the proxy patch', () => {
  const live = settings().proxies;
  const entry = { id: 'a', name: 'tor', url: 'socks5://u:***@h:1', keys: [] };

  it('is empty without changes, and names only what differs', () => {
    expect(proxiesPatch(live, structuredClone(live))).toEqual({});
    const form = { ...structuredClone(live), mode: 'chaos' as const, offline: 'open' as const, transparent: true, rotate_secs: 120 };
    expect(proxiesPatch(live, form)).toEqual({ mode: 'chaos', offline: 'open', transparent: true, rotate_secs: 120 });
  });

  it('sends the whole list, with ids for saved entries and none for new ones', () => {
    const base = { ...live, list: [entry] };
    const form = { ...base, list: [newEntry('new', 'socks5://n:1'), entry] };
    expect(proxiesPatch(base, form)).toEqual({ list: [{ name: 'new', url: 'socks5://n:1', keys: [] }, { id: 'a', name: 'tor', url: 'socks5://u:***@h:1', keys: [] }] });
  });

  it('reports a problem with an interval', () => {
    const f = (over: Partial<typeof live>) => proxiesPatch(live, { ...live, ...over });
    expect(f({ rotate_secs: NaN })).toBe('Fill in every interval.');
    expect(f({ chaos_min_secs: 10 })).toBe('Intervals run from 1 minute to 7 days.');
    expect(f({ chaos_max_secs: 700_000 })).toBe('Intervals run from 1 minute to 7 days.');
    expect(f({ chaos_min_secs: 900, chaos_max_secs: 300 })).toContain('longer than the longest');
  });
});
