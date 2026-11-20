<script>
  // Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
  // Svelte 4 syntax (also runs under Svelte 5 in legacy mode).
  import { invoiceView, progressMessage } from '@deadhop/client';

  /** @type {import('@deadhop/client').CheckoutState} */
  export let state;
  /** @type {import('@deadhop/client').Labels} */
  export let labels;
  /** @type {(text: string) => Promise<void>} */
  export let copy = (text) => navigator.clipboard.writeText(text);

  let copied = false;
  $: view = state.invoice ? invoiceView(state.invoice, state.now) : null;
  $: progress = progressMessage(state);

  async function copyAddress(address) {
    await copy(address);
    copied = true;
  }
</script>

{#if view}
  <div class="deadhop-invoice" data-status={view.status}>
    <p class="deadhop-status" role="status">{view.statusLabel}</p>
    <p class="deadhop-amount">{view.amountBtc}</p>
    <p class="deadhop-sats">{view.amountSats}</p>
    {#if view.memo !== null}
      <p class="deadhop-memo"><span class="deadhop-memo-label">{labels.memoNote}</span> <span>{view.memo}</span></p>
    {/if}
    {#if view.qr}
      <a class="deadhop-qr" href={view.bip21} aria-label={labels.openInWallet}>
        <svg class="deadhop-qr-code" viewBox={`0 0 ${view.qr.size} ${view.qr.size}`} role="img" aria-label={labels.qr} shape-rendering="crispEdges">
          <rect width={view.qr.size} height={view.qr.size} fill="#ffffff" />
          <path d={view.qr.path} fill="#000000" />
        </svg>
      </a>
      <p class="deadhop-address">{view.address}</p>
      <button type="button" class="deadhop-copy" on:click={() => copyAddress(view.address)}>{copied ? labels.copied : labels.copy}</button>
      {#if view.expiry}<p class="deadhop-expiry">{view.expiry}</p>{/if}
    {/if}
    {#if view.received}<p class="deadhop-received">{view.received}</p>{/if}
    {#if state.error}<p class="deadhop-error" role="alert">{state.error.message}</p>{/if}
  </div>
{:else if progress}
  <div class="deadhop-invoice"><p class="deadhop-status" role="status">{progress}</p></div>
{:else if state.error}
  <p class="deadhop-error" role="alert">{state.error.message}</p>
{/if}
