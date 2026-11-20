---
title: Tools API
description: Reference for the deadhop MCP tools served over ContextVM, for invoicing and for operator management, with inputs, outputs and error codes.
---

# Tools API

deadhop is an MCP server reached over Nostr with [ContextVM](https://contextvm.org).
Any MCP client that speaks the ContextVM transport can call it. Results come back as MCP
`structuredContent` that matches each tool's published output schema.

## Connecting

You need the service's public key (`<data dir>/server.pubkey`) and its relay list. With the
TypeScript SDK:

```ts
import { Client } from '@contextvm/mcp-sdk/client/index.js';
import { EncryptionMode, NostrClientTransport, PrivateKeySigner } from '@contextvm/sdk';

const transport = new NostrClientTransport({
  signer: new PrivateKeySigner(myHexSecretKey),
  relayHandler: ['wss://relay.one.example'],
  serverPubkey: SERVICE_PUBKEY_HEX,
  encryptionMode: EncryptionMode.REQUIRED, // the service refuses plaintext
});
const client = new Client({ name: 'my-shop', version: '1.0.0' });
await client.connect(transport);

const info = await client.callTool({ name: 'get_info', arguments: {} });
```

A non-operator calling `create_invoice` must mine NIP-13 proof of work into the signed
request event: add a `["nonce", "<counter>", "<target bits>"]` tag and vary the counter until
the event id has at least `pow_bits` leading zero bits. The dashboard's `src/lib/pow.ts` is a
working implementation that wraps any signer.

## Tools

All inputs reject unknown fields.

### `get_info`

Anyone. No arguments. Returns the service parameters for this caller.

| Field | Type | Meaning |
| --- | --- | --- |
| `name`, `version` | string | Service name and version |
| `network` | string | `bitcoin`, `testnet`, `signet` or `regtest` |
| `public_invoicing` | boolean | Whether non-operators may create invoices |
| `pow_bits` | integer | Required proof of work for non-operators |
| `pow_window_secs` | integer | Accepted age of a request event |
| `min_confirmations` | integer | Confirmations before `confirmed` |
| `default_expiry_secs` | integer | Lifetime when none is requested |
| `max_expiry_secs` | integer | Longest lifetime this caller may request |
| `min_amount_sat`, `max_amount_sat` | integer | Amount bounds |
| `is_admin` | boolean | Whether the caller is an operator |

### `create_invoice`

Operators, and the public when `public_invoicing` is on (with proof of work).

| Argument | Type | Meaning |
| --- | --- | --- |
| `amount_sat` | integer, required | Amount in satoshis, within the bounds above |
| `memo` | string, optional | Note stored with the invoice, at most 200 characters, no control or invisible characters |
| `expiry_secs` | integer, optional | Lifetime in seconds, from 60 up to `max_expiry_secs` |

Returns an **Invoice**:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | string | 32 lowercase hex characters (128 random bits). Whoever holds it can read the invoice. |
| `address` | string | Fresh native segwit address |
| `amount_sat` | integer | Requested amount |
| `memo` | string | Note, empty if none |
| `status` | string | `pending`, `detected`, `confirmed` or `expired` |
| `received_sat` | integer | Total seen, confirmed or not |
| `confirmed_sat` | integer | Total with enough confirmations |
| `created_at`, `expires_at` | integer | Unix seconds |
| `settled_at` | integer or null | When it became `confirmed` |
| `bip21` | string | `bitcoin:` URI with address and amount |
| `confirmations_required` | integer | Confirmations this invoice needs |

### `get_invoice`

Anyone holding the id. Argument: `id` (32 lowercase hex characters). Returns an Invoice.

### `list_invoices`

Operators only. Newest first.

| Argument | Type | Meaning |
| --- | --- | --- |
| `status` | string, optional | Only invoices in this state |
| `before` | integer, optional | Only derivation indices below this (pagination cursor) |
| `limit` | integer, optional | Page size, 1 to 200, default 50 |

Returns `{ invoices, next_before }`. Each entry is an Invoice plus `index` (derivation index)
and `creator` (hex key that created it). Pass `next_before` as `before` for the next page; it
is absent on the last page.

### `get_stats`

Operators only. No arguments.

| Field | Type | Meaning |
| --- | --- | --- |
| `pending`, `detected`, `confirmed`, `expired` | integer | Invoice counts per state |
| `confirmed_sat`, `detected_sat` | integer | Amounts in those states |
| `next_index` | integer | Next derivation index |
| `tip_height` | integer or null | Last chain tip seen |
| `last_poll_at` | integer or null | Unix seconds of the last poll |
| `chain_ok` | boolean | Whether the last poll reached the chain backend |
| `watch_errors` | integer | Invoices that could not be read in the last poll |

## Management tools

Operators only. Every tool that changes something needs a fresh request event addressed
to this service that was never used before (ids are remembered across restarts) and is
recorded in the audit log.

| Tool | Arguments | Returns |
| --- | --- | --- |
| `get_health` | none | `service` (version, start, uptime, online since, paused, restarts), `relays` (url, status, connected), `proxy` (password hidden), `chain`, `database` (integrity, size, invoices, backups), `warnings` |
| `get_metrics` | none | `addresses_created`, `invoices`, `payments`, `received_sat`, `windows` for `24h`, `7d`, `30d`, `365d` with `available` (false until the service has been online that long), `sweep` status |
| `get_payment_series` | `range`: `day`, `week`, `month` or `year` | `points` of `{ start, count, sat }`: 24 hourly, 7 or 30 daily, or 52 weekly buckets, gap-filled |
| `get_settings` | none | `settings` (live), `defaults` (environment), `overridden` (names), `fixed` (host-only values), `operators`, `restart_pending`, `warnings` |
| `update_settings` | `patch`: only the settings to change; `sweep` may be partial; `proxy: ""` turns the proxy off | Same as `get_settings` |
| `reset_settings` | none | Same as `get_settings` |
| `add_operator` | `pubkey` (hex, `npub` or `nprofile`), optional `label` | Same as `get_settings` |
| `remove_operator` | `pubkey` | Same as `get_settings`; environment keys are refused |
| `control_service` | `action`: `stop` (refuse new invoices), `start`, `restart` (reconnect relays in-process) | `{ paused, restart_pending }` |
| `backup_database` | none | `{ file, size_bytes, created_at }`, written to `<data dir>/backups` |
| `wipe_database` | `confirm: "WIPE"`, optional `include_settings`, `force` | `{ deleted, backup }` |
| `get_audit_log` | optional `limit` (1 to 200) | `entries` of `{ at, actor, action, detail }` |

Settings that can be patched: `name`, `relays`, `public_invoicing`, `pow_bits`,
`pow_window_secs`, `min_confirmations`, `default_expiry_secs`, `max_expiry_secs`,
`min_amount_sat`, `max_amount_sat`, `max_open_invoices`, `max_invoices_per_day`,
`rate_global_per_min`, `rate_per_key_per_min`, `rate_requests_per_min`,
`rate_requests_per_key_per_min`, `poll_secs`, `late_grace_secs`, `proxy` and
`sweep` (`enabled`, `mode` = `payjoin` or `coinjoin`, `threshold_sat`,
`max_fee_rate_sat_vb`, `min_anonymity_set`). Unknown fields are refused.

A request sent while the service is reconnecting is not answered; clients should time
out and retry.

## Errors

Errors are JSON-RPC errors with a stable `data.code`. Internal error text never reaches the
caller.

| JSON-RPC code | `data.code` | Meaning |
| --- | --- | --- |
| -32602 | `invalid_params` | Arguments out of range or malformed |
| -32001 | `unauthorized` | Operator-only tool, or public invoicing is off |
| -32002 | `not_found` | No invoice with that id |
| -32003 | `pow_rejected` | Missing or insufficient work, stale, replayed or not addressed to this service |
| -32004 | `rate_limited` | A rate limit was hit; retry later |
| -32005 | `capacity` | Open-invoice or daily cap reached |
| -32006 | `paused` | Invoicing is stopped by an operator |
| -32603 | `internal` | Something failed on the service side |

Arguments that do not match the input schema at all (wrong types, unknown fields) come back
as a tool result with `isError: true` rather than a JSON-RPC error.
