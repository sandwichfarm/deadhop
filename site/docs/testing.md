---
title: Testing
description: How cvmpay is tested. Unit and integration tests at enforced 100% coverage, end-to-end payments on regtest with a real browser, and linting.
---

# Testing

```sh
just test    # unit + integration tests, fails below 100% coverage
just e2e     # real payment on regtest, including a headless-Chrome run of the built dashboard
just check   # rustfmt, clippy (warnings are errors), svelte-check
```

| Suite | Tests | Coverage (enforced) |
| --- | --- | --- |
| Rust service (`cargo llvm-cov`) | 128 | 100% lines, 100% functions |
| Dashboard (`vitest`, v8) | 112 | 100% statements, branches, functions, lines |
| End to end (regtest) | 11 | n/a |

## What the numbers mean

- The Rust figure is line and function coverage. Region coverage is about 98.9%; the rest is
  the error side of `?` on calls that are exercised but never fail in tests, such as
  individual SQLite column reads.
- Three conditions that cannot occur in practice are `expect`s rather than untestable error
  branches: bech32-encoding a valid key, non-hardened BIP-32 derivation from a valid key
  (fails with probability about 2⁻¹²⁷), and the operating system's random source.
- Unit tests are hermetic: an in-process relay and a scripted Electrum server. The
  end-to-end suite uses real bitcoind, Fulcrum, a real relay and Chrome.
- Proxy support is tested against in-process SOCKS5 (with and without a password), HTTP
  CONNECT and HTTPS proxies in front of a real relay. The relay-change revert is tested
  with a relay and a proxy that do not exist.
- The high-severity findings from the security review each have a test that reproduces the
  attack: `survives_a_stray_first_message`, `one_unreadable_invoice_does_not_stall_the_others`
  and `operator_requests_cannot_be_replayed_or_borrowed`.
