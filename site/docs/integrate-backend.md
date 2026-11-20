---
title: Backend
description: Create deadhop invoices on your server with an operator key, hand their ids to the page, and check that an invoice is paid before you fulfil the order.
---

# Backend

Your server creates invoices and checks payments with `@deadhop/client`. It runs on
Node 20+, Deno and Bun. Read [Integrate payments into your app](./integrate.md) first.

Use a backend when you set the price, not the customer, when public invoicing is off,
or whenever you ship goods: every shop needs [the check before you fulfil](#check-before-you-fulfil),
even one that creates invoices in the browser.

## Give your backend a key

1. Make a key only for this backend, for example with `nak key generate` or any Nostr
   tool, and keep its `nsec` in your server's secrets.
2. Add its public key to the service's operators: `DEADHOP_ADMINS`, or *Settings →
   Operators* in the dashboard.

Operators create invoices without proof of work. **An operator key can also manage the
service** (settings, relays, wipe): never ship it to a browser.

## Install

```sh
npm install @deadhop/client       # Node, Bun
deno add jsr:@deadhop/client      # Deno
```

## Create invoices

```ts
// deadhop.server.ts
import { createDeadhop, secretKeySigner } from '@deadhop/client';

export const deadhop = createDeadhop({
  serverPubkey: process.env.DEADHOP_SERVER_PUBKEY!,
  relays: process.env.DEADHOP_RELAYS!.split(','),
  signer: secretKeySigner(process.env.DEADHOP_BACKEND_NSEC!),
});

export async function startCheckout(orderId: string, amountSat: number): Promise<string> {
  const invoice = await deadhop.createInvoice({ amount_sat: amountSat, memo: `Order ${orderId}` });
  await db.orders.update(orderId, { invoiceId: invoice.id });
  return invoice.id; // send this to the page
}
```

Create the client once per process. It connects on first use, keeps one connection,
and reconnects after a failure on the next call. `deadhop.close()` closes it on shutdown.

The page then shows the invoice with `Invoice`
([React](./integrate-react.md#an-invoice-your-backend-created),
[Solid](./integrate-solid.md#an-invoice-your-backend-created),
[Svelte 5](./integrate-svelte5.md#an-invoice-your-backend-created),
[Svelte 4](./integrate-svelte4.md#an-invoice-your-backend-created)) or
`checkout.watch(id)` ([plain JavaScript](./integrate-javascript.md)). Anyone with the id
can watch the invoice; no key is needed.

## Check before you fulfil

`onPaid` runs in the customer's browser, which they control. Before you ship, ask the
service yourself:

```ts
export async function isPaid(orderId: string): Promise<boolean> {
  const order = await db.orders.get(orderId);
  const invoice = await deadhop.getInvoice(order.invoiceId);
  return invoice.status === 'confirmed' && invoice.amount_sat === order.amountSat;
}
```

- `detected` means the payment was seen, not that it is final: an unconfirmed payment
  can still disappear. Wait for `confirmed` (`DEADHOP_MIN_CONFIRMATIONS`).
- An `expired` invoice can still become `confirmed`: the service keeps watching for late
  payments (`DEADHOP_LATE_GRACE_SECS`).
- Check the amount, too, if the browser created the invoice.
- There are no webhooks: call `getInvoice` when the page reports `onPaid`, or poll your
  open orders on a timer.

## Errors

Every failure is an `ApiError` with a stable `code`:

```ts
import { ApiError } from '@deadhop/client';

try {
  await deadhop.createInvoice({ amount_sat: 500 });
} catch (e) {
  if (e instanceof ApiError && e.code === 'invalid_params') {
    // below the service's minimum amount, for example
  }
}
```

| Code | Meaning |
| --- | --- |
| `unauthorized` | This key may not do that (is it in `DEADHOP_ADMINS`?) |
| `invalid_params` | The request was refused as invalid (amount out of range, bad id) |
| `not_found` | No such invoice |
| `rate_limited`, `capacity`, `paused` | Try again later, or the operator stopped invoicing |
| `timeout`, `network` | The service did not answer on these relays |
| `bad_response` | The answer failed validation and was discarded |

`deadhop.info()` returns the service's limits (`min_amount_sat`, `max_amount_sat`,
`max_expiry_secs`, …) if you want to check amounts before asking.
