// Deployment configuration, served next to the app as `config.json`.
import { ConfigSchema, type Config } from './schemas';

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
    throw new Error('config.json is invalid: it needs "serverPubkey" (hex or npub) and "relays" (ws:// or wss:// URLs).');
  }
  return parsed.data;
}
