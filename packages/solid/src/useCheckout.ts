// Solid binding of the headless checkout.
import { createSignal, onCleanup, type Accessor } from 'solid-js';
import { createCheckout, type Checkout, type CheckoutEvents, type CheckoutState, type Deadhop } from '@deadhop/client';

/** Options for `useCheckout`. Pass getters (or a props object) so callbacks stay current. */
export interface UseCheckoutOptions extends CheckoutEvents {
  pollMs?: number;
}

/** A checkout owned by the current component: `[state, checkout]`. */
export function useCheckout(client: Pick<Deadhop, 'createInvoice' | 'getInvoice'>, options: UseCheckoutOptions = {}): [Accessor<CheckoutState>, Checkout] {
  const checkout = createCheckout(client, {
    pollMs: options.pollMs,
    onCreated: (inv) => options.onCreated?.(inv),
    onDetected: (inv) => options.onDetected?.(inv),
    onPaid: (inv) => options.onPaid?.(inv),
    onError: (e) => options.onError?.(e),
  });
  const [state, setState] = createSignal(checkout.getState());
  const unsubscribe = checkout.subscribe((s) => setState(() => s));
  onCleanup(() => {
    unsubscribe();
    checkout.destroy();
  });
  return [state, checkout];
}
