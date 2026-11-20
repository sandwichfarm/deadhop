<script lang="ts">
  import type { PayApi } from '../lib/api';
  import type { AdminInvoice, Health, Metrics, PrivacyAudit, Range, Series } from '../lib/schemas';
  import { statusLabel } from '../lib/format';
  import { formatBtc, formatBytes, formatDuration, formatSats, formatTime } from '../lib/format';
  import Chart from './Chart.svelte';
  import ProxyStatus from './ProxyStatus.svelte';

  let { api, refreshMs = 15000, onAudit }: { api: PayApi; refreshMs?: number; onAudit: () => void } = $props();

  // The audit is optional on this page: if it fails, the tile is left out.
  let audit = $state<PrivacyAudit | null>(null);
  // Recent drops are optional on this page too.
  let recent = $state<AdminInvoice[]>([]);
  $effect(() => {
    Promise.resolve()
      .then(() => api.listInvoices({}))
      .then(
        (page) => (recent = page.invoices.slice(0, 5)),
        () => (recent = []),
      );
  });
  $effect(() => {
    api.getPrivacyAudit().then(
      (a) => (audit = a),
      () => (audit = null),
    );
  });

  let data = $state<{ health: Health; metrics: Metrics; series: Series } | null>(null);
  let range = $state<Range>('week');
  let metric = $state<'sat' | 'count'>('sat');
  let error = $state('');

  const RANGES: { value: Range; label: string }[] = [
    { value: 'day', label: '24 hours' },
    { value: 'week', label: '7 days' },
    { value: 'month', label: '30 days' },
    { value: 'year', label: '1 year' },
  ];
  const TRANSPORT: Record<Health['chain']['transport'], string> = {
    tcp: 'plain TCP',
    tls: 'TLS (public authorities)',
    'tls-pinned': 'TLS, pinned certificate',
  };
  const WINDOW_LABELS: Record<string, string> = { '24h': 'Last 24 hours', '7d': 'Last 7 days', '30d': 'Last 30 days', '365d': 'Last year' };

  async function load(which: Range) {
    try {
      const [health, metrics, series] = await Promise.all([api.getHealth(), api.getMetrics(), api.getSeries(which)]);
      data = { health, metrics, series };
      error = '';
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // Runs on mount and whenever the chart range changes.
  $effect(() => {
    const which = range;
    void load(which);
    const timer = setInterval(() => void load(which), refreshMs);
    return () => clearInterval(timer);
  });

</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

{#if data}
  {@const { health, metrics, series } = data}
  {@const connected = health.relays.filter((r) => r.connected).length}
  <header class="page-head">
    <div>
      <p class="eyebrow">Overview</p>
      <h1>{health.service.paused ? 'Invoicing stopped.' : 'Serving. Nothing to reach.'}</h1>
    </div>
  </header>
  {#each health.warnings as w (w)}
    <p class="note" role="status">{w}</p>
  {/each}
  <section class="stats wide" aria-label="Privacy and proxy">
    {#if audit}
      {@const toFix = audit.checks.filter((c) => c.result === 'fail' || c.result === 'to_fix').length}
      <button class="card score-tile" onclick={onAudit} aria-label={`Privacy score ${audit.score} out of 100. Open the privacy audit`}>
        <span class="muted">Privacy score</span>
        <strong class={audit.score >= 90 ? 'ok' : audit.score >= 60 ? 'warn' : 'bad'}>{`${audit.score} / 100`}</strong>
        <span>{toFix === 0 ? 'Nothing needs attention' : `${toFix} need${toFix === 1 ? 's' : ''} attention`}</span>
        <span class="muted">{`${audit.checked} of ${audit.total} checked · open the audit →`}</span>
      </button>
    {/if}
    <div class="card">
      <span class="muted">Proxy</span>
      {#if health.proxy === ''}
        <strong class="warn">None</strong>
        <span>Relays see this machine's IP address.</span>
      {:else}
        <strong class={connected > 0 ? 'ok' : 'bad'}>{connected > 0 ? 'Online' : 'Not connected'}</strong>
        <span class="mono">{health.proxy}</span>
        <span class="muted">{`${connected} of ${health.relays.length} relays through it`}</span>
        <ProxyStatus check={health.proxy_check} />
      {/if}
    </div>
  </section>
  <section class="stats" aria-label="Health">
    <div class="card">
      <span class="muted">Service</span>
      <strong class={health.service.paused ? 'bad' : 'ok'}>{health.service.paused ? 'Invoicing stopped' : 'Running'}</strong>
      <span>{`up ${formatDuration(health.service.uptime_secs)}, v${health.service.version}`}</span>
      <span class="muted">{`${health.service.restarts} restart${health.service.restarts === 1 ? '' : 's'}${health.service.restart_pending ? ', restarting…' : ''}`}</span>
    </div>
    <div class="card">
      <span class="muted">Relays</span>
      <strong class={connected > 0 ? 'ok' : 'bad'}>{`${connected} of ${health.relays.length} connected`}</strong>
      <span>{health.proxy === '' ? 'direct connection' : 'via proxy'}</span>
    </div>
    <div class="card">
      <span class="muted">Chain</span>
      <strong class={health.chain.ok ? 'ok' : 'bad'}>{health.chain.ok ? 'Connected' : 'Unreachable'}</strong>
      <span>{health.chain.tip_height === null ? 'block height not known yet' : `block ${health.chain.tip_height}`}</span>
      <span class="muted">{TRANSPORT[health.chain.transport]}</span>
      <span class="muted">
        {health.chain.last_poll_at === null ? 'not polled yet' : `polled ${formatTime(health.chain.last_poll_at)}`}
      </span>
    </div>
    <div class="card">
      <span class="muted">Database</span>
      <strong class={health.database.ok ? 'ok' : 'bad'}>{health.database.ok ? 'Healthy' : 'Integrity check failed'}</strong>
      <span>{`${formatBytes(health.database.size_bytes)}, ${health.database.invoices} invoices`}</span>
      <span class="muted">
        {health.database.last_backup_at === null ? 'never backed up' : `backed up ${formatTime(health.database.last_backup_at)}`}
      </span>
    </div>
  </section>

  <section class="card">
    <h2>Relays</h2>
    <ul class="plain">
      {#each health.relays as r (r.url)}
        <li><span class={r.connected ? 'ok' : 'bad'}>●</span> <code>{r.url}</code> <span class="muted">{r.status}</span></li>
      {/each}
    </ul>
    {#if health.chain.watch_errors > 0}
      <p class="error">{`${health.chain.watch_errors} invoice${health.chain.watch_errors === 1 ? '' : 's'} could not be read from the chain.`}</p>
    {/if}
    {#if !health.chain.backend_local}
      <p class="note">The Electrum server is not on this machine.</p>
    {/if}
  </section>

  <section class="stats" aria-label="Totals">
    <div class="card"><span class="muted">Addresses created</span><strong>{metrics.addresses_created}</strong></div>
    <div class="card"><span class="muted">Invoices</span><strong>{metrics.invoices}</strong></div>
    <div class="card"><span class="muted">Payments</span><strong>{metrics.payments}</strong></div>
    <div class="card"><span class="muted">Received</span><strong>{formatBtc(metrics.received_sat)}</strong></div>
  </section>

  <section class="card">
    <h2>Payments</h2>
    <p class="muted">{`Online since ${formatTime(metrics.online_since)} (${formatDuration(metrics.now - metrics.online_since)}).`}</p>
    <table class="wrap">
      <thead><tr><th>Period</th><th>Payments</th><th>Amount</th></tr></thead>
      <tbody>
        {#each metrics.windows as w (w.label)}
          <tr>
            <td>{WINDOW_LABELS[w.label]}</td>
            {#if w.available}
              <td>{w.count}</td><td>{formatSats(w.sat)}</td>
            {:else}
              <!-- Not online for the whole period yet: the figure covers only the time it has been. -->
              <td class="muted">{`${w.count} so far`}</td>
              <td class="muted">{`${formatSats(w.sat)}; complete after ${formatDuration(w.secs)} online`}</td>
            {/if}
          </tr>
        {/each}
      </tbody>
    </table>

    <div class="row spread">
      <div class="row" role="group" aria-label="Chart range">
        {#each RANGES as r (r.value)}
          <button class={range === r.value ? 'primary' : ''} aria-pressed={range === r.value} onclick={() => (range = r.value)}>{r.label}</button>
        {/each}
      </div>
      <div class="row">
        <label for="metric">Show</label>
        <select id="metric" bind:value={metric}>
          <option value="sat">Amount</option>
          <option value="count">Number of payments</option>
        </select>
      </div>
    </div>
    <Chart {series} {metric} />
  </section>

  <section class="card">
    <h2>PayJoin / CoinJoin sweep</h2>
    {#if metrics.sweep.enabled}
      <p>
        {`${metrics.sweep.mode === 'payjoin' ? 'PayJoin' : 'CoinJoin'} above ${formatBtc(metrics.sweep.threshold_sat)}. Received so far: ${formatBtc(metrics.sweep.received_sat)}.`}
      </p>
      <p class={metrics.sweep.due ? 'note' : 'muted'}>{metrics.sweep.due ? 'The threshold is reached: a sweep is due.' : 'Below the threshold.'}</p>
    {:else}
      <p class="muted">Off.</p>
    {/if}
    <p class="muted">This service holds no spending keys. The policy is for a separate signer to carry out; nothing is sent from here.</p>
  </section>
  <section class="card" aria-label="Relay connections">
    <h2>Relay connections</h2>
    <ul class="plain">
      {#each health.relays as r (r.url)}
        <li class="row spread"><span class="mono">{r.url}</span><span class={`badge ${r.connected ? 'confirmed' : 'expired'}`}>{r.status}</span></li>
      {/each}
    </ul>
    <p class="muted">If no relay is connected for 10 minutes, the service goes back to the relays from DEADHOP_RELAYS by itself.</p>
  </section>
  {#if recent.length > 0}
    <section class="card" aria-label="Recent drops">
      <h2>Recent drops</h2>
      <ul class="plain">
        {#each recent as inv (inv.id)}
          <li class="row spread">
            <span class="mono">{inv.index.toString(16).toUpperCase().padStart(4, '0')}</span>
            <span class={`badge ${inv.status}`}>{statusLabel(inv.status)}</span>
            <span>{formatSats(inv.amount_sat)}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/if}
