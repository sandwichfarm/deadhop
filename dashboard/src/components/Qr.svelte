<script lang="ts">
  import { qrShape } from '../lib/qr';

  let { text, label }: { text: string; label: string } = $props();
  const shape = $derived(qrShape(text));
</script>

<svg class="qr" viewBox={`0 0 ${shape.size} ${shape.size}`} role="img" aria-label={label} shape-rendering="crispEdges">
  <rect width={shape.size} height={shape.size} fill="#ffffff" />
  <path d={shape.path} fill="#000000" />
</svg>
