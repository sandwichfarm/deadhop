<script lang="ts">
  import type { PayApi, SettingsPatch } from '../lib/api';
  import { buildProxy, parseProxy, type ProxyFields } from '../lib/proxy';
  import type { ProxyCheck, Settings, SettingsView } from '../lib/schemas';
  import ProxyStatus from './ProxyStatus.svelte';

  let {
    api,
    dashboardRelays,
    onChanged,
    confirm = (message: string) => window.confirm(message),
  }: {
    api: PayApi;
    /** Relays this page itself uses (from config.json). */
    dashboardRelays: string[];
    onChanged: () => void;
    confirm?: (message: string) => boolean;
  } = $props();

  /** Wait this long after the last change to the proxy before checking it. */
  const CHECK_DELAY_MS = 800;

  let proxyCheck = $state<ProxyCheck | null>(null);
  let checking = $state(false);
  let checkError = $state('');

  let view = $state<SettingsView | null>(null);
  let form = $state<Settings | null>(null);
  let proxy = $state<ProxyFields>(parseProxy(''));
  let newRelay = $state('');
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);

  function show(v: SettingsView) {
    view = v;
    form = structuredClone($state.snapshot(v.settings)) as Settings;
    proxy = parseProxy(v.settings.proxy);
  }

  async function run(action: () => Promise<SettingsView>, done: string) {
    busy = true;
    error = '';
    notice = '';
    try {
      show(await action());
      notice = done;
      onChanged();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    api.getSettings().then(show, (e: Error) => (error = e.message));
  });

  /**
   * What to check: `undefined` for the live proxy (the fields are unchanged,
   * so the stored password is used), a URL for another one, or `null` while
   * the address is incomplete.
   */
  function proxyTarget(): string | undefined | null {
    const live = parseProxy(view!.settings.proxy);
    if ((['kind', 'host', 'port', 'username', 'password'] as const).every((k) => proxy[k] === live[k])) return undefined;
    const built = buildProxy(proxy);
    return 'url' in built ? built.url : null;
  }

  async function testProxy(target: string | undefined) {
    checking = true;
    checkError = '';
    try {
      proxyCheck = await api.checkProxy(target);
    } catch (e) {
      checkError = (e as Error).message;
    } finally {
      checking = false;
    }
  }

  // Check the proxy shortly after it stops changing (and once on open).
  $effect(() => {
    if (!view) return;
    const target = proxyTarget();
    if (target === null) return;
    const timer = setTimeout(() => void testProxy(target), CHECK_DELAY_MS);
    return () => clearTimeout(timer);
  });

  const isOverridden = (name: string) => view!.overridden.includes(name);

  /** Only the fields that differ from the live settings. */
  function diff(): SettingsPatch | string {
    const live = view!.settings;
    const f = form!;
    const patch: Record<string, unknown> = {};
    for (const key of Object.keys(f) as (keyof Settings)[]) {
      if (key === 'sweep' || key === 'relays' || key === 'proxy') continue;
      if (f[key] === null) return 'Fill in every number.';
      if (f[key] !== live[key]) patch[key] = f[key];
    }
    if (JSON.stringify(f.relays) !== JSON.stringify(live.relays)) patch.relays = [...f.relays];
    const sweep: Record<string, unknown> = {};
    for (const key of Object.keys(f.sweep) as (keyof Settings['sweep'])[]) {
      if (f.sweep[key] === null) return 'Fill in every number.';
      if (f.sweep[key] !== live.sweep[key]) sweep[key] = f.sweep[key];
    }
    if (Object.keys(sweep).length > 0) patch.sweep = sweep;
    const before = parseProxy(live.proxy);
    const changed = (['kind', 'host', 'port', 'username', 'password'] as const).some((k) => proxy[k] !== before[k]);
    if (changed) {
      const built = buildProxy(proxy);
      if ('error' in built) return built.error;
      patch.proxy = built.url;
    }
    return patch as SettingsPatch;
  }

  async function save() {
    const patch = diff();
    if (typeof patch === 'string') {
      error = patch;
      return;
    }
    if (Object.keys(patch).length === 0) {
      notice = 'Nothing to save.';
      return;
    }
    if ((patch.relays || patch.proxy !== undefined) && !confirm('The service will reconnect to its relays. If none connect, the change is undone automatically. Continue?')) {
      return;
    }
    await run(() => api.updateSettings(patch), 'Saved.');
  }

  async function reset() {
    if (!confirm('Forget every setting changed here and use the values from the environment?')) return;
    await run(() => api.resetSettings(), 'Back to the environment values.');
  }

  function addRelay() {
    const url = newRelay.trim();
    if (!/^wss?:\/\/[^\s]+$/.test(url)) {
      error = 'A relay address starts with wss:// (or ws:// for a relay on this machine).';
      return;
    }
    if (form!.relays.includes(url)) {
      error = 'That relay is already listed.';
      return;
    }
    error = '';
    form!.relays = [...form!.relays, url];
    newRelay = '';
  }

  function removeRelay(url: string) {
    form!.relays = form!.relays.filter((r) => r !== url);
  }


  const unreachable = $derived(form !== null && !dashboardRelays.some((r) => form!.relays.includes(r)));
</script>

{#snippet mark(name: string)}{#if isOverridden(name)}<span>{' *'}</span>{/if}{/snippet}

{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if notice}<p class="note" role="status">{notice}</p>{/if}

{#if view && form}
  {#each view.warnings as w (w)}
    <p class="note">{w}</p>
  {/each}
  {#if view.restart_pending}<p class="note" role="status">Reconnecting to the relays…</p>{/if}

  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <section class="card">
      <h2>Relays</h2>
      <p class="muted">The service listens on these relays. Changes reconnect it; if none of the new relays connect, it goes back to the previous list.</p>
      <ul class="plain">
        {#each form.relays as r (r)}
          <li class="row spread">
            <code>{r}</code>
            <button type="button" onclick={() => removeRelay(r)} disabled={form.relays.length === 1} aria-label={`Remove ${r}`}>Remove</button>
          </li>
        {/each}
      </ul>
      <div class="row">
        <label for="relay">Add relay</label>
        <input id="relay" placeholder="wss://relay.example.com" bind:value={newRelay} />
        <button type="button" onclick={addRelay} disabled={form.relays.length >= 8}>Add</button>
      </div>
      {#if unreachable}
        <p class="error" role="alert">
          {`This page talks to the service through ${dashboardRelays.join(', ')}. Keep one of them, or update config.json before saving.`}
        </p>
      {/if}
    </section>

    <section class="card">
      <h2>Proxy for relay connections</h2>
      {#if view.fixed.proxy_locked}
        <p class="muted">{`Locked on the host (DEADHOP_PROXY_LOCKED). Current: ${view.settings.proxy === '' ? 'none' : view.settings.proxy}`}</p>
      {:else}
        <p class="muted">Only connections to Nostr relays use the proxy. The Electrum server is always reached directly.</p>
        <div class="grid">
          <label for="proxy-kind">Type</label>
          <select id="proxy-kind" bind:value={proxy.kind}>
            <option value="none">None (direct)</option>
            <option value="socks5">SOCKS5 (Tor, VPN)</option>
            <option value="https">HTTPS proxy</option>
            <option value="http">HTTP proxy</option>
          </select>
          {#if proxy.kind !== 'none'}
            <label for="proxy-host">Host</label>
            <input id="proxy-host" bind:value={proxy.host} placeholder="127.0.0.1" />
            <label for="proxy-port">Port</label>
            <input id="proxy-port" bind:value={proxy.port} inputmode="numeric" placeholder="9050" />
            <label for="proxy-user">User name</label>
            <input id="proxy-user" bind:value={proxy.username} autocomplete="off" />
            <label for="proxy-pass">Password</label>
            <input
              id="proxy-pass"
              type="password"
              bind:value={proxy.password}
              autocomplete="new-password"
              placeholder={proxy.hasPassword ? 'unchanged' : ''}
            />
          {/if}
        </div>
      {/if}
      <div class="row">
        <button onclick={() => void testProxy(view!.fixed.proxy_locked ? undefined : (proxyTarget() ?? undefined))} disabled={checking}>Test</button>
        <span class="muted">Checked as you type, through the proxy to each relay.</span>
      </div>
      <ProxyStatus check={proxyCheck} {checking} error={checkError} />
    </section>

    <section class="card">
      <h2>Invoicing and proof of work</h2>
      <div class="grid">
        <label for="name">Name{@render mark('name')}</label>
        <input id="name" bind:value={form.name} maxlength="60" />
        <label for="public">Public invoicing{@render mark('public_invoicing')}</label>
        <input id="public" type="checkbox" bind:checked={form.public_invoicing} />
        <label for="pow">Proof of work (bits){@render mark('pow_bits')}</label>
        <input id="pow" type="number" min="0" max="64" bind:value={form.pow_bits} />
        <label for="window">Request age limit (s){@render mark('pow_window_secs')}</label>
        <input id="window" type="number" min="30" max="3600" bind:value={form.pow_window_secs} />
        <label for="rate-global">Public invoices / min{@render mark('rate_global_per_min')}</label>
        <input id="rate-global" type="number" min="1" bind:value={form.rate_global_per_min} />
        <label for="rate-key">Public invoices / min / key{@render mark('rate_per_key_per_min')}</label>
        <input id="rate-key" type="number" min="1" bind:value={form.rate_per_key_per_min} />
        <label for="req-global">Public requests / min{@render mark('rate_requests_per_min')}</label>
        <input id="req-global" type="number" min="1" bind:value={form.rate_requests_per_min} />
        <label for="req-key">Public requests / min / key{@render mark('rate_requests_per_key_per_min')}</label>
        <input id="req-key" type="number" min="1" bind:value={form.rate_requests_per_key_per_min} />
      </div>
      <p class="muted">Each extra bit of proof of work doubles the cost for customers and attackers alike. 20 bits is about a second in a browser.</p>
    </section>

    <section class="card">
      <h2>Limits</h2>
      <div class="grid">
        <label for="confs">Confirmations{@render mark('min_confirmations')}</label>
        <input id="confs" type="number" min="1" max="144" bind:value={form.min_confirmations} />
        <label for="min-amount">Smallest invoice (sats){@render mark('min_amount_sat')}</label>
        <input id="min-amount" type="number" min="1" bind:value={form.min_amount_sat} />
        <label for="max-amount">Largest invoice (sats){@render mark('max_amount_sat')}</label>
        <input id="max-amount" type="number" min="1" bind:value={form.max_amount_sat} />
        <label for="default-expiry">Default lifetime (s){@render mark('default_expiry_secs')}</label>
        <input id="default-expiry" type="number" min="60" bind:value={form.default_expiry_secs} />
        <label for="max-expiry">Longest lifetime (s){@render mark('max_expiry_secs')}</label>
        <input id="max-expiry" type="number" min="60" bind:value={form.max_expiry_secs} />
        <label for="open">Open invoices cap{@render mark('max_open_invoices')}</label>
        <input id="open" type="number" min="1" bind:value={form.max_open_invoices} />
        <label for="daily">Invoices per day cap{@render mark('max_invoices_per_day')}</label>
        <input id="daily" type="number" min="1" bind:value={form.max_invoices_per_day} />
        <label for="poll">Chain poll interval (s){@render mark('poll_secs')}</label>
        <input id="poll" type="number" min="1" max="3600" bind:value={form.poll_secs} />
        <label for="grace">Late payment grace (s){@render mark('late_grace_secs')}</label>
        <input id="grace" type="number" min="0" bind:value={form.late_grace_secs} />
      </div>
    </section>

    <section class="card">
      <h2>PayJoin / CoinJoin sweep</h2>
      <p class="muted">
        The policy for moving funds out once they pile up. This service holds no spending keys and never sends anything:
        a separate signer reads the policy and chooses the destination itself.
      </p>
      <div class="grid">
        <label for="sweep-on">Enabled</label>
        <input id="sweep-on" type="checkbox" bind:checked={form.sweep.enabled} />
        <label for="sweep-mode">Method</label>
        <select id="sweep-mode" bind:value={form.sweep.mode}>
          <option value="payjoin">PayJoin</option>
          <option value="coinjoin">CoinJoin</option>
        </select>
        <label for="sweep-threshold">Threshold (sats)</label>
        <input id="sweep-threshold" type="number" min="10000" bind:value={form.sweep.threshold_sat} />
        <label for="sweep-fee">Highest fee rate (sat/vB)</label>
        <input id="sweep-fee" type="number" min="1" max="1000" bind:value={form.sweep.max_fee_rate_sat_vb} />
        <label for="sweep-anon">Smallest CoinJoin anonymity set</label>
        <input id="sweep-anon" type="number" min="2" max="1000" bind:value={form.sweep.min_anonymity_set} />
      </div>
    </section>

    <div class="row">
      <button class="primary" type="submit" disabled={busy}>Save changes</button>
      <button type="button" onclick={reset} disabled={busy || view.overridden.length === 0}>Reset to environment</button>
      {#if view.overridden.length > 0}<span class="muted">* changed from the environment value</span>{/if}
    </div>
  </form>


  <section class="card">
    <h2>Fixed on the host</h2>
    <dl class="grid">
      <dt>Network</dt><dd>{view.fixed.network}</dd>
      <dt>Electrum server</dt><dd><code>{view.fixed.electrum}</code> <span class="muted">{view.fixed.electrum_transport}</span></dd>
      <dt>First receive address</dt><dd><code>{view.fixed.first_address}</code></dd>
      <dt>Discovery announcements</dt><dd>{view.fixed.announce ? 'on' : 'off'}</dd>
    </dl>
    <p class="muted">The wallet key, network, Electrum server and data directory decide where money goes, so they can only be changed on the host.</p>
  </section>
{/if}
