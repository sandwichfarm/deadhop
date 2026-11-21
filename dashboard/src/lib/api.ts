// Typed client for the payment service, for the dashboard. The invoice calls
// and the connection come from @deadhop/client; the operator tools are added
// here. Every response is validated before any of it reaches the UI.
import type { NostrSigner } from '@contextvm/sdk';
import {
  ApiError,
  CONNECT_TIMEOUT_MS,
  PayApi as InvoiceApi,
  connect as connectService,
  localSigner,
  type MineOptions,
} from '@deadhop/client';
import {
  AuditLogSchema,
  BackupListSchema,
  BackupSchema,
  ControlResultSchema,
  HealthSchema,
  InvoiceListSchema,
  MetricsSchema,
  SeriesSchema,
  SettingsViewSchema,
  StatsSchema,
  WipeResultSchema,
  type AuditLog,
  type Backup,
  type BackupList,
  type Config,
  type ControlResult,
  type Health,
  type Info,
  type InvoiceList,
  type Metrics,
  type Range,
  type Series,
  type Settings,
  type SettingsView,
  type Stats,
  type Status,
  type WipeResult,
  PrivacyAuditSchema,
  ProxyCheckSchema,
  RelayHealthReportSchema,
  RelayHistorySchema,
  type RelayHealthReport,
  type RelayHistory,
  type PrivacyAudit,
  type ProxyCheck,
} from './schemas';

export { ApiError, CONNECT_TIMEOUT_MS, toApiError, withTimeout } from '@deadhop/client';
export type { CreateInvoiceInput, ErrorCode, ToolCaller } from '@deadhop/client';

export interface ListInvoicesInput {
  status?: Status;
  before?: number;
  limit?: number;
}

/** Settings to change; anything left out stays as it is. */
export type SettingsPatch = Partial<Omit<Settings, 'sweep' | 'proxies'>> & {
  sweep?: Partial<Settings['sweep']>;
  proxies?: ProxiesPatch;
};

/** A proxy as sent: no `id` makes a new one; a hidden password stays with its `id`. */
export interface ProxyInput {
  id?: string;
  name?: string;
  url: string;
}

/** Changes to the proxy settings; `list` replaces the whole list. */
export interface ProxiesPatch extends Partial<Omit<Settings['proxies'], 'list'>> {
  list?: ProxyInput[];
}

export type ControlAction = 'stop' | 'start' | 'restart';

export interface WipeInput {
  confirm: string;
  include_settings?: boolean;
  force?: boolean;
}

/** The invoice calls plus the operator tools. */
export class PayApi extends InvoiceApi {
  listInvoices(input: ListInvoicesInput = {}): Promise<InvoiceList> {
    return this.call('list_invoices', { ...input }, InvoiceListSchema);
  }

  getStats(): Promise<Stats> {
    return this.call('get_stats', {}, StatsSchema);
  }

  // ---- Operator management ----

  getHealth(): Promise<Health> {
    return this.call('get_health', {}, HealthSchema);
  }

  getMetrics(): Promise<Metrics> {
    return this.call('get_metrics', {}, MetricsSchema);
  }

  async getSeries(range: Range): Promise<Series> {
    const series = await this.call('get_payment_series', { range }, SeriesSchema);
    if (series.range !== range) throw new ApiError('bad_response');
    return series;
  }

  getSettings(): Promise<SettingsView> {
    return this.call('get_settings', {}, SettingsViewSchema);
  }

  updateSettings(patch: SettingsPatch): Promise<SettingsView> {
    return this.call('update_settings', { patch }, SettingsViewSchema);
  }

  resetSettings(): Promise<SettingsView> {
    return this.call('reset_settings', {}, SettingsViewSchema);
  }

  addOperator(pubkey: string, label: string): Promise<SettingsView> {
    return this.call('add_operator', { pubkey, label }, SettingsViewSchema);
  }

  removeOperator(pubkey: string): Promise<SettingsView> {
    return this.call('remove_operator', { pubkey }, SettingsViewSchema);
  }

  control(action: ControlAction): Promise<ControlResult> {
    return this.call('control_service', { action }, ControlResultSchema);
  }

  backup(): Promise<Backup> {
    return this.call('backup_database', {}, BackupSchema);
  }

  /** Backups kept on the service host, newest first. */
  listBackups(): Promise<BackupList> {
    return this.call('list_backups', {}, BackupListSchema);
  }

  wipe(input: WipeInput): Promise<WipeResult> {
    return this.call('wipe_database', { ...input }, WipeResultSchema);
  }

  /**
   * Test a proxy against the live relays: a listed one by `id` (its saved
   * password is used), an address, or (no argument) the one in use.
   */
  checkProxy(target?: string | { id: string }): Promise<ProxyCheck> {
    const params = target === undefined ? {} : typeof target === 'string' ? { proxy: target } : { id: target.id };
    return this.call('check_proxy', params, ProxyCheckSchema);
  }

  /** Check every live relay end to end now. */
  checkRelays(): Promise<RelayHealthReport> {
    return this.call('check_relays', {}, RelayHealthReportSchema);
  }

  /** How each relay did over a window: checks, replies, failures and a series to plot. */
  getRelayHistory(window: RelayHistory['window'] = '1h'): Promise<RelayHistory> {
    return this.call('get_relay_history', { window }, RelayHistorySchema);
  }

  getPrivacyAudit(): Promise<PrivacyAudit> {
    return this.call('get_privacy_audit', {}, PrivacyAuditSchema);
  }

  getAuditLog(limit = 50): Promise<AuditLog> {
    return this.call('get_audit_log', { limit }, AuditLogSchema);
  }
}

export interface Session {
  api: PayApi;
  /** Hex public key of the signed-in identity. */
  pubkey: string;
  info: Info;
  close(): Promise<void>;
}

/** Connect to the service over the configured relays and learn its parameters. */
export async function connect(
  signer: NostrSigner,
  config: Config,
  mining: MineOptions = {},
  timeoutMs = CONNECT_TIMEOUT_MS,
): Promise<Session> {
  const session = await connectService(signer, config, { mining, timeoutMs, clientName: 'deadhop-dashboard' });
  return { api: new PayApi(session.callTool), pubkey: session.pubkey, info: session.info, close: session.close };
}

/**
 * Whether the service answers at all, for the sign-in page: one anonymous
 * request with a throwaway key. Reveals nothing but online or not.
 */
export async function probeService(config: Config, open: typeof connect = connect): Promise<void> {
  const session = await open(localSigner(undefined), config);
  await session.close();
}
