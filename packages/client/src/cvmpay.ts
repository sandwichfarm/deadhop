// The high-level client: one shared, lazily opened connection per service.
import type { NostrSigner } from '@contextvm/sdk';
import { connect, type ConnectOptions, type CreateInvoiceInput, type Session } from './api.ts';
import { parseConfig, type Invoice, type ServiceConfigInput, type ServiceInfo } from './schemas.ts';
import { localSigner } from './signer.ts';

/** Options for `createCvmpay`. */
export interface CvmpayOptions extends ServiceConfigInput, ConnectOptions {
  /**
   * Who signs requests. Default: a throwaway key made in memory, so customers
   * never need a Nostr identity and see no extension prompt. On a backend, pass
   * `secretKeySigner(operatorNsec)` to create invoices without proof of work.
   */
  signer?: NostrSigner;
  /** Replaces `connect` (tests and custom transports). */
  connector?: (signer: NostrSigner, config: { serverPubkey: string; relays: string[] }, options: ConnectOptions) => Promise<Session>;
}

/** A cvmpay service. Connects on first use; share one instance per page or process. */
export interface Cvmpay {
  /** The service's name, network, limits and the proof of work it asks of this signer. */
  info(): Promise<ServiceInfo>;
  createInvoice(input: CreateInvoiceInput): Promise<Invoice>;
  /** Look an invoice up by id. Works for any signer: the id is the capability. */
  getInvoice(id: string): Promise<Invoice>;
  /** Close the relay connections. The next call opens them again. */
  close(): Promise<void>;
}

/** Create a client for one cvmpay service. Throws if the config is unusable. */
export function createCvmpay(options: CvmpayOptions): Cvmpay {
  const config = parseConfig({ serverPubkey: options.serverPubkey, relays: options.relays });
  const signer = options.signer ?? localSigner();
  const open = options.connector ?? connect;
  let session: Promise<Session> | null = null;

  function current(): Promise<Session> {
    session ??= open(signer, config, options).catch((e: unknown) => {
      // A failed attempt is not remembered: the next call tries again.
      session = null;
      throw e;
    });
    return session;
  }

  return {
    info: async () => (await current()).info,
    createInvoice: async (input) => (await current()).api.createInvoice(input),
    getInvoice: async (id) => (await current()).api.getInvoice(id),
    async close() {
      const s = session;
      session = null;
      if (s) await (await s.catch(() => null))?.close();
    },
  };
}
