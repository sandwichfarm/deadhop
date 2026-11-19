// Headless checkout: create (or adopt) an invoice and follow it until it is
// paid. Framework components are thin views over this state machine.
//
// `subscribe` follows the Svelte store contract (called at once with the current
// state, returns an unsubscribe function), so the same object works as a Svelte
// store, with React's useSyncExternalStore and with Solid's `from`.
import { ApiError, toApiError, type CreateInvoiceInput } from './api.ts';
import type { Cvmpay } from './cvmpay.ts';
import type { Invoice } from './schemas.ts';

/**
 * - `idle`: nothing started yet
 * - `creating`: asking the service for an invoice (includes proof of work, which can take seconds)
 * - `loading`: fetching an existing invoice for the first time
 * - `pending`, `detected`, `confirmed`, `expired`: the invoice's status
 * - `error`: no invoice could be created or loaded (see `error`)
 */
export type CheckoutPhase = 'idle' | 'creating' | 'loading' | 'pending' | 'detected' | 'confirmed' | 'expired' | 'error';

/** Everything a view needs to render a checkout. */
export interface CheckoutState {
  phase: CheckoutPhase;
  invoice: Invoice | null;
  /** Latest problem. With an invoice present it is transient: polling continues. */
  error: ApiError | null;
  /** Unix seconds at the latest update, for countdowns. */
  now: number;
}

/** Callbacks fired once each, when the checkout first reaches that point. */
export interface CheckoutEvents {
  /** An invoice exists (created here or adopted by id). */
  onCreated?: (invoice: Invoice) => void;
  /**
   * A payment of the full amount is visible but not yet confirmed. Do not hand
   * over goods on this: an unconfirmed payment can still disappear.
   */
  onDetected?: (invoice: Invoice) => void;
  /**
   * The full amount is confirmed. This runs in the payer's browser, which they
   * control: fulfil orders only after your backend has checked the invoice with
   * `getInvoice(id)` itself.
   */
  onPaid?: (invoice: Invoice) => void;
  onError?: (error: ApiError) => void;
}

/** Options for `createCheckout`. */
export interface CheckoutOptions extends CheckoutEvents {
  /** How often to ask for news, in milliseconds (default 5000). */
  pollMs?: number;
  /** Clock in Unix seconds (injectable for tests). */
  clock?: () => number;
}

/** A running checkout. */
export interface Checkout {
  getState(): CheckoutState;
  subscribe(listener: (state: CheckoutState) => void): () => void;
  /** Create an invoice and follow it. Ignored while one is being created or followed. */
  create(input: CreateInvoiceInput): Promise<void>;
  /** Follow an invoice that already exists (for example, created by your backend). */
  watch(id: string): Promise<void>;
  /** Stop polling and forget the invoice, back to `idle`. */
  reset(): void;
  /** Stop polling for good (call when the view goes away). */
  destroy(): void;
}

/** Default polling interval. */
export const POLL_MS = 5000;

/** Create a checkout for `client`. Nothing happens until `create` or `watch`. */
export function createCheckout(client: Pick<Cvmpay, 'createInvoice' | 'getInvoice'>, options: CheckoutOptions = {}): Checkout {
  const clock = options.clock ?? (() => Math.floor(Date.now() / 1000));
  const pollMs = options.pollMs ?? POLL_MS;
  const listeners = new Set<(state: CheckoutState) => void>();
  let state: CheckoutState = { phase: 'idle', invoice: null, error: null, now: clock() };
  let timer: ReturnType<typeof setInterval> | undefined;
  // Bumped on reset/destroy so answers to older requests are dropped.
  let generation = 0;
  let destroyed = false;
  const fired = new Set<string>();

  function set(next: Partial<CheckoutState>) {
    state = { ...state, ...next, now: clock() };
    for (const l of listeners) l(state);
  }

  function once(name: string, fn: (() => void) | undefined) {
    if (fn && !fired.has(name)) {
      fired.add(name);
      fn();
    }
  }

  function stopPolling() {
    clearInterval(timer);
    timer = undefined;
  }

  function show(invoice: Invoice) {
    set({ phase: invoice.status, invoice, error: null });
    once('created', () => options.onCreated?.(invoice));
    if (invoice.status === 'detected' || invoice.status === 'confirmed') once('detected', () => options.onDetected?.(invoice));
    if (invoice.status === 'confirmed') {
      once('paid', () => options.onPaid?.(invoice));
      // A confirmed invoice never changes again.
      stopPolling();
    }
  }

  function fail(e: unknown) {
    const error = toApiError(e);
    set(state.invoice ? { error } : { phase: 'error', error });
    options.onError?.(error);
  }

  // Keep polling expired invoices too: the service still watches them for a
  // while, so a late payment can still arrive and confirm.
  function follow(id: string, run: number) {
    stopPolling();
    timer = setInterval(() => {
      client.getInvoice(id).then(
        (inv) => run === generation && show(inv),
        (e: unknown) => run === generation && fail(e),
      );
    }, pollMs);
  }

  function busy(): boolean {
    return destroyed || state.phase === 'creating' || state.phase === 'loading' || state.invoice !== null;
  }

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      listener(state);
      return () => listeners.delete(listener);
    },
    async create(input) {
      if (busy()) return;
      const run = generation;
      set({ phase: 'creating', error: null });
      try {
        const invoice = await client.createInvoice(input);
        if (run !== generation) return;
        show(invoice);
        if (invoice.status !== 'confirmed') follow(invoice.id, run);
      } catch (e) {
        if (run === generation) fail(e);
      }
    },
    async watch(id) {
      if (busy()) return;
      const run = generation;
      set({ phase: 'loading', error: null });
      try {
        const invoice = await client.getInvoice(id);
        if (run !== generation) return;
        show(invoice);
        if (invoice.status !== 'confirmed') follow(id, run);
      } catch (e) {
        if (run === generation) fail(e);
      }
    },
    reset() {
      generation++;
      stopPolling();
      fired.clear();
      set({ phase: 'idle', invoice: null, error: null });
    },
    destroy() {
      generation++;
      destroyed = true;
      stopPolling();
      listeners.clear();
    },
  };
}
