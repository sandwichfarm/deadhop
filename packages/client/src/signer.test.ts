import { describe, expect, it, vi } from 'vitest';
import { getPublicKey, nip19 } from 'nostr-tools';
import { hexToBytes } from '@noble/hashes/utils.js';
import { localSigner, nip07Signer, once, parseSecret, type Nip07 } from './signer.ts';
import { PUBKEY } from './test-helpers.ts';

const SECRET = '0000000000000000000000000000000000000000000000000000000000000001';

describe('nip07Signer', () => {
  it('refuses to work without a capable extension', () => {
    expect(() => nip07Signer(undefined)).toThrow('No Nostr extension');
    const noNip44 = { getPublicKey: vi.fn(), signEvent: vi.fn() } as unknown as Nip07;
    expect(() => nip07Signer(noNip44)).toThrow('NIP-44');
  });

  it('delegates every operation to the extension', async () => {
    const ext: Nip07 = {
      getPublicKey: vi.fn().mockResolvedValue(PUBKEY),
      signEvent: vi.fn().mockResolvedValue({ id: 'signed' }),
      nip44: {
        encrypt: vi.fn().mockResolvedValue('cipher'),
        decrypt: vi.fn().mockResolvedValue('plain'),
      },
    };
    const signer = nip07Signer(ext);
    expect(await signer.getPublicKey()).toBe(PUBKEY);
    const template = { kind: 1, tags: [], content: '', created_at: 0 };
    expect(await signer.signEvent(template)).toEqual({ id: 'signed' });
    expect(ext.signEvent).toHaveBeenCalledWith(template);
    expect(await signer.nip44!.encrypt('pk', 'plain')).toBe('cipher');
    expect(await signer.nip44!.decrypt('pk', 'cipher')).toBe('plain');
    expect(ext.nip44!.encrypt).toHaveBeenCalledWith('pk', 'plain');
    expect(ext.nip44!.decrypt).toHaveBeenCalledWith('pk', 'cipher');
    // The extension is asked for the public key once per sign-in.
    expect(await signer.getPublicKey()).toBe(PUBKEY);
    expect(ext.getPublicKey).toHaveBeenCalledTimes(1);
  });
});

describe('once', () => {
  it('reuses an answer but asks again after a refusal', async () => {
    const get = vi.fn().mockRejectedValueOnce(new Error('denied')).mockResolvedValue(PUBKEY);
    const cached = once(get);
    await expect(cached()).rejects.toThrow('denied');
    expect(await cached()).toBe(PUBKEY);
    expect(await cached()).toBe(PUBKEY);
    expect(get).toHaveBeenCalledTimes(2);
  });
});

describe('parseSecret', () => {
  it('accepts hex and nsec', () => {
    expect(parseSecret(` ${SECRET} `)).toBe(SECRET);
    expect(parseSecret('AB'.repeat(32))).toBe('ab'.repeat(32));
    expect(parseSecret(nip19.nsecEncode(hexToBytes(SECRET)))).toBe(SECRET);
  });

  it('rejects everything else', () => {
    for (const bad of ['', 'hunter2', nip19.npubEncode(PUBKEY), 'nsec1broken']) {
      expect(() => parseSecret(bad), bad).toThrow('Enter an nsec');
    }
  });
});

describe('localSigner', () => {
  it('uses the given key or makes a throwaway one', async () => {
    expect(await localSigner(SECRET).getPublicKey()).toBe(getPublicKey(hexToBytes(SECRET)));
    const a = await localSigner().getPublicKey();
    const b = await localSigner().getPublicKey();
    expect(a).toMatch(/^[0-9a-f]{64}$/);
    expect(a).not.toBe(b);
  });
});
