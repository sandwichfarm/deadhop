// Runtime validation of everything that crosses a trust boundary: the service
// configuration and every response from the payment service. Objects are
// strict, so a response with unexpected fields is rejected rather than shown.
//
// Public types are written out as interfaces; each schema is annotated with the
// interface it must produce, so the two cannot drift apart.
import { z } from 'zod';
import { nip19 } from 'nostr-tools';

const HEX64 = /^[0-9a-f]{64}$/;
const MAX_MONEY_SAT = 21_000_000 * 100_000_000;

/** Amounts in satoshis: whole, non-negative, at most 21 million BTC. */
export const sats: z.ZodNumber = z.number().int().min(0).max(MAX_MONEY_SAT);
/** Unix time in seconds. */
export const unixTime: z.ZodNumber = z.number().int().min(0);
/** 64 lowercase hex characters. */
export const hexKey: z.ZodString = z.string().regex(HEX64);

/**
 * Text of at most `max` characters, counted as the service counts them (Unicode
 * code points). `.max()` would count UTF-16 units, so text the service accepted,
 * such as a memo of emoji, would otherwise be refused here.
 */
export function text(max: number): z.ZodType<string> {
  return z.string().refine((s) => [...s].length <= max, { message: `at most ${max} characters` });
}

/** Where an invoice is in its life. */
export type InvoiceStatus = 'pending' | 'detected' | 'confirmed' | 'expired';

export const StatusSchema: z.ZodType<InvoiceStatus> = z.enum(['pending', 'detected', 'confirmed', 'expired']);

/** An invoice as the service reports it. */
export interface Invoice {
  /** 32 lowercase hex characters. Anyone who has the id can look the invoice up. */
  id: string;
  /** Native segwit address that receives the payment (fresh per invoice). */
  address: string;
  amount_sat: number;
  /** Written by whoever created the invoice, possibly a stranger: show it as plain text. */
  memo: string;
  status: InvoiceStatus;
  /** Paid to the address so far, confirmed or not. */
  received_sat: number;
  /** Paid with at least `confirmations_required` confirmations. */
  confirmed_sat: number;
  created_at: number;
  expires_at: number;
  settled_at: number | null;
  /** `bitcoin:` payment URI for exactly `address` and `amount_sat`. */
  bip21: string;
  confirmations_required: number;
}

// Shape shared by public and operator invoice views.
const invoiceShape = {
  id: z.string().regex(/^[0-9a-f]{32}$/),
  // Native segwit only; the charset also rules out anything that could break a URI.
  address: z.string().regex(/^(bc1|tb1|bcrt1)[02-9ac-hj-np-z]{20,87}$/),
  amount_sat: sats,
  memo: text(200),
  status: StatusSchema,
  received_sat: sats,
  confirmed_sat: sats,
  created_at: unixTime,
  expires_at: unixTime,
  settled_at: unixTime.nullable(),
  bip21: z.string().max(200),
  confirmations_required: z.number().int().min(1).max(144),
};

/** The payment URI must pay exactly the address and amount shown next to it. */
export function uriMatches(inv: { address: string; amount_sat: number; bip21: string }): boolean {
  const whole = Math.floor(inv.amount_sat / 100_000_000);
  const frac = String(inv.amount_sat % 100_000_000).padStart(8, '0');
  return inv.bip21 === `bitcoin:${inv.address}?amount=${whole}.${frac}`;
}

/** Refinement options for `uriMatches`. */
export const uriMessage: { message: string; path: string[] } = {
  message: 'payment URI does not match address and amount',
  path: ['bip21'],
};

export const InvoiceSchema: z.ZodType<Invoice> = z.strictObject(invoiceShape).refine(uriMatches, uriMessage);

/**
 * An invoice schema with extra fields, as strict as `InvoiceSchema` (for
 * operator views that add fields to the public invoice).
 */
export function invoiceWith<T extends Record<string, z.ZodType>>(extra: T): z.ZodType<Invoice & { [K in keyof T]: z.output<T[K]> }> {
  return z.strictObject({ ...invoiceShape, ...extra }).refine((v) => uriMatches(v as Invoice), uriMessage) as unknown as z.ZodType<
    Invoice & { [K in keyof T]: z.output<T[K]> }
  >;
}

/** Bitcoin network the service runs on. */
export type Network = 'bitcoin' | 'testnet' | 'testnet4' | 'signet' | 'regtest';

/** What the service tells every caller about itself and its limits. */
export interface ServiceInfo {
  name: string;
  version: string;
  network: Network;
  /** Whether callers that are not operators may create invoices (with proof of work). */
  public_invoicing: boolean;
  /** Proof of work asked of this caller for each new invoice (0 for operators). */
  pow_bits: number;
  pow_window_secs: number;
  min_confirmations: number;
  default_expiry_secs: number;
  max_expiry_secs: number;
  min_amount_sat: number;
  max_amount_sat: number;
  /** Whether the signing key is one of the service's operators. */
  is_admin: boolean;
}

export const InfoSchema: z.ZodType<ServiceInfo> = z.strictObject({
  name: text(100),
  version: z.string().max(40),
  network: z.enum(['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest']),
  public_invoicing: z.boolean(),
  pow_bits: z.number().int().min(0).max(64),
  pow_window_secs: z.number().int().min(1),
  min_confirmations: z.number().int().min(1).max(144),
  default_expiry_secs: z.number().int().min(60),
  max_expiry_secs: z.number().int().min(60),
  min_amount_sat: sats,
  max_amount_sat: sats,
  is_admin: z.boolean(),
});

/** Accepts a hex key, `npub` or `nprofile` (with or without `nostr:`); always yields lowercase hex. */
export function normalizePubkey(value: string): string | null {
  const v = value.trim().replace(/^nostr:/, '');
  if (HEX64.test(v.toLowerCase())) return v.toLowerCase();
  try {
    const decoded = nip19.decode(v);
    if (decoded.type === 'npub') return decoded.data;
    if (decoded.type === 'nprofile') return decoded.data.pubkey;
    return null;
  } catch {
    return null;
  }
}

/** Where to reach a cvmpay service: its public key and the relays it listens on. */
export interface ServiceConfig {
  /** Hex public key (normalized from hex, `npub` or `nprofile`). */
  serverPubkey: string;
  /** `wss://` (or `ws://`) relays shared with the service, 1 to 8. */
  relays: string[];
}

/** Input form of `ServiceConfig`: the key may also be an `npub` or `nprofile`. */
export interface ServiceConfigInput {
  serverPubkey: string;
  relays: string[];
}

export const ConfigSchema: z.ZodType<ServiceConfig, ServiceConfigInput> = z.strictObject({
  serverPubkey: z.string().transform((v, ctx) => {
    const hex = normalizePubkey(v);
    if (hex === null) {
      ctx.addIssue({ code: 'custom', message: 'serverPubkey must be a hex key or npub' });
      return z.NEVER;
    }
    return hex;
  }),
  relays: z
    .array(z.string().regex(/^wss?:\/\/[^\s]+$/))
    .min(1)
    .max(8),
});

/** Validate where to reach the service. Throws a readable error if it is unusable. */
export function parseConfig(input: ServiceConfigInput): ServiceConfig {
  const parsed = ConfigSchema.safeParse(input);
  if (!parsed.success) {
    throw new Error('Invalid cvmpay service config: it needs "serverPubkey" (hex or npub) and "relays" (ws:// or wss:// URLs).');
  }
  return parsed.data;
}
