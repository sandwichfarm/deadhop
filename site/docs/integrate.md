---
title: Integrate payments into your app
description: What every deadhop integration shares (the service, who creates invoices, checking payments, proof of work) and the guide for each client.
---

# Integrate payments into your app

A client package and one component package per framework put deadhop into an app you already have. This page covers what every
integration shares. For code, pick your client under [Choose your client](#choose-your-client).

| Package | What it is | Where |
| --- | --- | --- |
| `@deadhop/client` | Framework-agnostic client: connect, create and look up invoices, a headless checkout, QR codes | npm and JSR (the notemine adapter: npm only) |
| `@deadhop/react` | `PayButton`, `InvoiceForm`, `Invoice` and `useCheckout` for React | npm and JSR |
| `@deadhop/solid` | The same for Solid | npm and JSR |
| `@deadhop/svelte` | The same for Svelte 5, and for Svelte 4 at `@deadhop/svelte/svelte4` | npm only (JSR can't host `.svelte` files) |

## What your app talks to

Your app never calls an HTTP API. It sends encrypted messages to a deadhop service over
Nostr relays. Two values identify the service:

| Value | Where it comes from |
| --- | --- |
| `serverPubkey` | The service's public key (hex or `npub`). On the host: `deadhop pubkey`, or the `server.pubkey` file in its data directory. Behind the homelab tooling: `./homelab cvmpay pubkey`. It is also the `serverPubkey` in the dashboard's `config.json`. |
| `relays` | Relays the service listens on (`DEADHOP_RELAYS`, or as changed in the dashboard). Your page needs **at least one** of them. |

The service can be your own (self-hosted) or run by someone else (remote): the code is
the same either way, only these two values change. With a remote service, you trust its
operator to report payments truthfully. You can check its addresses against your own
wallet only if it uses your xpub.

```ts
import { createDeadhop } from '@deadhop/client';

export const deadhop = createDeadhop({
  serverPubkey: 'npub1…',               // the service's public key
  relays: ['wss://relay.example.com'],  // shared with the service
});
```

`createDeadhop` connects only when it is first used and keeps one connection, so create
it once per page (or per process) and pass it to every component.

## Two ways to create invoices

### 1. The browser creates them (public invoicing)

The customer's browser creates the invoice itself. A `PayButton` does this in one click.

- On the service: `DEADHOP_PUBLIC_INVOICING=true` (or *Public invoicing* in the
  dashboard's Settings).
- The browser signs with a throwaway key made in memory: customers need no Nostr
  account and see no extension prompt.
- Each invoice costs the browser a proof of work (`DEADHOP_POW_BITS`, default 20 bits).
  The built-in miner took 5.3 s on average for 20 bits in desktop Chrome, and a phone is
  slower; it is shown as *Preparing invoice…*. The service also rate-limits public
  invoices.

Good for tips, donations, a fixed-price product, or any page without a backend.

Nothing to configure in your app: the service tells each client how much work it wants
(`pow_bits` in `get_info`), and the client mines exactly that before each new invoice.
Operators owe none. To make it faster, see
[Faster proof of work](#faster-proof-of-work-notemine).

### 2. Your backend creates them (operator key)

Your server creates the invoice with an operator key and hands its id to the page. The
page only watches it with `<Invoice id>`, which any key may do: the id is the
capability.

- On the service: add the backend's public key to `DEADHOP_ADMINS` (or Settings →
  Operators). Public invoicing can stay off.
- No proof of work, and you decide the amount and memo on the server, not the customer.
- **An operator key can also manage the service** (settings, relays, wipe). Keep it on
  the server, never in a browser bundle, and use a key made only for this.

See [Backend](./integrate-backend.md) for the code.

## Fulfil orders only after your backend has checked

`onPaid` runs in the customer's browser, and the customer controls the browser. Use it to
update the page, never to release goods. Before you ship, ask the service yourself:

[Backend](./integrate-backend.md#check-before-you-fulfil) shows the check.


- `detected` means the payment was seen, not that it is final. An unconfirmed payment can
  still disappear. Wait for `confirmed` (`DEADHOP_MIN_CONFIRMATIONS`).
- An `expired` invoice can still become `confirmed`: the service keeps watching for late
  payments (`DEADHOP_LATE_GRACE_SECS`), and the components keep polling.
- Check that `invoice.amount_sat` is what you asked for.

## Choose your client

Every client gets the same three components, with the same props and events:

| Component | Does |
| --- | --- |
| `PayButton` | A button for a fixed `amount` (sats). On click it creates the invoice and shows it: QR code, address, copy button, countdown, then the payment's progress. |
| `InvoiceForm` | Amount and optional note fields. Creates the invoice and shows it, with *New invoice* to start over. |
| `Invoice` | Shows an existing invoice by `id` and follows it until paid. Use it with invoices your backend created. |

| Your app | Import from | Guide |
| --- | --- | --- |
| React 18 or 19 (Next.js too) | `@deadhop/react` | [React](./integrate-react.md) |
| Solid 1.8+ (SolidStart too) | `@deadhop/solid` | [Solid](./integrate-solid.md) |
| Svelte 5 (SvelteKit too) | `@deadhop/svelte` | [Svelte 5](./integrate-svelte5.md) |
| Svelte 4, or Svelte 5 in legacy mode | `@deadhop/svelte/svelte4` | [Svelte 4](./integrate-svelte4.md) |
| Any other framework, or none | `@deadhop/client` | [Plain JavaScript](./integrate-javascript.md) |
| A server (Node, Deno, Bun) | `@deadhop/client` | [Backend](./integrate-backend.md) |

Props every component takes:

| Prop | Type | Meaning |
| --- | --- | --- |
| `client` | `Deadhop` | From `createDeadhop`; required |
| `pollMs` | `number` | How often to check for news (default 5000) |
| `labels` | `Partial<Labels>` | Replace any text, for example for another language |
| `class` (React: `className`) | `string` | Extra class on the outer element |
| `copy` | `(text) => Promise<void>` | Copies the address (default: the browser clipboard) |
| `onCreated(invoice)` | function | An invoice exists |
| `onDetected(invoice)` | function | The full amount was seen, not yet confirmed |
| `onPaid(invoice)` | function | The full amount is confirmed (verify on your backend) |
| `onError(error)` | function | An `ApiError` with a `code` such as `unauthorized`, `rate_limited`, `timeout` |

`PayButton` also takes `amount`, `memo`, `expirySecs` and its label (`children` in React
and Solid, `label` in Svelte). `InvoiceForm` takes `defaultAmount`, `defaultMemo`,
`showMemo` and `expirySecs`. `Invoice` takes `id`.

## Faster proof of work (notemine)

The built-in miner hashes on the page's main thread, and gives up after 25 seconds.
Mining time varies a lot from one invoice to the next, so at the default 20 bits a slow
phone will sometimes run out of time. [notemine](https://github.com/sandwichfarm/notemine)
mines in WebAssembly across several Web Workers. In headless Chrome on a 16-core
machine, 20 bits took 0.4 s on average with notemine and 5.3 s with the built-in miner.

```sh
npm install @notemine/wrapper @notemine/core rxjs
```

```ts
import { createDeadhop } from '@deadhop/client';
import { notemineMiner } from '@deadhop/client/notemine';

export const deadhop = createDeadhop({
  serverPubkey: 'npub1…',
  relays: ['wss://relay.example.com'],
  miner: notemineMiner(), // pass to every component as usual
});
```

- npm only: `@deadhop/client/notemine` is not on JSR.
- `@notemine/wrapper` is an optional peer dependency: install it only if you use this.
  `@notemine/core` and `rxjs` are its own peer dependencies. npm installs them by itself,
  some other package managers don't. Without them, the built-in miner quietly takes over.
  Its code (about 500 KB) is a separate chunk that loads when the first invoice needs
  work, not with the page.
- A page with a Content-Security-Policy must allow notemine's worker and WebAssembly:
  `worker-src blob:` and `script-src 'self' 'wasm-unsafe-eval'`.
- Where notemine cannot run (a stricter policy, no Web Workers, server-side), the
  built-in miner takes over. Pass `notemineMiner({ fallback: false })` to fail instead.
  `workers` sets how many Web Workers it uses (default: one less than the device's
  cores, at most 8).
- Whichever miner runs, the client checks the result before it signs anything: the same
  request, a `nonce` tag and enough leading zero bits. A broken miner can't make the
  client send something the service would refuse.
- `@notemine/wrapper` is MIT, but it includes notemine's WebAssembly core, and
  `@notemine/core` is GPL-3.0. Check that this suits your app before you ship it.

To use a different miner, pass any function of type `Miner` (exported from
`@deadhop/client`).

## Styling and page security

- `styles.css` (`@deadhop/react/styles.css`, `@deadhop/solid/styles.css` or
  `@deadhop/svelte/styles.css`; the same file in each) is optional. It uses only `deadhop-` classes and custom
  properties (`--deadhop-accent`, `--deadhop-radius`, `--deadhop-qr-size`, …) that you can
  override. Without it, the components are unstyled semantic HTML.
- No inline styles or injected scripts: the components run under a strict
  Content-Security-Policy. Your page needs `connect-src` to allow `wss:` (the relays),
  and `img-src data:` only if you add images of your own that need it. With notemine,
  also `worker-src blob:` and `'wasm-unsafe-eval'` (see above).
- The QR code is an SVG drawn from the invoice's `bitcoin:` URI. The client checks that
  the URI pays exactly the address and amount shown.
- The memo is written by whoever created the invoice. The components show it as plain
  text under *Note from the invoice creator*.
- Importing a component never touches `window` or opens a connection. Nothing happens
  until it mounts, so server-side rendering works (tested for every framework).

## Installing

```sh
npm install @deadhop/client @deadhop/react    # or @deadhop/solid, @deadhop/svelte; plus your framework
npx jsr add @deadhop/client @deadhop/react     # JSR: the client, React and Solid
deno add jsr:@deadhop/client
```

`@deadhop/svelte` is only on npm: JSR can't host `.svelte` files. Every framework
package re-exports `createDeadhop`, so the client needs no separate import.

## Checklist for agents and developers

1. Get the service's `serverPubkey` and a relay it listens on.
2. Decide who creates invoices. In the browser: turn on public invoicing. On your
   backend: give it an operator key that stays on the server.
3. `createDeadhop({ serverPubkey, relays })` once, and pass it as `client`.
4. Render `PayButton`, `InvoiceForm` or `Invoice` from the subpath for your framework.
5. Allow `wss:` in `connect-src` if your page has a Content-Security-Policy.
   Optionally, for faster proof of work on phones, use `notemineMiner()`. With a
   Content-Security-Policy, that also needs `worker-src blob:` and `'wasm-unsafe-eval'`.
6. Fulfil an order only after your backend's `getInvoice(id)` returns `confirmed` for the
   right amount.

## How it is tested

- **Unit tests:** `@deadhop/client` and every framework's components, at 100% coverage.
- **Svelte 4:** the components also run on the real Svelte 4 runtime.
- **Server-side rendering:** every framework renders to a string.
- **Real stack:** on regtest, the built client and each framework's `PayButton` (in
  headless Chrome, under a strict CSP) pay real invoices through a real relay and service.
  So does a page using notemine, once with a policy that allows it and once with the
  strict policy, where the built-in miner takes over.
- **Packaging:** `just packages` packs all four packages, installs them into fresh React,
  Solid, Svelte 5 and Svelte 4 projects, and builds and type-checks each one. It also
  checks tree shaking (notemine stays out of apps that don't use it, and loads lazily in
  apps that do), then runs publint, Are the Types Wrong and a JSR dry run.
