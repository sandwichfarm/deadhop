<script>
  // Svelte 4 syntax (also runs under Svelte 5 in legacy mode).
  import { onDestroy } from 'svelte';
  import { LABELS, createCheckout } from '@deadhop/client';
  import InvoiceCard from './InvoiceCard.svelte';

  /** @type {Pick<import('@deadhop/client').Deadhop, 'createInvoice' | 'getInvoice'>} */
  export let client;
  /** Id of an invoice created elsewhere (for example by your backend). @type {string} */
  export let id;
  /** @type {number | undefined} */
  export let pollMs = undefined;
  /** @type {Partial<import('@deadhop/client').Labels>} */
  export let labels = {};
  let className = '';
  export { className as class };
  /** @type {((text: string) => Promise<void>) | undefined} */
  export let copy = undefined;
  /** @type {import('@deadhop/client').CheckoutEvents['onCreated']} */
  export let onCreated = undefined;
  /** @type {import('@deadhop/client').CheckoutEvents['onDetected']} */
  export let onDetected = undefined;
  /** @type {import('@deadhop/client').CheckoutEvents['onPaid']} */
  export let onPaid = undefined;
  /** @type {import('@deadhop/client').CheckoutEvents['onError']} */
  export let onError = undefined;

  const checkout = createCheckout(client, {
    pollMs,
    onCreated: (inv) => onCreated?.(inv),
    onDetected: (inv) => onDetected?.(inv),
    onPaid: (inv) => onPaid?.(inv),
    onError: (e) => onError?.(e),
  });
  onDestroy(() => checkout.destroy());

  $: follow(id);
  function follow(current) {
    checkout.reset();
    void checkout.watch(current);
  }
</script>

<div class={className ? `deadhop ${className}` : 'deadhop'}>
  <InvoiceCard state={$checkout} labels={{ ...LABELS, ...labels }} {copy} />
</div>
