<script lang="ts">
  export type SignInKind = 'extension' | 'secret' | 'guest';

  let {
    onSignIn,
    busy = false,
    error = '',
  }: { onSignIn: (kind: SignInKind, secret?: string) => void; busy?: boolean; error?: string } = $props();

  let secret = $state('');

  function submitSecret(event: SubmitEvent) {
    event.preventDefault();
    onSignIn('secret', secret);
    secret = '';
  }
</script>

<section class="card">
  <h2>Sign in with Nostr</h2>
  <p class="muted">Your Nostr key is your identity. There is no account and no password.</p>

  <button class="primary" disabled={busy} onclick={() => onSignIn('extension')}>Use browser extension</button>
  <p class="muted">Recommended: the key stays in the extension and never enters this page.</p>

  <form onsubmit={submitSecret}>
    <label for="secret">Or paste a secret key (kept in memory only; use a key made just for this service)</label>
    <div class="row">
      <input id="secret" type="password" autocomplete="off" spellcheck="false" placeholder="nsec1…" bind:value={secret} />
      <button type="submit" disabled={busy || secret.trim() === ''}>Sign in</button>
    </div>
  </form>

  <button class="link" disabled={busy} onclick={() => onSignIn('guest')}>Continue as guest</button>

  {#if busy}<p role="status">Connecting…</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>
