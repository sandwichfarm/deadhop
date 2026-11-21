<script lang="ts">
  import type { ProxyCheck } from '../lib/schemas';

  /** Slower than this (ms) counts as slow. */
  const SLOW_MS = 2000;

  let {
    check,
    checking = false,
    error = '',
    showRelays = true,
  }: { check: ProxyCheck | null; checking?: boolean; error?: string; showRelays?: boolean } = $props();

  type Tone = 'ok' | 'warn' | 'bad' | 'pending';
  const view = $derived.by((): { tone: Tone; title: string; detail: string } | null => {
    if (checking) return { tone: 'pending', title: 'Checking…', detail: 'Connecting to the proxy and through it to your relays.' };
    if (error) return { tone: 'bad', title: 'Could not run the check', detail: error };
    if (!check) return null;
    const reached = check.relays.filter((r) => r.ok);
    switch (check.status) {
      case 'direct':
        return { tone: 'warn', title: 'No proxy', detail: 'Relays connect straight to this machine and can see its IP address.' };
      case 'connected': {
        const slow = reached.some((r) => (r.ms ?? 0) > SLOW_MS);
        return slow
          ? { tone: 'warn', title: 'Connected, but slow', detail: 'The proxy answered slowly. Payments still arrive; operator actions will feel slow.' }
          : { tone: 'ok', title: 'Connected', detail: `Reached all ${check.relays.length} relays through the proxy.` };
      }
      case 'partial':
        return { tone: 'warn', title: `${reached.length} of ${check.relays.length} relays reachable`, detail: 'Some relays cannot be reached through the proxy.' };
      default:
        return { tone: 'bad', title: 'Not working', detail: 'No relay could be reached through the proxy.' };
    }
  });
</script>

{#if view}
  <div class={`proxy-status ${view.tone}`} role="status" aria-live="polite">
    <p class="proxy-title">{view.title}</p>
    <p class="proxy-detail">{view.detail}</p>
    {#if showRelays && check && !checking && !error && check.relays.length > 0}
      <ul class="proxy-relays">
        {#each check.relays as r (r.relay)}
          <li class={r.ok ? 'ok' : 'bad'}>
            <span class="mono">{r.relay}</span>
            <span class="mono code">{r.ms === null ? r.code : `${r.code} · ${r.ms} ms`}</span>
            {#if !r.ok}<span>{r.reason}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}
