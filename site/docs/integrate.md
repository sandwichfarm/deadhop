---
title: Integrate payments into your app
description: Add bitcoin invoices and pay buttons to React, Solid, Svelte 5 or Svelte 4 apps with @cvmpay/client and @cvmpay/components, for a self-hosted or remote service.
---

# Integrate payments into your app

Two packages put cvmpay into an app you already have:

| Package | What it is | Where |
| --- | --- | --- |
| `@cvmpay/client` | Framework-agnostic client: connect, create and look up invoices, a headless checkout, QR codes | npm and JSR |
| `@cvmpay/components` | `PayButton`, `InvoiceForm` and `Invoice` for React, Solid, Svelte 5 and Svelte 4 | npm (React and Solid also on JSR) |

::: warning Not published yet
The name `cvmpay` is not final. The `@cvmpay` npm organization and JSR scope don't
exist yet, so `npm install @cvmpay/components` won't work until the packages are
published under their final name. Until then, build them from this repository (see
[Using the packages before they are published](#using-the-packages-before-they-are-published)).
:::

## What your app talks to

Your app never calls an HTTP API. It sends encrypted messages to a cvmpay service over
Nostr relays. Two values identify the service:

| Value | Where it comes from |
| --- | --- |
| `serverPubkey` | The service's public key (hex or `npub`). On the host: `cvmpay pubkey`, or the `server.pubkey` file in its data directory. Behind the homelab tooling: `./homelab cvmpay pubkey`. It is also the `serverPubkey` in the dashboard's `config.json`. |
| `relays` | Relays the service listens on (`CVMPAY_RELAYS`, or as changed in the dashboard). Your page needs **at least one** of them. |

The service can be your own (self-hosted) or run by someone else (remote): the code is
the same either way, only these two values change. With a remote service, you trust its
operator to report payments truthfully. You can check its addresses against your own
wallet only if it uses your xpub.

```ts
import { createCvmpay } from '@cvmpay/client';

export const cvmpay = createCvmpay({
  serverPubkey: 'npub1…',               // the service's public key
  relays: ['wss://relay.example.com'],  // shared with the service
});
```

`createCvmpay` connects only when it is first used and keeps one connection, so create
it once per page (or per process) and pass it to every component.

## Two ways to create invoices

### 1. The browser creates them (public invoicing)

The customer's browser creates the invoice itself. A `PayButton` does this in one click.

- On the service: `CVMPAY_PUBLIC_INVOICING=true` (or *Public invoicing* in the
  dashboard's Settings).
- The browser signs with a throwaway key made in memory: customers need no Nostr
  account and see no extension prompt.
- Each invoice costs the browser a proof of work (`CVMPAY_POW_BITS`, default 20 bits).
  On a phone, that takes a few seconds, shown as *Preparing invoice…*. The service also
  rate-limits public invoices.

Good for tips, donations, a fixed-price product, or any page without a backend.

### 2. Your backend creates them (operator key)

Your server creates the invoice with an operator key and hands its id to the page. The
page only watches it with `<Invoice id>`, which any key may do: the id is the
capability.

- On the service: add the backend's public key to `CVMPAY_ADMINS` (or Settings →
  Operators). Public invoicing can stay off.
- No proof of work, and you decide the amount and memo on the server, not the customer.
- **An operator key can also manage the service** (settings, relays, wipe). Keep it on
  the server, never in a browser bundle, and use a key made only for this.

```ts
// server.ts (Node 20+)
import { createCvmpay, secretKeySigner } from '@cvmpay/client';

const cvmpay = createCvmpay({
  serverPubkey: process.env.CVMPAY_SERVER_PUBKEY!,
  relays: process.env.CVMPAY_RELAYS!.split(','),
  signer: secretKeySigner(process.env.CVMPAY_BACKEND_NSEC!),
});

export async function startCheckout(orderId: string, amountSat: number) {
  const invoice = await cvmpay.createInvoice({ amount_sat: amountSat, memo: `Order ${orderId}` });
  await db.orders.update(orderId, { invoiceId: invoice.id });
  return invoice.id; // send this to the page
}
```

## Fulfil orders only after your backend has checked

`onPaid` runs in the customer's browser, and the customer controls the browser. Use it to
update the page, never to release goods. Before you ship, ask the service yourself:

```ts
export async function isPaid(invoiceId: string): Promise<boolean> {
  const invoice = await cvmpay.getInvoice(invoiceId);
  return invoice.status === 'confirmed';
}
```

- `detected` means the payment was seen, not that it is final. An unconfirmed payment can
  still disappear. Wait for `confirmed` (`CVMPAY_MIN_CONFIRMATIONS`).
- An `expired` invoice can still become `confirmed`: the service keeps watching for late
  payments (`CVMPAY_LATE_GRACE_SECS`), and the components keep polling.
- Check that `invoice.amount_sat` is what you asked for.

## Components

All frameworks get the same three components, props and events.

| Component | Does |
| --- | --- |
| `PayButton` | A button for a fixed `amount` (sats). On click it creates the invoice and shows it: QR code, address, copy button, countdown, then the payment's progress. |
| `InvoiceForm` | Amount and optional note fields. Creates the invoice and shows it, with *New invoice* to start over. |
| `Invoice` | Shows an existing invoice by `id` and follows it until paid. Use it with invoices your backend created. |

Common props:

| Prop | Type | Meaning |
| --- | --- | --- |
| `client` | `Cvmpay` | From `createCvmpay`; required |
| `pollMs` | `number` | How often to check for news (default 5000) |
| `labels` | `Partial<Labels>` | Replace any text, for example for another language |
| `class` (React: `className`) | `string` | Extra class on the outer element |
| `onCreated(invoice)` | function | An invoice exists |
| `onDetected(invoice)` | function | The full amount was seen, not yet confirmed |
| `onPaid(invoice)` | function | The full amount is confirmed (verify on your backend) |
| `onError(error)` | function | An `ApiError` with a `code` such as `unauthorized`, `rate_limited`, `timeout` |

`PayButton` also takes `amount`, `memo`, `expirySecs` and its label (`children` in React
and Solid, `label` in Svelte). `InvoiceForm` takes `defaultAmount`, `defaultMemo`,
`showMemo` and `expirySecs`. `Invoice` takes `id`.

### React

```tsx
import { PayButton } from '@cvmpay/components/react';
import '@cvmpay/components/styles.css';
import { cvmpay } from './cvmpay';

export function Tip() {
  return <PayButton client={cvmpay} amount={5000} memo="Thanks!" onPaid={(inv) => console.log('paid', inv.id)} />;
}
```

The built files start with `'use client'`, so they work in Next.js App Router pages.
React 18 and 19 are supported (tested with 19). `useCheckout(client, options)` gives you the state without
any markup.

### Solid

```tsx
import { InvoiceForm } from '@cvmpay/components/solid'; // or '@cvmpay/components/solidjs'
import '@cvmpay/components/styles.css';
import { cvmpay } from './cvmpay';

export const Shop = () => <InvoiceForm client={cvmpay} defaultAmount={21000} />;
```

Solid 1.8 and later 1.x are supported (tested with 1.9). Solid 2 is not supported yet. The package ships a compiled build
and, under the `solid` export condition, JSX that SolidStart and other SSR setups
compile themselves.

### Svelte 5

```svelte
<script lang="ts">
  import { Invoice } from '@cvmpay/components/svelte5';
  import '@cvmpay/components/styles.css';
  import { cvmpay } from './cvmpay';
  let { invoiceId }: { invoiceId: string } = $props();
</script>

<Invoice client={cvmpay} id={invoiceId} onPaid={() => location.reload()} />
```

### Svelte 4

```svelte
<script>
  import { PayButton } from '@cvmpay/components/svelte4';
  import { cvmpay } from './cvmpay';
</script>

<PayButton client={cvmpay} amount={5000} label="Buy the ebook" />
```

These components use Svelte 4 syntax, so they also run under Svelte 5 in legacy mode.
They are tested on the real Svelte 4 runtime.

### Anything else: the headless checkout

The components are thin views over `createCheckout` from `@cvmpay/client`. Use it to
build your own markup in any framework, or none:

```ts
import { createCheckout, invoiceView } from '@cvmpay/client';

const checkout = createCheckout(cvmpay, { onPaid: (inv) => notifyBackend(inv.id) });
checkout.subscribe((state) => {
  // state.phase: idle | creating | loading | pending | detected | confirmed | expired | error
  if (state.invoice) render(invoiceView(state.invoice, state.now)); // labels, QR path, address…
});
await checkout.create({ amount_sat: 5000, memo: 'Order 42' }); // or checkout.watch(id)
// later: checkout.destroy();
```

`subscribe` follows the Svelte store contract, so a checkout is also a Svelte store
(`$checkout`), works with React's `useSyncExternalStore` and with Solid's `from`.

## Styling and page security

- `@cvmpay/components/styles.css` is optional. It uses only `cvmpay-` classes and custom
  properties (`--cvmpay-accent`, `--cvmpay-radius`, `--cvmpay-qr-size`, …) that you can
  override. Without it, the components are unstyled semantic HTML.
- No inline styles or injected scripts: the components run under a strict
  Content-Security-Policy. Your page needs `connect-src` to allow `wss:` (the relays),
  and `img-src data:` only if you add images of your own that need it.
- The QR code is an SVG drawn from the invoice's `bitcoin:` URI. The client checks that
  the URI pays exactly the address and amount shown.
- The memo is written by whoever created the invoice. The components show it as plain
  text under *Note from the invoice creator*.
- Importing a component never touches `window` or opens a connection. Nothing happens
  until it mounts, so server-side rendering works (tested for every framework).

## Installing

Once the packages are published:

```sh
npm install @cvmpay/client @cvmpay/components   # plus your framework
npx jsr add @cvmpay/client                        # JSR (client, and React/Solid components)
deno add jsr:@cvmpay/client
```

The Svelte components are only on npm: JSR can't host `.svelte` files.

### Using the packages before they are published

```sh
git clone <this repository> && cd cvmpay
just install
npm run build --workspace packages/client --workspace packages/components
(cd packages/client && npm pack) && (cd packages/components && npm pack)
```

Then, in your app, install both tarballs together. The override makes npm take the client
from the tarball instead of looking for it in the registry:

```json
{
  "dependencies": {
    "@cvmpay/client": "file:../cvmpay/packages/client/cvmpay-client-0.1.0.tgz",
    "@cvmpay/components": "file:../cvmpay/packages/components/cvmpay-components-0.1.0.tgz"
  },
  "overrides": { "@cvmpay/client": "file:../cvmpay/packages/client/cvmpay-client-0.1.0.tgz" }
}
```

## Checklist for agents and developers

1. Get the service's `serverPubkey` and a relay it listens on.
2. Decide who creates invoices. In the browser: turn on public invoicing. On your
   backend: give it an operator key that stays on the server.
3. `createCvmpay({ serverPubkey, relays })` once, and pass it as `client`.
4. Render `PayButton`, `InvoiceForm` or `Invoice` from the subpath for your framework.
5. Allow `wss:` in `connect-src` if your page has a Content-Security-Policy.
6. Fulfil an order only after your backend's `getInvoice(id)` returns `confirmed` for the
   right amount.

## How it is tested

- **Unit tests:** `@cvmpay/client` and every framework's components, at 100% coverage.
- **Svelte 4:** the components also run on the real Svelte 4 runtime.
- **Server-side rendering:** every framework renders to a string.
- **Real stack:** on regtest, the built client and each framework's `PayButton` (in
  headless Chrome, under a strict CSP) pay real invoices through a real relay and service.
- **Packaging:** `just packages` packs both packages, installs them into fresh React,
  Solid, Svelte 5 and Svelte 4 projects, and builds and type-checks each one. It also
  checks tree shaking, then runs publint, Are the Types Wrong and a JSR dry run.
