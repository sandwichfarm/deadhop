#!/usr/bin/env bash
# Pay a regtest invoice from the miner wallet: dev-pay.sh <address> <sats> [blocks-to-mine]
set -euo pipefail
source "$(dirname "$0")/lib.sh"
address="$1"; sats="$2"; blocks="${3:-0}"
[[ "$sats" =~ ^[0-9]+$ && "$blocks" =~ ^[0-9]+$ ]] || { echo "usage: dev-pay.sh <address> <sats> [blocks]" >&2; exit 1; }
btc="$(printf '%d.%08d' $((sats / 100000000)) $((sats % 100000000)))"
bcli -rpcwallet=miner sendtoaddress "$address" "$btc"
if [ "$blocks" -gt 0 ]; then
  bcli generatetoaddress "$blocks" "$(bcli -rpcwallet=miner getnewaddress)" >/dev/null
fi
