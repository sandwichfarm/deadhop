---
title: Svelte 4
description: Add deadhop pay buttons, invoice forms and invoices to a Svelte 4 app (or Svelte 5 in legacy mode), or build your own UI with the checkout store.
---

# Svelte 4

These components use Svelte 4 syntax, so they also run under Svelte 5 in legacy mode.
They are tested on the real Svelte 4 runtime. Read
[Integrate payments into your app](./integrate.md) first: it explains the service values,
who creates invoices, and why `onPaid` is not proof of payment.

## Install

```sh
npm install @deadhop/client @deadhop/svelte svelte@4
```

npm only: JSR can't host `.svelte` files.

## Create the client once

```js
// src/lib/deadhop.js
import { createDeadhop } from '@deadhop/svelte/svelte4'; // same as '@deadhop/client'

export const deadhop = createDeadhop({
  serverPubkey: 'npub1…',
  relays: ['wss://relay.example.com'],
});
```

## A pay button

```svelte
<script>
  import { PayButton } from '@deadhop/svelte/svelte4';
  import '@deadhop/svelte/styles.css';
  import { deadhop } from './lib/deadhop.js';
</script>

<PayButton client={deadhop} amount={5000} label="Buy the ebook" onPaid={(inv) => console.log('paid', inv.id)} />
```

Events are callback props (`onPaid`, `onDetected`, …), not `on:` events. Needs
[public invoicing](./integrate.md#_1-the-browser-creates-them-public-invoicing) on the
service.

## An invoice form

```svelte
<InvoiceForm client={deadhop} defaultAmount={21000} showMemo={false} />
```

## An invoice your backend created

```svelte
<script>
  import { Invoice } from '@deadhop/svelte/svelte4';
  export let invoiceId;
</script>

<Invoice client={deadhop} id={invoiceId} pollMs={2000} />
```

See [Backend](./integrate-backend.md) for creating the invoice on your server.

## Your own markup: the checkout store

```svelte
<script>
  import { onDestroy } from 'svelte';
  import { createCheckout, InvoiceCard } from '@deadhop/svelte/svelte4';
  import { LABELS } from '@deadhop/client';
  import { deadhop } from './lib/deadhop.js';

  const checkout = createCheckout(deadhop, { onPaid: (inv) => console.log(inv.id) });
  onDestroy(() => checkout.destroy());
</script>

{#if $checkout.phase === 'idle'}
  <button on:click={() => checkout.create({ amount_sat: 5000 })}>Buy</button>
{:else}
  <InvoiceCard state={$checkout} labels={LABELS} />
{/if}
```

A checkout follows the Svelte store contract, so `$checkout` works directly. Destroy it
when the component goes away to stop polling.

## TypeScript

The package ships types for every component. In `<script lang="ts">`, add
`vitePreprocess()` to `svelte.config.js` as usual for Svelte 4; `svelte-check` then
catches wrong props, such as a string `amount`.

## Language and styling

`labels` replaces any text, `class` adds a class to the outer element, and the stylesheet
is optional; see [Styling and page security](./integrate.md#styling-and-page-security).
For faster proof of work on phones, see
[Faster proof of work](./integrate.md#faster-proof-of-work-notemine).
