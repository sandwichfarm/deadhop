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
  let picked = $state<AdminInvoice | null>(null);

  const FILTERS: [Status | '', string][] = [
    ['', 'All'],
    ['pending', 'Awaiting payment'],
    ['detected', 'Confirming'],
    ['confirmed', 'Paid'],
    ['expired', 'Expired'],
  ];
  /** Progress of a drop: created, seen, confirmed. Done steps are solid; the one in transit is dashed. */
  function steps(inv: AdminInvoice): [string, string][] {
    const reached = { pending: 1, detected: 2, confirmed: 3, expired: 1 }[inv.status];
    return (['created', 'seen', 'confirmed'] as const).map((label, i) => [
      label,
      i < reached ? 'done' : i === reached && inv.status !== 'expired' ? 'transit' : 'todo',
    ]);
  }

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

  /** Short drop id shown to operators: the address index in hex. */
  function dropId(index: number): string {
    return index.toString(16).toUpperCase().padStart(4, '0');
  }
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
    <div>
      <h2>Drops</h2>
      <p class="muted">Every invoice is a drop: one fresh address, left for the customer to pay into.</p>
    </div>
    <div class="row">
      <div class="row" role="group" aria-label="Status">
        {#each FILTERS as [value, label] (value)}
          <button class={filter === value ? 'primary' : ''} aria-pressed={filter === value} onclick={() => (filter = value)}>{label}</button>
        {/each}
      </div>
      <button onclick={() => load()}>Refresh</button>
    </div>
  </div>

  {#if invoices.length === 0}
    <p class="muted">No invoices yet.</p>
  {:else}
    <div class="scroll">
      <table>
        <thead>
          <tr><th>Drop</th><th>Status</th><th>Amount</th><th>Received</th><th>Note</th><th>Created</th><th>By</th></tr>
        </thead>
        <tbody>
          {#each invoices as inv (inv.id)}
            <tr>
              <td><button class="link mono" aria-pressed={picked?.id === inv.id} onclick={() => (picked = inv)} title={`Address index ${inv.index}`}>{dropId(inv.index)}</button></td>
              <td><span class={`badge ${inv.status}`}>{statusLabel(inv.status)}</span></td>
              <td>{formatSats(inv.amount_sat)}</td>
              <td>{formatSats(inv.received_sat)}</td>
              <td>{inv.memo}</td>
              <td>{formatTime(inv.created_at)}</td>
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

{#if picked}
  <aside class="card drop-detail" aria-label={`Drop ${dropId(picked.index)}`}>
    <div class="row spread">
      <span class="mono muted">{`DROP ${dropId(picked.index)}`}</span>
      <span class={`badge ${picked.status}`}>{statusLabel(picked.status)}</span>
    </div>
    <p class="drop-amount">{formatSats(picked.amount_sat)}</p>
    {#if picked.memo}<p>{`Note: ${picked.memo}`}</p>{/if}
    <p class="mono address">{picked.address}</p>
    <ol class="progress">
      {#each steps(picked) as [label, state] (label)}
        <li class={state}>{label}</li>
      {/each}
    </ol>
    <p class="muted">{`Created ${formatTime(picked.created_at)} · expires ${formatTime(picked.expires_at)} · received ${formatSats(picked.received_sat)}`}</p>
    <div class="row">
      <a href={payHash(picked.id)}>Open payment page</a>
      <button onclick={() => (picked = null)}>Close</button>
    </div>
  </aside>
{/if}
