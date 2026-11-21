<script lang="ts">
  import { untrack } from 'svelte';
  import { splitDuration, toSecs, type Unit } from '../lib/proxy';

  let { id, label, secs, onchange }: { id: string; label: string; secs: number; onchange: (secs: number) => void } = $props();

  // Shown as a number and a unit; `secs` follows. An empty box is NaN, which the
  // form reports as an unfilled interval.
  const start = untrack(() => splitDuration(secs));
  let text = $state(String(start.n));
  let unit = $state<Unit>(start.unit);

  function update() {
    onchange(text.trim() === '' ? NaN : toSecs(Number(text), unit));
  }
</script>

<div class="field duration">
  <label for={id}>{label}</label>
  <div class="duration-row">
    <input
      {id}
      type="number"
      min="1"
      step="any"
      value={text}
      oninput={(e) => {
        text = e.currentTarget.value;
        update();
      }}
    />
    <select
      aria-label={`${label}: unit`}
      onchange={(e) => {
        unit = e.currentTarget.value as Unit;
        update();
      }}
    >
      <option value="s" selected={unit === 's'}>seconds</option>
      <option value="m" selected={unit === 'm'}>minutes</option>
      <option value="h" selected={unit === 'h'}>hours</option>
      <option value="d" selected={unit === 'd'}>days</option>
    </select>
  </div>
</div>
