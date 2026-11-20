<script context="module">
  let forms = 0;
</script>

<script>
  // Svelte 4 syntax (also runs under Svelte 5 in legacy mode).
  import { onDestroy } from 'svelte';
  import { LABELS, createCheckout, parseSats } from '@deadhop/client';
  import InvoiceCard from './InvoiceCard.svelte';

  /** @type {Pick<import('@deadhop/client').Deadhop, 'createInvoice' | 'getInvoice'>} */
  export let client;
  /** Amount filled in at first, in sats. @type {number | undefined} */
  export let defaultAmount = undefined;
  /** Note filled in at first. */
  export let defaultMemo = '';
  /** Show the note field. */
  export let showMemo = true;
  /** @type {number | undefined} */
  export let expirySecs = undefined;
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

  const uid = `deadhop-form-${++forms}`;
  let amount = defaultAmount ? String(defaultAmount) : '';
  let memo = defaultMemo;
  let invalid = false;
  $: all = { ...LABELS, ...labels };
  $: editing = $checkout.phase === 'idle' || $checkout.phase === 'error';

  function submit() {
    const sats = parseSats(amount);
    invalid = sats === null;
    if (sats === null) return;
    const note = memo.trim();
    checkout.reset();
    void checkout.create({ amount_sat: sats, ...(note ? { memo: note } : {}), ...(expirySecs ? { expiry_secs: expirySecs } : {}) });
  }
</script>

<div class={className ? `deadhop ${className}` : 'deadhop'}>
  {#if editing}
    <form class="deadhop-form" on:submit|preventDefault={submit} novalidate>
      <label for={`${uid}-amount`}>{all.amount}</label>
      <input id={`${uid}-amount`} name="amount" inputmode="numeric" autocomplete="off" bind:value={amount} />
      {#if showMemo}
        <label for={`${uid}-memo`}>{all.memo}</label>
        <input id={`${uid}-memo`} name="memo" maxlength="200" bind:value={memo} />
      {/if}
      {#if invalid}<p class="deadhop-error" role="alert">{all.invalidAmount}</p>{/if}
      <button type="submit" class="deadhop-create">{all.create}</button>
    </form>
  {:else}
    <button type="button" class="deadhop-again" on:click={() => checkout.reset()}>{all.again}</button>
  {/if}
  <InvoiceCard state={$checkout} labels={all} {copy} />
</div>
