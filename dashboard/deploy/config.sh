#!/bin/sh
# Writes /tmp/deadhop/config.json for the dashboard: the service's public key
# (published by the deadhop container to /public/server.pubkey, or given as
# DEADHOP_SERVER_PUBKEY), the administration key operators sign in through
# (/public/admin.pubkey, or DEADHOP_ADMIN_PUBKEY) and the relays the page
# connects through (DEADHOP_DASHBOARD_RELAYS, else DEADHOP_RELAYS; the
# administration key's own: DEADHOP_DASHBOARD_ADMIN_RELAYS). Refuses to start
# on bad input.
set -eu
fail() { echo "deadhop-dashboard: $*" >&2; exit 1; }

key="${DEADHOP_SERVER_PUBKEY:-}"
if [ -z "$key" ]; then
  i=0
  while [ ! -s /public/server.pubkey ]; do
    i=$((i + 1))
    [ "$i" -le 120 ] || fail "no /public/server.pubkey after 120 s and DEADHOP_SERVER_PUBKEY is unset"
    sleep 1
  done
  key="$(tr -d ' \r\n' < /public/server.pubkey)"
fi
printf '%s' "$key" | grep -Eq '^([0-9a-f]{64}|npub1[02-9ac-hj-np-z]{58})$' \
  || fail "server public key is not 64 hex characters or an npub"

admin="${DEADHOP_ADMIN_PUBKEY:-}"
if [ -z "$admin" ] && [ -s /public/admin.pubkey ]; then
  admin="$(tr -d ' \r\n' < /public/admin.pubkey)"
fi
if [ -n "$admin" ]; then
  printf '%s' "$admin" | grep -Eq '^([0-9a-f]{64}|npub1[02-9ac-hj-np-z]{58})$' \
    || fail "administration public key is not 64 hex characters or an npub"
fi

# A comma-separated list of relay urls as a JSON array body.
relay_json() {
  out=""
  old_ifs="$IFS"; IFS=','
  for r in $1; do
    r="$(printf '%s' "$r" | tr -d ' \t\r\n')"
    [ -n "$r" ] || continue
    printf '%s' "$r" | grep -Eq '^wss?://[A-Za-z0-9._~:/?#@!$&()*+;=%-]+$' || fail "relay '$r' is not a ws:// or wss:// url"
    case "$r" in *'"'*|*'\'*) fail "relay '$r' contains quotes" ;; esac
    out="${out:+$out,}\"$r\""
  done
  IFS="$old_ifs"
  printf '%s' "$out"
}
json="$(relay_json "${DEADHOP_DASHBOARD_RELAYS:-${DEADHOP_RELAYS:-}}")"
[ -n "$json" ] || fail "set DEADHOP_RELAYS (or DEADHOP_DASHBOARD_RELAYS)"
admin_json="$(relay_json "${DEADHOP_DASHBOARD_ADMIN_RELAYS:-}")"

extra=""
[ -z "$admin" ] || extra="$extra,\"adminPubkey\":\"$admin\""
[ -z "$admin_json" ] || extra="$extra,\"adminRelays\":[$admin_json]"

mkdir -p /tmp/deadhop
printf '{"serverPubkey":"%s","relays":[%s]%s}\n' "$key" "$json" "$extra" > /tmp/deadhop/config.json.new
mv /tmp/deadhop/config.json.new /tmp/deadhop/config.json
echo "deadhop-dashboard: serving for $key"
