---
title: React
description: Add deadhop pay buttons, invoice forms and invoices to a React 18 or 19 app (Next.js App Router too), or build your own UI with the useCheckout hook.
---

# React

React 18 and 19 are supported (tested with 19). Read
[Integrate payments into your app](./integrate.md) first: it explains the service values,
who creates invoices, and why `onPaid` is not proof of payment.

## Install

```sh
npm install @deadhop/client @deadhop/components react react-dom
```

The packages aren't published yet; until they are, see
[Using the packages before they are published](./integrate.md#using-the-packages-before-they-are-published).

## Create the client once

```ts
// src/deadhop.ts
import { createDeadhop } from '@deadhop/components/react'; // same as '@deadhop/client'

export const deadhop = createDeadhop({
  serverPubkey: 'npub1…',               // the service's public key
  relays: ['wss://relay.example.com'],  // a relay the service listens on
});
```

`createDeadhop` connects on first use and keeps one connection. Create it at module level,
not inside a component, so every render and component shares it.

## A pay button

```tsx
import { PayButton } from '@deadhop/components/react';
import '@deadhop/components/styles.css';
import { deadhop } from './deadhop';

export function Tip() {
  return (
    <PayButton client={deadhop} amount={5000} memo="Thanks!" onPaid={(inv) => console.log('paid', inv.id)}>
      Tip 5,000 sats
    </PayButton>
  );
}
```

The button text is `children`; without it the button reads *Pay 5,000 sats*. Needs
[public invoicing](./integrate.md#_1-the-browser-creates-them-public-invoicing) on the
service.

## An invoice form

```tsx
import { InvoiceForm } from '@deadhop/components/react';

<InvoiceForm client={deadhop} defaultAmount={21000} showMemo onCreated={(inv) => saveOrder(inv.id)} />
```

## An invoice your backend created

```tsx
import { Invoice } from '@deadhop/components/react';

export function Checkout({ invoiceId }: { invoiceId: string }) {
  return <Invoice client={deadhop} id={invoiceId} onPaid={() => router.refresh()} />;
}
```

Your server creates the invoice with an operator key (see [Backend](./integrate-backend.md))
and sends the page its id. The page only watches it, so public invoicing can stay off.

## Your own markup: `useCheckout`

```tsx
import { useCheckout, InvoiceCard } from '@deadhop/components/react';
import { LABELS } from '@deadhop/client';

export function Buy() {
  const [state, checkout] = useCheckout(deadhop, { onPaid: (inv) => console.log(inv.id) });
  if (state.phase === 'idle') {
    return <button onClick={() => checkout.create({ amount_sat: 5000, memo: 'Ebook' })}>Buy the ebook</button>;
  }
  return <InvoiceCard state={state} labels={LABELS} />;
}
```

`state.phase` is one of `idle`, `creating`, `loading`, `pending`, `detected`, `confirmed`,
`expired` or `error`. `InvoiceCard` draws any state; replace it with your own markup if
you like, using `invoiceView(state.invoice, state.now)` from `@deadhop/client` for the
labels and the QR path. Event callbacks always see the latest props.

## Next.js and server rendering

- The built files start with `'use client'`, so you can import the components straight
  into App Router pages.
- Importing a component never touches `window` or opens a connection; it renders to
  HTML on the server and connects only after it mounts.
- Create invoices on the server with `@deadhop/client` in a route handler or server
  action, never with an operator key in client code (see [Backend](./integrate-backend.md)).

## Language and styling

- `labels` replaces any text, for example for another language:

  ```tsx
  <PayButton client={deadhop} amount={5000} labels={{ pay: (sats) => `${sats} sats zahlen`, copy: 'Adresse kopieren' }} />
  ```

- `className` adds a class to the outer element. The stylesheet is optional; see
  [Styling and page security](./integrate.md#styling-and-page-security).
- For faster proof of work on phones, pass `miner: notemineMiner()` to `createDeadhop`
  (see [Faster proof of work](./integrate.md#faster-proof-of-work-notemine)).
