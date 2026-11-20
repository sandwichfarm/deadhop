---
title: Design decisions
description: Why deadhop uses Electrum instead of Bitcoin Core RPC, mines proof of work into the request, has no HTTP (so no NIP-98) and is watch-only.
---

# Design decisions

## Electrum, not Bitcoin Core RPC

No node wallet to manage, no HTTP client in the binary, and it works with a node you already
run. The cost is trust in the Electrum server; see [Security](./security.md#open-risks-you-are-accepting).

## Proof of work on the request event itself

The client mines the `nonce` tag into the ContextVM request before signing it, so the work
is bound to the caller's key, to this service and to that one request. It cannot be
reused or transferred.

## No HTTP, therefore no NIP-98

NIP-98 authenticates HTTP requests. This design has none: every request is already a signed
Nostr event. If an HTTP endpoint is ever added (for example a CSV export), it should require
NIP-98.

## Watch-only by construction

The service is given an account public key, never a seed or private key. Anything that needs
to spend (sweeps, PayJoin, CoinJoin) belongs in a separate signer process, so a compromised
payment service still cannot move funds.

## Zero-confirmation is informational

`detected` means seen, not paid. Only `confirmed` should release goods.

## Operators skip work, not checks

Operators are exempt from proof of work and rate limits, but their requests must still be
fresh, addressed to this service and unused. Without that, a captured operator request could
be replayed forever.
