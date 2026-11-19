---
title: Overview
description: cvmpay accepts on-chain bitcoin payments from a service that has no public address. Clients reach it only through Nostr relays using ContextVM.
---

# cvmpay documentation

cvmpay creates on-chain bitcoin invoices, watches the chain and reports payments, like a
BTCPay-style server does for simple invoicing. It never listens on a port. Customers and
operators reach it only through Nostr relays, using [ContextVM](https://contextvm.org)
(MCP over Nostr), so an attacker has no IP address, HTTP endpoint or login form to aim at.

## What you get

- **A fresh address per invoice**, derived from a watch-only BIP-84 account key. The
  service cannot spend.
- **Payment detection and confirmation tracking** against your own Electrum server.
- **A static Svelte dashboard** for checkout and for operators. Sign in with a Nostr key;
  there are no accounts.
- **Strictly typed tools** over end-to-end encrypted Nostr messages: five for invoicing,
  twelve for operators (health, statistics, settings, keys, control, backups).

## Components

| Part | Technology | Role |
| --- | --- | --- |
| Service (`server/`) | Rust, one binary (about 8 MB), SQLite | Issues invoices, polls the chain, answers requests |
| Dashboard (`dashboard/`) | Svelte 5 static page | Checkout for customers, overview for operators |
| Chain backend | Fulcrum or electrs | Reports transactions for each invoice address |
| Transport | Any Nostr relays | Carries encrypted requests and responses |

```
 customer / operator           relays                 your machine
 ┌──────────────────┐  wss  ┌────────────┐  wss  ┌──────────────────────────┐
 │ Svelte dashboard ├──────►│ Nostr      │◄──────┤ cvmpay (Rust)            │
 │  Nostr identity  │       │ relay(s)   │       │  outbound only           │
 │  NIP-44 + NIP-13 │       │ ciphertext │       │  watch-only xpub, SQLite │
 └──────────────────┘       └────────────┘       └────────────┬─────────────┘
                                                              │ Electrum protocol
                                                      ┌───────▼────────┐
                                                      │ Fulcrum/electrs│
                                                      └────────────────┘
```

## Where to go next

- [Quick start](./quickstart.md): run everything on regtest and pay an invoice.
- [How it works](./how-it-works.md): data flow, invoice states, who can create invoices.
- [Configuration](./configuration.md) and [Deployment](./deployment.md): run it for real.
- [Integrate into your app](./integrate.md): pay buttons and invoices for React, Solid and Svelte.
- [Tools API](./api.md): invoicing and operator tools, for building your own client.
- [Security](./security.md): threat model, findings and the risks you accept.
