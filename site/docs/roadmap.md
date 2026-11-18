---
title: Roadmap
description: What cvmpay does not do yet, including automatic PayJoin or CoinJoin sweeps above a balance threshold, TLS to Electrum, SPV verification and push notifications.
---

# Roadmap

Not built yet:

- **Automatic PayJoin or CoinJoin sweep above a balance threshold.** The policy can already
  be set from the dashboard and is reported as due; nothing carries it out yet. This limits what is lost
  if the service is compromised and breaks the trail to where funds go. It needs spending
  keys, which the service deliberately does not have. The safe shape is a separate signer
  process (or a hardware-backed one) that the service can ask to sweep, so a compromised
  payment service still cannot send funds anywhere it likes.
- **SPV verification**: merkle proofs against a header chain, to reduce trust in the Electrum
  server.
- **Push notifications** of status changes instead of polling, and webhooks for shop back
  ends.
- **Taproot (BIP-86)** receive addresses.
- **Signed address attestations** from an offline key, so customers can verify an address
  belongs to the merchant.
