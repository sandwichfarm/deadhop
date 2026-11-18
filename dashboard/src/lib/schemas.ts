// Runtime validation of everything that crosses a trust boundary: the
// deployment config and every response from the payment service. Objects are
// strict, so a response with unexpected fields is rejected rather than rendered.
import { z } from 'zod';
import { nip19 } from 'nostr-tools';

const HEX64 = /^[0-9a-f]{64}$/;
const MAX_MONEY_SAT = 21_000_000 * 100_000_000;

const sats = z.number().int().min(0).max(MAX_MONEY_SAT);
const unixTime = z.number().int().min(0);
const hexKey = z.string().regex(HEX64);

/**
 * Text of at most `max` characters, counted as the service counts them (Unicode
 * code points). `.max()` would count UTF-16 units, so text the service accepted,
 * such as a memo of emoji, would otherwise be refused here and break whole views.
 */
const text = (max: number) =>
  z.string().refine((s) => [...s].length <= max, { message: `at most ${max} characters` });

export const StatusSchema = z.enum(['pending', 'detected', 'confirmed', 'expired']);
export type Status = z.infer<typeof StatusSchema>;

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
function uriMatches(inv: { address: string; amount_sat: number; bip21: string }): boolean {
  const whole = Math.floor(inv.amount_sat / 100_000_000);
  const frac = String(inv.amount_sat % 100_000_000).padStart(8, '0');
  return inv.bip21 === `bitcoin:${inv.address}?amount=${whole}.${frac}`;
}
const uriMessage = { message: 'payment URI does not match address and amount', path: ['bip21'] };

export const InvoiceSchema = z.strictObject(invoiceShape).refine(uriMatches, uriMessage);
export type Invoice = z.infer<typeof InvoiceSchema>;

export const AdminInvoiceSchema = z
  .strictObject({ ...invoiceShape, index: z.number().int().min(0), creator: hexKey })
  .refine(uriMatches, uriMessage);
export type AdminInvoice = z.infer<typeof AdminInvoiceSchema>;

export const InvoiceListSchema = z.strictObject({
  invoices: z.array(AdminInvoiceSchema).max(200),
  next_before: z.number().int().min(0).nullable(),
});
export type InvoiceList = z.infer<typeof InvoiceListSchema>;

export const InfoSchema = z.strictObject({
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
export type Info = z.infer<typeof InfoSchema>;

export const StatsSchema = z.strictObject({
  pending: z.number().int().min(0),
  detected: z.number().int().min(0),
  confirmed: z.number().int().min(0),
  expired: z.number().int().min(0),
  confirmed_sat: z.number().int().min(0),
  detected_sat: z.number().int().min(0),
  next_index: z.number().int().min(0),
  tip_height: z.number().int().min(0).nullable(),
  last_poll_at: unixTime.nullable(),
  chain_ok: z.boolean(),
  watch_errors: z.number().int().min(0),
});
export type Stats = z.infer<typeof StatsSchema>;

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

export const ConfigSchema = z.strictObject({
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
export type Config = z.infer<typeof ConfigSchema>;

// ---- Operator management --------------------------------------------------

const count = z.number().int().min(0);
const shortText = z.string().max(500);

export const RelayStateSchema = z.strictObject({
  url: z.string().max(200),
  status: z.string().max(40),
  connected: z.boolean(),
});
export type RelayState = z.infer<typeof RelayStateSchema>;

export const HealthSchema = z.strictObject({
  service: z.strictObject({
    version: z.string().max(40),
    started_at: unixTime,
    uptime_secs: count,
    online_since: unixTime,
    paused: z.boolean(),
    restarts: count,
    restart_pending: z.boolean(),
  }),
  relays: z.array(RelayStateSchema).max(8),
  proxy: z.string().max(300),
  chain: z.strictObject({
    ok: z.boolean(),
    tip_height: count.nullable(),
    last_poll_at: unixTime.nullable(),
    poll_secs: count,
    watch_errors: count,
    backend_local: z.boolean(),
    transport: z.enum(['tcp', 'tls', 'tls-pinned']),
  }),
  database: z.strictObject({
    ok: z.boolean(),
    size_bytes: count,
    invoices: count,
    backups: count,
    last_backup_at: unixTime.nullable(),
  }),
  warnings: z.array(shortText).max(20),
});
export type Health = z.infer<typeof HealthSchema>;

export const SweepModeSchema = z.enum(['payjoin', 'coinjoin']);
export type SweepMode = z.infer<typeof SweepModeSchema>;

export const MetricsSchema = z.strictObject({
  now: unixTime,
  online_since: unixTime,
  addresses_created: count,
  invoices: count,
  payments: count,
  received_sat: sats,
  windows: z
    .array(
      z.strictObject({
        label: z.enum(['24h', '7d', '30d', '365d']),
        secs: count,
        available: z.boolean(),
        count,
        sat: sats,
      }),
    )
    .max(4),
  sweep: z.strictObject({
    enabled: z.boolean(),
    mode: SweepModeSchema,
    threshold_sat: sats,
    received_sat: sats,
    due: z.boolean(),
  }),
});
export type Metrics = z.infer<typeof MetricsSchema>;

export const RangeSchema = z.enum(['day', 'week', 'month', 'year']);
export type Range = z.infer<typeof RangeSchema>;

export const SeriesSchema = z.strictObject({
  range: RangeSchema,
  bucket_secs: z.number().int().min(1),
  online_since: unixTime,
  points: z.array(z.strictObject({ start: unixTime, count, sat: sats })).min(1).max(60),
});
export type Series = z.infer<typeof SeriesSchema>;

const SweepSchema = z.strictObject({
  enabled: z.boolean(),
  mode: SweepModeSchema,
  threshold_sat: sats,
  max_fee_rate_sat_vb: count,
  min_anonymity_set: count,
});

export const SettingsSchema = z.strictObject({
  name: text(100),
  relays: z.array(z.string().max(200)).min(1).max(8),
  public_invoicing: z.boolean(),
  pow_bits: z.number().int().min(0).max(64),
  pow_window_secs: count,
  min_confirmations: z.number().int().min(1).max(144),
  default_expiry_secs: count,
  max_expiry_secs: count,
  min_amount_sat: sats,
  max_amount_sat: sats,
  max_open_invoices: count,
  max_invoices_per_day: count,
  rate_global_per_min: count,
  rate_per_key_per_min: count,
  rate_requests_per_min: count,
  rate_requests_per_key_per_min: count,
  poll_secs: count,
  late_grace_secs: count,
  proxy: z.string().max(300),
  sweep: SweepSchema,
});
export type Settings = z.infer<typeof SettingsSchema>;

export const OperatorSchema = z.strictObject({
  pubkey: hexKey,
  label: text(60),
  source: z.enum(['environment', 'dashboard']),
  added_at: unixTime.nullable(),
});
export type Operator = z.infer<typeof OperatorSchema>;

export const SettingsViewSchema = z.strictObject({
  settings: SettingsSchema,
  defaults: SettingsSchema,
  overridden: z.array(z.string().max(60)).max(40),
  fixed: z.strictObject({
    network: z.string().max(20),
    electrum: z.string().max(300),
    electrum_transport: z.enum(['tcp', 'tls', 'tls-pinned']),
    first_address: z.string().max(100),
    announce: z.boolean(),
    proxy_locked: z.boolean(),
  }),
  operators: z.array(OperatorSchema).max(200),
  restart_pending: z.boolean(),
  warnings: z.array(shortText).max(20),
});
export type SettingsView = z.infer<typeof SettingsViewSchema>;

export const ControlResultSchema = z.strictObject({ paused: z.boolean(), restart_pending: z.boolean() });
export type ControlResult = z.infer<typeof ControlResultSchema>;

export const BackupSchema = z.strictObject({
  file: z.string().regex(/^cvmpay-\d{12}-\d{3}\.sqlite$/),
  size_bytes: count,
  created_at: unixTime,
});
export type Backup = z.infer<typeof BackupSchema>;

export const WipeResultSchema = z.strictObject({ deleted: count, backup: BackupSchema });
export type WipeResult = z.infer<typeof WipeResultSchema>;

export const AuditLogSchema = z.strictObject({
  entries: z
    .array(
      z.strictObject({
        at: unixTime,
        actor: z.string().max(64),
        action: z.string().max(40),
        detail: z.string().max(4000),
      }),
    )
    .max(200),
});
export type AuditLog = z.infer<typeof AuditLogSchema>;
