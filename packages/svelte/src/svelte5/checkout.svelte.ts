// Svelte 5 binding of the headless checkout: reactive state that follows it.
import { createCheckout, type Checkout, type CheckoutEvents, type CheckoutState, type Deadhop } from '@deadhop/client';

/** Options for `useCheckout`. Pass a getter for events that change over time. */
export interface UseCheckoutOptions {
  pollMs?: number;
  events?: () => CheckoutEvents;
}

/** A checkout owned by the calling component (or `$effect.root`); read `.state`, call `.checkout`. */
export function useCheckout(
  client: Pick<Deadhop, 'createInvoice' | 'getInvoice'>,
  options: UseCheckoutOptions = {},
): { readonly state: CheckoutState; checkout: Checkout } {
  const events: () => CheckoutEvents = options.events ?? (() => ({}));
  const checkout = createCheckout(client, {
    pollMs: options.pollMs,
    onCreated: (inv) => events().onCreated?.(inv),
    onDetected: (inv) => events().onDetected?.(inv),
    onPaid: (inv) => events().onPaid?.(inv),
    onError: (e) => events().onError?.(e),
  });
  let state = $state(checkout.getState());
  const unsubscribe = checkout.subscribe((s) => (state = s));
  // Torn down with the owning component (or effect root).
  $effect(() => () => {
    unsubscribe();
    checkout.destroy();
  });
  return {
    get state() {
      return state;
    },
    checkout,
  };
}
