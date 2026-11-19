<script>
  // Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
  // Svelte 4 syntax (also runs under Svelte 5 in legacy mode).
  import { invoiceView, progressMessage } from '@cvmpay/client';

  /** @type {import('@cvmpay/client').CheckoutState} */
  export let state;
  /** @type {import('@cvmpay/client').Labels} */
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
  <div class="cvmpay-invoice" data-status={view.status}>
    <p class="cvmpay-status" role="status">{view.statusLabel}</p>
    <p class="cvmpay-amount">{view.amountBtc}</p>
    <p class="cvmpay-sats">{view.amountSats}</p>
    {#if view.memo !== null}
      <p class="cvmpay-memo"><span class="cvmpay-memo-label">{labels.memoNote}</span> <span>{view.memo}</span></p>
    {/if}
    {#if view.qr}
      <a class="cvmpay-qr" href={view.bip21} aria-label={labels.openInWallet}>
        <svg class="cvmpay-qr-code" viewBox={`0 0 ${view.qr.size} ${view.qr.size}`} role="img" aria-label={labels.qr} shape-rendering="crispEdges">
          <rect width={view.qr.size} height={view.qr.size} fill="#ffffff" />
          <path d={view.qr.path} fill="#000000" />
        </svg>
      </a>
      <p class="cvmpay-address">{view.address}</p>
      <button type="button" class="cvmpay-copy" on:click={() => copyAddress(view.address)}>{copied ? labels.copied : labels.copy}</button>
      {#if view.expiry}<p class="cvmpay-expiry">{view.expiry}</p>{/if}
    {/if}
    {#if view.received}<p class="cvmpay-received">{view.received}</p>{/if}
    {#if state.error}<p class="cvmpay-error" role="alert">{state.error.message}</p>{/if}
  </div>
{:else if progress}
  <div class="cvmpay-invoice"><p class="cvmpay-status" role="status">{progress}</p></div>
{:else if state.error}
  <p class="cvmpay-error" role="alert">{state.error.message}</p>
{/if}
