import { describe, expect, it } from 'vitest';
import { NO_PROXY, buildProxy, parseProxy } from './proxy';

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
});
