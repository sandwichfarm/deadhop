import { describe, expect, it, vi } from 'vitest';

const defaults = vi.hoisted(() => ({ connect: vi.fn() }));
vi.mock('./api.ts', async (original) => ({ ...(await original<typeof import('./api.ts')>()), connect: defaults.connect }));
import { nip19 } from 'nostr-tools';
import type { NostrSigner } from '@contextvm/sdk';
import { ApiError, PayApi, type Session } from './api.ts';
import { createCvmpay } from './cvmpay.ts';
import { guestSigner, secretKeySigner } from './signers.ts';
import { PUBKEY, info, invoice, toolResult } from './test-helpers.ts';

function fakeSession(close = vi.fn(async () => undefined)): Session {
  const callTool = vi.fn(async (name: string) => toolResult(name === 'get_info' ? info() : invoice()));
  return { api: new PayApi(callTool), callTool, pubkey: PUBKEY, info: info(), close };
}

const relays = ['wss://relay.example'];

describe('createCvmpay', () => {
  it('validates the config up front', () => {
    expect(() => createCvmpay({ serverPubkey: 'nope', relays })).toThrow('Invalid cvmpay service config');
  });

  it('connects once, lazily, and shares the connection', async () => {
    const session = fakeSession();
    const connector = vi.fn(async () => session);
    const signer = guestSigner();
    const cvmpay = createCvmpay({ serverPubkey: nip19.npubEncode(PUBKEY), relays, signer, connector, timeoutMs: 5 });
    expect(connector).not.toHaveBeenCalled();
    expect(await cvmpay.info()).toEqual(info());
    expect(await cvmpay.createInvoice({ amount_sat: 50_000 })).toEqual(invoice());
    expect(await cvmpay.getInvoice(invoice().id)).toEqual(invoice());
    expect(connector).toHaveBeenCalledTimes(1);
    const [usedSigner, config, options] = connector.mock.calls[0] as unknown as [NostrSigner, unknown, { timeoutMs: number }];
    expect(usedSigner).toBe(signer);
    expect(config).toEqual({ serverPubkey: PUBKEY, relays });
    expect(options.timeoutMs).toBe(5);

    await cvmpay.close();
    expect(session.close).toHaveBeenCalledTimes(1);
    await cvmpay.close();
    await cvmpay.info();
    expect(connector).toHaveBeenCalledTimes(2);
  });

  it('tries again after a failed connection, and closes cleanly if it never opened', async () => {
    const connector = vi
      .fn()
      .mockRejectedValueOnce(new ApiError('timeout'))
      .mockResolvedValue(fakeSession());
    const cvmpay = createCvmpay({ serverPubkey: PUBKEY, relays, connector });
    await expect(cvmpay.info()).rejects.toMatchObject({ code: 'timeout' });
    expect((await cvmpay.info()).name).toBe('cvmpay');

    const failing = createCvmpay({ serverPubkey: PUBKEY, relays, connector: vi.fn().mockRejectedValue(new ApiError('network')) });
    const pending = failing.info().catch(() => undefined);
    await failing.close();
    await pending;
  });

  it('uses the ContextVM connection by default', async () => {
    defaults.connect.mockResolvedValue(fakeSession());
    expect((await createCvmpay({ serverPubkey: PUBKEY, relays }).info()).name).toBe('cvmpay');
    expect(defaults.connect).toHaveBeenCalledTimes(1);
  });

  it('defaults to a throwaway key and can use a secret key', async () => {
    const connector = vi.fn(async () => fakeSession());
    await createCvmpay({ serverPubkey: PUBKEY, relays, connector }).info();
    const a = (connector.mock.calls[0] as unknown as [NostrSigner])[0];
    await createCvmpay({ serverPubkey: PUBKEY, relays, connector }).info();
    const b = (connector.mock.calls[1] as unknown as [NostrSigner])[0];
    expect(await a.getPublicKey()).not.toBe(await b.getPublicKey());
    const secret = 'a'.repeat(64);
    expect(await secretKeySigner(secret).getPublicKey()).toBe(await secretKeySigner(nip19.nsecEncode(Uint8Array.from(Buffer.from(secret, 'hex')))).getPublicKey());
  });
});
