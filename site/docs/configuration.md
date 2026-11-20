---
title: Configuration
description: Every deadhop environment variable, its default and what it controls, plus the startup checks the service performs.
---

# Configuration

All configuration is environment variables. Nothing has a network-reaching default.

| Variable | Default | Meaning |
| --- | --- | --- |
| `DEADHOP_RELAYS` | required | Comma-separated `wss://` relay URLs |
| `DEADHOP_ELECTRUM` | required | Electrum server: `host:port` (plain TCP) or `tls://host:port` |
| `DEADHOP_ELECTRUM_CERT_SHA256` | none | Pin the server's TLS certificate: SHA-256 fingerprint(s), comma-separated |
| `DEADHOP_XPUB` | see below | Your wallet's BIP-84 **account** key (`xpub`/`zpub`, or `tpub`/`vpub` on test networks). Unset: one is generated (Compose) |
| `DEADHOP_XPUB_FILE` | none | Read the account key from this file when `DEADHOP_XPUB` is unset (the wallet service writes it) |
| `DEADHOP_NETWORK` | `bitcoin` | `bitcoin`, `testnet`, `signet` or `regtest` |
| `DEADHOP_ADMINS` | none | Comma-separated operator public keys (hex or `npub`) |
| `DEADHOP_PUBLIC_INVOICING` | `false` | Let non-operators create invoices (with proof of work) |
| `DEADHOP_POW_BITS` | `20` | NIP-13 difficulty for non-operators |
| `DEADHOP_POW_WINDOW_SECS` | `600` | Accepted age and clock skew of a request event |
| `DEADHOP_MIN_CONFIRMATIONS` | `1` | Confirmations before `confirmed` |
| `DEADHOP_DEFAULT_EXPIRY_SECS` | `3600` | Invoice lifetime when none is requested |
| `DEADHOP_MAX_EXPIRY_SECS` | `86400` | Longest lifetime an operator may request |
| `DEADHOP_MIN_AMOUNT_SAT` | `1000` | Smallest invoice |
| `DEADHOP_MAX_AMOUNT_SAT` | `100000000` | Largest invoice |
| `DEADHOP_MAX_OPEN_INVOICES` | `500` | Open invoices before the public is refused |
| `DEADHOP_MAX_INVOICES_PER_DAY` | `2000` | Invoices (address indices) per 24 h before the public is refused |
| `DEADHOP_RATE_GLOBAL_PER_MIN` | `30` | Public invoice creations per minute, all callers |
| `DEADHOP_RATE_PER_KEY_PER_MIN` | `6` | Public invoice creations per minute, per key |
| `DEADHOP_RATE_REQUESTS_PER_MIN` | `1200` | Public tool calls per minute, all callers |
| `DEADHOP_RATE_REQUESTS_PER_KEY_PER_MIN` | `120` | Public tool calls per minute, per key |
| `DEADHOP_POLL_SECS` | `10` | Chain polling interval |
| `DEADHOP_LATE_GRACE_SECS` | `86400` | How long expired invoices are still watched |
| `DEADHOP_DATA_DIR` | `data` | Database and the service's Nostr key (created `0700`) |
| `DEADHOP_ANNOUNCE` | `false` | Publish public ContextVM discovery announcements |
| `DEADHOP_NAME` | `deadhop` | Name shown in the dashboard |
| `DEADHOP_PROXY` | none | Proxy for relay connections only: `socks5://`, `https://` or `http://`, optionally `user:pass@` |
| `DEADHOP_PROXY_LOCKED` | `false` | Refuse proxy changes from the dashboard |
| `DEADHOP_LOG` | `warn,deadhop=info,contextvm_sdk=off,rmcp=off` | Log filter |

At least one of `DEADHOP_ADMINS` or `DEADHOP_PUBLIC_INVOICING=true` is required.
`DEADHOP_ADMINS` accepts hex, `npub` and `nprofile` keys, with or without `nostr:`.

## Changing settings from the dashboard

Operators can change the relays, proxy, proof of work, rate limits, amount and lifetime
limits, confirmations, caps, polling, the late-payment grace and the PayJoin/CoinJoin
policy at runtime, and add or remove operator keys. Changes are stored in the database as
overrides on top of the environment and survive restarts. *Reset to environment* drops
them all.

- A new relay list or proxy reconnects the service. If no relay connects within 30
  seconds, the previous ones are restored.
- Values are checked against the same bounds as the environment.
- If the environment later changes so that stored overrides no longer fit, they are
  ignored and a warning is shown.
- These stay host-only: `DEADHOP_XPUB`, `DEADHOP_NETWORK`, `DEADHOP_ELECTRUM`,
  `DEADHOP_DATA_DIR`, `DEADHOP_ANNOUNCE` and the keys in `DEADHOP_ADMINS`. With
  `DEADHOP_PROXY_LOCKED=true` the proxy is host-only too.

## Electrum over TLS

For an Electrum server that is not on this machine, use its TLS port and pin its
certificate:

```sh
deadhop fingerprint node.lan:50002     # prints the certificate's SHA-256 and the settings below
export DEADHOP_ELECTRUM=tls://node.lan:50002
export DEADHOP_ELECTRUM_CERT_SHA256=<fingerprint printed above>
```

- The command trusts whatever answers, so compare the fingerprint with the certificate on
  the node, for example `openssl x509 -in <fulcrum cert> -noout -fingerprint -sha256`.
- A pinned certificate is accepted whatever its name, issuer or expiry, which suits
  self-signed Fulcrum certificates. The server must still prove it holds the
  certificate's private key, on every connection.
- List two fingerprints, comma-separated, while rotating a certificate.
- Without a pin, `tls://` checks the certificate against the public authorities (and the
  service warns that it is not pinned).
- A certificate that does not match stops the service at startup and fails every
  reconnection.

## Proxy

Only connections to Nostr relays go through the proxy; the Electrum server is always
reached directly.

| Form | Use |
| --- | --- |
| `socks5://127.0.0.1:9050` | Tor or a VPN's SOCKS server; host names are resolved by the proxy, so `.onion` relays work |
| `https://user:pass@proxy.example:443` | An HTTPS proxy (TLS to the proxy, checked against the public web roots, then `CONNECT`) |
| `http://127.0.0.1:3128` | A plain HTTP `CONNECT` proxy on this machine or a private network |

Operators are exempt from proof of work and rate limits. The public cannot request a
lifetime longer than `DEADHOP_DEFAULT_EXPIRY_SECS`.

## Startup checks

- **Identity.** On first start the service creates its Nostr key in
  `<data dir>/server.nsec` (mode `0600`) and writes the public key to
  `<data dir>/server.pubkey`. It refuses to start if the key file is readable by others.
- **Chain.** It refuses to start if the Electrum server's genesis block does not match
  `DEADHOP_NETWORK`.
- **Warnings.** It logs a warning for a remote Electrum server, weak proof of work with
  public invoicing on, and `ws://` relays on mainnet.
- **First address.** It logs the receive address at index 0. Confirm that your wallet shows
  that exact address before taking real payments. A wrong account key (for example BIP-49
  instead of BIP-84) still produces valid addresses, but your wallet will not show payments
  to them.

## Example

```sh
export DEADHOP_RELAYS=wss://relay.one.example,wss://relay.two.example
export DEADHOP_ELECTRUM=127.0.0.1:50001
export DEADHOP_XPUB=zpub...                # BIP-84 account key, public only
export DEADHOP_ADMINS=npub1...             # your operator key
export DEADHOP_MIN_CONFIRMATIONS=2
deadhop
```
