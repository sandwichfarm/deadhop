#!/usr/bin/env bash
# Start the isolated regtest stack (bitcoind + Fulcrum + private relay) and
# prepare two wallets: "miner" (has coins) and "merchant" (the shop's wallet,
# whose account xpub the payment service watches).
set -euo pipefail
source "$(dirname "$0")/lib.sh"
mkdir -p "$DEV_DIR"
chmod 700 "$ROOT/data" "$DEV_DIR"

docker compose -f "$COMPOSE" up -d

echo "waiting for bitcoind..."
for _ in $(seq 1 60); do bcli getblockchaininfo >/dev/null 2>&1 && break; sleep 1; done

for w in miner merchant; do
  bcli loadwallet "$w" >/dev/null 2>&1 || bcli createwallet "$w" >/dev/null 2>&1 || true
done

if [ "$(bcli getblockcount)" -lt 101 ]; then
  bcli generatetoaddress 101 "$(bcli -rpcwallet=miner getnewaddress)" >/dev/null
fi

# Account-level key of the merchant wallet's native segwit (BIP-84) receive chain.
# Anchored on the start of the descriptor so that sh(wpkh(...)) (BIP-49) is not picked by mistake.
bcli -rpcwallet=merchant listdescriptors \
  | grep -o '"desc": "wpkh(\[[^]]*/84h/1h/0h\]tpub[A-Za-z0-9]*/0/\*' | head -1 | grep -o 'tpub[A-Za-z0-9]*' > "$DEV_DIR/xpub"
[ -s "$DEV_DIR/xpub" ] || { echo "could not read the merchant wallet's BIP-84 account key" >&2; exit 1; }

echo "waiting for the Electrum server..."
for _ in $(seq 1 60); do nc -z 127.0.0.1 50001 >/dev/null 2>&1 && break; sleep 1; done

echo "regtest stack is up: relay $RELAY_URL, electrum $ELECTRUM_ADDR, height $(bcli getblockcount)"
