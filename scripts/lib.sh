# Shared helpers for the local regtest development stack.
# Everything these scripts create is private to the current user.
umask 077
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE="$ROOT/deploy/docker-compose.yml"
DEV_DIR="$ROOT/data/dev"
RELAY_URL="ws://127.0.0.1:7777"
ELECTRUM_ADDR="127.0.0.1:50001"

# Your local settings (gitignored), read on every run: for example
#   CVMPAY_ADMINS=npub1...        your operator key(s), comma-separated
#   DOCKER_CONTEXT=orbstack       which Docker engine runs the stack
if [ -f "$ROOT/.env.local" ]; then
  set -a
  # shellcheck disable=SC1091
  . "$ROOT/.env.local"
  set +a
fi

bcli() {
  docker compose -f "$COMPOSE" exec -T bitcoind \
    bitcoin-cli -regtest -rpcuser=cvmpay -rpcpassword=cvmpay-dev-only "$@"
}
