// Proxy settings as form fields and back. The service stores one URL and
// shows the password as `***`; it is never sent back unless re-entered.
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
