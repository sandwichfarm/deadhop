#!/usr/bin/env bash
# Run the payment service against the local regtest stack. Creates a dev
# operator key on first use and writes the dashboard's config.json.
set -euo pipefail
source "$(dirname "$0")/lib.sh"
[ -s "$DEV_DIR/xpub" ] || { echo "run scripts/dev-up.sh first" >&2; exit 1; }

if [ ! -s "$DEV_DIR/operator.nsec" ]; then
  (cd "$ROOT/dashboard" && node scripts/keygen.mjs "$DEV_DIR/operator")
fi

(cd "$ROOT/server" && cargo build --release)

# The dashboard learns the server's key once the server has written it.
(
  for _ in $(seq 1 100); do [ -s "$DEV_DIR/server/server.pubkey" ] && break; sleep 0.2; done
  printf '{\n  "serverPubkey": "%s",\n  "relays": ["%s"]\n}\n' "$(cat "$DEV_DIR/server/server.pubkey")" "$RELAY_URL" \
    > "$ROOT/dashboard/public/config.json"
) &

export DEADHOP_NAME="${DEADHOP_NAME:-deadhop (regtest)}"
export DEADHOP_RELAYS="$RELAY_URL"
export DEADHOP_ELECTRUM="$ELECTRUM_ADDR"
export DEADHOP_NETWORK=regtest
export DEADHOP_XPUB="$(cat "$DEV_DIR/xpub")"
# Operators: the dev key, plus any keys you set in DEADHOP_ADMINS (hex, npub or nprofile).
export DEADHOP_ADMINS="$(cat "$DEV_DIR/operator.pubkey")${DEADHOP_ADMINS:+,$DEADHOP_ADMINS}"
export DEADHOP_PUBLIC_INVOICING=true
export DEADHOP_POW_BITS="${DEADHOP_POW_BITS:-16}"
export DEADHOP_POLL_SECS="${DEADHOP_POLL_SECS:-2}"
export DEADHOP_DATA_DIR="$DEV_DIR/server"
exec "$ROOT/server/target/release/deadhop"
