import { describe, expect, it } from 'vitest';
import { nip19 } from 'nostr-tools';
import {
  AdminInvoiceSchema,
  ConfigSchema,
  InfoSchema,
  InvoiceListSchema,
  InvoiceSchema,
  StatsSchema,
  normalizePubkey,
} from './schemas';
import { ADDRESS, PUBKEY, adminInvoice, info, invoice, stats } from '../test-helpers';

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
});

describe('admin schemas', () => {
  it('accepts admin invoices and lists', () => {
    expect(AdminInvoiceSchema.parse(adminInvoice())).toEqual(adminInvoice());
    const list = { invoices: [adminInvoice(), adminInvoice({ index: 1 })], next_before: 1 };
    expect(InvoiceListSchema.parse(list)).toEqual(list);
    expect(InvoiceListSchema.parse({ invoices: [], next_before: null }).invoices).toEqual([]);
  });

  it('rejects bad admin data', () => {
    expect(AdminInvoiceSchema.safeParse(adminInvoice({ creator: 'nobody' })).success).toBe(false);
    expect(AdminInvoiceSchema.safeParse({ ...adminInvoice(), bip21: 'bitcoin:x' }).success).toBe(false);
    expect(InvoiceListSchema.safeParse({ invoices: [invoice()], next_before: null }).success).toBe(false);
    expect(InvoiceListSchema.safeParse({ invoices: [] }).success).toBe(false);
  });

  it('validates info and stats', () => {
    expect(InfoSchema.parse(info())).toEqual(info());
    expect(InfoSchema.safeParse({ ...info(), network: 'dogecoin' }).success).toBe(false);
    expect(InfoSchema.safeParse({ ...info(), pow_bits: 65 }).success).toBe(false);
    expect(StatsSchema.parse(stats())).toEqual(stats());
    expect(StatsSchema.parse(stats({ tip_height: null, last_poll_at: null })).tip_height).toBeNull();
    expect(StatsSchema.safeParse({ ...stats(), chain_ok: 'yes' }).success).toBe(false);
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
    expect(ConfigSchema.parse({ serverPubkey: nip19.npubEncode(PUBKEY), relays }).serverPubkey).toBe(PUBKEY);
  });

  it('rejects bad configuration', () => {
    expect(ConfigSchema.safeParse({ serverPubkey: 'me', relays }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays: [] }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays: ['https://relay.example'] }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays: Array(9).fill(relays[0]) }).success).toBe(false);
    expect(ConfigSchema.safeParse({ serverPubkey: PUBKEY, relays, extra: 1 }).success).toBe(false);
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

