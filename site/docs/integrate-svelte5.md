---
title: Svelte 5
description: Add deadhop pay buttons, invoice forms and invoices to a Svelte 5 or SvelteKit app, or build your own UI with useCheckout.
---

# Svelte 5

These components use runes and need Svelte 5. For Svelte 4, or Svelte 5 in legacy mode,
use [Svelte 4](./integrate-svelte4.md). Read
[Integrate payments into your app](./integrate.md) first: it explains the service values,
who creates invoices, and why `onPaid` is not proof of payment.

## Install

```sh
npm install @deadhop/client @deadhop/components svelte
```

npm only: JSR can't host `.svelte` files. The packages aren't published yet; until they
are, see [Using the packages before they are published](./integrate.md#using-the-packages-before-they-are-published).

## Create the client once

```ts
// src/lib/deadhop.ts
import { createDeadhop } from '@deadhop/components/svelte5'; // same as '@deadhop/client'

export const deadhop = createDeadhop({
  serverPubkey: 'npub1…',
  relays: ['wss://relay.example.com'],
});
```

## A pay button

```svelte
<script lang="ts">
  import { PayButton } from '@deadhop/components/svelte5';
  import '@deadhop/components/styles.css';
  import { deadhop } from '$lib/deadhop';
</script>

<PayButton client={deadhop} amount={5000} memo="Thanks!" label="Tip 5,000 sats" onPaid={(inv) => console.log('paid', inv.id)} />
```

Events are callback props (`onPaid`), as usual in Svelte 5. Needs
[public invoicing](./integrate.md#_1-the-browser-creates-them-public-invoicing) on the
service.

## An invoice form

```svelte
<InvoiceForm client={deadhop} defaultAmount={21000} showMemo={false} />
```

## An invoice your backend created

```svelte
<script lang="ts">
  import { Invoice } from '@deadhop/components/svelte5';
  import { invalidateAll } from '$app/navigation';
  import { deadhop } from '$lib/deadhop';
  let { data } = $props(); // from +page.server.ts: { invoiceId }
</script>

<Invoice client={deadhop} id={data.invoiceId} onPaid={() => invalidateAll()} />
```

Create the invoice in `+page.server.ts` with an operator key; see
[Backend](./integrate-backend.md).

## Your own markup: `useCheckout`

```svelte
<script lang="ts">
  import { useCheckout, InvoiceCard } from '@deadhop/components/svelte5';
  import { LABELS } from '@deadhop/client';
  import { deadhop } from '$lib/deadhop';

  const buy = useCheckout(deadhop, { events: () => ({ onPaid: (inv) => console.log(inv.id) }) });
</script>

{#if buy.state.phase === 'idle'}
  <button onclick={() => buy.checkout.create({ amount_sat: 5000 })}>Buy</button>
{:else}
  <InvoiceCard state={buy.state} labels={LABELS} />
{/if}
```

`buy.state` is reactive. Call `useCheckout` during component setup (or inside
`$effect.root`); the checkout stops when the component is destroyed. `events` is a
getter, so callbacks that depend on props stay current.

## SvelteKit and server rendering

Importing a component never touches `window` or opens a connection: pages render on the
server and connect only in the browser.

## Language and styling

`labels` replaces any text, `class` adds a class to the outer element, and the stylesheet
is optional; see [Styling and page security](./integrate.md#styling-and-page-security).
For faster proof of work on phones, see
[Faster proof of work](./integrate.md#faster-proof-of-work-notemine).
