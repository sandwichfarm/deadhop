# @cvmpay/client

Framework-agnostic client for a [cvmpay](../../README.md) payment service: create
on-chain bitcoin invoices over ContextVM (MCP over Nostr), follow them until they are
paid, and render them. Works in browsers, Node 20+, Deno and Bun.

> **Not published yet.** The name `cvmpay` is not final and the `@cvmpay` npm
> organization and JSR scope don't exist yet. See
> [Integrate payments](../../site/docs/integrate.md#using-the-packages-before-they-are-published)
> for how to use the packages before then.

```sh
npm install @cvmpay/client        # npm
npx jsr add @cvmpay/client        # JSR
```

## Use

```ts
import { createCvmpay, createCheckout } from '@cvmpay/client';

// The service's public key and a relay it listens on (self-hosted or remote).
const cvmpay = createCvmpay({ serverPubkey: 'npub1…', relays: ['wss://relay.example.com'] });

const checkout = createCheckout(cvmpay, {
  onDetected: (invoice) => console.log('seen, confirming', invoice.id),
  onPaid: (invoice) => console.log('confirmed', invoice.id), // verify on your backend before fulfilling
});
checkout.subscribe((state) => console.log(state.phase, state.invoice?.address));
await checkout.create({ amount_sat: 5000, memo: 'Order 42' });
```

On a backend, sign with an operator key to create invoices without proof of work, and
check payments before fulfilling orders:

```ts
import { createCvmpay, secretKeySigner } from '@cvmpay/client';

const cvmpay = createCvmpay({ serverPubkey, relays, signer: secretKeySigner(process.env.CVMPAY_BACKEND_NSEC!) });
const invoice = await cvmpay.createInvoice({ amount_sat: 21000, memo: 'Order 42' });
// …later, before shipping:
const paid = (await cvmpay.getInvoice(invoice.id)).status === 'confirmed';
```

## API

| Export | What it is |
| --- | --- |
| `createCvmpay(options)` | One lazily connected client per service: `info()`, `createInvoice()`, `getInvoice()`, `close()` |
| `createCheckout(client, options)` | Headless checkout: `create()`, `watch(id)`, `reset()`, `destroy()`, `subscribe()`, `getState()` |
| `invoiceView(invoice, now)` | Everything to render an invoice: labels, amounts, QR path, countdown |
| `guestSigner()`, `secretKeySigner(nsec)`, `nip07Signer(window.nostr)` | Who signs requests |
| `PayApi`, `connect()` | Lower level: one connection and the validated tool calls |
| `ApiError` | Every failure, with a stable `code` |
| `qrShape`, `formatBtc`, `formatSats`, `statusLabel`, `parseSats`, `LABELS` | Presentation helpers |
| `@cvmpay/client/schemas` | The strict zod schemas behind the validation |

Every response from the service is validated against a strict schema: unexpected fields,
a payment URI that doesn't match the address and amount, or an answer about another
invoice are rejected, never shown.

Components for React, Solid and Svelte are in
[`@cvmpay/components`](../components/README.md). Full guide:
[Integrate payments into your app](../../site/docs/integrate.md).

## License

MIT
