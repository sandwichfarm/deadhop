// The built package (dist/, exactly what npm ships) against the real local
// regtest stack: relay, payment service and bitcoind. Requires `just up` and
// `just server`; run with `npm run e2e`.
//
// Covers both ways of creating invoices and the backend check that a merchant
// must do before fulfilling an order.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createCheckout, createCvmpay, secretKeySigner, type Cvmpay, type Invoice } from '../dist/mod.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8').trim();

function bitcoinCli(...args: string[]): string {
  return execFileSync(
    'docker',
    ['compose', '-f', resolve(root, 'deploy/docker-compose.yml'), 'exec', '-T', 'bitcoind', 'bitcoin-cli', '-regtest', '-rpcuser=cvmpay', '-rpcpassword=cvmpay-dev-only', ...args],
    { encoding: 'utf8' },
  ).trim();
}
const toBtc = (sat: number) => `${Math.floor(sat / 1e8)}.${String(sat % 1e8).padStart(8, '0')}`;
const mine = (n: number) => bitcoinCli('generatetoaddress', String(n), bitcoinCli('-rpcwallet=miner', 'getnewaddress'));
const pay = (inv: Invoice) => bitcoinCli('-rpcwallet=miner', 'sendtoaddress', inv.address, toBtc(inv.amount_sat));

/** Resolve when `predicate` holds, failing after `ms`. */
function until(predicate: () => boolean, ms = 60_000): Promise<void> {
  return new Promise((ok, fail) => {
    const deadline = Date.now() + ms;
    const timer = setInterval(() => {
      if (predicate()) {
        clearInterval(timer);
        ok();
      } else if (Date.now() > deadline) {
        clearInterval(timer);
        fail(new Error('timed out'));
      }
    }, 100);
  });
}

describe('@cvmpay/client (built) against a real service', () => {
  const config = JSON.parse(read('dashboard/public/config.json')) as { serverPubkey: string; relays: string[] };
  let guest: Cvmpay;
  let backend: Cvmpay;

  beforeAll(() => {
    // The browser side: a throwaway key, proof of work for each invoice.
    guest = createCvmpay(config);
    // The merchant's backend: an operator key, no proof of work.
    backend = createCvmpay({ ...config, signer: secretKeySigner(read('data/dev/operator.nsec')) });
  });

  afterAll(async () => {
    await guest.close();
    await backend.close();
  });

  it('knows who it is talking as', async () => {
    const [g, b] = await Promise.all([guest.info(), backend.info()]);
    expect(g.is_admin).toBe(false);
    expect(g.public_invoicing).toBe(true);
    expect(g.pow_bits).toBeGreaterThan(0);
    expect(b.is_admin).toBe(true);
    expect(b.network).toBe('regtest');
  });

  it('browser-created invoice: a checkout follows it to paid, then the backend verifies it', async () => {
    const seen: string[] = [];
    let detected: Invoice | null = null;
    let paid: Invoice | null = null;
    const checkout = createCheckout(guest, {
      pollMs: 500,
      onDetected: (inv) => (detected = inv),
      onPaid: (inv) => (paid = inv),
    });
    checkout.subscribe((s) => seen.push(s.phase));
    await checkout.create({ amount_sat: 12_345, memo: 'e2e browser order' });
    const invoice = checkout.getState().invoice!;
    expect(invoice).toMatchObject({ status: 'pending', amount_sat: 12_345, memo: 'e2e browser order' });

    pay(invoice);
    await until(() => detected !== null);
    mine(1);
    await until(() => paid !== null);
    checkout.destroy();
    expect(seen).toEqual(expect.arrayContaining(['idle', 'creating', 'pending', 'detected', 'confirmed']));

    // What a merchant does before shipping: ask the service itself.
    const verified = await backend.getInvoice(invoice.id);
    expect(verified.status).toBe('confirmed');
    expect(verified.confirmed_sat).toBeGreaterThanOrEqual(12_345);
  });

  it('backend-created invoice: the browser only watches it by id', async () => {
    const invoice = await backend.createInvoice({ amount_sat: 7_000, memo: 'e2e backend order' });
    let paid = false;
    const checkout = createCheckout(guest, { pollMs: 500, onPaid: () => (paid = true) });
    await checkout.watch(invoice.id);
    expect(checkout.getState().phase).toBe('pending');
    pay(invoice);
    mine(1);
    await until(() => paid);
    checkout.destroy();
    expect((await backend.getInvoice(invoice.id)).status).toBe('confirmed');
  });

  it('refuses an unknown invoice with a typed error', async () => {
    await expect(guest.getInvoice('f'.repeat(32))).rejects.toMatchObject({ code: 'not_found' });
  });
});
