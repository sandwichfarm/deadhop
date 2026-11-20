import { describe, expect, it, vi } from 'vitest';
import { getEventHash, verifyEvent } from 'nostr-tools';
import { PrivateKeySigner } from '@contextvm/sdk';
import { CTXVM_KIND, MAX_POW_BITS, PowError, checkWork, eventId, leadingZeroBits, mine, needsWork, powSigner, type Miner, type UnsignedEvent } from './pow.ts';
import { PUBKEY } from './test-helpers.ts';

const createCall = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'create_invoice' } });

function template(content = createCall, kind = CTXVM_KIND): UnsignedEvent {
  return { kind, tags: [['p', PUBKEY]], content, created_at: 1_700_000_000 };
}

describe('leadingZeroBits', () => {
  it('counts bits across nibbles', () => {
    expect(leadingZeroBits('ffff')).toBe(0);
    expect(leadingZeroBits('7fff')).toBe(1);
    expect(leadingZeroBits('1fff')).toBe(3);
    expect(leadingZeroBits('0fff')).toBe(4);
    expect(leadingZeroBits('002f')).toBe(10);
    expect(leadingZeroBits('0000')).toBe(16);
  });
});

describe('eventId', () => {
  it('matches the reference NIP-01 implementation', () => {
    const ev = template('say "hi" \n ünïcode');
    expect(eventId(PUBKEY, ev)).toBe(getEventHash({ ...ev, pubkey: PUBKEY }));
  });
});

describe('mine', () => {
  it('finds a nonce and commits to the target', async () => {
    const mined = await mine(PUBKEY, template(), 8);
    expect(leadingZeroBits(eventId(PUBKEY, mined))).toBeGreaterThanOrEqual(8);
    const nonce = mined.tags.at(-1)!;
    expect(nonce[0]).toBe('nonce');
    expect(nonce[2]).toBe('8');
    expect(mined.tags[0]).toEqual(['p', PUBKEY]);
    expect(mined.content).toBe(createCall);
  });

  it('replaces a stale nonce tag and reports progress while yielding', async () => {
    const onProgress = vi.fn();
    const stale = { ...template(), tags: [['nonce', '5', '2'], ['p', PUBKEY]] };
    const mined = await mine(PUBKEY, stale, 10, { batch: 16, onProgress });
    expect(mined.tags.filter((t) => t[0] === 'nonce')).toHaveLength(1);
    expect(leadingZeroBits(eventId(PUBKEY, mined))).toBeGreaterThanOrEqual(10);
    expect(onProgress).toHaveBeenCalled();
    expect(onProgress.mock.calls[0][0]).toBe(16);
  });

  it('refuses absurd difficulty instead of hanging the tab', async () => {
    await expect(mine(PUBKEY, template(), MAX_POW_BITS + 1)).rejects.toBeInstanceOf(PowError);
    await expect(mine(PUBKEY, template(), 64)).rejects.toThrow('this app stops at 28');
  });

  it('gives up after its time limit', async () => {
    let t = 0;
    const slow = mine(PUBKEY, template(), MAX_POW_BITS, { batch: 4, maxMs: 10, now: () => (t += 6) });
    await expect(slow).rejects.toThrow('took too long');
    await expect(slow).rejects.toBeInstanceOf(PowError);
  });

  it('yields without a progress callback', async () => {
    const mined = await mine(PUBKEY, template(), 6, { batch: 1 });
    expect(leadingZeroBits(eventId(PUBKEY, mined))).toBeGreaterThanOrEqual(6);
  });
});

describe('needsWork', () => {
  it('is true only for create_invoice tool calls', () => {
    expect(needsWork(template())).toBe(true);
    expect(needsWork(template(createCall, 1))).toBe(false);
    expect(needsWork(template(JSON.stringify({ method: 'tools/call', params: { name: 'get_invoice' } })))).toBe(false);
    expect(needsWork(template(JSON.stringify({ method: 'tools/list' })))).toBe(false);
    expect(needsWork(template(JSON.stringify({ method: 'tools/call' })))).toBe(false);
    expect(needsWork(template('null'))).toBe(false);
    expect(needsWork(template('not json'))).toBe(false);
  });
});

describe('powSigner', () => {
  it('mines invoice requests and leaves everything else untouched', async () => {
    const inner = new PrivateKeySigner();
    let bits = 8;
    const signer = powSigner(inner, () => bits);
    expect(await signer.getPublicKey()).toBe(await inner.getPublicKey());
    expect(signer.nip44).toBe(inner.nip44);

    const signed = await signer.signEvent(template() as never);
    expect(verifyEvent(signed)).toBe(true);
    expect(leadingZeroBits(signed.id)).toBeGreaterThanOrEqual(8);
    expect(signed.tags.some((t) => t[0] === 'nonce' && t[2] === '8')).toBe(true);

    const other = await signer.signEvent(template(JSON.stringify({ method: 'tools/list' })) as never);
    expect(verifyEvent(other)).toBe(true);
    expect(other.tags.some((t) => t[0] === 'nonce')).toBe(false);

    bits = 0;
    const free = await signer.signEvent(template() as never);
    expect(free.tags.some((t) => t[0] === 'nonce')).toBe(false);
  });
});

describe('checkWork', () => {
  const NOW = 1_700_000_000;
  it('accepts real work, with the original or a fresh created_at', async () => {
    const mined = await mine(PUBKEY, template(), 8);
    expect(() => checkWork(PUBKEY, template(), mined, 8, NOW)).not.toThrow();
    expect(() => checkWork(PUBKEY, template(), mined, 8, NOW + 3600)).not.toThrow();
    const later = await mine(PUBKEY, { ...template(), created_at: NOW + 30 }, 8);
    expect(() => checkWork(PUBKEY, template(), later, 8, NOW)).not.toThrow();
  });

  it('refuses anything the service would refuse, or that changes the request', async () => {
    const ev = template();
    const mined = await mine(PUBKEY, ev, 8);
    const bad: UnsignedEvent[] = [
      { ...mined, kind: 1 },
      { ...mined, content: '{}' },
      { ...mined, tags: mined.tags.filter((t) => t[0] !== 'p') },
      { ...mined, tags: [...mined.tags, ['p', 'f'.repeat(64)]] },
      { ...mined, tags: mined.tags.filter((t) => t[0] !== 'nonce') },
      { ...mined, tags: [...mined.tags, ['nonce', '1', '8']] },
      { ...mined, tags: mined.tags.map((t) => (t[0] === 'nonce' ? [t[0], t[1], '7'] : t)) },
      { ...mined, tags: mined.tags.map((t) => (t[0] === 'nonce' ? [t[0], t[1]] : t)) },
      { ...mined, created_at: NOW + 61 },
      { ...mined, created_at: NOW + 0.5 },
    ];
    for (const b of bad) expect(() => checkWork(PUBKEY, ev, b, 8, NOW), JSON.stringify(b)).toThrow(PowError);
    // Not enough leading zeros for 30 bits, even though the tag claims it.
    const claims = { ...mined, tags: mined.tags.map((t) => (t[0] === 'nonce' ? [t[0], t[1], '30'] : t)) };
    expect(() => checkWork(PUBKEY, ev, claims, 30, NOW)).toThrow('without the required work');
  });
});

describe('powSigner with another miner', () => {
  it('uses the given miner and passes it the options', async () => {
    const inner = new PrivateKeySigner();
    const opts = { batch: 7 };
    const miner = vi.fn<Miner>((pubkey, ev, bits, o) => mine(pubkey, ev, bits, o));
    const signer = powSigner(inner, () => 8, opts, miner);
    const signed = await signer.signEvent(template() as never);
    expect(verifyEvent(signed)).toBe(true);
    expect(miner).toHaveBeenCalledWith(await inner.getPublicKey(), expect.objectContaining({ kind: CTXVM_KIND }), 8, opts);
  });

  it('never signs what a broken miner returns', async () => {
    const inner = new PrivateKeySigner();
    const sign = vi.spyOn(inner, 'signEvent');
    const signer = powSigner(inner, () => 8, { now: () => 1_700_000_000_000 }, async (_pk, ev) => ev);
    await expect(signer.signEvent(template() as never)).rejects.toBeInstanceOf(PowError);
    expect(sign).not.toHaveBeenCalled();
  });
});
