---
title: Deployment
description: Run deadhop in production. Supervise the service, host the static dashboard with security headers, and place it behind a VPN or Tor with your own Electrum server.
---

# Deployment

## 1. The service

1. Build it: `cargo build --release` in `server/` produces `target/release/deadhop`.
2. Configure it with environment variables (see [Configuration](./configuration.md)).
3. Run it as its own user under a supervisor such as systemd with `Restart=always`. If the
   request loop ever stops, the process exits with an error instead of staying up while
   answering nobody.
4. Back up the data directory with its permissions intact. It holds the SQLite database and
   the service's Nostr key.

Example systemd unit:

```ini
[Unit]
Description=deadhop
After=network-online.target
Wants=network-online.target

[Service]
User=deadhop
EnvironmentFile=/etc/deadhop/env
WorkingDirectory=/var/lib/deadhop
ExecStart=/usr/local/bin/deadhop
Restart=always
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/deadhop
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

## 2. The dashboard

```sh
cd dashboard && npm ci && npm run build
```

Serve `dashboard/dist/` from any static host, with a `config.json` next to `index.html`
(see `public/config.example.json`) holding the service's public key and the relay list.

The page ships a strict Content-Security-Policy and makes no requests other than
`config.json` and the relay websockets. It refuses to run inside a frame and shows the
service key it is talking to in its footer, so it can be compared with the one you publish.

Have the web server add these headers (a `<meta>` policy cannot express the first):

```
Content-Security-Policy: frame-ancestors 'none'
X-Content-Type-Options: nosniff
Referrer-Policy: no-referrer
```

## 3. Network placement

- **Hide the service's address.** Run it behind a VPN or Tor so relays see the exit, not
  you: route the whole host, or set `DEADHOP_PROXY` (for example `socks5://127.0.0.1:9050`
  for Tor) for relay connections. Add `DEADHOP_PROXY_LOCKED=true` so that an operator key
  cannot swap in a proxy that reveals your address. The service needs outbound access to
  the relays and the Electrum server only; drop all inbound traffic.
- **Use your own Electrum server.** Fulcrum or electrs on your node, on the same host or a
  private network; across a network, use `tls://` with a pinned certificate (see
  [Configuration](./configuration.md#electrum-over-tls)). A third-party server learns
  every address you issue and decides what you believe was paid.
- **Use more than one relay**, prefer relays you run, and use `wss://` only.

## Checklist

- [ ] Electrum server is your own and reachable only locally or over a private network
- [ ] If it is not on the same machine: `tls://` with a pinned `DEADHOP_ELECTRUM_CERT_SHA256`
- [ ] Either `DEADHOP_XPUB` is your wallet's BIP-84 account key and the first receive
      address logged at startup appears in that wallet, or the wallet was generated and its
      recovery phrase has been exported (`deadhop wallet export`) and stored offline
- [ ] Wallet gap limit raised if public invoicing is on
- [ ] Public invoicing off unless you need it; `DEADHOP_POW_BITS` at 20 or more if on
- [ ] `DEADHOP_MIN_CONFIRMATIONS` chosen for your amounts
- [ ] Service runs as its own user, behind a VPN or Tor, with inbound traffic dropped
- [ ] A supervisor restarts the process if it exits
- [ ] Two or more relays, at least one you control, all `wss://`
- [ ] Data directory backed up with its permissions intact
- [ ] Dashboard served over HTTPS with the headers above
- [ ] Operators use a browser extension and a key made only for this service
- [ ] Someone watches for `chain_ok = false` and `watch_errors > 0` in the operator view
- [ ] `DEADHOP_PROXY` and `DEADHOP_PROXY_LOCKED=true` set on the host if the service's location
      must stay hidden even from someone holding an operator key
- [ ] Backups in `<data dir>/backups` are copied off the host regularly

## Testing against a real Electrum server

A read-only smoke test asks a server for its genesis hash and chain tip. Put the host in the
gitignored `.env.local`:

```sh
# .env.local:  DEADHOP_TEST_ELECTRUM_HOST=...   [DEADHOP_TEST_ELECTRUM_PORT=50001]
just live-electrum
```
