# @cvmpay/components

`PayButton`, `InvoiceForm` and `Invoice` for [cvmpay](../../README.md), with the same
props and events in every framework:

| Import | Framework |
| --- | --- |
| `@cvmpay/components/react` | React 18 and 19 (`'use client'` included) |
| `@cvmpay/components/solid` (alias `/solidjs`) | Solid 1.8 and later 1.x |
| `@cvmpay/components/svelte5` | Svelte 5 (runes) |
| `@cvmpay/components/svelte4` | Svelte 4 (also runs on Svelte 5 in legacy mode) |
| `@cvmpay/components/styles.css` | Optional default styles (`cvmpay-` classes, CSS variables) |

Each subpath only pulls in its own framework, and unused components are tree-shaken.
Built on [`@cvmpay/client`](../client/README.md).

> **Not published yet.** The name `cvmpay` is not final and the `@cvmpay` npm
> organization and JSR scope don't exist yet. See
> [Integrate payments](../../site/docs/integrate.md#using-the-packages-before-they-are-published).

```sh
npm install @cvmpay/client @cvmpay/components
npx jsr add @cvmpay/components   # React and Solid only: JSR cannot host .svelte files
```

## Use

```tsx
import { createCvmpay } from '@cvmpay/client';
import { PayButton } from '@cvmpay/components/react';
import '@cvmpay/components/styles.css';

const cvmpay = createCvmpay({ serverPubkey: 'npub1…', relays: ['wss://relay.example.com'] });

<PayButton client={cvmpay} amount={5000} memo="Thanks!" onPaid={(inv) => console.log(inv.id)} />;
```

```svelte
<script lang="ts">
  import { Invoice } from '@cvmpay/components/svelte5';
  let { cvmpay, invoiceId } = $props();
</script>

<Invoice client={cvmpay} id={invoiceId} />
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
[Integrate payments into your app](../../site/docs/integrate.md).

## License

MIT
