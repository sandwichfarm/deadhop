# @deadhop/solid

`PayButton`, `InvoiceForm`, `Invoice` and the headless `useCheckout` for Solid 1.8 and
later 1.x (SolidStart too), for taking on-chain bitcoin payments through a
[deadhop](https://github.com/sandwichfarm/deadhop#readme) service.

```sh
npm install @deadhop/client @deadhop/solid
npx jsr add @deadhop/client @deadhop/solid
```

```tsx
import { createDeadhop, PayButton } from '@deadhop/solid';
import '@deadhop/solid/styles.css';

const deadhop = createDeadhop({ serverPubkey: 'npub1…', relays: ['wss://relay.example.com'] });

<PayButton client={deadhop} amount={5000} memo="Thanks!" onPaid={(inv) => console.log(inv.id)} />;
```

Apps that compile Solid themselves (for server rendering) get the uncompiled JSX through
the `solid` export condition; everyone else gets a compiled build.

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

Guide: [Solid](https://github.com/sandwichfarm/deadhop/blob/main/site/docs/integrate-solid.md), and what every integration shares:
[Integrate payments into your app](https://github.com/sandwichfarm/deadhop/blob/main/site/docs/integrate.md).

## License

MIT
