# deadhop developer tasks. Run `just` to list them.
default:
    @just --list

# Start the isolated regtest stack (bitcoind, Fulcrum, private relay)
up:
    scripts/dev-up.sh

# Stop the regtest stack and delete its chain data
down:
    docker compose -f deploy/docker-compose.yml down -v

# Run the payment service against the regtest stack
server:
    scripts/dev-server.sh

# Install the JavaScript workspace (packages, dashboard) and the Svelte 4 test project
install:
    npm ci
    npm ci --prefix packages/svelte/test-svelte4

# Build the dashboard and serve the production build at http://127.0.0.1:4173
# (the production build is what the strict Content-Security-Policy is written for)
dashboard:
    cd dashboard && npm run build && npm run preview -- --host 127.0.0.1 --port 4173 --strictPort

# Pay a regtest invoice: just pay <address> <sats> [blocks]
pay address sats blocks="1":
    scripts/dev-pay.sh {{address}} {{sats}} {{blocks}}

# Unit and integration tests with enforced 100% coverage
test:
    cd server && cargo llvm-cov --all-targets --fail-under-lines 100 --fail-under-functions 100
    npm test --workspace packages/client
    npm test --workspace packages/react
    npm test --workspace packages/solid
    npm test --workspace packages/svelte
    npm run test:svelte4 --workspace packages/svelte
    npm test --workspace dashboard

# End-to-end payments on regtest (needs `just up` and `just server` running):
# dashboard, the built client package, and every framework's components in Chrome
e2e:
    npm run e2e --workspace dashboard
    npm run e2e --workspace packages/client
    npm run e2e --workspace packages/e2e

# Lints and type checks
check:
    cd server && cargo fmt --check && cargo clippy --all-targets -- -D warnings
    npm run check --workspace packages/client
    npm run check --workspace packages/react
    npm run check --workspace packages/solid
    npm run check --workspace packages/svelte
    npm run check --workspace dashboard

# Prove the npm packages are distributable: pack, install into fresh React,
# Solid, Svelte 5 and Svelte 4 projects, build, type-check, publint, attw and a
# JSR dry run. Publishes nothing.
packages:
    scripts/check-packages.sh

# Read-only check of a real Electrum server; reads its address from .env.local
live-electrum:
    set -a && . ./.env.local && set +a && cd server && cargo test --test live_electrum -- --ignored --nocapture

# Build the website (Astro landing page + VitePress docs at /docs) and check it.
# Set SITE_URL=https://your.domain (a path is allowed: https://host/deadhop) for a production build.
site:
    cd site && npm run build && npm run check

# Serve the built website at http://127.0.0.1:4330
site-preview:
    cd site && npm run preview
