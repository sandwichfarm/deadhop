<script lang="ts">
  import type { ControlAction, PayApi } from '../lib/api';
  import type { Health } from '../lib/schemas';
  import { formatBytes, formatTime } from '../lib/format';

  let {
    api,
    confirm = (message: string) => window.confirm(message),
  }: { api: PayApi; confirm?: (message: string) => boolean } = $props();

  let health = $state<Health | null>(null);
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

{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if notice}<p class="note" role="status">{notice}</p>{/if}

<section class="card">
  <h2>Service</h2>
  {#if health}
    <p>
      <strong class={health.service.paused ? 'bad' : 'ok'}>{health.service.paused ? 'Invoicing is stopped.' : 'Invoicing is running.'}</strong>
      {health.service.restart_pending ? ' Restarting…' : ''}
    </p>
  {/if}
  <div class="row">
    {#if health?.service.paused}
      <button class="primary" onclick={() => control('start')} disabled={busy}>Start invoicing</button>
    {:else}
      <button onclick={() => control('stop')} disabled={busy}>Stop invoicing</button>
    {/if}
    <button onclick={() => control('restart')} disabled={busy}>Restart relay connections</button>
  </div>
  <p class="muted">
    Stopping refuses new invoices but keeps watching payments, so nothing that arrives is missed. The process itself can only be stopped
    on the host: without it nobody could start it again from here.
  </p>
</section>

<section class="card">
  <h2>Backup</h2>
  {#if health}
    <p class="muted">
      {health.database.last_backup_at === null
        ? 'No backup yet.'
        : `Last backup ${formatTime(health.database.last_backup_at)}; ${health.database.backups} kept.`}
    </p>
  {/if}
  <button onclick={backup} disabled={busy}>Back up database</button>
  <p class="muted">Backups are written next to the database on the service host (the newest 10 are kept). There is no download: the service has no web server.</p>
</section>

<section class="card danger">
  <h2>Wipe database</h2>
  <p>Deletes every invoice. A backup is taken first. Address numbering continues, so no address is ever handed out twice.</p>
  <div class="row">
    <input id="wipe-settings" type="checkbox" bind:checked={wipeSettings} />
    <label for="wipe-settings">Also forget settings changed here</label>
  </div>
  <div class="row">
    <input id="wipe-force" type="checkbox" bind:checked={wipeForce} />
    <label for="wipe-force">Wipe even if customers may still pay open invoices</label>
  </div>
  <div class="row">
    <label for="wipe-confirm">Type WIPE to confirm</label>
    <input id="wipe-confirm" bind:value={wipeText} autocomplete="off" />
    <button class="bad" onclick={wipe} disabled={busy || wipeText !== 'WIPE'}>Wipe</button>
  </div>
</section>

