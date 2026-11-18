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

export CVMPAY_NAME="${CVMPAY_NAME:-cvmpay (regtest)}"
export CVMPAY_RELAYS="$RELAY_URL"
export CVMPAY_ELECTRUM="$ELECTRUM_ADDR"
export CVMPAY_NETWORK=regtest
export CVMPAY_XPUB="$(cat "$DEV_DIR/xpub")"
# Operators: the dev key, plus any keys you set in CVMPAY_ADMINS (hex, npub or nprofile).
export CVMPAY_ADMINS="$(cat "$DEV_DIR/operator.pubkey")${CVMPAY_ADMINS:+,$CVMPAY_ADMINS}"
export CVMPAY_PUBLIC_INVOICING=true
export CVMPAY_POW_BITS="${CVMPAY_POW_BITS:-16}"
export CVMPAY_POLL_SECS="${CVMPAY_POLL_SECS:-2}"
export CVMPAY_DATA_DIR="$DEV_DIR/server"
exec "$ROOT/server/target/release/cvmpay"
