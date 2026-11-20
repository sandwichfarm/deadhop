<script lang="ts">
  import type { PayApi } from '../lib/api';
  import type { AdminInvoice, Info, Invoice, Stats, Status } from '../lib/schemas';
  import { formatBtc, formatSats, formatTime, shortKey, statusLabel } from '../lib/format';
  import { payHash } from '../lib/route';
  import CreateInvoice from './CreateInvoice.svelte';

  let {
    api,
    info,
    onCreated,
    refreshMs = 15000,
  }: { api: PayApi; info: Info; onCreated: (invoice: Invoice) => void; refreshMs?: number } = $props();

  let stats = $state<Stats | null>(null);
  let invoices = $state<AdminInvoice[]>([]);
  let nextBefore = $state<number | null>(null);
  let filter = $state<Status | ''>('');
  let error = $state('');

  async function load() {
    try {
      const [s, page] = await Promise.all([api.getStats(), api.listInvoices(filter === '' ? {} : { status: filter })]);
      stats = s;
      invoices = page.invoices;
      nextBefore = page.next_before;
      error = '';
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function loadMore(before: number) {
    try {
      const page = await api.listInvoices(filter === '' ? { before } : { status: filter, before });
      invoices = [...invoices, ...page.invoices];
      nextBefore = page.next_before;
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // Runs on mount and again whenever the status filter changes.
  $effect(() => {
    void load();
    const timer = setInterval(() => void load(), refreshMs);
    return () => clearInterval(timer);
  });
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

{#if stats && stats.watch_errors > 0}
  <p class="error" role="alert">
    {`${stats.watch_errors} open invoice${stats.watch_errors === 1 ? '' : 's'} could not be read from the chain and need a look (see the service log).`}
  </p>
{/if}

{#if stats}
  <section class="stats">
    <div class="card"><span class="muted">Confirmed</span><strong>{formatBtc(stats.confirmed_sat)}</strong><span>{`${stats.confirmed} paid`}</span></div>
    <div class="card"><span class="muted">Confirming</span><strong>{formatBtc(stats.detected_sat)}</strong><span>{`${stats.detected} seen`}</span></div>
    <div class="card"><span class="muted">Open</span><strong>{stats.pending}</strong><span>{`${stats.expired} expired`}</span></div>
    <div class="card">
      <span class="muted">Chain</span>
      <strong class={stats.chain_ok ? 'ok' : 'bad'}>{stats.chain_ok ? 'Connected' : 'Unreachable'}</strong>
      <span>{stats.tip_height === null ? 'block height not known yet' : `block ${stats.tip_height}`}</span>
    </div>
  </section>
{/if}

<CreateInvoice {api} {info} {onCreated} />

<section class="card">
  <div class="row spread">
    <h2>Invoices</h2>
    <div class="row">
      <label for="filter">Status</label>
      <select id="filter" bind:value={filter}>
        <option value="">All</option>
        <option value="pending">Awaiting payment</option>
        <option value="detected">Confirming</option>
        <option value="confirmed">Paid</option>
        <option value="expired">Expired</option>
      </select>
      <button onclick={() => load()}>Refresh</button>
    </div>
  </div>

  {#if invoices.length === 0}
    <p class="muted">No invoices yet.</p>
  {:else}
    <div class="scroll">
      <table>
        <thead>
          <tr><th>#</th><th>Created</th><th>Amount</th><th>Status</th><th>Received</th><th>Note</th><th>By</th></tr>
        </thead>
        <tbody>
          {#each invoices as inv (inv.id)}
            <tr>
              <td><a href={payHash(inv.id)}>{inv.index}</a></td>
              <td>{formatTime(inv.created_at)}</td>
              <td>{formatSats(inv.amount_sat)}</td>
              <td><span class={`badge ${inv.status}`}>{statusLabel(inv.status)}</span></td>
              <td>{formatSats(inv.received_sat)}</td>
              <td>{inv.memo}</td>
              <td title={inv.creator}>{shortKey(inv.creator)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if nextBefore !== null}
      <button onclick={() => loadMore(nextBefore!)}>Load more</button>
    {/if}
  {/if}
</section>
