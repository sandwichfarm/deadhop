<script lang="ts">
  import { usersOf } from '../lib/keys';
  import type { KeyInfo } from '../lib/schemas';

  // "Used by": one switch per key for a relay or a proxy. Everything on is the
  // default and is saved as "all keys". The last key cannot be switched off:
  // something that no key uses has no purpose, and removing it is a button of
  // its own. Hidden while the service has only one key.
  let {
    keys,
    assigned,
    label,
    onchange,
    disabled = false,
  }: {
    keys: KeyInfo[];
    /** Key ids assigned; empty is every key. */
    assigned: string[];
    /** What the switches are for, for screen readers. */
    label: string;
    onchange: (id: string, on: boolean) => void;
    disabled?: boolean;
  } = $props();

  const users = $derived(usersOf(assigned, keys.map((k) => k.id)));
</script>

{#if keys.length > 1}
  <span class="key-chips" role="group" aria-label={`${label}: used by`}>
    <span class="lbl">Used by</span>
    {#each keys as k (k.id)}
      {@const on = users.includes(k.id)}
      <label class={on ? 'key-chip on' : 'key-chip'}>
        <input type="checkbox" checked={on} disabled={disabled || (on && users.length === 1)} onchange={(e) => onchange(k.id, e.currentTarget.checked)} aria-label={`${label} used by ${k.label}`} />{k.label}
      </label>
    {/each}
    {#if users.length === keys.length}<span class="hint inline">all keys</span>{/if}
  </span>
{/if}
