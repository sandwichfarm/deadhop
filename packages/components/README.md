# @deadhop/components

`PayButton`, `InvoiceForm` and `Invoice` for [deadhop](../../README.md), with the same
props and events in every framework:

| Import | Framework |
| --- | --- |
| `@deadhop/components/react` | React 18 and 19 (`'use client'` included) |
| `@deadhop/components/solid` (alias `/solidjs`) | Solid 1.8 and later 1.x |
| `@deadhop/components/svelte5` | Svelte 5 (runes) |
| `@deadhop/components/svelte4` | Svelte 4 (also runs on Svelte 5 in legacy mode) |
| `@deadhop/components/styles.css` | Optional default styles (`deadhop-` classes, CSS variables) |

Each subpath only pulls in its own framework, and unused components are tree-shaken.
Built on [`@deadhop/client`](../client/README.md).

```sh
npm install @deadhop/client @deadhop/components
npx jsr add @deadhop/components   # React and Solid only: JSR cannot host .svelte files
```

## Use

```tsx
import { createDeadhop } from '@deadhop/client';
import { PayButton } from '@deadhop/components/react';
import '@deadhop/components/styles.css';

const deadhop = createDeadhop({ serverPubkey: 'npub1…', relays: ['wss://relay.example.com'] });

<PayButton client={deadhop} amount={5000} memo="Thanks!" onPaid={(inv) => console.log(inv.id)} />;
```

```svelte
<script lang="ts">
  import { Invoice } from '@deadhop/components/svelte5';
  let { deadhop, invoiceId } = $props();
</script>

<Invoice client={deadhop} id={invoiceId} />
```

| Component | Does |
| --- | --- |
| `PayButton` | Creates an invoice for a fixed `amount` when clicked, then shows it until paid |
| `InvoiceForm` | Amount and note fields; creates the invoice and shows it |
| `Invoice` | Shows an existing invoice by `id` (for example one your backend created) |

Events: `onCreated`, `onDetected` (seen, not final), `onPaid` (confirmed) and `onError`.
**`onPaid` runs in the payer's browser: fulfil orders only after your backend has
checked the invoice with `getInvoice(id)`.**

The components need no inline styles or scripts (they run under a strict
Content-Security-Policy; allow `wss:` in `connect-src`), show the memo as plain text,
and render on the server without touching browser APIs.

Full guide, with the service settings each setup needs:
[Integrate payments into your app](../../site/docs/integrate.md), then the guide for
[React](../../site/docs/integrate-react.md), [Solid](../../site/docs/integrate-solid.md),
[Svelte 5](../../site/docs/integrate-svelte5.md) or [Svelte 4](../../site/docs/integrate-svelte4.md).

## License

MIT
