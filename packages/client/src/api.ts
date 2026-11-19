// Typed client for the payment service over ContextVM. Every response is
// validated before any of it reaches the caller.
import { Client } from '@contextvm/mcp-sdk/client';
import { EncryptionMode, NostrClientTransport, type NostrSigner } from '@contextvm/sdk';
import type { z } from 'zod';
import { PowError, powSigner, type MineOptions } from './pow.ts';
import { InfoSchema, InvoiceSchema, type Invoice, type ServiceConfig, type ServiceInfo } from './schemas.ts';

/** Why a call failed. The first eight come from the service itself. */
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
  | 'timeout'
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
  timeout: 'The service did not answer. It may be offline, still starting, or not listening on these relays.',
};

/** A failed call, with a stable `code` and a message fit to show to people. */
export class ApiError extends Error {
  constructor(
    readonly code: ErrorCode,
    detail?: string,
  ) {
    super(detail ? `${MESSAGES[code]} (${detail})` : MESSAGES[code]);
    this.name = 'ApiError';
  }
}

/** Map anything thrown by the transport or the service to an `ApiError`. */
export function toApiError(e: unknown): ApiError {
  if (e instanceof ApiError) return e;
  if (e instanceof PowError) return new ApiError('pow_failed', e.message);
  const code = (e as { data?: { code?: unknown } } | null)?.data?.code;
  if (typeof code === 'string' && SERVER_CODES.includes(code)) return new ApiError(code as ErrorCode);
  return new ApiError('network');
}

/** Performs one MCP `tools/call` and returns the raw result. */
export type ToolCaller = (name: string, args: Record<string, unknown>) => Promise<unknown>;

/** What to put on a new invoice. */
export interface CreateInvoiceInput {
  amount_sat: number;
  /** Shown to whoever pays; up to 200 characters. */
  memo?: string;
  /** Lifetime in seconds (default and maximum are set by the service). */
  expiry_secs?: number;
}

/** The service's tools, with every answer validated. */
export class PayApi {
  constructor(private readonly callTool: ToolCaller) {}

  /**
   * Call any tool and validate its answer with `schema`. The typed methods below
   * are built on this; it is public so operator tools can be added on top.
   */
  async call<T>(name: string, args: Record<string, unknown>, schema: z.ZodType<T>): Promise<T> {
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

  getInfo(): Promise<ServiceInfo> {
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
}

/** An open connection to the service. */
export interface Session {
  api: PayApi;
  /** The raw tool caller of this connection (for operator tools). */
  callTool: ToolCaller;
  /** Hex public key of the signing identity. */
  pubkey: string;
  info: ServiceInfo;
  close(): Promise<void>;
}

/** How long connecting waits for the service to answer before giving up. */
export const CONNECT_TIMEOUT_MS = 20_000;

/** Settle with `work`, or reject with a timeout error after `ms`. */
export function withTimeout<T>(work: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const expired = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new ApiError('timeout')), ms);
  });
  return Promise.race([work, expired]).finally(() => clearTimeout(timer));
}

/** Options for `connect`. */
export interface ConnectOptions {
  /** Proof-of-work mining options (batch size, progress, time limit). */
  mining?: MineOptions;
  /** Give up connecting after this many milliseconds (default 20 s). */
  timeoutMs?: number;
  /** Client name sent to the service. */
  clientName?: string;
}

/** Connect to the service over its relays and learn its parameters. */
export async function connect(signer: NostrSigner, config: ServiceConfig, options: ConnectOptions = {}): Promise<Session> {
  let bits = 0;
  const transport = new NostrClientTransport({
    signer: powSigner(signer, () => bits, options.mining),
    relayHandler: config.relays,
    serverPubkey: config.serverPubkey,
    encryptionMode: EncryptionMode.REQUIRED,
    logLevel: 'silent',
  });
  const client = new Client({ name: options.clientName ?? 'cvmpay-client', version: '0.1.0' });
  try {
    const callTool: ToolCaller = (name, args) => client.callTool({ name, arguments: args });
    const api = new PayApi(callTool);
    const info = await withTimeout(
      client.connect(transport).then(() => api.getInfo()),
      options.timeoutMs ?? CONNECT_TIMEOUT_MS,
    );
    // Operators are exempt from proof of work.
    bits = info.is_admin ? 0 : info.pow_bits;
    return { api, callTool, pubkey: await signer.getPublicKey(), info, close: () => client.close() };
  } catch (e) {
    await client.close().catch(() => undefined);
    throw toApiError(e);
  }
}
