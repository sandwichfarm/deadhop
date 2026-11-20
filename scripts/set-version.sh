#!/usr/bin/env bash
# Sets one version on every published package (npm and JSR manifests) and on
# the framework packages' dependency on @deadhop/client, which is pinned
# exactly. Then refreshes the lockfile.
#   scripts/set-version.sh 0.2.0
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
V="${1:?usage: set-version.sh VERSION}"
[[ "$V" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "not a version: $V" >&2; exit 2; }
node - "$ROOT" "$V" <<'JS'
const fs = require('fs');
const [root, v] = process.argv.slice(2);
const edit = (file, fn) => {
  const path = `${root}/${file}`;
  if (!fs.existsSync(path)) return;
  const json = JSON.parse(fs.readFileSync(path, 'utf8'));
  fn(json);
  fs.writeFileSync(path, JSON.stringify(json, null, 2) + '\n');
};
for (const p of ['client', 'react', 'solid', 'svelte']) {
  for (const f of ['package.json', 'deno.json']) edit(`packages/${p}/${f}`, (j) => { j.version = v; });
  edit(`packages/${p}/package.json`, (j) => { if (j.dependencies?.['@deadhop/client']) j.dependencies['@deadhop/client'] = v; });
}
edit('packages/e2e/package.json', (j) => {
  for (const d of Object.keys(j.devDependencies)) if (d.startsWith('@deadhop/')) j.devDependencies[d] = v;
});
edit('dashboard/package.json', (j) => { if (j.dependencies?.['@deadhop/client']) j.dependencies['@deadhop/client'] = v; });
JS
(cd "$ROOT" && npm install --silent --no-audit --no-fund >/dev/null)
echo "all packages at $V; commit, then tag v$V to publish"
