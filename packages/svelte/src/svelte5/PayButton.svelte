<script lang="ts">
  import { LABELS, type CheckoutEvents, type Deadhop, type Labels } from '@deadhop/client';
  import InvoiceCard from './InvoiceCard.svelte';
  import { useCheckout } from './checkout.svelte.js';

  let {
    client,
    amount,
    memo,
    expirySecs,
    label,
    pollMs,
    labels = {},
    class: className = '',
    copy,
    ...events
  }: CheckoutEvents & {
    client: Pick<Deadhop, 'createInvoice' | 'getInvoice'>;
    /** Amount in sats. */
    amount: number;
    memo?: string;
    expirySecs?: number;
    /** Button text (default `Pay 5,000 sats`). */
    label?: string;
    pollMs?: number;
    labels?: Partial<Labels>;
    class?: string;
    copy?: (text: string) => Promise<void>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const c = useCheckout(client, { pollMs, events: () => events });
  const all = $derived({ ...LABELS, ...labels });
  const idle = $derived(c.state.phase === 'idle' || c.state.phase === 'error');

  function pay() {
    c.checkout.reset();
    void c.checkout.create({ amount_sat: amount, ...(memo ? { memo } : {}), ...(expirySecs ? { expiry_secs: expirySecs } : {}) });
  }
</script>

<div class={className ? `deadhop ${className}` : 'deadhop'}>
  {#if idle}
    <button type="button" class="deadhop-pay" onclick={pay}>{label ?? all.pay(amount)}</button>
  {/if}
  <InvoiceCard state={c.state} labels={all} {copy} />
</div>
