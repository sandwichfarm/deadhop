#!/bin/sh
# Container entrypoint. Publishes the service's public keys (never a secret
# key) to /public for the dashboard, then runs the service as PID 1 so that
# SIGTERM reaches it directly.
set -eu
if [ "$#" -gt 0 ]; then exec deadhop "$@"; fi
# server.pubkey in the data directory is rewritten once the service is serving;
# the health check looks for it, so drop the copy from the previous run.
rm -f "${DEADHOP_DATA_DIR}/server.pubkey" "${DEADHOP_DATA_DIR}/admin.pubkey"
if [ -d /public ] && [ -w /public ]; then
  # The administration key first: the dashboard waits for server.pubkey and
  # reads admin.pubkey right after it.
  deadhop pubkey admin > /public/admin.pubkey.new
  mv /public/admin.pubkey.new /public/admin.pubkey
  deadhop pubkey > /public/server.pubkey.new
  mv /public/server.pubkey.new /public/server.pubkey
fi
exec deadhop
