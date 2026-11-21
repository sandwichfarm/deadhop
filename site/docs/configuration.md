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
| `DEADHOP_PROXY` | none | A proxy for relay connections only: `socks5://`, `https://` or `http://`, optionally `user:pass@`. The first entry of the proxy list |
| `DEADHOP_PROXIES` | none | More proxies, comma-separated, after `DEADHOP_PROXY` (at most 8 in all) |
| `DEADHOP_PROXY_MODE` | `failover` | How the list is used: `failover`, `round_robin` or `chaos` |
| `DEADHOP_PROXY_ROTATE_SECS` | `3600` | Round robin: seconds between changes (60 to 604800) |
| `DEADHOP_PROXY_CHAOS_MIN_SECS` / `DEADHOP_PROXY_CHAOS_MAX_SECS` | `300` / `900` | Chaos: shortest and longest time before the next change |
| `DEADHOP_PROXY_OFFLINE` | `kill_switch` | When no proxy is reachable: `kill_switch` (disconnect from every relay), `admin_only` (connect straight, serve operators only), `invoicing_only` (connect straight, serve the public only) or `open` (connect straight, carry on) |
| `DEADHOP_TRANSPARENT_PROXY` | `false` | You run a transparent proxy around this service: nothing is checked or rotated, the proxy list is not used and the privacy audit says it cannot see the path |
| `DEADHOP_PROXY_LOCKED` | `false` | Refuse every proxy setting (list, mode, intervals, offline policy, transparent) from the dashboard |
| `DEADHOP_LOG` | `warn,deadhop=info,contextvm_sdk=off,rmcp=off` | Log filter |

At least one of `DEADHOP_ADMINS` or `DEADHOP_PUBLIC_INVOICING=true` is required.
`DEADHOP_ADMINS` accepts hex, `npub` and `nprofile` keys, with or without `nostr:`.

## Changing settings from the dashboard

Operators can change the relays, proxy, proof of work, rate limits, amount and lifetime
limits, confirmations, caps, polling, the late-payment grace and the PayJoin/CoinJoin
policy at runtime, and add or remove operator keys. Changes are stored in the database as
overrides on top of the environment and survive restarts. *Reset to environment* drops
them all.

- A new relay list reconnects the service. If no relay connects within 30
  seconds, the previous one is restored. A changed proxy list is probed at once; if none
  of the new proxies is reachable, the previous list is restored (recorded in the audit
  log).
- If later no relay is connected for 10 minutes, the service drops the relay override
  and goes back to `DEADHOP_RELAYS` by itself (recorded in
  the audit log). The same happens at startup when no saved relay connects within 30
  seconds, so restarting the host is a way back too. Saved relays otherwise take
  precedence over `DEADHOP_RELAYS`.
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

## Proxies

Relay connections (and only those) can go through a list of proxies. In the dashboard
the list is under **Settings**: *Create proxy* adds one, and the order is set with the
drag handle or the up and down arrows. From the environment, `DEADHOP_PROXY` is the first
proxy and `DEADHOP_PROXIES` adds more.

A proxy counts as reachable when a tunnel through it can be opened to at least one live
relay. Every proxy is probed once a minute, and the result is on the Overview and in
`get_health` (`proxy_pool`).

| Mode | Use |
| --- | --- |
| Failover | The first reachable proxy in list order. When a proxy earlier in the list comes back, the service goes back to it only after it has stayed up for 2 minutes. |
| Round robin | The reachable proxies in turn, one change every `DEADHOP_PROXY_ROTATE_SECS`, never per connection. |
| Chaos | A random other reachable proxy after a random time between `DEADHOP_PROXY_CHAOS_MIN_SECS` and `DEADHOP_PROXY_CHAOS_MAX_SECS` (for example 5 to 15 minutes). |

Every change of proxy reconnects the relays, which is why intervals are at least a
minute. Round robin and chaos need two reachable proxies. With only one, rotation is
**suspended** and that proxy keeps carrying traffic until a second one is back.

**When no proxy is reachable**, `DEADHOP_PROXY_OFFLINE` (or the setting in the dashboard)
decides what the service does until one returns. It checks the proxies all the while, so
it recovers by itself.

| Policy | The service |
| --- | --- |
| `kill_switch` (default) | disconnects from every relay: nothing is served and nothing reaches a relay |
| `admin_only` | connects straight (the relays see this machine's IP address) and answers operators only, so the proxies can be repaired from the dashboard, unless they are locked |
| `invoicing_only` | connects straight and answers invoicing only; operators are not served |
| `open` | connects straight and carries on as usual |

**Transparent proxy.** If you have set up a transparent proxy yourself (the host's
routing, a Tor gateway, a VPN), tell the service with `DEADHOP_TRANSPARENT_PROXY=true`
or the toggle in Settings. It then does not probe or use any proxy, and the privacy audit
reports that it cannot check the path. There is no check on purpose: there are too many
ways to build one, and nothing in this process could tell them apart. The list stays
saved but inactive. A wrong claim leaks the address, so `DEADHOP_PROXY_LOCKED` covers it.
