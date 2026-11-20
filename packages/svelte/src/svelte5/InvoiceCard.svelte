<script lang="ts">
  // Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
  import { invoiceView, progressMessage, type CheckoutState, type Labels } from '@deadhop/client';

  let {
    state: current,
    labels,
    copy = (text: string) => navigator.clipboard.writeText(text),
  }: { state: CheckoutState; labels: Labels; copy?: (text: string) => Promise<void> } = $props();

  let copied = $state(false);
  const view = $derived(current.invoice ? invoiceView(current.invoice, current.now) : null);
  const progress = $derived(progressMessage(current));

  async function copyAddress(address: string) {
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
      <button type="button" class="deadhop-copy" onclick={() => copyAddress(view!.address)}>{copied ? labels.copied : labels.copy}</button>
      {#if view.expiry}<p class="deadhop-expiry">{view.expiry}</p>{/if}
    {/if}
    {#if view.received}<p class="deadhop-received">{view.received}</p>{/if}
    {#if current.error}<p class="deadhop-error" role="alert">{current.error.message}</p>{/if}
  </div>
{:else if progress}
  <div class="deadhop-invoice"><p class="deadhop-status" role="status">{progress}</p></div>
{:else if current.error}
  <p class="deadhop-error" role="alert">{current.error.message}</p>
{/if}
