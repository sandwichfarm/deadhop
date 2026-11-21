<script lang="ts">
  import type { PayApi } from '../lib/api';
  import { formatTime } from '../lib/format';
  import { WINDOWS, errorRate, type HistoryWindow } from '../lib/relays';
  import type { Health, RelayHealth, RelayHistory } from '../lib/schemas';
  import RelayBars from './RelayBars.svelte';

  let { api, refreshMs = 30000, onBack }: { api: PayApi; refreshMs?: number; onBack: () => void } = $props();

  let window = $state<HistoryWindow>('1h');
  let history = $state<RelayHistory | null>(null);
  let health = $state<Health | null>(null);
  let error = $state('');
  let checking = $state(false);

  async function load(which: HistoryWindow) {
    try {
      [history, health] = await Promise.all([api.getRelayHistory(which), api.getHealth()]);
      error = '';
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // Runs on mount and whenever the window changes.
  $effect(() => {
    const which = window;
    void load(which);
    const timer = setInterval(() => void load(which), refreshMs);
    return () => clearInterval(timer);
  });

  async function checkNow() {
    checking = true;
    try {
      await api.checkRelays();
      await load(window);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      checking = false;
    }
  }

  const bare = (u: string) => u.replace(/\/$/, '');
  const STAMP: Record<RelayHealth['status'], [string, string]> = {
    ok: ['badge confirmed', 'ok'],
    degraded: ['badge to_fix', 'degraded'],
    down: ['badge fail', 'down'],
  };
  const DOT: Record<RelayHealth['status'], string> = { ok: 'ok', degraded: 'warn', down: 'bad' };

  // The history and the health are loaded together, so one is there when the other is.
  /** The last end-to-end check of a relay, if one ran. */
  const checkOf = (url: string) => health!.relay_health.find((c) => bare(c.relay) === bare(url)) ?? null;
  const live = (url: string) => health!.relays.some((r) => bare(r.url) === bare(url));
</script>

<button class="link back" onclick={onBack}>← Overview</button>

<header class="page-head">
  <div>
    <p class="eyebrow">Connectivity</p>
    <h1>Relays</h1>
  </div>
  <button onclick={checkNow} disabled={checking}>{checking ? 'Checking…' : 'Check now'}</button>
</header>

<div class="toolbar">
  <div class="segmented" role="group" aria-label="Window">
    {#each WINDOWS as w (w.value)}
      <button class={window === w.value ? 'primary' : ''} aria-pressed={window === w.value} onclick={() => (window = w.value)}>{w.label}</button>
    {/each}
  </div>
  {#if history}<span class="hint inline">{`${formatTime(history.since)} to ${formatTime(history.until)}`}</span>{/if}
</div>

{#if error}<p class="error" role="alert">{error}</p>{/if}

{#if history}
  {#each history.relays as s (s.relay)}
    {@const c = checkOf(s.relay)}
    {@const rate = errorRate(s)}
    <section class="card relay-card" aria-label={s.relay}>
      <div class="relay-head">
        <span class={`dot ${c === null ? (live(s.relay) ? 'ok' : 'bad') : DOT[c.status]}`} aria-hidden="true"></span>
        <span class="mono url">{s.relay}</span>
        {#if !live(s.relay)}<span class="badge expired">removed</span>{/if}
        {#if c}<span class={STAMP[c.status][0]}>{STAMP[c.status][1]}</span>{/if}
        <span class="rate">
          <span class={`big ${rate.tone}`}>{rate.text}</span>
          <span class="lbl">errors</span>
        </span>
      </div>
      <p class="relay-stats">
        {`${s.checks} health check${s.checks === 1 ? '' : 's'} (${s.check_failures} failed) · ${s.sends} repl${s.sends === 1 ? 'y' : 'ies'} (${s.send_failures} failed)${s.avg_ms === null ? '' : ` · average ${s.avg_ms} ms`}`}
      </p>
      {#if c && c.status !== 'ok'}<p class="relay-detail">{c.detail}</p>{/if}
      <RelayBars points={s.points} label={s.relay} tall />
      <div class="bar-axis"><span>{formatTime(s.points[0].start)}</span><span>now</span></div>
    </section>
  {/each}
{/if}
