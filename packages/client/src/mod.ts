/**
 * Framework-agnostic client for a deadhop payment service: create bitcoin
 * invoices over ContextVM (Nostr), follow them until paid, and render them.
 *
 * @example
 * ```ts
 * import { createDeadhop, createCheckout } from '@deadhop/client';
 *
 * const deadhop = createDeadhop({ serverPubkey: 'npub1...', relays: ['wss://relay.example'] });
 * const checkout = createCheckout(deadhop, { onPaid: (inv) => console.log('paid', inv.id) });
 * checkout.subscribe((s) => console.log(s.phase));
 * await checkout.create({ amount_sat: 5000, memo: 'Order 42' });
 * ```
 *
 * @module
 */
export { createDeadhop, type Deadhop, type DeadhopOptions } from './deadhop.ts';
export {
  createCheckout,
  POLL_MS,
  type Checkout,
  type CheckoutEvents,
  type CheckoutOptions,
  type CheckoutPhase,
  type CheckoutState,
} from './checkout.ts';
export { invoiceView, progressMessage, payLabel, parseSats, LABELS, type InvoiceView, type Labels } from './view.ts';
export {
  ApiError,
  PayApi,
  connect,
  toApiError,
  withTimeout,
  CONNECT_TIMEOUT_MS,
  type ConnectOptions,
  type CreateInvoiceInput,
  type ErrorCode,
  type Session,
  type ToolCaller,
} from './api.ts';
export { localSigner, nip07Signer, parseSecret, once, type Nip07 } from './signer.ts';
export { guestSigner, secretKeySigner } from './signers.ts';
export { mine, needsWork, powSigner, checkWork, PowError, MAX_POW_BITS, CTXVM_KIND, type MineOptions, type Miner, type UnsignedEvent } from './pow.ts';
export { qrShape, type QrShape } from './qr.ts';
export { formatBtc, formatSats, receivedSummary, statusLabel, timeLeft } from './format.ts';
export {
  normalizePubkey,
  parseConfig,
  type Invoice,
  type InvoiceStatus,
  type Network,
  type ServiceConfig,
  type ServiceConfigInput,
  type ServiceInfo,
} from './schemas.ts';
export type { NostrSigner } from '@contextvm/sdk';
