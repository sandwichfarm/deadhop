// Deployment configuration, served next to the app as `config.json`.
import { ConfigSchema, type Config } from './schemas';

/** Where operators reach the service: the administration key, on its relays (the page's own by default). */
export const adminTarget = (c: Config): Config => ({ ...c, serverPubkey: c.adminPubkey, relays: c.adminRelays ?? c.relays });

export async function loadConfig(fetcher: typeof fetch = fetch): Promise<Config> {
  let raw: unknown;
  try {
    const res = await fetcher('config.json', { cache: 'no-store' });
    if (!res.ok) throw new Error(String(res.status));
    raw = await res.json();
  } catch {
    throw new Error('config.json is missing or unreadable. Copy config.example.json and fill it in.');
  }
  const parsed = ConfigSchema.safeParse(raw);
  if (!parsed.success) {
    throw new Error('config.json is invalid: it needs "serverPubkey" (hex or npub), "adminPubkey" (what `deadhop pubkey admin` prints) and "relays" (ws:// or wss:// URLs); "adminRelays" is optional.');
  }
  return parsed.data;
}
