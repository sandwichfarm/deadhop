<script lang="ts">
  import type { ControlAction, PayApi } from '../lib/api';
  import type { Backup, Health } from '../lib/schemas';
  import { formatBytes, formatTime } from '../lib/format';

  let {
    api,
    confirm = (message: string) => window.confirm(message),
  }: { api: PayApi; confirm?: (message: string) => boolean } = $props();

  let health = $state<Health | null>(null);
  let backups = $state<Backup[] | null>(null);
  let backupsError = $state('');
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);
  let wipeText = $state('');
  let wipeSettings = $state(false);
  let wipeForce = $state(false);

  async function load() {
    try {
      health = await api.getHealth();
    } catch (e) {
      error = (e as Error).message;
    }
    try {
      backups = (await api.listBackups()).backups;
      backupsError = '';
    } catch (e) {
      backupsError = `Could not list the backups: ${(e as Error).message}`;
    }
  }

  $effect(() => {
    void load();
  });

  async function act(work: () => Promise<string>) {
    busy = true;
    error = '';
    notice = '';
    try {
      notice = await work();
      await load();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  const MESSAGES: Record<ControlAction, string> = {
    stop: 'Invoicing stopped. Payments to existing invoices are still watched.',
    start: 'Invoicing started.',
    restart: 'Reconnecting to the relays. This page may need a few seconds to get answers again.',
  };

  function control(action: ControlAction) {
    if (action === 'stop' && !confirm('Stop accepting new invoices? Existing invoices keep being watched.')) return;
    void act(async () => {
      await api.control(action);
      return MESSAGES[action];
    });
  }

  function backup() {
    void act(async () => {
      const b = await api.backup();
      return `Backup written on the service host: backups/${b.file} (${formatBytes(b.size_bytes)}).`;
    });
  }

  function wipe() {
    void act(async () => {
      const w = await api.wipe({ confirm: wipeText, include_settings: wipeSettings, force: wipeForce });
      wipeText = '';
      return `Deleted ${w.deleted} invoice${w.deleted === 1 ? '' : 's'}. A backup was taken first: backups/${w.backup.file}.`;
    });
  }
</script>

<header class="page-head">
  <div>
    <p class="eyebrow">Maintenance</p>
    <h1>Keep it running</h1>
  </div>
</header>

{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if notice}<p class="note" role="status">{notice}</p>{/if}

<div class="two-col">
  <section class="card" aria-labelledby="m-invoicing">
    <div class="card-head">
      <h2 id="m-invoicing">Invoicing</h2>
      {#if health}
        <span class={health.service.paused ? 'badge detected' : 'badge confirmed'}>{health.service.paused ? 'stopped' : 'accepting'}</span>
      {/if}
    </div>
    {#if health?.service.restart_pending}<p class="muted" role="status">Reconnecting to the relays…</p>{/if}
    <p class="hint">Stop refuses new invoices only. The chain is still watched, so a payment made while stopped is still credited.</p>
    {#if health?.service.paused}
      <button class="primary" onclick={() => control('start')} disabled={busy}>Start invoicing</button>
    {:else}
      <button onclick={() => control('stop')} disabled={busy}>Stop invoicing</button>
    {/if}
  </section>

  <section class="card" aria-labelledby="m-relays">
    <h2 id="m-relays">Relay connections</h2>
    <p class="hint">
      Reconnect to the relays without restarting the process. The process itself cannot be stopped from here: nothing could start it again.
    </p>
    <button onclick={() => control('restart')} disabled={busy}>Restart relay connections</button>
  </section>
</div>

<section class="card" aria-labelledby="m-backups">
  <div class="card-head">
    <h2 id="m-backups">Backups</h2>
    <button class="primary" onclick={backup} disabled={busy}>Back up now</button>
  </div>
  <p class="hint">
    Written on the service host, next to the database in its <span class="mono">backups</span> folder; the newest 10 are kept. There is no download: the
    service has no web server, so copy them off the host yourself.
  </p>
  {#if backupsError}<p class="error">{backupsError}</p>{/if}
  {#if backups?.length === 0}
    <p class="muted">No backup yet.</p>
  {:else if backups}
    <ul class="plain backup-list" aria-label="Backups on the host">
      {#each backups as b (b.file)}
        <li>
          <span class="mono">{`${b.file}`}</span>
          <span class="muted">{`${formatBytes(b.size_bytes)}`}</span>
          <span class="muted">{`${formatTime(b.created_at)}`}</span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<section class="card danger" aria-labelledby="m-wipe">
  <h2 id="m-wipe" class="bad">Wipe invoices</h2>
  <p class="hint soft">
    Deletes every invoice. A backup is always taken first, the audit log is kept, and the address counter is kept, so no address is ever handed out
    twice. Refused while customers may still pay open invoices, unless you force it.
  </p>
  <div class="wipe-row">
    <div class="field">
      <label for="wipe-confirm">Type WIPE to confirm</label>
      <input id="wipe-confirm" bind:value={wipeText} autocomplete="off" />
    </div>
    <label class="check"><input type="checkbox" bind:checked={wipeSettings} /> Also forget settings changed here</label>
    <label class="check"><input type="checkbox" bind:checked={wipeForce} /> Wipe even if customers may still pay open invoices</label>
    <button class="danger" onclick={wipe} disabled={busy || wipeText !== 'WIPE'}>Back up and wipe</button>
  </div>
</section>
