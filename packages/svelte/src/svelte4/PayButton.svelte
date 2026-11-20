<script>
  // Svelte 4 syntax (also runs under Svelte 5 in legacy mode).
  import { onDestroy } from 'svelte';
  import { LABELS, createCheckout } from '@deadhop/client';
  import InvoiceCard from './InvoiceCard.svelte';

  /** @type {Pick<import('@deadhop/client').Deadhop, 'createInvoice' | 'getInvoice'>} */
  export let client;
  /** Amount in sats. @type {number} */
  export let amount;
  /** @type {string | undefined} */
  export let memo = undefined;
  /** @type {number | undefined} */
  export let expirySecs = undefined;
  /** Button text (default `Pay 5,000 sats`). @type {string | undefined} */
  export let label = undefined;
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

  $: all = { ...LABELS, ...labels };
  $: idle = $checkout.phase === 'idle' || $checkout.phase === 'error';

  function pay() {
    checkout.reset();
    void checkout.create({ amount_sat: amount, ...(memo ? { memo } : {}), ...(expirySecs ? { expiry_secs: expirySecs } : {}) });
  }
</script>

<div class={className ? `deadhop ${className}` : 'deadhop'}>
  {#if idle}
    <button type="button" class="deadhop-pay" on:click={pay}>{label ?? all.pay(amount)}</button>
  {/if}
  <InvoiceCard state={$checkout} labels={all} {copy} />
</div>
