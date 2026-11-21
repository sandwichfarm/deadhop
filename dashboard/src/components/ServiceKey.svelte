<script lang="ts">
  import { nip19 } from 'nostr-tools';

  // The service's public key, with copy buttons: apps need it (`serverPubkey`)
  // to reach the service, and customers compare it with the key the merchant
  // published. The nprofile also carries the relays the service listens on, so
  // a client can find it from that one string. `compact` is the footer version.
  let {
    pubkey,
    relays = [],
    compact = false,
    label = 'Service key',
    purpose = '',
    profile = true,
    copy = (text: string) => navigator.clipboard.writeText(text),
  }: {
    pubkey: string;
    relays?: string[];
    compact?: boolean;
    /** What the row is called. */
    label?: string;
    /** One line on who uses this key. */
    purpose?: string;
    /** Offer an nprofile (key plus relays). Not for a key that must not be published. */
    profile?: boolean;
    copy?: (text: string) => Promise<void>;
  } = $props();

  type Form = 'nprofile' | 'npub' | 'hex';
  const npub = $derived(nip19.npubEncode(pubkey));
  const nprofile = $derived(nip19.nprofileEncode({ pubkey, relays }));
  let copied = $state<Form | ''>('');
  let failed = $state(false);

  async function copyKey(which: Form) {
    try {
      await copy(which === 'nprofile' ? nprofile : which === 'npub' ? npub : pubkey);
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
  <section class="key-strip" aria-label={label}>
    <span class="lbl">{label}</span>
    <code class="key" title={npub}>{npub}</code>
    {#if profile}<button class="primary" onclick={() => copyKey('nprofile')}>{copied === 'nprofile' ? 'Copied' : 'Copy nprofile'}</button>{/if}
    <button onclick={() => copyKey('npub')}>{copied === 'npub' ? 'Copied' : 'Copy npub'}</button>
    <button onclick={() => copyKey('hex')}>{copied === 'hex' ? 'Copied' : 'Copy hex'}</button>
    {#if purpose !== ''}<span class="purpose">{purpose}</span>{/if}
  </section>
  {#if failed}<p class="error" role="alert">Could not copy: select the key and copy it by hand.</p>{/if}
{/if}
