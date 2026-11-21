---
title: Security
description: Security analysis of deadhop. Benefits of the design, threat model, findings from an adversarial review and their status, open risks and supply chain.
---

# Security

Analysis dated 2026-11-18, covering the service and dashboard at version 0.1.0. The same
document is `SECURITY.md` in the repository.

## Security benefits

| Benefit | How |
| --- | --- |
| No IP address to attack | The service makes outbound connections only. Behind a VPN or Tor, relays see the exit. |
| End-to-end encryption | NIP-44 with NIP-59 gift wrap. Plaintext requests are refused. |
| No passwords or accounts | Callers are the Nostr key that signed the request. Operators are a list of public keys. |
| Strictly typed interface | Every tool has a JSON schema; unknown fields rejected; SQL parameterised; internal errors hidden. The dashboard validates every response with strict schemas. |
| Proof of work and budgets | NIP-13 work on public invoice creation, bound to caller, service and one use; per-key and global rate limits; open-invoice and daily caps. |
| Replay protection | Every invoice request must be fresh, addressed to this service and unused, operators included. |
| Cannot spend | Watch-only BIP-84 account key. A compromised host cannot move funds already received. |
| Verified payment data | Transactions re-hashed against their ids, duplicates counted once, sums capped, genesis checked on reconnect, failures isolated per invoice. |
| Safe rendering | Server strings rendered as text only; QR built from a module matrix; the `bitcoin:` link re-derived from the validated address and amount; responses bound to requests. |
| Small surface | One Rust binary, no HTTP stack, SQLite with `0600` files in a `0700` directory. |
| Safe remote management | Operator tools are replay-protected (also across restarts), audited, and cannot change the wallet, network, Electrum server or host-configured operators. Relay and proxy changes revert if nothing connects. |
| Tested | 100% line and function coverage in Rust, 100% coverage in the dashboard, end-to-end tests on regtest with a real browser. |

## Summary

The core idea holds up: a payment service that only makes outbound connections to Nostr
relays has no network address to attack, and with a watch-only wallet a full compromise of
the service cannot spend funds. That stays true when the wallet is generated for you: the
recovery phrase is kept by a separate container with no network (see open risk 10).

What the idea does **not** give you for free:

- The attack surface moves from "an HTTP server" to "a message handler anyone can write
  to". Anyone who knows the service's public key can send it requests. Denial of service
  is still possible; it just looks different.
- The service believes its Electrum server completely. Whoever runs that server, or can
  tamper with the connection to it, decides which invoices look paid.
- Proof of work slows abuse down but is cheap for a determined attacker. Public invoicing
  can be degraded by someone willing to spend a little CPU.

No way was found for a non-operator to read operator data, forge an identity, or redirect
a payment. An independent code review found three high-severity problems (two denial of
service, one trust issue). Both denial-of-service problems are fixed and covered by tests
that reproduce the attack; the trust issue is inherent to using an Electrum server and is
mitigated and documented below.

## How this analysis was done

| Activity | Result |
| --- | --- |
| Threat model of the design against its claims | This document |
| Independent adversarial code review (a second reviewer with no access to the author's reasoning), all source files | 11 findings: 3 high, 3 medium, 4 low, 1 informational |
| Reproduction of the high-severity findings before fixing | Stray-first-message kill reproduced in a test; dust-flood stall confirmed by reading and now covered by a test |
| Automated tests | 128 Rust tests and 112 dashboard tests at enforced 100% coverage, plus 11 end-to-end tests on regtest including a real browser |
| Remote-management additions (`manage.rs`, `settings.rs`, `relay.rs`, the restart supervisor, the dashboard's operator views) | Built against the rules in "Remote management" and covered by tests that try to break them (replay across restarts, lockout, bad relays and proxies, wipe without confirmation, oversized text). **Not independently reviewed** the way the original code was |
| Dependency advisories (`cargo audit`, `npm audit`) | One applicable advisory found and fixed (SQLite); none open in shipped code. See "Supply chain" |

Not done: an independent review of the remote-management code added on 2026-11-18, fuzzing, a third-party penetration test, a line-by-line audit of dependencies
(`contextvm-sdk`, `rmcp`, `nostr-sdk`, `rusqlite`, `bitcoin`), or testing on mainnet with
real funds. Treat this as a careful first review, not a certification.

## What is being protected

1. **Funds.** Payments must reach addresses only the merchant can spend from.
2. **Accounting.** An invoice is `confirmed` only if the full amount really is confirmed.
3. **Availability.** Honest customers can create and pay invoices; payments are noticed.
4. **Privacy.** Invoice contents and the service's network location stay private.
5. **Keys.** The service's Nostr key, and the operator's.

## Who might attack, and what they can do

| Attacker | Can | Cannot |
| --- | --- | --- |
| Anonymous Nostr user | Send requests; create invoices if public invoicing is on (with proof of work); spend the public rate budget; send dust to invoice addresses | Read other invoices without their id; use operator tools; mark anything paid |
| Relay operator / observer | See that *someone* talks to the service key, when, how much, and from which IP; drop or delay events; replay ciphertext | Read or alter requests and responses (NIP-44, signed); impersonate either side |
| Electrum server operator, or anyone on the path to it | Make any invoice look paid or unpaid; learn every address issued | Spend funds; learn the xpub from the protocol |
| Whoever serves the dashboard files or `config.json` | Show customers a different address; capture a pasted operator key | Affect payments made outside the dashboard |
| Another local user on the server host | Nothing by default (data directory is `0700`, files `0600`) | Read the database or key |
| Someone who fully compromises the service host | Read all invoice history; steal the service's Nostr key; issue invoices to their own addresses from then on | Spend funds already received |

## The design claims, one by one

**"Attackers have no IP or obvious attack surface."** Mostly true. There is no listening
socket and nothing to port-scan. The surface that remains is the request handler, reachable
by anyone who knows the public key. That handler takes nineteen typed tools (fourteen of them operator-only); every parameter
struct rejects unknown fields, every SQL statement is parameterised, and internal error
text never reaches callers.

**"Relays act as an implicit proxy."** True, with two caveats. A relay learns the service's
IP address unless the service connects through a VPN or Tor. And a relay can deny service
by dropping events, so use more than one and prefer relays you control.

**"A VPN makes locating the service hard for most attackers."** Reasonable. The relay sees
the VPN exit. A party that can observe both the VPN provider's traffic and the relay's can
correlate timing; that is the nation-state case the hypothesis already concedes. Note that
the Electrum connection should stay on the local machine or a private network regardless.

**"Strictly typed, validated responses greatly reduce injection."** Confirmed in both
directions. The dashboard validates every response against strict schemas, renders
server-supplied strings only as text, builds the QR code from a module matrix rather than
injected markup, and re-derives the `bitcoin:` link from the validated address and amount,
so it cannot become a different scheme or carry a different address. It also checks that an
answer is about the invoice and amount that were asked for.

**"Proof of work slows DDoS and sybil attacks."** Partly. It works as designed for what it
is attached to (creating invoices), and it is bound to the caller's key, to this service
and to a single use. Two limits:

- It does not protect the layer below. Before the service can check any proof of work it
  has to download and decrypt the event. Flooding protection for that layer belongs on the
  relay (rate limits, NIP-42 authentication, NIP-13 minimum difficulty), which is another
  reason to run your own.
- At the default 20 bits a request costs about a second in a browser and a few
  milliseconds on native hardware. It is a speed bump, not a wall. See "Open risks".

## Findings and their status

Severity is the reviewer's. "Fixed" means changed in code and covered by a test.

### High

| # | Finding | Status |
| --- | --- | --- |
| 1 | **One anonymous message could silence the service.** The MCP library completes its handshake on the first message it receives and gives up permanently if that is not a well-formed `initialize`. A single stray notification sent after each restart stopped all request handling while the process stayed up. | **Fixed.** The handshake is now completed locally before any network message is read (`handshake.rs`), so remote peers cannot take part in it. The request loop is also supervised: if it ever ends, the process exits non-zero for its supervisor to restart. Test: `survives_a_stray_first_message` reproduced the failure before the fix. |
| 2 | **A payer could stall all payment detection.** More than 200 transactions to one invoice address made every poll abort, so newer invoices never updated, and nothing expired. Cost: about 200 dust outputs. | **Fixed.** Failures are isolated per invoice. An unreadable invoice keeps its last known amounts, is counted in `watch_errors` (shown to the operator), and still expires; everything else proceeds. A down backend no longer blocks expiry either. Test: `one_unreadable_invoice_does_not_stall_the_others`. |
| 3 | **The Electrum server is fully trusted, over plaintext.** It can report a fabricated transaction and height and the invoice becomes `confirmed`. The service checks that a transaction hashes to its txid, but not that it is in a block. | **Mitigated, not eliminated.** The genesis hash is now re-checked on every reconnection, duplicate history entries are counted once, sums saturate and are capped at the bitcoin supply, and the service warns at startup when the Electrum server is not on the local machine. The link can now be TLS with a pinned certificate (`tls://` plus `DEADHOP_ELECTRUM_CERT_SHA256`), so nobody on the network path can read or alter it; the pin is checked on every reconnection and the server must prove it holds the certificate's key. Trust in whoever runs the server remains: see "Open risks". |

### Medium

| # | Finding | Status |
| --- | --- | --- |
| 4 | **Public invoicing could be exhausted cheaply**: the per-minute budget, the open-invoice slots, and (unboundedly) address indices. | **Partly fixed.** New daily cap on invoices (`DEADHOP_MAX_INVOICES_PER_DAY`, default 2000) bounds address-index growth; the public can no longer hold a slot longer than the default expiry; a rate-limited request no longer wastes its proof of work. A funded attacker can still deny *public* invoicing. See "Open risks". |
| 5 | **Operator requests had no replay or audience protection.** Operators skipped the proof-of-work check, which was also the only freshness and replay check. A signed operator `create_invoice` captured anywhere could be re-wrapped and replayed without limit. | **Fixed.** Every invoice creation, operator or not, must carry a request event that is addressed to this service (`p` tag), of the ContextVM kind, fresh, and never seen before. Operators are exempt only from the work. Test: `operator_requests_cannot_be_replayed_or_borrowed`. |
| 6 | **Unauthenticated callers could impose unbounded work**: 100 MiB oversized-transfer reassembly was enabled, read-only tools were unthrottled, and junk events flooded the log. | **Fixed where it is ours.** Oversized transfer is disabled; every tool call by the public draws on a request budget (`DEADHOP_RATE_REQUESTS_PER_MIN`, default 1200, and 120 per key); the transport and MCP libraries' logging is off by default, since both write a line for every junk event or refused request. Remaining: a per-client set inside the transport library grows by one entry per distinct key (upstream). |

### Low and informational

| # | Finding | Status |
| --- | --- | --- |
| 7 | `confirmed` is final and the default is one confirmation, so a one-block reorg is not noticed. | **Accepted, documented.** Raise `DEADHOP_MIN_CONFIRMATIONS` for larger amounts. |
| 8 | Invoice ids (bearer capabilities) were written to the log. | **Fixed.** Logs carry the derivation index only. |
| 9 | Database and journal files were world-readable inside a private directory; key file checks were by path. | **Fixed.** Database is created `0600` (and tightened if found looser), the directory is created `0700`, the key's permissions are read from the open handle, and the key is synced to disk. |
| 10a | The dashboard could be framed (clickjacking). | **Fixed in the page, header still recommended.** The app refuses to start inside a frame. Also send `Content-Security-Policy: frame-ancestors 'none'` as an HTTP header; a `<meta>` policy cannot express it. |
| 10b | A stranger's memo is shown on the merchant's page and could be used for phishing; invisible and direction-changing characters were allowed. | **Fixed.** Those characters are rejected, and the page labels the text "Note from the invoice creator". |
| 10c | In-browser mining had no upper bound. | **Fixed.** Refuses above 28 bits and stops after 25 seconds. |
| 10d | Responses were not bound to requests. | **Fixed.** Amount and id are compared with what was asked. |
| 10e | `config.json` is a single point of trust. | **Accepted, documented.** The page now shows the service key so it can be compared. See "Open risks". |
| 11 | The development stack is unsafe if copied to production. | **Documented** in `deploy/docker-compose.yml`; dev scripts now use a private umask and validate their arguments. |

### Red-team review (GitHub issues)

Findings filed as issues against the service, after the review above.

| Issue | Finding | Status |
| --- | --- | --- |
| [#6](https://github.com/sandwichfarm/deadhop/issues/6) (medium) | **Relays that died after the 30-second check left the service unreachable for good.** Relays were checked only once per change. If they connected and then died (for example a stolen operator key pointing the service at relays it later shuts down), the process stayed up but deaf. Every management tool travels over those relays, and a restart re-applied the saved list. | **Fixed.** A watchdog reverts to the environment's relays and proxy after 10 minutes with no relay connected, and at startup saved relays that do not connect within 30 seconds give way to the environment's. Both are recorded in the audit log. Tests: `relays_that_die_after_the_check_are_replaced_by_the_environments`, `saved_relays_that_are_dead_at_startup_give_way_to_the_environments`, `dead_environment_relays_are_only_retried`, `a_fallback_that_cannot_be_recorded_stops_the_process`. |
| [#5](https://github.com/sandwichfarm/deadhop/issues/5) (medium) | **A hostile Electrum server could exhaust memory.** Only single reply lines were capped (4 MiB): up to 200 transaction replies were buffered at once, and every fetched transaction was kept for the whole poll, gigabytes in the worst case. With a supervisor restarting the process, the outage repeated every poll. | **Fixed.** A transaction reply may be at most 2 MiB (any standard transaction fits), transactions are fetched 16 at a time, only each transaction's value to the watched addresses is kept, and all replies of one poll share a 64 MiB budget. Over the budget, the remaining invoices are counted as unreadable (`watch_errors`) for that poll instead of the process growing. Test: `a_hostile_server_cannot_make_a_poll_buffer_unbounded_data`. |
| [#8](https://github.com/sandwichfarm/deadhop/issues/8) (low) | **The last dashboard operator could remove themselves while public invoicing was on.** With no operators in `DEADHOP_ADMINS`, nobody could then stop invoicing, change settings or add an operator; recovery meant editing SQLite on the host. | **Fixed.** Without host operators, the last operator can never be removed remotely; add another operator first. Test: `the_last_operator_cannot_lock_everyone_out`. |
| [#9](https://github.com/sandwichfarm/deadhop/issues/9) (low) | **A hostile Electrum server controlled text in the service's logs.** Its JSON-RPC error was embedded in chain errors, which are logged on every poll: line separators (U+2028), bidi controls and megabytes of text per invoice reached the log. | **Fixed.** Chain error text is reduced to one printable line of at most 200 characters before it is stored or logged. Test: `transport_failures_and_reconnect`. |
| [#7](https://github.com/sandwichfarm/deadhop/issues/7) (low) | **The mainnet rule "TLS relays only, unless on this machine" could be bypassed with userinfo.** `ws://127.0.0.1:80@evil.example` looked local to a string check on the raw authority, but connects to `evil.example` over plaintext. | **Fixed.** Relay URLs are parsed and the rule is decided on the parsed host; URLs with a user name or password are refused on every network. Test: `relay_lists_are_checked`. |

A problem found by testing rather than review: the development script picked the wrong
account key from the wallet (BIP-49 instead of BIP-84). The service derived valid addresses
the wallet did not recognise. It was caught by the end-to-end test that checks the merchant
wallet's balance. The service now logs its first receive address at startup so an operator
can compare it with their wallet.

## Open risks you are accepting

These are not bugs to fix later so much as properties of the design. Decide on each.

1. **Your Electrum server can lie to you.** Run your own (Fulcrum or electrs next to your
   node). Keep the connection on the local machine, or use TLS with a pinned certificate
   when it crosses a network. `deadhop fingerprint host:port` reads the fingerprint, but
   trusts whatever answers at that moment, so compare it with the certificate on the node. Do not point a
   production service at a public Electrum server. Removing this trust means verifying
   merkle proofs against a validated header chain, which is not built.

2. **Public invoicing can be denied by a modest attacker.** Thirty invoices a minute costs
   an attacker well under 1 MH/s. Operator invoicing is unaffected, so the robust setup for
   a shop is: public invoicing **off**, and the shop's back end holds an operator key. If
   you need public invoicing, raise `DEADHOP_POW_BITS` (each extra bit doubles the cost for
   attackers and customers alike) and expect it to be degradable.

3. **Address gap.** Every invoice uses the next address, paid or not. Wallets only look a
   limited number of unused addresses ahead (often 20). With many unpaid invoices, a real
   payment can land beyond that and not show in your wallet until you raise its gap limit.
   The funds are safe; they are just not displayed. Raise the gap limit in your wallet, and
   keep `DEADHOP_MAX_INVOICES_PER_DAY` as low as your business allows.

4. **Unconfirmed payments can vanish.** `detected` means seen, not paid. Release goods on
   `confirmed`, and use more than one confirmation for amounts worth a miner's effort.

5. **Whoever serves the dashboard is trusted by its users.** A tampered page or
   `config.json` can show a different address. Host it yourself, send security headers, and
   publish the service's `npub` somewhere customers can compare. Operators should sign in
   with a browser extension and a key made only for this service.

6. **A compromised service host can redirect future payments** by issuing invoices to
   addresses of the attacker's choosing, and can read invoice history. It cannot move funds
   already received. Nothing in the protocol lets a customer verify that an address belongs
   to the merchant; pre-signed address attestations from an offline key would.

7. **Relays see metadata.** Message timing, sizes and client IP addresses are visible to
   relays even though contents are not.

8. **~~Restart forgets the replay cache.~~ Closed.** The ids of used request events
   are now stored in the database until they can no longer be accepted, so a captured
   request cannot be replayed after a restart either. The original note: a request event still inside its 10-minute
   freshness window can be replayed once after a restart. The rate limits still apply.

9. **Database growth is bounded per day but never pruned.** At the default cap that is at
   most 2000 rows a day.

10. **A generated wallet's recovery phrase is on the host.** If you leave `DEADHOP_XPUB`
   unset, the `wallet` service generates the wallet and keeps its 24-word phrase in its
   own volume. That container has no network, and the internet-facing payment service
   only ever sees the public key, so compromising the payment service still cannot spend.
   Someone who takes over the host itself, or copies that volume, can. If the volume is
   lost before you export the phrase, the funds are lost with it. The phrase is not in
   database backups. Export it once (`deadhop wallet export`) and keep it offline, keep
   the balance small and sweep it to your own wallet regularly. For larger amounts, set
   `DEADHOP_XPUB` to a wallet whose keys never touch the host.

## Remote management

Operators can now run the service from the dashboard: health and payment statistics,
stop/start of invoicing, an in-process restart of the relay connections, runtime
settings (relays, proof of work, limits, a proxy for relay traffic, a PayJoin/CoinJoin
policy), operator keys, database backups and wiping. Fourteen operator-only tools carry
this, including a live proxy check and a privacy audit. That is new attack surface, so it was built with these rules:

| Rule | How |
| --- | --- |
| Nothing that decides where money goes is remote | The xpub, network, Electrum server, data directory and the operator keys in `DEADHOP_ADMINS` can only be changed on the host. The PayJoin/CoinJoin policy has no destination field and is never executed by this service. |
| Every change is a one-time, fresh, addressed request | Each state-changing tool needs a signed request event addressed to this service, inside the freshness window, and never used before. Used ids are stored in SQLite, so the check survives restarts. |
| Every change is recorded | An audit log (who, what, when) is kept in the database and survives wipes. Proxy passwords are masked in it and in every view. |
| Owners cannot be locked out | Operators from the environment cannot be removed remotely. When there are none, the last dashboard operator cannot be removed either: a service without an operator could never be managed again. Public invoicing cannot be turned off when no operator exists. |
| The service cannot be cut off by a bad relay or proxy | A new relay list is tried; if no relay connects within 30 seconds the previous one is restored (and logged). A new proxy list is probed at once, and if none of the new proxies is reachable the previous list is restored (and logged). If even that fails, the process exits for its supervisor. Afterwards, if no relay is connected for 10 minutes, the service goes back to the relays from the environment by itself, and it does the same at startup when no saved relay connects: every remote fix would travel over the dead relays. |
| "Stop" never loses a payment | Stop refuses new invoices only. The chain is still watched, so a payment during a stop is credited. The process itself cannot be stopped remotely: nothing could start it again. |
| Wiping is deliberate and recoverable | It needs the typed confirmation `WIPE`, refuses while customers may still pay open invoices unless forced, always takes a backup first, and keeps the address high-water mark, so no address is ever handed out twice. |
| Settings are validated with the same bounds as the environment | One table of bounds is shared by both paths. Stored settings that no longer fit (for example after the environment changed) are ignored with a warning instead of being half-applied. |

**What a stolen operator key can now do** (beyond reading invoices and totals):

- Weaken abuse protection (lower proof of work, raise rate limits) or stop invoicing.
- Point the service at other relays. The worst case is availability and metadata:
  requests stay end-to-end encrypted and the key still decides who may operate.
- **Set a proxy it controls and so learn the service's real IP address.** This defeats
  the location-hiding goal. If location privacy matters, set `DEADHOP_PROXY` on the host
  and `DEADHOP_PROXY_LOCKED=true`, which refuses every proxy setting from the dashboard.
  Three other proxy settings reach the same goal and are therefore under the same lock:
  claiming a transparent proxy (nothing is checked), the offline policy (a policy other
  than the kill switch connects straight when the proxies fail, so a key that can list
  dead proxies could force that), and the mode and intervals.
- Add more operator keys, to keep access after the original key is revoked. Check the
  operator list and the audit log after any suspected compromise; remove the keys on
  the host if needed (dashboard keys are in the `operators` table).
- Wipe the invoice list (a backup is always taken first and stays on the host).
- Test proxies with `check_proxy`, which makes this machine open connections to a proxy
  host and port the key chooses. It is limited like a change: a fresh signed request,
  recorded in the audit log, one at a time and 6 per minute, only towards the live
  relays, and with `DEADHOP_PROXY_LOCKED=true` only for the current proxy.

It still cannot spend funds, change the receiving wallet, change the Electrum server,
read the service's private key, or remove the operators configured on the host.

**Proxy scope.** Only relay connections use a proxy (`socks5://`, `https://` or
`http://`, with optional credentials). Up to 8 can be listed; failover, round robin and
chaos choose among the reachable ones, and what happens when none is reachable is a
setting that defaults to the kill switch: no relay connection at all (see
[configuration](configuration.md)). The other policies connect straight, so they
are an operator's explicit trade of privacy for availability. Passwords stay hidden in
every view: entries have stable ids, so saving a reordered list does not overwrite a
password with its `***` mask. Changes of proxy are not audited one by one (a rotation
every few minutes would fill the log), but every settings change that reorders, adds or
removes one is, and the proxy in use is in `get_health`. SOCKS5 resolves host names at the proxy, so
`.onion` relays work through Tor. An `https://` proxy is verified against the public web
roots. The Electrum connection is always direct, since it should be local.

**Backups stay on the host.** The service has no web server and large transfers are
disabled, so backups are written to `<data dir>/backups` (`0700`, files `0600`, newest 10
kept). Copy them off the host yourself.

## Supply chain

- Dashboard runtime dependencies: `npm audit --omit=dev` reports **0** vulnerabilities.
- Dashboard development tooling: 8 "high" entries, all one advisory (a regular-expression
  denial of service in `picomatch`, a glob matcher inside the build and test tools). It
  runs only at build time on patterns in this repository and is not part of the shipped
  bundle. The offered "fix" is a major-version downgrade of the toolchain and was not
  applied.
- Rust (`cargo audit`, 229 crates in the release build):
  - **Fixed:** a CVE in the bundled SQLite (RUSTSEC-2026-0459). `rusqlite` was upgraded
    from 0.32 to 0.43, which carries the patched SQLite.
  - **Not applicable:** RUSTSEC-2026-0462, unbounded buffering in `rmcp`'s Streamable HTTP
    client. That feature is not compiled in (verified: no HTTP client crate is in the
    dependency tree). The `rmcp` version is fixed by `contextvm-sdk` 0.2.2 and cannot be
    raised independently.
  - **Warnings, no fix available to this project:** `lru` 0.12.5 is flagged unsound
    (RUSTSEC-2026-0002, RUSTSEC-2026-0253) and is used by `contextvm-sdk` for its session
    and de-duplication caches; `nostr-relay-pool`, `chrono` and `instant` are flagged
    unmaintained and come in through `nostr-sdk` and `rmcp`. The `lru` issues need a panic
    inside a cache operation or a specific iterator misuse to matter; no path to either
    was identified, but this was not proven. These clear when `contextvm-sdk` moves to
    newer releases of its dependencies, which is worth tracking.
  - `nostr-relay-builder` (unmaintained) is a test-only dependency.
- Website (`site/`): `npm audit --omit=dev` reports **0**; the published site is static
  files, and the GSAP and three.js bundles are its only runtime code. The build tools (Astro,
  VitePress and their Vite, esbuild and `picomatch`) carry 11 advisories, all about their
  development servers or build-time globbing. Run `astro dev` / `vitepress dev` on loopback
  only; nothing from them is deployed.
- Docker images in the development stack are unpinned (`latest`); they are not part of a
  production deployment.

See the [deployment checklist](./deployment.md#checklist).
