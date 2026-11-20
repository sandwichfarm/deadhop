---
title: Solid
description: Add deadhop pay buttons, invoice forms and invoices to a Solid 1.8+ app (SolidStart too), or build your own UI with the useCheckout primitive.
---

# Solid

Solid 1.8 and later 1.x are supported (tested with 1.9); Solid 2 is not supported yet.
Read [Integrate payments into your app](./integrate.md) first: it explains the service
values, who creates invoices, and why `onPaid` is not proof of payment.

## Install

```sh
npm install @deadhop/client @deadhop/components solid-js
```

## Create the client once

```ts
// src/deadhop.ts
import { createDeadhop } from '@deadhop/components/solid'; // same as '@deadhop/client'

export const deadhop = createDeadhop({
  serverPubkey: 'npub1…',
  relays: ['wss://relay.example.com'],
});
```

## A pay button

```tsx
import { PayButton } from '@deadhop/components/solid'; // or '@deadhop/components/solidjs'
import '@deadhop/components/styles.css';
import { deadhop } from './deadhop';

export const Tip = () => (
  <PayButton client={deadhop} amount={5000} memo="Thanks!" onPaid={(inv) => console.log('paid', inv.id)}>
    Tip 5,000 sats
  </PayButton>
);
```

The button text is `children`; without it the button reads *Pay 5,000 sats*. Needs
[public invoicing](./integrate.md#_1-the-browser-creates-them-public-invoicing) on the
service.

## An invoice form

```tsx
import { InvoiceForm } from '@deadhop/components/solid';

export const Shop = () => <InvoiceForm client={deadhop} defaultAmount={21000} />;
```

## An invoice your backend created

```tsx
import { Invoice } from '@deadhop/components/solid';

export const Checkout = (props: { invoiceId: string }) => <Invoice client={deadhop} id={props.invoiceId} />;
```

See [Backend](./integrate-backend.md) for creating the invoice on your server.

## Your own markup: `useCheckout`

```tsx
import { Show } from 'solid-js';
import { useCheckout, InvoiceCard } from '@deadhop/components/solid';
import { LABELS } from '@deadhop/client';

export function Buy() {
  const [state, checkout] = useCheckout(deadhop, { onPaid: (inv) => console.log(inv.id) });
  return (
    <Show when={state().phase !== 'idle'} fallback={<button onClick={() => checkout.create({ amount_sat: 5000 })}>Buy</button>}>
      <InvoiceCard state={state()} labels={LABELS} />
    </Show>
  );
}
```

`state` is an accessor. The checkout stops polling when the owning component is
disposed. To keep callbacks current, pass the component's `props` object (or getters) as
the options.

## SolidStart and server rendering

The package ships a compiled build and, under the `solid` export condition, JSX that
SolidStart and other SSR setups compile themselves. Rendering on the server never opens
a connection.

## Language and styling

`labels` replaces any text, `class` adds a class to the outer element, and the stylesheet
is optional; see [Styling and page security](./integrate.md#styling-and-page-security).
For faster proof of work on phones, see
[Faster proof of work](./integrate.md#faster-proof-of-work-notemine).
