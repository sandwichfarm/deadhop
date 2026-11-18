// Full payment flow against the local regtest stack, using the same client
// library the dashboard uses. Requires `just up` and `just server`.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { connect, type Session } from '../src/lib/api';
import { loadConfig } from '../src/lib/config';
import { localSigner } from '../src/lib/signer';
import type { Config, Invoice } from '../src/lib/schemas';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8').trim();

function bitcoinCli(...args: string[]): string {
  return execFileSync(
    'docker',
    ['compose', '-f', resolve(root, 'deploy/docker-compose.yml'), 'exec', '-T', 'bitcoind', 'bitcoin-cli', '-regtest', '-rpcuser=cvmpay', '-rpcpassword=cvmpay-dev-only', ...args],
    { encoding: 'utf8' },
  ).trim();
}

const toBtc = (sat: number) => `${Math.floor(sat / 1e8)}.${String(sat % 1e8).padStart(8, '0')}`;
const mineBlocks = (n: number) => bitcoinCli('generatetoaddress', String(n), bitcoinCli('-rpcwallet=miner', 'getnewaddress'));

async function waitForStatus(session: Session, id: string, status: Invoice['status']): Promise<Invoice> {
  const deadline = Date.now() + 60_000;
  for (;;) {
    const inv = await session.api.getInvoice(id);
    if (inv.status === status) return inv;
    if (Date.now() > deadline) throw new Error(`invoice stayed ${inv.status}, expected ${status}`);
    await new Promise((r) => setTimeout(r, 500));
  }
}

describe('on-chain payment over ContextVM', () => {
  let config: Config;
  let operator: Session;
  let guest: Session;

  beforeAll(async () => {
    // Read config.json exactly as the browser would, just from disk.
    config = await loadConfig((async () => ({ ok: true, json: async () => JSON.parse(read('dashboard/public/config.json')) })) as unknown as typeof fetch);
    operator = await connect(localSigner(read('data/dev/operator.nsec')), config);
    guest = await connect(localSigner(), config);
  });

  afterAll(async () => {
    await operator?.close();
    await guest?.close();
  });

  it('reports who is an operator', () => {
    expect(operator.info.is_admin).toBe(true);
    expect(guest.info.is_admin).toBe(false);
    expect(operator.info.network).toBe('regtest');
    expect(guest.info.pow_bits).toBeGreaterThan(0);
  });

  it('lets a guest create an invoice with proof of work and pay it', async () => {
    const before = await operator.api.getStats();
    const merchantBefore = Number(bitcoinCli('-rpcwallet=merchant', 'getbalance'));

    const started = Date.now();
    const invoice = await guest.api.createInvoice({ amount_sat: 75_000, memo: 'e2e order' });
    console.log(`invoice ${invoice.id} created in ${Date.now() - started} ms with ${guest.info.pow_bits} bits of work`);
    expect(invoice.status).toBe('pending');
    expect(invoice.address).toMatch(/^bcrt1q/);

    // The address really belongs to the merchant's own wallet.
    const owned = JSON.parse(bitcoinCli('-rpcwallet=merchant', 'getaddressinfo', invoice.address));
    expect(owned.ismine).toBe(true);

    // Customer pays; first seen unconfirmed, then confirmed after a block.
    bitcoinCli('-rpcwallet=miner', 'sendtoaddress', invoice.address, toBtc(invoice.amount_sat));
    const seen = await waitForStatus(guest, invoice.id, 'detected');
    expect(seen.received_sat).toBe(75_000);
    expect(seen.confirmed_sat).toBe(0);

    mineBlocks(1);
    const paid = await waitForStatus(guest, invoice.id, 'confirmed');
    expect(paid.confirmed_sat).toBe(75_000);
    expect(paid.settled_at).not.toBeNull();

    // The operator sees it, and the money is in the merchant's wallet.
    const list = await operator.api.listInvoices({ status: 'confirmed', limit: 5 });
    expect(list.invoices[0].id).toBe(invoice.id);
    expect(list.invoices[0].creator).toBe(guest.pubkey);
    const after = await operator.api.getStats();
    expect(after.confirmed).toBe(before.confirmed + 1);
    expect(after.confirmed_sat).toBe(before.confirmed_sat + 75_000);
    expect(after.chain_ok).toBe(true);
    const merchantAfter = Number(bitcoinCli('-rpcwallet=merchant', 'getbalance'));
    expect(Math.round((merchantAfter - merchantBefore) * 1e8)).toBe(75_000);
  });

  it('keeps an underpaid invoice pending', async () => {
    const invoice = await operator.api.createInvoice({ amount_sat: 40_000 });
    bitcoinCli('-rpcwallet=miner', 'sendtoaddress', invoice.address, toBtc(15_000));
    mineBlocks(1);
    const deadline = Date.now() + 30_000;
    let current = invoice;
    while (current.confirmed_sat !== 15_000 && Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 500));
      current = await operator.api.getInvoice(invoice.id);
    }
    expect(current.confirmed_sat).toBe(15_000);
    expect(current.status).toBe('pending');
  });

  it('refuses operator tools and unpaid-for requests from strangers', async () => {
    await expect(guest.api.listInvoices()).rejects.toMatchObject({ code: 'unauthorized' });
    await expect(guest.api.getStats()).rejects.toMatchObject({ code: 'unauthorized' });
    await expect(guest.api.getInvoice('0'.repeat(32))).rejects.toMatchObject({ code: 'not_found' });
    await expect(guest.api.createInvoice({ amount_sat: 1 })).rejects.toMatchObject({ code: 'invalid_params' });
  });
});
