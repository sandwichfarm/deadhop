<script lang="ts">
  import type { NostrSigner } from '@contextvm/sdk';
  import { connect as defaultConnect, type Session } from '../lib/api';
  import { loadConfig as defaultLoadConfig } from '../lib/config';
  import { nip19 } from 'nostr-tools';
  import { shortKey } from '../lib/format';
  import { parseRoute, payHash, type Route } from '../lib/route';
  import type { Config, Invoice } from '../lib/schemas';
  import { localSigner, nip07Signer, type Nip07 } from '@deadhop/client';
  import Admin from './Admin.svelte';
  import Checkout from './Checkout.svelte';
  import CreateInvoice from './CreateInvoice.svelte';
  import Login, { type SignInKind } from './Login.svelte';
  import ServiceKey from './ServiceKey.svelte';

  let {
    loadConfig = defaultLoadConfig,
    connect = defaultConnect,
    extension = () => (window as unknown as { nostr?: Nip07 }).nostr,
  }: {
    loadConfig?: () => Promise<Config>;
    connect?: (signer: NostrSigner, config: Config) => Promise<Session>;
    extension?: () => Nip07 | undefined;
  } = $props();

  let config = $state<Config | null>(null);
  let fatal = $state('');
  let session = $state<Session | null>(null);
  let busy = $state(false);
  let error = $state('');
  let route = $state<Route>(parseRoute(location.hash));

  async function signIn(kind: SignInKind, secret?: string) {
    busy = true;
    error = '';
    try {
      const signer = kind === 'extension' ? nip07Signer(extension()) : localSigner(kind === 'secret' ? secret : undefined);
      session = await connect(signer, config!);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function signOut() {
    const old = session;
    session = null;
    await old?.close().catch(() => undefined);
  }

  // Limits and proof of work can change from the settings page.
  async function refreshInfo() {
    const current = session!;
    try {
      current.info = await current.api.getInfo();
    } catch {
      // Keep the previous values; the next action reports any problem.
    }
  }

  function created(invoice: Invoice) {
    location.hash = payHash(invoice.id);
  }

  $effect(() => {
    const onHash = () => (route = parseRoute(location.hash));
    window.addEventListener('hashchange', onHash);
    loadConfig().then(
      (c) => (config = c),
      (e: Error) => (fatal = e.message),
    );
    return () => window.removeEventListener('hashchange', onHash);
  });

  // A payment link must work for someone who has no Nostr identity at all.
  $effect(() => {
    if (route.name === 'pay' && config !== null && session === null && !busy && error === '') void signIn('guest');
  });
</script>

<header>
  <a class="brand" href="#/">{session?.info.name ?? 'deadhop'}</a>
  {#if session}
    <span class="badge net">{session.info.network}</span>
    <span class="muted" title={session.pubkey}>{shortKey(session.pubkey)}</span>
    <button class="link" onclick={signOut}>Sign out</button>
  {/if}
</header>

<main>
  {#if fatal}
    <p class="error" role="alert">{fatal}</p>
  {:else if config === null}
    <p role="status">Loading…</p>
  {:else if session === null}
    {#if route.name === 'pay' && error === ''}
      <p role="status">Connecting…</p>
    {:else}
      <Login onSignIn={signIn} {busy} {error} />
    {/if}
  {:else if route.name === 'pay'}
    <Checkout api={session.api} id={route.id} />
  {:else if session.info.is_admin}
    <Admin
      api={session.api}
      info={session.info}
      onCreated={created}
      dashboardRelays={config.relays}
      serverPubkey={config.serverPubkey}
      onSettingsChanged={refreshInfo}
    />
  {:else if session.info.public_invoicing}
    <CreateInvoice api={session.api} info={session.info} onCreated={created} />
    <p class="muted">
      {`Signed in as ${shortKey(nip19.npubEncode(session.pubkey))}, which is not an operator of this service. To manage it, sign in with an operator key, or have an operator add this key under Settings → Operators.`}
    </p>
  {:else}
    <p class="card">This key is not an operator of this service, and public invoicing is switched off.</p>
  {/if}
</main>

{#if config}
  <!-- Lets anyone compare the key this page talks to with the one the merchant published. -->
  <footer class="muted">
    <ServiceKey pubkey={config.serverPubkey} compact />
  </footer>
{/if}
