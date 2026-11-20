# @deadhop/client

Framework-agnostic client for a [deadhop](../../README.md) payment service: create
on-chain bitcoin invoices over ContextVM (MCP over Nostr), follow them until they are
paid, and render them. Works in browsers, Node 20+, Deno and Bun.

```sh
npm install @deadhop/client        # npm
npx jsr add @deadhop/client        # JSR
```

## Use

```ts
import { createDeadhop, createCheckout } from '@deadhop/client';

// The service's public key and a relay it listens on (self-hosted or remote).
const deadhop = createDeadhop({ serverPubkey: 'npub1…', relays: ['wss://relay.example.com'] });

const checkout = createCheckout(deadhop, {
  onDetected: (invoice) => console.log('seen, confirming', invoice.id),
  onPaid: (invoice) => console.log('confirmed', invoice.id), // verify on your backend before fulfilling
});
checkout.subscribe((state) => console.log(state.phase, state.invoice?.address));
await checkout.create({ amount_sat: 5000, memo: 'Order 42' });
```

On a backend, sign with an operator key to create invoices without proof of work, and
check payments before fulfilling orders:

```ts
import { createDeadhop, secretKeySigner } from '@deadhop/client';

const deadhop = createDeadhop({ serverPubkey, relays, signer: secretKeySigner(process.env.DEADHOP_BACKEND_NSEC!) });
const invoice = await deadhop.createInvoice({ amount_sat: 21000, memo: 'Order 42' });
// …later, before shipping:
const paid = (await deadhop.getInvoice(invoice.id)).status === 'confirmed';
```

## Faster proof of work

When the service asks for proof of work, the client mines it before sending each new
invoice request. The built-in miner runs on the main thread and is slow on phones. For a
page that creates invoices, [notemine](https://github.com/sandwichfarm/notemine) is much faster: it mines
in WebAssembly across several Web Workers.

```sh
npm install @notemine/wrapper @notemine/core rxjs
```

```ts
import { notemineMiner } from '@deadhop/client/notemine';

const deadhop = createDeadhop({ serverPubkey, relays, miner: notemineMiner() });
```

Its code (about 500 KB) loads only when the first invoice needs work. A page with a
Content-Security-Policy must allow `worker-src blob:` and `script-src 'wasm-unsafe-eval'`.
Where notemine cannot run, the built-in miner takes over. npm only (not on JSR). See
[Faster proof of work](../../site/docs/integrate.md#faster-proof-of-work-notemine).

## API

| Export | What it is |
| --- | --- |
| `createDeadhop(options)` | One lazily connected client per service: `info()`, `createInvoice()`, `getInvoice()`, `close()` |
| `createCheckout(client, options)` | Headless checkout: `create()`, `watch(id)`, `reset()`, `destroy()`, `subscribe()`, `getState()` |
| `invoiceView(invoice, now)` | Everything to render an invoice: labels, amounts, QR path, countdown |
| `guestSigner()`, `secretKeySigner(nsec)`, `nip07Signer(window.nostr)` | Who signs requests |
| `PayApi`, `connect()` | Lower level: one connection and the validated tool calls |
| `ApiError` | Every failure, with a stable `code` |
| `qrShape`, `formatBtc`, `formatSats`, `statusLabel`, `parseSats`, `LABELS` | Presentation helpers |
| `@deadhop/client/schemas` | The strict zod schemas behind the validation |
| `@deadhop/client/notemine` | `notemineMiner()`: proof of work with notemine (npm only, optional peer dependency) |
| `mine`, `checkWork`, `Miner` | The built-in miner, the check every miner's result passes, and the type to write your own |

Every response from the service is validated against a strict schema: unexpected fields,
a payment URI that doesn't match the address and amount, or an answer about another
invoice are rejected, never shown.

Components for React, Solid and Svelte are in
[`@deadhop/components`](../components/README.md). Full guide:
[Integrate payments into your app](../../site/docs/integrate.md), with guides for
[plain JavaScript](../../site/docs/integrate-javascript.md) and your
[backend](../../site/docs/integrate-backend.md).

## License

MIT
