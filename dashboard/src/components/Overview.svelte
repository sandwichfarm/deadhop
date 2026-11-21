<script lang="ts">
  import type { ControlAction, PayApi } from '../lib/api';
  import type { AdminInvoice, Health, Metrics, PrivacyAudit, Range, RelayHealth, Series } from '../lib/schemas';
  import { statusLabel } from '../lib/format';
  import { dropId, formatBtc, formatBytes, formatDuration, formatSats, formatTime } from '../lib/format';
  import Chart from './Chart.svelte';
  import ServiceKey from './ServiceKey.svelte';
  import Warnings from './Warnings.svelte';

  let {
    api,
    serverPubkey,
    refreshMs = 15000,
    onAudit,
    onDrops,
    confirm = (message: string) => window.confirm(message),
  }: {
    api: PayApi;
    serverPubkey: string;
    refreshMs?: number;
    onAudit: () => void;
    onDrops: () => void;
    confirm?: (message: string) => boolean;
  } = $props();

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
  let notice = $state('');
  let busy = $state(false);

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
  const MESSAGES: Record<ControlAction, string> = {
    stop: 'Invoicing stopped. Payments to existing invoices are still watched.',
    start: 'Invoicing started.',
    restart: 'Reconnecting to the relays. This page may need a few seconds to get answers again.',
  };
  const DOT: Record<RelayHealth['status'], string> = { ok: 'ok', degraded: 'warn', down: 'bad' };
  let checking = $state(false);

  /** The last end-to-end check of a relay, if one ran. */
  function relayCheck(h: Health, url: string): RelayHealth | null {
    const bare = (u: string) => u.replace(/\/$/, '');
    return h.relay_health.find((c) => bare(c.relay) === bare(url)) ?? null;
  }

  function relayNote(c: RelayHealth | null): string {
    if (c === null) return 'not checked yet';
    return c.status === 'ok' ? `carries ContextVM events · ${c.ms} ms` : c.detail;
  }

  async function checkRelays() {
    checking = true;
    try {
      await api.checkRelays();
      await load(range);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      checking = false;
    }
  }

  /** Length of the score ring (radius 34). */
  const RING = 2 * Math.PI * 34;

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

  async function control(action: ControlAction) {
    if (action === 'stop' && !confirm('Stop accepting new invoices? Existing invoices keep being watched.')) return;
    busy = true;
    notice = '';
    try {
      await api.control(action);
      notice = MESSAGES[action];
      await load(range);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  function tone(score: number): string {
    return score >= 90 ? 'ok' : score >= 60 ? 'warn' : 'bad';
  }

  function fixes(a: PrivacyAudit): string {
    const n = a.checks.filter((c) => c.result === 'fail' || c.result === 'to_fix').length;
    return n === 0 ? 'Nothing to fix' : `${n} thing${n === 1 ? '' : 's'} to fix`;
  }

  function heading(d: typeof data): string {
    if (d === null) return 'Connecting…';
    return d.health.service.paused ? 'Invoicing stopped.' : 'Serving. Nothing to reach.';
  }

  /** What the proxy tile says, from the live relay states and the last check. */
  function proxyTile(h: Health) {
    const connected = h.relays.filter((r) => r.connected).length;
    if (h.proxy === '') {
      return {
        stamp: 'none',
        stampClass: 'badge to_fix',
        title: 'None',
        tone: 'warn',
        line: 'direct',
        text: 'Relays connect straight to this machine and can see its IP address.',
      };
    }
    const check = h.proxy_check;
    const line = check === null ? h.proxy : `${h.proxy} · last check ${check.status}`;
    const checked = check === null ? 'Not checked yet.' : `Checked ${formatTime(check.checked_at)}.`;
    if (connected === 0) {
      return { stamp: 'offline', stampClass: 'badge fail', title: 'Not connected', tone: 'bad', line, text: `No relay is connected through it. ${checked}` };
    }
    const timed = check?.relays.find((r) => r.ok && r.ms !== null);
    return {
      stamp: 'online',
      stampClass: 'badge confirmed',
      title: timed ? `Reachable · ${timed.ms} ms` : 'Reachable',
      tone: 'ok',
      line,
      text: `${connected} of ${h.relays.length} relays through it: they see the proxy, not this machine. ${checked}`,
    };
  }
</script>

<header class="page-head">
  <div>
    <p class="eyebrow">Overview</p>
    <h1>{heading(data)}</h1>
  </div>
  {#if data}
    <button onclick={() => control('restart')} disabled={busy}>Restart relays</button>
    {#if data.health.service.paused}
      <button class="primary" onclick={() => control('start')} disabled={busy}>Start invoicing</button>
    {:else}
      <button onclick={() => control('stop')} disabled={busy}>Stop invoicing</button>
    {/if}
  {/if}
</header>

{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if notice}<p class="note" role="status">{notice}</p>{/if}

{#if data}
  {@const { health, metrics, series } = data}
  {@const connected = health.relays.filter((r) => r.connected).length}
  {@const proxy = proxyTile(health)}
  <Warnings warnings={health.warnings} />

  <section class="tiles wide" aria-label="Privacy and proxy">
    {#if audit}
      <button class="tile score-tile" onclick={onAudit} aria-label={`Privacy score ${audit.score} out of 100. Open the privacy audit`}>
        <svg class={`ring small ${tone(audit.score)}`} viewBox="0 0 84 84" aria-hidden="true">
          <circle class="track" cx="42" cy="42" r="34" />
          <circle class="value" cx="42" cy="42" r="34" stroke-dasharray={`${(audit.score / 100) * RING} ${RING}`} transform="rotate(-90 42 42)" />
          <text x="42" y="50" text-anchor="middle">{audit.score}</text>
        </svg>
        <span class="tile-body">
          <span class="lbl">Privacy score</span>
          <span class={`big ${tone(audit.score)}`}>{fixes(audit)}</span>
          <span class="note-line">{`${audit.checked} of ${audit.total} checks could run here.`}</span>
          <span class="go">Open the audit →</span>
        </span>
      </button>
    {/if}
    <div class="tile">
      <div class="tile-top">
        <span class="lbl">Proxy</span>
        <span class={proxy.stampClass}>{proxy.stamp}</span>
      </div>
      <span class={`big ${proxy.tone}`}>{proxy.title}</span>
      <span class="mono soft">{proxy.line}</span>
      <span class="note-line">{proxy.text}</span>
    </div>
  </section>

  <section class="tiles" aria-label="Health">
    <div class="tile">
      <span class="lbl">Service</span>
      <span class={`big ${health.service.paused ? 'bad' : 'ok'}`}>{health.service.paused ? 'Invoicing stopped' : 'Running'}</span>
      <span class="note-line"
        >{`up ${formatDuration(health.service.uptime_secs)} · v${health.service.version} · ${health.service.restarts} restart${health.service.restarts === 1 ? '' : 's'}${health.service.restart_pending ? ' · restarting…' : ''}`}</span
      >
    </div>
    <div class="tile">
      <span class="lbl">Relays</span>
      <span class={`big ${connected > 0 ? 'ok' : 'bad'}`}>{`${connected} of ${health.relays.length} connected`}</span>
      <span class="note-line">{health.proxy === '' ? 'direct connection' : 'through the proxy'}</span>
    </div>
    <div class="tile">
      <span class="lbl">Chain</span>
      <span class={`big ${health.chain.ok ? 'ok' : 'bad'}`}
        >{health.chain.ok ? (health.chain.tip_height === null ? 'Connected' : `Block ${health.chain.tip_height.toLocaleString('en-US')}`) : 'Unreachable'}</span
      >
      <span class="note-line"
        >{`${TRANSPORT[health.chain.transport]} · ${health.chain.last_poll_at === null ? 'not polled yet' : `polled ${formatTime(health.chain.last_poll_at)}`}`}</span
      >
    </div>
    <div class="tile">
      <span class="lbl">Database</span>
      <span class={`big ${health.database.ok ? 'ok' : 'bad'}`}>{health.database.ok ? 'Healthy' : 'Integrity check failed'}</span>
      <span class="note-line"
        >{`${formatBytes(health.database.size_bytes)} · ${health.database.last_backup_at === null ? 'never backed up' : `backed up ${formatTime(health.database.last_backup_at)}`}`}</span
      >
    </div>
  </section>

  <section class="card" aria-labelledby="o-relays">
    <div class="card-head">
      <h2 id="o-relays">Relays</h2>
      <span class="hint inline">One working relay is enough. Each is checked end to end every 5 minutes.</span>
      <button class="small" onclick={checkRelays} disabled={checking}>{checking ? 'Checking…' : 'Check now'}</button>
    </div>
    <ul class="plain relay-list">
      {#each health.relays as r (r.url)}
        {@const c = relayCheck(health, r.url)}
        <li>
          <span class={`dot ${c === null ? (r.connected ? 'ok' : 'bad') : DOT[c.status]}`} aria-hidden="true"></span>
          <span class="mono url">{r.url}</span>
          <span class={`relay-note ${c === null ? '' : c.status}`}>{relayNote(c)}</span>
          <span class={`badge ${r.connected ? 'confirmed' : 'expired'}`}>{r.status}</span>
        </li>
      {/each}
    </ul>
    {#if health.chain.watch_errors > 0}
      <p class="error">{`${health.chain.watch_errors} invoice${health.chain.watch_errors === 1 ? '' : 's'} could not be read from the chain.`}</p>
    {/if}
    {#if !health.chain.backend_local}
      <p class="hint">The Electrum server is not on this machine.</p>
    {/if}
  </section>

  <section class="totals" aria-label="Totals">
    <div>
      <span class="lbl">Received</span>
      <strong>{formatBtc(metrics.received_sat)}</strong>
      <span class="note-line">{formatSats(metrics.received_sat)}</span>
    </div>
    <div>
      <span class="lbl">Payments</span>
      <strong>{metrics.payments}</strong>
      <span class="note-line">{`${metrics.windows[0].count} in the last 24 hours`}</span>
    </div>
    <div>
      <span class="lbl">Invoices</span>
      <strong>{metrics.invoices}</strong>
      <span class="note-line">{`${health.database.invoices} in the database`}</span>
    </div>
    <div>
      <span class="lbl">Addresses created</span>
      <strong>{metrics.addresses_created}</strong>
      <span class="note-line">never reused</span>
    </div>
  </section>

  <section class="card" aria-labelledby="o-payments">
    <div class="card-head">
      <h2 id="o-payments">Payments</h2>
      <div class="segmented" role="group" aria-label="Chart range">
        {#each RANGES as r (r.value)}
          <button class={range === r.value ? 'primary' : ''} aria-pressed={range === r.value} onclick={() => (range = r.value)}>{r.label}</button>
        {/each}
      </div>
      <label class="inline-select" for="metric">Show</label>
      <select id="metric" bind:value={metric}>
        <option value="sat">Amount</option>
        <option value="count">Number of payments</option>
      </select>
    </div>
    <Chart {series} {metric} />
    <details class="periods">
      <summary>Totals by period</summary>
      <p class="hint">{`Online since ${formatTime(metrics.online_since)} (${formatDuration(metrics.now - metrics.online_since)}).`}</p>
      <div class="scroll">
        <table>
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
      </div>
    </details>
  </section>

  {#if recent.length > 0}
    <section class="card flush" aria-labelledby="o-drops">
      <div class="card-head">
        <h2 id="o-drops">Recent drops</h2>
        <button class="link" onclick={onDrops}>All invoices</button>
      </div>
      <div class="scroll">
        <table class="data">
          <thead><tr><th>Drop</th><th>Status</th><th class="num">Amount</th><th>Address</th><th>Created</th></tr></thead>
          <tbody>
            {#each recent as inv (inv.id)}
              <tr>
                <td class="mono">{dropId(inv.index)}</td>
                <td><span class={`badge ${inv.status}`}>{statusLabel(inv.status)}</span></td>
                <td class="num">{formatSats(inv.amount_sat)}</td>
                <td class="mono muted address-cell">{inv.address}</td>
                <td class="muted">{formatTime(inv.created_at)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}

  <section class="card" aria-labelledby="o-sweep">
    <div class="card-head">
      <h2 id="o-sweep">PayJoin / CoinJoin sweep</h2>
      <span class={metrics.sweep.enabled ? 'badge confirmed' : 'badge expired'}>{metrics.sweep.enabled ? 'on' : 'off'}</span>
    </div>
    {#if metrics.sweep.enabled}
      <p>
        {`${metrics.sweep.mode === 'payjoin' ? 'PayJoin' : 'CoinJoin'} above ${formatBtc(metrics.sweep.threshold_sat)}. Received so far: ${formatBtc(metrics.sweep.received_sat)}.`}
      </p>
      <p class={metrics.sweep.due ? 'warning' : 'muted'}>{metrics.sweep.due ? 'The threshold is reached: a sweep is due.' : 'Below the threshold.'}</p>
    {/if}
    <p class="hint">This service holds no spending keys. The policy is for a separate signer to carry out; nothing is sent from here.</p>
  </section>

  <ServiceKey pubkey={serverPubkey} relays={health.relays.map((r) => r.url)} />
{/if}
