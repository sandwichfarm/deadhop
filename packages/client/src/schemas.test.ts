import { describe, expect, it } from 'vitest';
import { nip19 } from 'nostr-tools';
import { z } from 'zod';
import { ConfigSchema, InfoSchema, InvoiceSchema, hexKey, invoiceWith, normalizePubkey, parseConfig, sats, text, unixTime } from './schemas.ts';
import { ADDRESS, PUBKEY, info, invoice } from './test-helpers.ts';

describe('invoice schema', () => {
  it('accepts a well-formed invoice', () => {
    expect(InvoiceSchema.parse(invoice())).toEqual(invoice());
    const big = invoice({ amount_sat: 250_000_000, bip21: `bitcoin:${ADDRESS}?amount=2.50000000` });
    expect(InvoiceSchema.safeParse(big).success).toBe(true);
  });

  it('rejects unknown fields', () => {
    expect(InvoiceSchema.safeParse({ ...invoice(), html: '<script>' }).success).toBe(false);
  });

  it('rejects a payment URI that disagrees with the address or amount', () => {
    const otherAddress = 'bcrt1qcr8te4kr609gcawutmrza0j4xv80jy8zmfp6l4';
    expect(InvoiceSchema.safeParse(invoice({ bip21: `bitcoin:${otherAddress}?amount=0.00050000` })).success).toBe(false);
    expect(InvoiceSchema.safeParse(invoice({ bip21: `bitcoin:${ADDRESS}?amount=0.00000500` })).success).toBe(false);
    expect(InvoiceSchema.safeParse(invoice({ bip21: 'javascript:alert(1)' })).success).toBe(false);
  });

  it('rejects malformed fields', () => {
    const bad: Array<Record<string, unknown>> = [
      { id: 'xyz' },
      { address: '1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa' },
      { address: `${ADDRESS}?amount=1` },
      { amount_sat: -1 },
      { amount_sat: 1.5 },
      { amount_sat: 2_100_000_000_000_001 },
      { status: 'refunded' },
      { memo: 'x'.repeat(201) },
      { settled_at: 'yesterday' },
      { confirmations_required: 0 },
    ];
    for (const patch of bad) {
      expect(InvoiceSchema.safeParse({ ...invoice(), ...patch }).success, JSON.stringify(patch)).toBe(false);
    }
  });

  it('extends the invoice for operator views, as strictly', () => {
    const Admin = invoiceWith({ index: z.number(), creator: hexKey });
    const admin = { ...invoice(), index: 3, creator: '0'.repeat(64) };
    expect(Admin.parse(admin)).toEqual(admin);
    expect(Admin.safeParse({ ...admin, creator: 'x' }).success).toBe(false);
    expect(Admin.safeParse({ ...admin, bip21: 'bitcoin:x' }).success).toBe(false);
    expect(Admin.safeParse({ ...admin, extra: 1 }).success).toBe(false);
    expect(sats.safeParse(-1).success).toBe(false);
    expect(unixTime.safeParse(1.5).success).toBe(false);
    expect(text(2).safeParse('abc').success).toBe(false);
  });

  it('validates info', () => {
    expect(InfoSchema.parse(info())).toEqual(info());
    expect(InfoSchema.safeParse({ ...info(), network: 'dogecoin' }).success).toBe(false);
    expect(InfoSchema.safeParse({ ...info(), pow_bits: 65 }).success).toBe(false);
  });
});

describe('config', () => {
  const relays = ['wss://relay.example', 'ws://127.0.0.1:7777'];

  it('normalises public keys', () => {
    expect(normalizePubkey(PUBKEY)).toBe(PUBKEY);
    expect(normalizePubkey(` ${PUBKEY.toUpperCase()} `)).toBe(PUBKEY);
    expect(normalizePubkey(nip19.npubEncode(PUBKEY))).toBe(PUBKEY);
    expect(normalizePubkey(`nostr:${nip19.npubEncode(PUBKEY)}`)).toBe(PUBKEY);
    expect(normalizePubkey(nip19.nprofileEncode({ pubkey: PUBKEY, relays: ['wss://r.example'] }))).toBe(PUBKEY);
    expect(normalizePubkey(nip19.noteEncode(PUBKEY))).toBeNull();
    expect(normalizePubkey('npub1nonsense')).toBeNull();
    expect(normalizePubkey('')).toBeNull();
  });

  it('accepts hex or npub server keys', () => {
    expect(ConfigSchema.parse({ serverPubkey: PUBKEY, relays })).toEqual({ serverPubkey: PUBKEY, relays });
    expect(parseConfig({ serverPubkey: nip19.npubEncode(PUBKEY), relays }).serverPubkey).toBe(PUBKEY);
  });

  it('rejects bad configuration', () => {
    expect(ConfigSchema.safeParse({ serverPubkey: 'me', relays }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays: [] }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays: ['https://relay.example'] }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays: Array(9).fill(relays[0]) }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays, extra: 1 }).success).toBe(false);
    expect(() => parseConfig({ serverPubkey: 'me', relays })).toThrow('Invalid deadhop service config');
  });
});

describe('text limits', () => {
  it('count characters the way the service does, not UTF-16 units', () => {
    const emoji = '🙂'.repeat(200);
    expect(emoji.length).toBe(400);
    expect(InvoiceSchema.safeParse(invoice({ memo: emoji })).success).toBe(true);
    expect(InvoiceSchema.safeParse(invoice({ memo: emoji + 'x' })).success).toBe(false);
    expect(InfoSchema.safeParse(info({ name: '🙂'.repeat(60) })).success).toBe(true);
    expect(InfoSchema.safeParse(info({ name: 'x'.repeat(101) })).success).toBe(false);
  });
});
