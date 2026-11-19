import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from './api.ts';
import { createCheckout, type CheckoutState } from './checkout.ts';
import type { Invoice } from './schemas.ts';
import { invoice } from './test-helpers.ts';

function client(sequence: Array<Invoice | Error>, created: Invoice | Error = invoice()) {
  const queue = [...sequence];
  return {
    createInvoice: vi.fn(async () => {
      if (created instanceof Error) throw created;
      return created;
    }),
    getInvoice: vi.fn(async () => {
      const next = queue.length > 1 ? queue.shift()! : queue[0];
      if (next instanceof Error) throw next;
      return next;
    }),
  };
}

async function tick(ms: number) {
  await vi.advanceTimersByTimeAsync(ms);
}

describe('createCheckout', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('creates an invoice and follows it to confirmed, firing each event once', async () => {
    const c = client([invoice({ status: 'pending' }), invoice({ status: 'detected', received_sat: 50_000 }), invoice({ status: 'confirmed', received_sat: 50_000, confirmed_sat: 50_000 })]);
    const events = { onCreated: vi.fn(), onDetected: vi.fn(), onPaid: vi.fn(), onError: vi.fn() };
    let t = 100;
    const checkout = createCheckout(c, { ...events, pollMs: 1000, clock: () => t });
    const seen: CheckoutState['phase'][] = [];
    const stop = checkout.subscribe((s) => seen.push(s.phase));
    expect(seen).toEqual(['idle']);

    await checkout.create({ amount_sat: 50_000, memo: 'x' });
    expect(c.createInvoice).toHaveBeenCalledWith({ amount_sat: 50_000, memo: 'x' });
    expect(checkout.getState()).toMatchObject({ phase: 'pending', error: null });
    expect(events.onCreated).toHaveBeenCalledTimes(1);

    // A second create while one is followed is ignored.
    await checkout.create({ amount_sat: 1 });
    expect(c.createInvoice).toHaveBeenCalledTimes(1);
    await checkout.watch('f'.repeat(32));
    expect(c.getInvoice).not.toHaveBeenCalled();

    t = 105;
    await tick(1000);
    expect(checkout.getState().phase).toBe('pending');
    expect(checkout.getState().now).toBe(105);
    await tick(1000);
    expect(checkout.getState().phase).toBe('detected');
    expect(events.onDetected).toHaveBeenCalledTimes(1);
    await tick(1000);
    expect(checkout.getState().phase).toBe('confirmed');
    expect(events.onPaid).toHaveBeenCalledTimes(1);
    expect(events.onDetected).toHaveBeenCalledTimes(1);

    // Confirmed: polling stops.
    const calls = c.getInvoice.mock.calls.length;
    await tick(5000);
    expect(c.getInvoice.mock.calls.length).toBe(calls);
    expect(seen).toEqual(['idle', 'creating', 'pending', 'pending', 'detected', 'confirmed']);
    expect(events.onError).not.toHaveBeenCalled();
    stop();
    checkout.destroy();
  });

  it('keeps following an expired invoice, since a late payment can still confirm', async () => {
    const c = client([invoice({ status: 'expired' }), invoice({ status: 'expired' }), invoice({ status: 'confirmed', received_sat: 50_000, confirmed_sat: 50_000 })]);
    const onPaid = vi.fn();
    const checkout = createCheckout(c, { pollMs: 10, onPaid });
    await checkout.watch(invoice().id);
    expect(checkout.getState().phase).toBe('expired');
    await tick(10);
    expect(checkout.getState().phase).toBe('expired');
    await tick(10);
    expect(onPaid).toHaveBeenCalledTimes(1);
  });

  it('adopts an already confirmed invoice without polling', async () => {
    const c = client([invoice({ status: 'confirmed' })], invoice({ status: 'confirmed' }));
    const watched = createCheckout(c, { pollMs: 10 });
    await watched.watch(invoice().id);
    const created = createCheckout(c, { pollMs: 10 });
    await created.create({ amount_sat: 50_000 });
    await tick(100);
    expect(c.getInvoice).toHaveBeenCalledTimes(1);
    expect(created.getState().phase).toBe('confirmed');
  });

  it('reports failures: fatal without an invoice, transient with one', async () => {
    const onError = vi.fn();
    const failing = createCheckout(client([], new ApiError('unauthorized')), { onError });
    await failing.create({ amount_sat: 1 });
    expect(failing.getState()).toMatchObject({ phase: 'error', invoice: null });
    expect(failing.getState().error?.code).toBe('unauthorized');
    expect(onError).toHaveBeenCalledTimes(1);

    // After an error, a new attempt is allowed.
    const lost = createCheckout(client([new ApiError('not_found')]), { onError });
    await lost.watch('f'.repeat(32));
    expect(lost.getState().phase).toBe('error');
    await lost.watch('f'.repeat(32));
    expect(lost.getState().error?.code).toBe('not_found');

    const flaky = createCheckout(client([invoice(), new Error('socket'), invoice({ status: 'detected' })]), { pollMs: 10, onError });
    await flaky.watch(invoice().id);
    await tick(10);
    expect(flaky.getState()).toMatchObject({ phase: 'pending' });
    expect(flaky.getState().error?.code).toBe('network');
    await tick(10);
    expect(flaky.getState()).toMatchObject({ phase: 'detected', error: null });
    flaky.destroy();
  });

  it('drops answers that arrive after a reset or destroy', async () => {
    let resolveCreate: (inv: Invoice) => void = () => undefined;
    let rejectWatch: (e: Error) => void = () => undefined;
    const c = {
      createInvoice: vi.fn(() => new Promise<Invoice>((r) => (resolveCreate = r))),
      getInvoice: vi.fn(() => new Promise<Invoice>((_, reject) => (rejectWatch = reject))),
    };
    const checkout = createCheckout(c, { pollMs: 10 });
    const creating = checkout.create({ amount_sat: 50_000 });
    expect(checkout.getState().phase).toBe('creating');
    checkout.reset();
    resolveCreate(invoice());
    await creating;
    expect(checkout.getState().phase).toBe('idle');

    const watching = checkout.watch(invoice().id);
    checkout.reset();
    rejectWatch(new Error('late'));
    await watching;
    expect(checkout.getState()).toMatchObject({ phase: 'idle', error: null });

    let rejectCreate: (e: Error) => void = () => undefined;
    c.createInvoice.mockImplementationOnce(() => new Promise<Invoice>((_, r) => (rejectCreate = r)));
    const refused = checkout.create({ amount_sat: 1 });
    checkout.reset();
    rejectCreate(new Error('late'));
    await refused;
    expect(checkout.getState()).toMatchObject({ phase: 'idle', error: null });

    let resolveWatch: (inv: Invoice) => void = () => undefined;
    c.getInvoice.mockImplementationOnce(() => new Promise<Invoice>((r) => (resolveWatch = r)));
    const adopting = checkout.watch(invoice().id);
    checkout.reset();
    resolveWatch(invoice());
    await adopting;
    expect(checkout.getState()).toMatchObject({ phase: 'idle', invoice: null });

    // Polls in flight when the checkout is reset are dropped too.
    const slow = client([invoice()]);
    const followed = createCheckout(slow, { pollMs: 10 });
    await followed.watch(invoice().id);
    let pollAnswer: (inv: Invoice) => void = () => undefined;
    let pollFail: (e: Error) => void = () => undefined;
    slow.getInvoice.mockImplementationOnce(() => new Promise<Invoice>((r) => (pollAnswer = r)));
    await tick(10);
    followed.reset();
    pollAnswer(invoice({ status: 'confirmed' }));
    await tick(0);
    expect(followed.getState().phase).toBe('idle');
    await followed.watch(invoice().id);
    slow.getInvoice.mockImplementationOnce(() => new Promise<Invoice>((_, r) => (pollFail = r)));
    await tick(10);
    followed.destroy();
    pollFail(new Error('late'));
    await tick(0);
    expect(followed.getState().error).toBeNull();
    // A destroyed checkout does nothing more.
    await followed.create({ amount_sat: 1 });
    expect(slow.createInvoice).not.toHaveBeenCalled();
  });

  it('uses the wall clock and default interval when none are given', async () => {
    vi.setSystemTime(new Date('2026-01-01T00:00:00Z'));
    const c = client([invoice()]);
    const checkout = createCheckout(c);
    expect(checkout.getState().now).toBe(Date.UTC(2026, 0, 1) / 1000);
    await checkout.watch(invoice().id);
    await tick(4999);
    expect(c.getInvoice).toHaveBeenCalledTimes(1);
    await tick(1);
    expect(c.getInvoice).toHaveBeenCalledTimes(2);
    checkout.destroy();
  });
});
