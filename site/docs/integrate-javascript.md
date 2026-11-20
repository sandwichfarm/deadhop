---
title: Plain JavaScript
description: Use deadhop from any framework, or none, with the headless checkout in @deadhop/client - your own markup, with invoice state, QR code and countdown worked out for you.
---

# Plain JavaScript

The framework components are thin views over `createCheckout` from `@deadhop/client`.
Use it directly for Vue, Angular, Lit, Preact, web components or plain DOM code. Read
[Integrate payments into your app](./integrate.md) first: it explains the service values,
who creates invoices, and why `onPaid` is not proof of payment.

## Install

```sh
npm install @deadhop/client      # npm
npx jsr add @deadhop/client      # JSR
deno add jsr:@deadhop/client     # Deno
```

The packages aren't published yet; until they are, see
[Using the packages before they are published](./integrate.md#using-the-packages-before-they-are-published).

## A pay button without a framework

```html
<button id="pay">Pay 5,000 sats</button>
<div id="invoice" hidden>
  <p id="status"></p>
  <a id="qr-link"><svg id="qr" width="220" height="220"><path /></svg></a>
  <code id="address"></code>
  <p id="expiry"></p>
</div>
```

```ts
import { createDeadhop, createCheckout, invoiceView, progressMessage } from '@deadhop/client';

const deadhop = createDeadhop({ serverPubkey: 'npub1…', relays: ['wss://relay.example.com'] });
const checkout = createCheckout(deadhop, { onPaid: (inv) => console.log('paid', inv.id) });
const $ = (id: string) => document.getElementById(id)!;

checkout.subscribe((state) => {
  $('pay').hidden = state.phase !== 'idle' && state.phase !== 'error';
  $('invoice').hidden = state.invoice === null;
  if (!state.invoice) {
    $('status').textContent = progressMessage(state) ?? state.error?.message ?? '';
    return;
  }
  const v = invoiceView(state.invoice, state.now);
  $('status').textContent = v.statusLabel;
  $('address').textContent = v.address; // always textContent: never put service data in innerHTML
  $('expiry').textContent = v.expiry;
  $('qr-link').setAttribute('href', v.bip21);
  if (v.qr) {
    $('qr').setAttribute('viewBox', `0 0 ${v.qr.size} ${v.qr.size}`);
    $('qr').querySelector('path')!.setAttribute('d', v.qr.path);
  }
});

$('pay').addEventListener('click', () => checkout.create({ amount_sat: 5000, memo: 'Order 42' }));
// When the view goes away: checkout.destroy();
```

The address, amount and payment URI are validated before they reach `subscribe`: the QR
code always pays exactly the address and amount shown.

## The checkout

| Call | Does |
| --- | --- |
| `createCheckout(client, options)` | A checkout. Options: `pollMs`, `onCreated`, `onDetected`, `onPaid`, `onError` |
| `checkout.create(input)` | Create an invoice (`amount_sat`, optional `memo`, `expiry_secs`) and follow it |
| `checkout.watch(id)` | Follow an invoice that already exists, such as one [your backend](./integrate-backend.md) created |
| `checkout.reset()` | Forget the invoice and go back to `idle` |
| `checkout.destroy()` | Stop polling for good |
| `checkout.subscribe(fn)` | Called at once and on every change; returns an unsubscribe function |
| `checkout.getState()` | The current state: `phase`, `invoice`, `error`, `now` |

`phase` is one of `idle`, `creating` (includes the proof of work), `loading`, `pending`,
`detected`, `confirmed`, `expired` or `error`. An expired invoice is still polled, because a
late payment can still arrive.

## Rendering helpers

| Helper | Gives |
| --- | --- |
| `invoiceView(invoice, now)` | Status label, amounts in BTC and sats, memo, address, `bip21`, QR `path` and `size`, countdown, received summary, `payable` |
| `progressMessage(state)` | *Preparing invoice…* and similar, before an invoice exists |
| `LABELS` | The English texts, to replace for another language |
| `formatBtc`, `formatSats`, `parseSats` | Amounts in and out |

## In other frameworks

`subscribe` follows the Svelte store contract, so a checkout plugs into most reactive
systems:

```ts
// Vue 3
const state = shallowRef(checkout.getState());
onUnmounted(checkout.subscribe((s) => (state.value = s)));

// Preact (or React without @deadhop/components)
const state = useSyncExternalStore(checkout.subscribe, checkout.getState);

// Angular
state = signal(checkout.getState());
constructor() { inject(DestroyRef).onDestroy(checkout.subscribe((s) => this.state.set(s))); }
```

## Page security

Don't put service data into `innerHTML`; use `textContent` and attributes as above. The
memo is written by whoever created the invoice, possibly a stranger. Your page needs
`connect-src wss:` if it has a Content-Security-Policy; see
[Styling and page security](./integrate.md#styling-and-page-security).
