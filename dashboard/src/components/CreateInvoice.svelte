<script lang="ts">
  import type { PayApi } from '../lib/api';
  import type { Info, Invoice } from '../lib/schemas';
  import { formatSats } from '../lib/format';

  let { api, info, onCreated }: { api: PayApi; info: Info; onCreated: (invoice: Invoice) => void } = $props();

  let amount = $state('');
  let memo = $state('');
  let busy = $state(false);
  let error = $state('');

  const mining = $derived(!info.is_admin && info.pow_bits > 0);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    error = '';
    const amount_sat = Number(amount);
    if (!Number.isInteger(amount_sat) || amount_sat < info.min_amount_sat || amount_sat > info.max_amount_sat) {
      error = `Enter a whole number between ${formatSats(info.min_amount_sat)} and ${formatSats(info.max_amount_sat)}.`;
      return;
    }
    busy = true;
    try {
      const text = memo.trim();
      onCreated(await api.createInvoice(text === '' ? { amount_sat } : { amount_sat, memo: text }));
      amount = '';
      memo = '';
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
</script>

<section class="card">
  <h2>New invoice</h2>
  <form onsubmit={submit}>
    <label for="amount">Amount (sats)</label>
    <input id="amount" inputmode="numeric" autocomplete="off" placeholder={String(info.min_amount_sat)} bind:value={amount} />
    <label for="memo">Note (optional)</label>
    <input id="memo" maxlength="200" autocomplete="off" bind:value={memo} />
    <button class="primary" type="submit" disabled={busy}>Create invoice</button>
  </form>
  {#if busy}
    <p role="status">{mining ? `Computing proof of work (${info.pow_bits} bits)…` : 'Creating…'}</p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>
