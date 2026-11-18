<script lang="ts">
  // A small line chart drawn as SVG: no chart library, nothing executed from data.
  import type { Series } from '../lib/schemas';
  import { formatSats, formatTime } from '../lib/format';

  let { series, metric = 'sat' }: { series: Series; metric?: 'sat' | 'count' } = $props();

  const W = 600;
  const H = 180;
  const PAD = 8;

  const values = $derived(series.points.map((p) => (metric === 'sat' ? p.sat : p.count)));
  const peak = $derived(Math.max(0, ...values));
  const max = $derived(Math.max(1, peak));
  const step = $derived((W - 2 * PAD) / Math.max(1, values.length - 1));
  const coords = $derived(values.map((v, i) => [PAD + i * step, H - PAD - (v / max) * (H - 2 * PAD)] as const));
  const line = $derived(coords.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`).join(' '));
  const area = $derived(`${line} L${coords[coords.length - 1][0].toFixed(1)},${H - PAD} L${PAD},${H - PAD} Z`);
  // Buckets before the service first ran are shaded: no data could exist there.
  const offline = $derived(series.points.filter((p) => p.start + series.bucket_secs <= series.online_since).length);
  const total = $derived(values.reduce((a, b) => a + b, 0));
  const label = (v: number) => (metric === 'sat' ? formatSats(v) : `${v} payment${v === 1 ? '' : 's'}`);
  const summary = $derived(
    `${label(total)} in ${values.length} buckets from ${formatTime(series.points[0].start)}; highest bucket ${label(peak)}.`,
  );
</script>

<div class="chart">
<figure>
  <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={summary} preserveAspectRatio="none">
    {#if offline > 0}
      <rect class="offline" x="0" y="0" width={PAD + (offline - 0.5) * step} height={H} />
    {/if}
    <line class="axis" x1={PAD} y1={H - PAD} x2={W - PAD} y2={H - PAD} />
    <path class="area" d={area} />
    <path class="line" d={line} />
    {#each coords as [x, y], i (i)}
      {#if values[i] > 0}<circle class="dot" cx={x} cy={y} r="3" />{/if}
    {/each}
  </svg>
  <figcaption class="row spread muted">
    <span>{formatTime(series.points[0].start)}</span>
    <span>{`peak ${label(peak)}`}</span>
    <span>{formatTime(series.points[series.points.length - 1].start)}</span>
  </figcaption>
</figure>
  <details>
    <summary>Show as table</summary>
    <table>
      <thead><tr><th>From</th><th>Payments</th><th>Amount</th></tr></thead>
      <tbody>
        {#each series.points as p (p.start)}
          <tr><td>{formatTime(p.start)}</td><td>{p.count}</td><td>{formatSats(p.sat)}</td></tr>
        {/each}
      </tbody>
    </table>
  </details>
</div>
