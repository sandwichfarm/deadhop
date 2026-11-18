// Operator management against the live regtest service: health, metrics,
// settings round trip, stop/start, backup, operators and an in-process
// restart. Requires `just up` and `just server`.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { nip19, getPublicKey, generateSecretKey } from 'nostr-tools';
import { bytesToHex } from '@noble/hashes/utils.js';
import { connect, type Session } from '../src/lib/api';
import { loadConfig } from '../src/lib/config';
import { localSigner } from '../src/lib/signer';
import type { Config } from '../src/lib/schemas';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8').trim();

/** Retry `f` until it passes; requests sent mid-restart are never answered. */
async function eventually<T>(f: () => Promise<T>, ok: (v: T) => boolean, ms = 60_000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    try {
      const v = await Promise.race([f(), new Promise<never>((_, no) => setTimeout(() => no(new Error('slow')), 5_000))]);
      if (ok(v)) return v;
    } catch {
      // try again
    }
    if (Date.now() > deadline) throw new Error('condition not reached');
    await new Promise((r) => setTimeout(r, 500));
  }
}

describe('operator management over ContextVM', () => {
  let config: Config;
  let operator: Session;
  let guest: Session;

  beforeAll(async () => {
    config = await loadConfig((async () => ({ ok: true, json: async () => JSON.parse(read('dashboard/public/config.json')) })) as unknown as typeof fetch);
    operator = await connect(localSigner(read('data/dev/operator.nsec')), config);
    guest = await connect(localSigner(), config);
  });

  afterAll(async () => {
    await operator?.api.resetSettings().catch(() => undefined);
    await operator?.api.control('start').catch(() => undefined);
    await operator?.close();
    await guest?.close();
  });

  it('reports health, metrics and a payment series', async () => {
    const h = await eventually(() => operator.api.getHealth(), (h) => h.relays.some((r) => r.connected));
    expect(h.database.ok).toBe(true);
    expect(h.chain.ok).toBe(true);
    const m = await operator.api.getMetrics();
    expect(m.addresses_created).toBeGreaterThanOrEqual(m.invoices);
    expect(m.windows.map((w) => w.label)).toEqual(['24h', '7d', '30d', '365d']);
    for (const [range, n] of [['day', 24], ['week', 7], ['month', 30], ['year', 52]] as const) {
      expect((await operator.api.getSeries(range)).points).toHaveLength(n);
    }
    await expect(guest.api.getHealth()).rejects.toMatchObject({ code: 'unauthorized' });
  });

  it('changes settings live and resets them', async () => {
    const v = await operator.api.updateSettings({ pow_bits: 17, sweep: { enabled: true, threshold_sat: 500_000 } });
    expect(v.overridden).toEqual(expect.arrayContaining(['pow_bits', 'sweep.enabled', 'sweep.threshold_sat']));
    expect((await guest.api.getInfo()).pow_bits).toBe(17);
    expect((await operator.api.getMetrics()).sweep.threshold_sat).toBe(500_000);
    await expect(operator.api.updateSettings({ pow_bits: 99 })).rejects.toMatchObject({ code: 'invalid_params' });
    const reset = await operator.api.resetSettings();
    expect(reset.overridden).toEqual([]);
    expect((await guest.api.getInfo()).pow_bits).toBe(reset.defaults.pow_bits);
  });

  it('stops and starts invoicing', async () => {
    await operator.api.control('stop');
    await expect(guest.api.createInvoice({ amount_sat: 5_000 })).rejects.toMatchObject({ code: 'paused' });
    expect((await operator.api.getHealth()).service.paused).toBe(true);
    await operator.api.control('start');
    expect((await operator.api.getHealth()).service.paused).toBe(false);
  });

  it('backs up the database next to it, privately', async () => {
    const b = await operator.api.backup();
    const path = resolve(root, 'data/dev/server/backups', b.file);
    expect(existsSync(path)).toBe(true);
    const { statSync } = await import('node:fs');
    expect(statSync(path).mode & 0o777).toBe(0o600);
    const log = await operator.api.getAuditLog(5);
    expect(log.entries[0].action).toBe('backup');
  });

  it('adds an operator by nprofile and removes it again', async () => {
    const secret = generateSecretKey();
    const pubkey = getPublicKey(secret);
    const profile = nip19.nprofileEncode({ pubkey, relays: config.relays });
    const v = await operator.api.addOperator(profile, 'e2e');
    expect(v.operators.some((o) => o.pubkey === pubkey && o.source === 'dashboard')).toBe(true);
    const second = await connect(localSigner(bytesToHex(secret)), config);
    try {
      expect(second.info.is_admin).toBe(true);
      await operator.api.removeOperator(pubkey);
      expect((await second.api.getInfo()).is_admin).toBe(false);
    } finally {
      await second.close();
    }
  });

  it('restarts its relay connections in-process and stays reachable', async () => {
    const before = (await operator.api.getHealth()).service.restarts;
    await operator.api.control('restart');
    const h = await eventually(() => operator.api.getHealth(), (h) => h.service.restarts === before + 1 && h.relays.some((r) => r.connected));
    expect(h.service.restart_pending).toBe(false);
  });
});
