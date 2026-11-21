// Runtime validation of everything that crosses a trust boundary. The public
// invoice and service schemas come from @deadhop/client; the operator
// management schemas below exist only in the dashboard. Objects are strict, so
// a response with unexpected fields is rejected rather than rendered.
import { z } from 'zod';
import {
  ConfigSchema,
  InfoSchema,
  InvoiceSchema,
  StatusSchema,
  hexKey,
  invoiceWith,
  normalizePubkey,
  sats,
  text,
  unixTime,
  type Invoice,
  type InvoiceStatus,
  type ServiceConfig,
  type ServiceInfo,
} from '@deadhop/client/schemas';

export { ConfigSchema, InfoSchema, InvoiceSchema, StatusSchema, normalizePubkey };
export type { Invoice };
export type Status = InvoiceStatus;
export type Info = ServiceInfo;
export type Config = ServiceConfig;

export const AdminInvoiceSchema = invoiceWith({ index: z.number().int().min(0), creator: hexKey });
export type AdminInvoice = z.infer<typeof AdminInvoiceSchema>;

export const InvoiceListSchema = z.strictObject({
  invoices: z.array(AdminInvoiceSchema).max(200),
  next_before: z.number().int().min(0).nullable(),
});
export type InvoiceList = z.infer<typeof InvoiceListSchema>;

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

// ---- Operator management --------------------------------------------------

const count = z.number().int().min(0);
const shortText = z.string().max(500);

export const RelayStateSchema = z.strictObject({
  url: z.string().max(200),
  status: z.string().max(40),
  connected: z.boolean(),
});
export type RelayState = z.infer<typeof RelayStateSchema>;

export const RelayCheckSchema = z.strictObject({
  relay: z.string().max(200),
  ok: z.boolean(),
  code: z.string().max(40),
  reason: shortText,
  ms: count.nullable(),
});
export type RelayCheck = z.infer<typeof RelayCheckSchema>;

export const ProxyCheckSchema = z.strictObject({
  proxy: z.string().max(300),
  status: z.enum(['connected', 'partial', 'failed', 'direct']),
  checked_at: unixTime,
  relays: z.array(RelayCheckSchema).max(8),
});
export type ProxyCheck = z.infer<typeof ProxyCheckSchema>;

/** One relay's end-to-end check: connects, carries ContextVM events, not rate-limiting. */
export const RelayHealthSchema = z.strictObject({
  relay: z.string().max(200),
  status: z.enum(['ok', 'degraded', 'down']),
  connects: z.boolean(),
  forwards: z.boolean().nullable(),
  rate_limited: z.boolean(),
  ms: count.nullable(),
  detail: shortText,
  checked_at: unixTime,
});
export type RelayHealth = z.infer<typeof RelayHealthSchema>;

export const RelayHealthReportSchema = z.strictObject({ relays: z.array(RelayHealthSchema).max(8) });
export type RelayHealthReport = z.infer<typeof RelayHealthReportSchema>;

export const RelayPointSchema = z.strictObject({ start: unixTime, ok: count, fail: count });
export type RelayPoint = z.infer<typeof RelayPointSchema>;

export const RelayStatsSchema = z.strictObject({
  relay: z.string().max(200),
  checks: count,
  check_failures: count,
  sends: count,
  send_failures: count,
  avg_ms: count.nullable(),
  points: z.array(RelayPointSchema).max(60),
});
export type RelayStats = z.infer<typeof RelayStatsSchema>;

export const RelayHistorySchema = z.strictObject({
  window: z.enum(['15m', '1h', '12h', '1d', '1w']),
  bucket_secs: count,
  since: unixTime,
  until: unixTime,
  relays: z.array(RelayStatsSchema).max(40),
});
export type RelayHistory = z.infer<typeof RelayHistorySchema>;

export const PrivacyCheckSchema = z.strictObject({
  id: z.string().max(40),
  area: z.enum(['network', 'ports', 'exposure']),
  title: shortText,
  result: z.enum(['pass', 'to_fix', 'fail', 'note', 'not_checked']),
  points: count.max(100),
  summary: shortText,
  evidence: z.array(shortText).max(20),
  fix: shortText.nullable(),
});
export type PrivacyCheck = z.infer<typeof PrivacyCheckSchema>;

export const PrivacyAuditSchema = z.strictObject({
  score: count.max(100),
  checked: count,
  total: count,
  ran_at: unixTime,
  checks: z.array(PrivacyCheckSchema).max(40),
});
export type PrivacyAudit = z.infer<typeof PrivacyAuditSchema>;

export const ProxyModeSchema = z.enum(['failover', 'round_robin', 'chaos']);
export type ProxyMode = z.infer<typeof ProxyModeSchema>;
export const OfflinePolicySchema = z.enum(['kill_switch', 'admin_only', 'invoicing_only', 'open']);
export type OfflinePolicy = z.infer<typeof OfflinePolicySchema>;

export const ProxyEntrySchema = z.strictObject({
  id: z.string().min(1).max(24),
  name: text(60),
  url: z.string().max(300),
});
export type ProxyEntry = z.infer<typeof ProxyEntrySchema>;

export const ProxySettingsSchema = z.strictObject({
  list: z.array(ProxyEntrySchema).max(8),
  mode: ProxyModeSchema,
  rotate_secs: count,
  chaos_min_secs: count,
  chaos_max_secs: count,
  offline: OfflinePolicySchema,
  transparent: z.boolean(),
});
export type ProxySettings = z.infer<typeof ProxySettingsSchema>;

export const ProxyPoolSchema = z.strictObject({
  state: z.enum(['transparent', 'direct', 'active', 'suspended', 'offline']),
  posture: z.enum(['normal', 'off', 'admin_only', 'invoicing_only', 'open']),
  mode: ProxyModeSchema,
  offline: OfflinePolicySchema,
  transparent: z.boolean(),
  active: z.string().max(24).nullable(),
  reachable: count,
  total: count,
  next_switch_at: unixTime.nullable(),
  proxies: z
    .array(
      z.strictObject({
        id: z.string().max(24),
        name: text(60),
        url: z.string().max(300),
        reachable: z.boolean().nullable(),
        checked_at: unixTime.nullable(),
        status: z.string().max(20).nullable(),
        reason: z.string().max(300).nullable(),
      }),
    )
    .max(8),
});
export type ProxyPool = z.infer<typeof ProxyPoolSchema>;

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
  proxy_check: ProxyCheckSchema.nullable(),
  proxy_pool: ProxyPoolSchema,
  relay_health: z.array(RelayHealthSchema).max(8),
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
  proxies: ProxySettingsSchema,
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
  file: z.string().regex(/^deadhop-\d{12}-\d{3}\.sqlite$/),
  size_bytes: count,
  created_at: unixTime,
});
export type Backup = z.infer<typeof BackupSchema>;

/** The service keeps 10; the bound leaves room for a host that kept more. */
export const BackupListSchema = z.strictObject({ backups: z.array(BackupSchema).max(100) });
export type BackupList = z.infer<typeof BackupListSchema>;

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
