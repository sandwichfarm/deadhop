<script lang="ts">
  // Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
  import { invoiceView, progressMessage, type CheckoutState, type Labels } from '@cvmpay/client';

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
      <button type="button" class="cvmpay-copy" onclick={() => copyAddress(view!.address)}>{copied ? labels.copied : labels.copy}</button>
      {#if view.expiry}<p class="cvmpay-expiry">{view.expiry}</p>{/if}
    {/if}
    {#if view.received}<p class="cvmpay-received">{view.received}</p>{/if}
    {#if current.error}<p class="cvmpay-error" role="alert">{current.error.message}</p>{/if}
  </div>
{:else if progress}
  <div class="cvmpay-invoice"><p class="cvmpay-status" role="status">{progress}</p></div>
{:else if current.error}
  <p class="cvmpay-error" role="alert">{current.error.message}</p>
{/if}
