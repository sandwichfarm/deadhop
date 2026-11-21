<script lang="ts">
  import { loadDismissed, saveDismissed } from '../lib/dismissed';

  // The service's warnings, each of which the operator can dismiss in this browser.
  let { warnings }: { warnings: string[] } = $props();

  let dismissed = $state(loadDismissed());
  const shown = $derived(warnings.filter((w) => !dismissed.includes(w)));
  const hidden = $derived(warnings.length - shown.length);

  function dismiss(w: string) {
    dismissed = [...dismissed, w];
    saveDismissed(dismissed);
  }

  function restore() {
    dismissed = dismissed.filter((d) => !warnings.includes(d));
    saveDismissed(dismissed);
  }
</script>

{#each shown as w (w)}
  <div class="warning" role="status">
    <span class="warning-text">{w}</span>
    <button type="button" class="dismiss" aria-label={`Dismiss: ${w}`} onclick={() => dismiss(w)}
      ><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg></button
    >
  </div>
{/each}
{#if hidden > 0}
  <button type="button" class="link restore" onclick={restore}>{`Show ${hidden} dismissed warning${hidden === 1 ? '' : 's'}`}</button>
{/if}
