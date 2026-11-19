---
title: How it works
description: How cvmpay receives requests over Nostr, checks proof of work, derives addresses, detects payments and moves invoices between states.
---

# How it works

## Request path

1. A client (the dashboard, or your own) builds an MCP request and signs it as a Nostr event
   of kind 25910 (ContextVM).
2. The event is NIP-44 encrypted to the service's key and gift-wrapped (NIP-59), then
   published to the relays the service listens on.
3. The service, which holds open outbound websocket connections to those relays, receives
   it, unwraps it and identifies the caller by the key that signed the inner event.
4. The service checks admission (see below), runs the tool, and sends back an encrypted,
   gift-wrapped response the same way.

Plaintext requests are refused. The service never accepts an inbound connection.

| Concern | Mechanism |
| --- | --- |
| Reachability | Only outbound websocket connections to relays |
| Confidentiality | NIP-44 encryption with NIP-59 gift wrap on every message |
| Identity | The Nostr key that signed the request; operators are a configured list of keys |
| Typed interface | MCP tools with published JSON schemas (five for invoicing, twelve for operators); unknown fields are rejected |
| Abuse resistance | NIP-13 proof of work for public invoice creation, then rate limits and caps |
| Funds | Watch-only account public key; one fresh native segwit address per invoice |
| Payment detection | Polls an Electrum server per open invoice, re-hashes every transaction, counts confirmations |

## Who can create invoices

You choose with `CVMPAY_PUBLIC_INVOICING`.

**Off (the default): operators only.** An operator is a Nostr public key listed in
`CVMPAY_ADMINS` (hex, `npub` or `nprofile`), or added from the dashboard by another
operator. There is no account or password: the dashboard signs each request with
the operator's key, and the service checks the signature against the list. Use this when you
hand out invoices yourself, or when a shop back end holds an operator key and creates
invoices for customers.

**On: anyone can request one.** The dashboard's **Continue as guest** creates a throwaway key
in the browser. Each public `create_invoice` must carry proof of work
(`CVMPAY_POW_BITS`, default 20) and is subject to per-key and global rate limits, a cap on
open invoices and a daily cap. The public cannot request a lifetime longer than the
default.

In both modes:

- `list_invoices` and `get_stats` are operator-only.
- `get_invoice` works for anyone who holds the invoice's 128-bit random id. The id is the
  capability, so customers see only the invoices they were given.
- Paying never touches the service: the customer sends bitcoin to the address.

## Proof of work

The client mines a NIP-13 `nonce` tag into the request event itself before signing it.
The tag commits to the target difficulty. The service accepts the event only if all of
these hold:

- it is signed by the caller and is of the ContextVM kind;
- it is addressed to this service (`p` tag);
- it is fresh, within `CVMPAY_POW_WINDOW_SECS` (default 600 s);
- it has never been used before;
- it meets the difficulty, both the committed target and the required one.

Operators skip only the difficulty check. The others apply to them too, so a captured
operator request cannot be replayed.

## Addresses

`CVMPAY_XPUB` is a BIP-84 account key (`xpub`/`zpub`, or `tpub`/`vpub` on test networks).
Leave it unset under Compose and the offline `wallet` service generates a 24-word wallet,
keeps the phrase to itself (no network, its own volume) and passes only the account key on;
`cvmpay wallet export` prints the phrase so the funds can be spent from any wallet app.
Invoice *n* gets the P2WPKH address at `0/n` on that account. Index allocation is atomic in
SQLite, so two invoices never share an address.

## Payment detection

Every `CVMPAY_POLL_SECS` (default 10 s) the service asks the Electrum server for the
history of each open invoice's script, plus the chain tip. For each transaction it:

- checks that the raw transaction hashes to the txid it was given;
- counts each transaction once, even if the server lists it twice;
- sums outputs to the invoice address, capped at the bitcoin supply;
- computes confirmations from the transaction's height and the tip.

Each invoice is evaluated on its own. One that cannot be read keeps its last known amounts
and is counted in `watch_errors`, and the others carry on. On every reconnection the
service checks the Electrum server's genesis block against `CVMPAY_NETWORK`.

## Invoice states

| State | Meaning |
| --- | --- |
| `pending` | Waiting for payment |
| `detected` | The full amount has been seen, not yet with enough confirmations |
| `confirmed` | The full amount has at least `CVMPAY_MIN_CONFIRMATIONS` confirmations. Final. |
| `expired` | The lifetime passed before the full amount was seen |

An unconfirmed payment that disappears (replaced or double-spent) moves a `detected`
invoice back. Expired invoices are still watched for `CVMPAY_LATE_GRACE_SECS` (default one
day), so a late payment that covers the amount still moves the invoice to `detected` and then
`confirmed`.

`detected` means *seen*, not *paid*. Release goods only on `confirmed`.
