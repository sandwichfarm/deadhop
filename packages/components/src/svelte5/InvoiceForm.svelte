<script lang="ts">
  import { LABELS, parseSats, type CheckoutEvents, type Cvmpay, type Labels } from '@cvmpay/client';
  import InvoiceCard from './InvoiceCard.svelte';
  import { useCheckout } from './checkout.svelte.js';

  let {
    client,
    defaultAmount,
    defaultMemo = '',
    showMemo = true,
    expirySecs,
    pollMs,
    labels = {},
    class: className = '',
    copy,
    ...events
  }: CheckoutEvents & {
    client: Pick<Cvmpay, 'createInvoice' | 'getInvoice'>;
    defaultAmount?: number;
    defaultMemo?: string;
    showMemo?: boolean;
    expirySecs?: number;
    pollMs?: number;
    labels?: Partial<Labels>;
    class?: string;
    copy?: (text: string) => Promise<void>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const c = useCheckout(client, { pollMs, events: () => events });
  const all = $derived({ ...LABELS, ...labels });
  // svelte-ignore state_referenced_locally
  let amount = $state(defaultAmount ? String(defaultAmount) : '');
  // svelte-ignore state_referenced_locally
  let memo = $state(defaultMemo);
  let invalid = $state(false);
  const uid = $props.id();
  const editing = $derived(c.state.phase === 'idle' || c.state.phase === 'error');

  function submit(e: SubmitEvent) {
    e.preventDefault();
    const sats = parseSats(amount);
    invalid = sats === null;
    if (sats === null) return;
    const note = memo.trim();
    c.checkout.reset();
    void c.checkout.create({ amount_sat: sats, ...(note ? { memo: note } : {}), ...(expirySecs ? { expiry_secs: expirySecs } : {}) });
  }
</script>

<div class={className ? `cvmpay ${className}` : 'cvmpay'}>
  {#if editing}
    <form class="cvmpay-form" onsubmit={submit} novalidate>
      <label for={`${uid}-amount`}>{all.amount}</label>
      <input id={`${uid}-amount`} name="amount" inputmode="numeric" autocomplete="off" bind:value={amount} />
      {#if showMemo}
        <label for={`${uid}-memo`}>{all.memo}</label>
        <input id={`${uid}-memo`} name="memo" maxlength="200" bind:value={memo} />
      {/if}
      {#if invalid}<p class="cvmpay-error" role="alert">{all.invalidAmount}</p>{/if}
      <button type="submit" class="cvmpay-create">{all.create}</button>
    </form>
  {:else}
    <button type="button" class="cvmpay-again" onclick={() => c.checkout.reset()}>{all.again}</button>
  {/if}
  <InvoiceCard state={c.state} labels={all} {copy} />
</div>
