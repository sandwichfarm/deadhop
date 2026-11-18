// Typed client for the payment service. Every response is validated before
// any of it reaches the UI.
import { Client } from '@contextvm/mcp-sdk/client/index.js';
import { EncryptionMode, NostrClientTransport, type NostrSigner } from '@contextvm/sdk';
import type { z } from 'zod';
import { PowError, powSigner, type MineOptions } from './pow';
import {
  AuditLogSchema,
  BackupSchema,
  ControlResultSchema,
  HealthSchema,
  InfoSchema,
  InvoiceListSchema,
  InvoiceSchema,
  MetricsSchema,
  SeriesSchema,
  SettingsViewSchema,
  StatsSchema,
  WipeResultSchema,
  type AuditLog,
  type Backup,
  type Config,
  type ControlResult,
  type Health,
  type Info,
  type Invoice,
  type InvoiceList,
  type Metrics,
  type Range,
  type Series,
  type Settings,
  type SettingsView,
  type Stats,
  type Status,
  type WipeResult,
} from './schemas';

export type ErrorCode =
  | 'invalid_params'
  | 'unauthorized'
  | 'not_found'
  | 'pow_rejected'
  | 'rate_limited'
  | 'capacity'
  | 'paused'
  | 'internal'
  | 'bad_response'
  | 'pow_failed'
  | 'network';

const SERVER_CODES: readonly string[] = [
  'invalid_params',
  'unauthorized',
  'not_found',
  'pow_rejected',
  'rate_limited',
  'capacity',
  'paused',
  'internal',
];

const MESSAGES: Record<ErrorCode, string> = {
  invalid_params: 'The service rejected the request as invalid.',
  unauthorized: 'This key is not allowed to do that.',
  not_found: 'Invoice not found.',
  pow_rejected: 'Proof of work was rejected. Check that your clock is correct and try again.',
  rate_limited: 'Too many requests. Wait a minute and try again.',
  capacity: 'The service is busy. Try again later.',
  paused: 'Invoicing is stopped by the operator.',
  internal: 'The service had an internal error.',
  bad_response: 'The service sent a response this app does not understand.',
  pow_failed: 'Could not compute the proof of work.',
  network: 'Could not reach the service.',
};

export class ApiError extends Error {
  constructor(
    readonly code: ErrorCode,
    detail?: string,
  ) {
    super(detail ? `${MESSAGES[code]} (${detail})` : MESSAGES[code]);
    this.name = 'ApiError';
  }
}

/** Classify anything thrown by the transport or the MCP client. */
export function toApiError(e: unknown): ApiError {
  if (e instanceof ApiError) return e;
  if (e instanceof PowError) return new ApiError('pow_failed', e.message);
  const code = (e as { data?: { code?: unknown } } | null)?.data?.code;
  if (typeof code === 'string' && SERVER_CODES.includes(code)) return new ApiError(code as ErrorCode);
  return new ApiError('network');
}

/** Performs one MCP `tools/call` and returns the raw result. */
export type ToolCaller = (name: string, args: Record<string, unknown>) => Promise<unknown>;

export interface CreateInvoiceInput {
  amount_sat: number;
  memo?: string;
  expiry_secs?: number;
}

export interface ListInvoicesInput {
  status?: Status;
  before?: number;
  limit?: number;
}

/** Settings to change; anything left out stays as it is. */
export type SettingsPatch = Partial<Omit<Settings, 'sweep'>> & { sweep?: Partial<Settings['sweep']> };

export type ControlAction = 'stop' | 'start' | 'restart';

export interface WipeInput {
  confirm: string;
  include_settings?: boolean;
  force?: boolean;
}

export class PayApi {
  constructor(private readonly callTool: ToolCaller) {}

  private async call<S extends z.ZodType>(name: string, args: Record<string, unknown>, schema: S): Promise<z.infer<S>> {
    let raw: unknown;
    try {
      raw = await this.callTool(name, args);
    } catch (e) {
      throw toApiError(e);
    }
    const result = raw as { isError?: unknown; structuredContent?: unknown } | null;
    // Schema violations come back as a tool-level error result.
    if (result?.isError === true) throw new ApiError('invalid_params');
    const parsed = schema.safeParse(result?.structuredContent);
    if (!parsed.success) throw new ApiError('bad_response');
    return parsed.data;
  }

  getInfo(): Promise<Info> {
    return this.call('get_info', {}, InfoSchema);
  }

  // The two calls below also check that the answer is about what was asked,
  // so a response for another invoice or amount is never shown as this one.
  async createInvoice(input: CreateInvoiceInput): Promise<Invoice> {
    const invoice = await this.call('create_invoice', { ...input }, InvoiceSchema);
    if (invoice.amount_sat !== input.amount_sat) throw new ApiError('bad_response');
    return invoice;
  }

  async getInvoice(id: string): Promise<Invoice> {
    const invoice = await this.call('get_invoice', { id }, InvoiceSchema);
    if (invoice.id !== id) throw new ApiError('bad_response');
    return invoice;
  }

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

  wipe(input: WipeInput): Promise<WipeResult> {
    return this.call('wipe_database', { ...input }, WipeResultSchema);
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
export async function connect(signer: NostrSigner, config: Config, mining: MineOptions = {}): Promise<Session> {
  let bits = 0;
  const transport = new NostrClientTransport({
    signer: powSigner(signer, () => bits, mining),
    relayHandler: config.relays,
    serverPubkey: config.serverPubkey,
    encryptionMode: EncryptionMode.REQUIRED,
    logLevel: 'silent',
  });
  const client = new Client({ name: 'cvmpay-dashboard', version: '0.1.0' });
  try {
    await client.connect(transport);
    const api = new PayApi((name, args) => client.callTool({ name, arguments: args }));
    const info = await api.getInfo();
    // Operators are exempt from proof of work.
    bits = info.is_admin ? 0 : info.pow_bits;
    return { api, pubkey: await signer.getPublicKey(), info, close: () => client.close() };
  } catch (e) {
    await client.close().catch(() => undefined);
    throw toApiError(e);
  }
}
