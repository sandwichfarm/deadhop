---
title: Quick start
description: Run cvmpay, its dashboard, bitcoind, Fulcrum and a private relay on a local regtest chain, then create and pay an invoice.
---

# Quick start

Run the whole system on a throwaway regtest chain, on loopback only.

## Requirements

- Rust 1.88 or newer
- Node 22 or newer
- Docker
- [`just`](https://github.com/casey/just)

## Start it

```sh
just up          # regtest bitcoind + Fulcrum + private relay, a funded "miner" and a "merchant" wallet
just server      # builds and runs cvmpay; writes dashboard/public/config.json
just dashboard   # serves the dashboard at http://127.0.0.1:4173
```

Run `npm install` in `dashboard/` once before `just dashboard`.

## Pay an invoice

1. Open http://127.0.0.1:4173 and choose **Continue as guest**.
2. Enter an amount and choose **Create invoice**. The page mines proof of work, then shows
   the QR code and address.
3. Pay it from the regtest miner wallet. This also mines one block:

   ```sh
   just pay <address> <sats>
   ```

4. The page moves to *Payment seen* and then *Paid* on its own.
5. Sign in as the operator by pasting the key in `data/dev/operator.nsec` to see the
   operator dashboard. To use your own key instead, add it under **Settings → Operators**,
   or put `CVMPAY_ADMINS=npub1…` in `.env.local` (gitignored; the dev scripts read it on every run). Any other key only
   gets the invoice form.

## Stop it

```sh
just down        # removes the stack and its chain data
```

The development stack uses unpinned images and no authentication. Never use it as a
production deployment; see [Deployment](./deployment.md).
