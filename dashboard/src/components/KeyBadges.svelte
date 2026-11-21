<script lang="ts">
  import { keyTag } from '../lib/keys';

  // Small badges naming the keys that use a relay or a proxy. Nothing is shown
  // when every key does (the usual case), so a restriction stands out.
  let { keys, total }: { keys: string[]; total: number } = $props();
</script>

{#if keys.length > 0 && keys.length < total}
  <span class="key-badges">{#each keys as k (k)}<span class="badge key">{keyTag(k)}</span>{/each}</span>
{/if}
