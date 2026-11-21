<script lang="ts">
  import type { RelayPoint } from '../lib/schemas';
  import { formatTime } from '../lib/format';

  // A relay's results over time as bars: green for what worked, red for what
  // failed, stacked, one bar per bucket. A failure is always at least 2 units
  // tall, so one error among hundreds still shows.
  let { points, label, tall = false }: { points: RelayPoint[]; label: string; tall?: boolean } = $props();

  const H = 40;
  const STEP = 10;
  const peak = $derived(Math.max(1, ...points.map((p) => p.ok + p.fail)));
  const bars = $derived(
    points.map((p, i) => {
      const fail = p.fail === 0 ? 0 : Math.max(2, (p.fail / peak) * H);
      const ok = p.ok === 0 ? 0 : Math.max(2, (p.ok / peak) * H);
      return { x: i * STEP + 1, fail, ok, empty: p.ok + p.fail === 0 };
    }),
  );
  const failed = $derived(points.reduce((n, p) => n + p.fail, 0));
  const total = $derived(points.reduce((n, p) => n + p.ok + p.fail, 0));
</script>

<svg
  class={tall ? 'bars tall' : 'bars'}
  viewBox={`0 0 ${points.length * STEP} ${H}`}
  preserveAspectRatio="none"
  role="img"
  aria-label={`${label}: ${failed} of ${total} results failed, from ${formatTime(points[0].start)}`}
>
  {#each bars as b, i (i)}
    {#if b.empty}
      <rect class="bar-none" x={b.x} y={H - 1} width="8" height="1" />
    {:else}
      {#if b.ok > 0}<rect class="bar-ok" x={b.x} y={H - b.ok - b.fail} width="8" height={b.ok} />{/if}
      {#if b.fail > 0}<rect class="bar-fail" x={b.x} y={H - b.fail} width="8" height={b.fail} />{/if}
    {/if}
  {/each}
</svg>
