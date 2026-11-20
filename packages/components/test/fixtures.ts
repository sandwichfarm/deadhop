// A scripted service for component tests: every component, in every
// framework, is driven through the same scenarios.
import { vi } from 'vitest';
import { ApiError, type Invoice } from '@deadhop/client';

export const ADDRESS = 'bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080';
export const ID = '0123456789abcdef0123456789abcdef';

export function invoice(over: Partial<Invoice> = {}): Invoice {
  return {
    id: ID,
    address: ADDRESS,
    amount_sat: 5000,
    memo: 'Order 42',
    status: 'pending',
    received_sat: 0,
    confirmed_sat: 0,
    created_at: 1_700_000_000,
    expires_at: 4_000_000_000,
    settled_at: null,
    bip21: `bitcoin:${ADDRESS}?amount=0.00005000`,
    confirmations_required: 1,
    ...over,
  };
}

export const detected = (): Invoice => invoice({ status: 'detected', received_sat: 5000 });
export const confirmed = (): Invoice => invoice({ status: 'confirmed', received_sat: 5000, confirmed_sat: 5000, settled_at: 1_700_000_100 });

/** A fake service: `createInvoice` answers `created`; `getInvoice` walks through `polls`, repeating the last. */
export function fakeClient(polls: Array<Invoice | Error> = [invoice()], created: Invoice | Error = invoice()) {
  const queue = [...polls];
  let release: (() => void) | null = null;
  let hold = false;
  const client = {
    createInvoice: vi.fn(async (input: { amount_sat: number }) => {
      if (hold) await new Promise<void>((r) => (release = r));
      if (created instanceof Error) throw created;
      return { ...created, amount_sat: input.amount_sat, bip21: `bitcoin:${created.address}?amount=${(input.amount_sat / 1e8).toFixed(8)}` };
    }),
    getInvoice: vi.fn(async (id: string) => {
      const next = queue.length > 1 ? queue.shift()! : queue[0];
      if (next instanceof Error) throw next;
      return { ...next, id };
    }),
    /** Make the next createInvoice wait until `release()` (to see the preparing state). */
    holdCreate() {
      hold = true;
      return () => {
        hold = false;
        release?.();
      };
    },
  };
  return client;
}

export const unauthorized = (): ApiError => new ApiError('unauthorized');
export const notFound = (): ApiError => new ApiError('not_found');
