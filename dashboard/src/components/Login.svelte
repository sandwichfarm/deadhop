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

<section class="login" aria-labelledby="login-title">
  <svg class="login-mark" viewBox="0 0 32 32" aria-hidden="true" focusable="false"
    ><circle cx="4.6" cy="20" r="3.1" fill="currentColor" /><path
      d="M7.4 16.2C10.5 5 20 2.6 24.5 14.8"
      fill="none"
      stroke="currentColor"
      stroke-width="2.8"
      stroke-linecap="round"
      stroke-dasharray="4.2 3.6"
    /><path
      d="M18 18.5H22.3V20.2a1 1 0 0 0 1 1H25.7a1 1 0 0 0 1-1V18.5H31V28.5a1.5 1.5 0 0 1-1.5 1.5H19.5A1.5 1.5 0 0 1 18 28.5Z"
      fill="currentColor"
    /></svg
  >
  <h1 id="login-title">Sign in with Nostr</h1>
  <p class="login-sub">Your key is your account.</p>

  <button class="primary wide" disabled={busy} onclick={() => onSignIn('extension')}>Use browser extension</button>

  <p class="or" aria-hidden="true"><span>or</span></p>

  <form class="secret-form" onsubmit={submitSecret}>
    <label class="visually-hidden" for="secret">Secret key</label>
    <input
      id="secret"
      class="mono"
      type="password"
      autocomplete="off"
      spellcheck="false"
      placeholder="nsec1… kept in memory only"
      bind:value={secret}
    />
    <button type="submit" disabled={busy || secret.trim() === ''}>Sign in</button>
  </form>

  <button class="link guest" disabled={busy} onclick={() => onSignIn('guest')}>Continue as guest</button>

  {#if busy}<p class="muted" role="status">Connecting…</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>
