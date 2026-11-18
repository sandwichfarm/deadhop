#!/bin/sh
# Container entrypoint. Publishes the service's public key (never the secret
# key) to /public for the dashboard, then runs the service as PID 1 so that
# SIGTERM reaches it directly.
set -eu
if [ "$#" -gt 0 ]; then exec cvmpay "$@"; fi
# server.pubkey in the data directory is rewritten once the service is serving;
# the health check looks for it, so drop the copy from the previous run.
rm -f "${CVMPAY_DATA_DIR}/server.pubkey"
if [ -d /public ] && [ -w /public ]; then
  cvmpay pubkey > /public/server.pubkey.new
  mv /public/server.pubkey.new /public/server.pubkey
fi
exec cvmpay
