#!/usr/bin/env bash
# Proves the packages are distributable: packs @deadhop/client and
# @deadhop/components exactly as npm would publish them, installs the tarballs
# into one fresh project per framework (with only that framework installed),
# then builds and type-checks each project, checks tree shaking, and imports
# the client in plain Node. Also runs publint, attw and a JSR dry run.
# Nothing is published.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/deadhop-packages.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
say() { printf '\n== %s\n' "$*"; }

say "build"
npm run build --workspace packages/client >/dev/null
npm run build --workspace packages/components >/dev/null

say "lint package metadata (publint, attw)"
(cd "$ROOT/packages/client" && npx publint && npx attw --pack . --profile esm-only)
(cd "$ROOT/packages/components" && npm run --silent lint:package)

say "JSR dry run"
(cd "$ROOT" && deno publish --dry-run --allow-dirty >/dev/null 2>&1 && echo "jsr: ok")

say "pack"
CLIENT_TGZ="$(cd "$ROOT/packages/client" && npm pack --silent --pack-destination "$WORK")"
COMPONENTS_TGZ="$(cd "$ROOT/packages/components" && npm pack --silent --pack-destination "$WORK")"
CLIENT_TGZ="$WORK/$CLIENT_TGZ"; COMPONENTS_TGZ="$WORK/$COMPONENTS_TGZ"
tar -tzf "$CLIENT_TGZ" | grep -q 'package/dist/mod.js'
if tar -tzf "$CLIENT_TGZ" | grep -E '\.test\.ts$|test-helpers'; then echo "tests leaked into the client tarball" >&2; exit 1; fi
if tar -tzf "$COMPONENTS_TGZ" | grep -E 'test|node_modules'; then echo "tests leaked into the components tarball" >&2; exit 1; fi
echo "client: $(basename "$CLIENT_TGZ"), components: $(basename "$COMPONENTS_TGZ")"

# A fresh project. The client tarball is forced with an override so npm never
# looks for @deadhop/client in the registry (the scope is not ours yet).
project() {
  local dir="$WORK/$1"; shift
  mkdir -p "$dir/src"
  node -e '
    const [dir, client, components, ...deps] = process.argv.slice(1);
    const pkg = { name: "consumer", private: true, type: "module",
      dependencies: { "@deadhop/client": "file:" + client, "@deadhop/components": "file:" + components },
      devDependencies: {}, overrides: { "@deadhop/client": "file:" + client } };
    for (const d of deps) { const i = d.lastIndexOf("@"); pkg.devDependencies[d.slice(0, i)] = d.slice(i + 1); }
    require("fs").writeFileSync(dir + "/package.json", JSON.stringify(pkg, null, 2));
  ' "$dir" "$CLIENT_TGZ" "$COMPONENTS_TGZ" "$@"
  (cd "$dir" && npm install --silent --no-audit --no-fund >/dev/null)
  echo "$dir"
}

cat > "$WORK/index.html" <<'HTML'
<!doctype html><html><body><div id="app"></div><script type="module" src="/src/main"></script></body></html>
HTML

say "React"
D="$(project react react@^19 react-dom@^19 @types/react@^19 @types/react-dom@^19 vite@^8 @vitejs/plugin-react@^5 typescript@^5.9)"
cp "$WORK/index.html" "$D/" && sed -i.bak 's#/src/main#/src/main.tsx#' "$D/index.html"
cat > "$D/src/main.tsx" <<'TS'
import { createRoot } from 'react-dom/client';
import { createDeadhop, PayButton } from '@deadhop/components/react';
import '@deadhop/components/styles.css';
const deadhop = createDeadhop({ serverPubkey: '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798', relays: ['wss://relay.example'] });
createRoot(document.getElementById('app')!).render(<PayButton client={deadhop} amount={5000} memo="Order 42" onPaid={(i) => console.log(i.id)} />);
TS
# Type-checked but not bundled: the notemine subpath must type-check without notemine installed.
cat > "$D/src/miner.ts" <<'TS'
import { notemineMiner } from '@deadhop/client/notemine';
import type { Miner } from '@deadhop/client';
export const miner: Miner = notemineMiner({ workers: 2 });
TS
cat > "$D/tsconfig.json" <<'JSON'
{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler", "jsx": "react-jsx", "strict": true, "noEmit": true, "skipLibCheck": true, "lib": ["ES2022", "DOM"], "types": [] }, "include": ["src"] }
JSON
cat > "$D/vite.config.js" <<'JS'
import react from '@vitejs/plugin-react';
export default { plugins: [react()], build: { minify: false } };
JS
(cd "$D" && npx tsc -p . && npx vite build --logLevel error)
grep -q 'deadhop-pay' "$D"/dist/assets/*.js
! grep -q 'deadhop-form' "$D"/dist/assets/*.js || { echo "InvoiceForm was not tree-shaken" >&2; exit 1; }
! grep -q 'solid-js\|svelte' "$D"/dist/assets/*.js || { echo "another framework leaked into the React bundle" >&2; exit 1; }
! grep -q 'mine_event' "$D"/dist/assets/*.js || { echo "notemine leaked into a bundle that does not use it" >&2; exit 1; }
grep -q 'deadhop-invoice' "$D"/dist/assets/*.css
(cd "$D" && node --input-type=module -e "
  import { renderToString } from 'react-dom/server';
  import { createElement } from 'react';
  import { PayButton, createDeadhop } from '@deadhop/components/react';
  const c = createDeadhop({ serverPubkey: '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798', relays: ['wss://relay.example'] });
  const html = renderToString(createElement(PayButton, { client: c, amount: 5000 }));
  if (!html.includes('Pay 5,000 sats')) throw new Error(html);
  console.log('react: node server render ok');
")
echo "react: build, types, tree shaking ok"

say "Solid"
D="$(project solid solid-js@^1.9 vite@^8 vite-plugin-solid@^2 typescript@^5.9)"
cp "$WORK/index.html" "$D/" && sed -i.bak 's#/src/main#/src/main.tsx#' "$D/index.html"
cat > "$D/src/main.tsx" <<'TS'
import { render } from 'solid-js/web';
import { createDeadhop, InvoiceForm } from '@deadhop/components/solidjs';
const deadhop = createDeadhop({ serverPubkey: '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798', relays: ['wss://relay.example'] });
render(() => <InvoiceForm client={deadhop} defaultAmount={5000} onPaid={(i) => console.log(i.id)} />, document.getElementById('app')!);
TS
cat > "$D/tsconfig.json" <<'JSON'
{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler", "jsx": "preserve", "jsxImportSource": "solid-js", "strict": true, "noEmit": true, "skipLibCheck": true, "lib": ["ES2022", "DOM"], "types": [] }, "include": ["src"] }
JSON
cat > "$D/vite.config.js" <<'JS'
import solid from 'vite-plugin-solid';
export default { plugins: [solid()], build: { minify: false } };
JS
(cd "$D" && npx tsc -p . && npx vite build --logLevel error)
grep -q 'deadhop-form' "$D"/dist/assets/*.js
! grep -q 'deadhop-pay"' "$D"/dist/assets/*.js || { echo "PayButton was not tree-shaken" >&2; exit 1; }
echo "solid: build, types, tree shaking ok"

say "Svelte 5"
D="$(project svelte5 svelte@^5 vite@^8 @sveltejs/vite-plugin-svelte@^7 svelte-check@^4 typescript@^5.9)"
cp "$WORK/index.html" "$D/" && sed -i.bak 's#/src/main#/src/main.ts#' "$D/index.html"
cat > "$D/src/App.svelte" <<'SV'
<script lang="ts">
  import { createDeadhop, Invoice, PayButton } from '@deadhop/components/svelte5';
  const deadhop = createDeadhop({ serverPubkey: '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798', relays: ['wss://relay.example'] });
</script>
<PayButton client={deadhop} amount={5000} onPaid={(i) => console.log(i.id)} />
<Invoice client={deadhop} id="0123456789abcdef0123456789abcdef" />
SV
cat > "$D/src/main.ts" <<'TS'
import { mount } from 'svelte';
import App from './App.svelte';
mount(App, { target: document.getElementById('app')! });
TS
cat > "$D/tsconfig.json" <<'JSON'
{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler", "strict": true, "noEmit": true, "skipLibCheck": true, "verbatimModuleSyntax": true, "lib": ["ES2022", "DOM"], "types": ["svelte"] }, "include": ["src"] }
JSON
cat > "$D/vite.config.js" <<'JS'
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default { plugins: [svelte()], build: { minify: false } };
JS
(cd "$D" && npx svelte-check --tsconfig ./tsconfig.json --threshold error >/dev/null && npx vite build --logLevel error)
grep -q 'deadhop-pay' "$D"/dist/assets/*.js
! grep -q 'deadhop-form' "$D"/dist/assets/*.js || { echo "InvoiceForm was not tree-shaken" >&2; exit 1; }
echo "svelte 5: build, types, tree shaking ok"

say "Svelte 4"
D="$(project svelte4 svelte@^4 vite@^5 @sveltejs/vite-plugin-svelte@^3 svelte-check@^3 typescript@^5.9)"
cp "$WORK/index.html" "$D/" && sed -i.bak 's#/src/main#/src/main.js#' "$D/index.html"
cat > "$D/src/App.svelte" <<'SV'
<script lang="ts">
  import type { Invoice as InvoiceData } from '@deadhop/client';
  import { createDeadhop, Invoice, InvoiceForm, PayButton } from '@deadhop/components/svelte4';
  const deadhop = createDeadhop({ serverPubkey: '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798', relays: ['wss://relay.example'] });
  const paid = (i: InvoiceData): void => console.log(i.id);
</script>
<PayButton client={deadhop} amount={5000} memo="Order 42" label="Buy" onPaid={paid} />
<InvoiceForm client={deadhop} defaultAmount={21} showMemo={false} />
<Invoice client={deadhop} id="0123456789abcdef0123456789abcdef" pollMs={2000} />
SV
cat > "$D/src/Wrong.svelte" <<'SV'
<script lang="ts">
  import { createDeadhop, PayButton } from '@deadhop/components/svelte4';
  const deadhop = createDeadhop({ serverPubkey: '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798', relays: ['wss://relay.example'] });
</script>
<PayButton client={deadhop} amount="lots" />
SV
cat > "$D/tsconfig.json" <<'JSON'
{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler", "strict": true, "noEmit": true, "skipLibCheck": true, "verbatimModuleSyntax": true, "lib": ["ES2022", "DOM"], "types": ["svelte"] }, "include": ["src"] }
JSON
cat > "$D/svelte.config.js" <<'JS'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
export default { preprocess: vitePreprocess() };
JS
cat > "$D/src/main.js" <<'JS'
import App from './App.svelte';
new App({ target: document.getElementById('app') });
JS
cat > "$D/vite.config.js" <<'JS'
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default { plugins: [svelte()], build: { minify: false } };
JS
# The hand-written Svelte 4 types must accept correct props and reject wrong ones.
(cd "$D" && npx svelte-check --tsconfig ./tsconfig.json --threshold error > check.log 2>&1) && { echo "svelte-check accepted a wrong prop" >&2; exit 1; }
grep -q 'Wrong.svelte' "$D/check.log" || { cat "$D/check.log" >&2; exit 1; }
if grep -q 'App.svelte' "$D/check.log"; then cat "$D/check.log" >&2; exit 1; fi
rm "$D/src/Wrong.svelte"
(cd "$D" && npx svelte-check --tsconfig ./tsconfig.json --threshold error >/dev/null && npx vite build --logLevel error)
grep -q 'deadhop-pay' "$D"/dist/assets/*.js
grep -q 'SvelteComponent' "$D"/dist/assets/*.js
echo "svelte 4: types (svelte-check), build with the Svelte 4 compiler ok"

say "notemine"
# With the optional peer installed, notemine is bundled as a separate chunk,
# fetched only when the first invoice needs work.
D="$(project notemine @notemine/wrapper@^0.1.7 vite@^8 typescript@^5.9)"
cp "$WORK/index.html" "$D/" && sed -i.bak 's#/src/main#/src/main.ts#' "$D/index.html"
cat > "$D/src/main.ts" <<'TS'
import { createDeadhop } from '@deadhop/client';
import { notemineMiner } from '@deadhop/client/notemine';
export const deadhop = createDeadhop({ serverPubkey: '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798', relays: ['wss://relay.example'], miner: notemineMiner() });
TS
cat > "$D/tsconfig.json" <<'JSON'
{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler", "strict": true, "noEmit": true, "skipLibCheck": true, "lib": ["ES2022", "DOM"], "types": [] }, "include": ["src"] }
JSON
(cd "$D" && npx tsc -p . && npx vite build --logLevel error --minify false)
MAIN="$(grep -l 'notemineMiner\|relay.example' "$D"/dist/assets/*.js)"
! grep -q 'mine_event' $MAIN || { echo "notemine was bundled into the main chunk instead of loading lazily" >&2; exit 1; }
grep -lq 'mine_event' "$D"/dist/assets/*.js || { echo "notemine is missing from the build" >&2; exit 1; }
echo "notemine: types, lazy chunk ok"

say "client in plain Node"
D="$(project node)"
(cd "$D" && node --input-type=module -e "
  import { createDeadhop, createCheckout, parseSats, invoiceView } from '@deadhop/client';
  import { secretKeySigner } from '@deadhop/client';
  const c = createDeadhop({ serverPubkey: 'npub10elfcs4fr0l0r8af98jlmgdh9c8tcxjvz9qkw038js35mp4dma8qzvjptg', relays: ['wss://relay.example'], signer: secretKeySigner('a'.repeat(64)) });
  const checkout = createCheckout(c);
  if (checkout.getState().phase !== 'idle' || parseSats('5,000') !== 5000) throw new Error('bad');
  console.log('node: client ok');
")
# Without notemine installed and without Web Workers, the notemine miner falls back to the built-in one.
(cd "$D" && node --input-type=module -e "
  import { checkWork, CTXVM_KIND } from '@deadhop/client';
  import { notemineMiner } from '@deadhop/client/notemine';
  const pk = '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798';
  const ev = { kind: CTXVM_KIND, tags: [['p', pk]], content: '{}', created_at: Math.floor(Date.now() / 1000) };
  const mined = await notemineMiner()(pk, ev, 8, {});
  checkWork(pk, ev, mined, 8, Math.floor(Date.now() / 1000));
  console.log('node: notemine falls back without notemine ok');
")

say "all package checks passed"
