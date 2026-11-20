# @deadhop/svelte

`PayButton`, `InvoiceForm` and `Invoice` for Svelte 5 (runes, SvelteKit too) and
Svelte 4, for taking on-chain bitcoin payments through a
[deadhop](https://github.com/sandwichfarm/deadhop#readme) service.

| Import | For |
| --- | --- |
| `@deadhop/svelte` | Svelte 5. Also `useCheckout` for your own markup |
| `@deadhop/svelte/svelte4` | Svelte 4, or Svelte 5 in legacy mode. The checkout is a store: `$checkout` |
| `@deadhop/svelte/styles.css` | Optional default styles |

```sh
npm install @deadhop/client @deadhop/svelte
```

npm only: JSR can't host `.svelte` files.

```svelte
<script lang="ts">
  import { createDeadhop, PayButton } from '@deadhop/svelte';
  import '@deadhop/svelte/styles.css';

  const deadhop = createDeadhop({ serverPubkey: 'npub1…', relays: ['wss://relay.example.com'] });
</script>

<PayButton client={deadhop} amount={5000} memo="Thanks!" onPaid={(inv) => console.log(inv.id)} />
```

| Component | Does |
| --- | --- |
| `PayButton` | Creates an invoice for a fixed `amount` when clicked, then shows it until paid |
| `InvoiceForm` | Amount and note fields; creates the invoice and shows it |
| `Invoice` | Shows an existing invoice by `id` (for example one your backend created) |

Events: `onCreated`, `onDetected` (seen, not yet confirmed), `onPaid` (confirmed) and
`onError`. **`onPaid` runs in the payer's browser: fulfil orders only after your backend
has checked the invoice with `getInvoice(id)`.**

The components need no inline styles or scripts (they run under a strict
Content-Security-Policy; allow `wss:` in `connect-src`), show the memo as plain text,
and render on the server without touching browser APIs. `styles.css` is optional: it
uses only `deadhop-` classes and CSS variables you can override.

The client, the other frameworks and the service: [deadhop](https://github.com/sandwichfarm/deadhop#readme).

Guides: [Svelte 5](https://github.com/sandwichfarm/deadhop/blob/main/site/docs/integrate-svelte5.md),
[Svelte 4](https://github.com/sandwichfarm/deadhop/blob/main/site/docs/integrate-svelte4.md), and what every integration shares:
[Integrate payments into your app](https://github.com/sandwichfarm/deadhop/blob/main/site/docs/integrate.md).

## License

MIT
