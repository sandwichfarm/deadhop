<script context="module">
  let forms = 0;
</script>

<script>
  // Svelte 4 syntax (also runs under Svelte 5 in legacy mode).
  import { onDestroy } from 'svelte';
  import { LABELS, createCheckout, parseSats } from '@cvmpay/client';
  import InvoiceCard from './InvoiceCard.svelte';

  /** @type {Pick<import('@cvmpay/client').Cvmpay, 'createInvoice' | 'getInvoice'>} */
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
  /** @type {Partial<import('@cvmpay/client').Labels>} */
  export let labels = {};
  let className = '';
  export { className as class };
  /** @type {((text: string) => Promise<void>) | undefined} */
  export let copy = undefined;
  /** @type {import('@cvmpay/client').CheckoutEvents['onCreated']} */
  export let onCreated = undefined;
  /** @type {import('@cvmpay/client').CheckoutEvents['onDetected']} */
  export let onDetected = undefined;
  /** @type {import('@cvmpay/client').CheckoutEvents['onPaid']} */
  export let onPaid = undefined;
  /** @type {import('@cvmpay/client').CheckoutEvents['onError']} */
  export let onError = undefined;

  const checkout = createCheckout(client, {
    pollMs,
    onCreated: (inv) => onCreated?.(inv),
    onDetected: (inv) => onDetected?.(inv),
    onPaid: (inv) => onPaid?.(inv),
    onError: (e) => onError?.(e),
  });
  onDestroy(() => checkout.destroy());

  const uid = `cvmpay-form-${++forms}`;
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

<div class={className ? `cvmpay ${className}` : 'cvmpay'}>
  {#if editing}
    <form class="cvmpay-form" on:submit|preventDefault={submit} novalidate>
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
    <button type="button" class="cvmpay-again" on:click={() => checkout.reset()}>{all.again}</button>
  {/if}
  <InvoiceCard state={$checkout} labels={all} {copy} />
</div>
