'use client';
// React binding of the headless checkout.
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { createCheckout, type Checkout, type CheckoutEvents, type CheckoutState, type Cvmpay } from '@cvmpay/client';

/** Options for `useCheckout`. Callbacks always see the latest props. */
export interface UseCheckoutOptions extends CheckoutEvents {
  pollMs?: number;
}

/** A checkout that lives as long as the component: `[state, checkout]`. */
export function useCheckout(client: Pick<Cvmpay, 'createInvoice' | 'getInvoice'>, options: UseCheckoutOptions = {}): [CheckoutState, Checkout] {
  const latest = useRef(options);
  latest.current = options;
  const [checkout] = useState(() =>
    createCheckout(client, {
      pollMs: options.pollMs,
      onCreated: (inv) => latest.current.onCreated?.(inv),
      onDetected: (inv) => latest.current.onDetected?.(inv),
      onPaid: (inv) => latest.current.onPaid?.(inv),
      onError: (e) => latest.current.onError?.(e),
    }),
  );
  // Reset rather than destroy: React may unmount and mount again (StrictMode),
  // and resetting stops polling and drops answers to requests still in flight.
  useEffect(() => () => checkout.reset(), [checkout]);
  const state = useSyncExternalStore(checkout.subscribe, checkout.getState, checkout.getState);
  return [state, checkout];
}
