<script lang="ts">
  import { nip19 } from 'nostr-tools';

  // The service's public key in full, with copy buttons: apps need it
  // (`serverPubkey`) to reach the service, and customers compare it with the
  // key the merchant published. `compact` is the one-line version for the footer.
  let {
    pubkey,
    relays = [],
    compact = false,
    copy = (text: string) => navigator.clipboard.writeText(text),
  }: {
    pubkey: string;
    relays?: string[];
    compact?: boolean;
    copy?: (text: string) => Promise<void>;
  } = $props();

  const npub = $derived(nip19.npubEncode(pubkey));
  let copied = $state<'npub' | 'hex' | ''>('');
  let failed = $state(false);

  async function copyKey(which: 'npub' | 'hex') {
    try {
      await copy(which === 'npub' ? npub : pubkey);
      copied = which;
      failed = false;
    } catch {
      copied = '';
      failed = true;
    }
  }
</script>

{#if compact}
  <span>Service key:</span>
  <code class="key">{npub}</code>
  <button class="link" onclick={() => copyKey('npub')}>{copied === 'npub' ? 'Copied' : 'Copy'}</button>
  {#if failed}<span class="error" role="alert">Could not copy: select the key and copy it by hand.</span>{/if}
{:else}
  <section class="card service-key" aria-labelledby="service-key-title">
    <h2 id="service-key-title">Service key</h2>
    <p class="muted">
      Apps and pay buttons reach this service with this key (<code>serverPubkey</code>) and one of its relays.
      Publish it where your customers can compare it with the key their page shows.
    </p>
    <code class="key">{npub}</code>
    <div class="actions">
      <button class="primary" onclick={() => copyKey('npub')}>{copied === 'npub' ? 'Copied' : 'Copy npub'}</button>
      <button onclick={() => copyKey('hex')}>{copied === 'hex' ? 'Copied' : 'Copy hex'}</button>
    </div>
    {#if failed}<p class="error" role="alert">Could not copy: select the key and copy it by hand.</p>{/if}
    {#if relays.length > 0}
      <p class="muted">{`Relays this page uses: ${relays.join(', ')}`}</p>
    {/if}
  </section>
{/if}
