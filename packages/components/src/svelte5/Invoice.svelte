<script lang="ts">
  import { LABELS, type CheckoutEvents, type Deadhop, type Labels } from '@deadhop/client';
  import InvoiceCard from './InvoiceCard.svelte';
  import { useCheckout } from './checkout.svelte.js';

  let {
    client,
    id,
    pollMs,
    labels = {},
    class: className = '',
    copy,
    ...events
  }: CheckoutEvents & {
    client: Pick<Deadhop, 'createInvoice' | 'getInvoice'>;
    id: string;
    pollMs?: number;
    labels?: Partial<Labels>;
    class?: string;
    copy?: (text: string) => Promise<void>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const c = useCheckout(client, { pollMs, events: () => events });

  $effect(() => {
    const current = id;
    c.checkout.reset();
    void c.checkout.watch(current);
  });
</script>

<div class={className ? `deadhop ${className}` : 'deadhop'}>
  <InvoiceCard state={c.state} labels={{ ...LABELS, ...labels }} {copy} />
</div>
