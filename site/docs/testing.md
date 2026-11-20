---
title: Testing
description: How deadhop is tested. Unit and integration tests at enforced 100% coverage, end-to-end payments on regtest with a real browser, and linting.
---

# Testing

```sh
just test      # unit + integration tests, fails below 100% coverage
just e2e      # real payments on regtest: dashboard, built client, every framework's components in Chrome
just check    # rustfmt, clippy (warnings are errors), tsc, svelte-check
just packages # pack, install into fresh React/Solid/Svelte 5/Svelte 4 apps, build, publint, attw, JSR dry run
```

| Suite | Tests | Coverage (enforced) |
| --- | --- | --- |
| Rust service (`cargo llvm-cov`) | 143 | 100% lines, 100% functions |
| `@deadhop/client` (`vitest`, v8) | 72 | 100% statements, branches, functions, lines |
| `@deadhop/react` (`vitest`, v8) | 12, plus 1 server-rendering | 100% statements, branches, functions, lines |
| `@deadhop/solid` (`vitest`, v8) | 13, plus 1 server-rendering | 100% statements, branches, functions, lines |
| `@deadhop/svelte`: Svelte 5 and Svelte 4 syntax (`vitest`, v8) | 27, plus 2 server-rendering | 100% statements, branches, functions, lines |
| `@deadhop/svelte/svelte4` on the real Svelte 4 runtime | 15 | n/a |
| Dashboard (`vitest`, v8) | 98 | 100% statements, branches, functions, lines |
| End to end (regtest) | 21 | n/a |

## What the numbers mean

- The Rust figure is line and function coverage. Region coverage is about 98.9%; the rest is
  the error side of `?` on calls that are exercised but never fail in tests, such as
  individual SQLite column reads.
- Three conditions that cannot occur in practice are `expect`s rather than untestable error
  branches: bech32-encoding a valid key, non-hardened BIP-32 derivation from a valid key
  (fails with probability about 2⁻¹²⁷), and the operating system's random source.
- Unit tests are hermetic: an in-process relay and a scripted Electrum server. The
  end-to-end suite uses real bitcoind, Fulcrum, a real relay and Chrome.
- Proof of work in the browser is tested both ways: notemine mining in Web Workers when the
  page's Content-Security-Policy allows them, and the built-in miner taking over under the
  strict policy. Whatever miner is used, the client checks its result before signing.
- Proxy support is tested against in-process SOCKS5 (with and without a password), HTTP
  CONNECT and HTTPS proxies in front of a real relay. The relay-change revert is tested
  with a relay and a proxy that do not exist.
- The high-severity findings from the security review each have a test that reproduces the
  attack: `survives_a_stray_first_message`, `one_unreadable_invoice_does_not_stall_the_others`
  and `operator_requests_cannot_be_replayed_or_borrowed`.
