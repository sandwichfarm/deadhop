// Builds one test page per framework from the built packages (dist/), the
// way an app would consume them. Run in its own process (see the e2e test).
import { svelte } from '@sveltejs/vite-plugin-svelte';
import solid from 'vite-plugin-solid';
import { build } from 'vite';
import { fileURLToPath } from 'node:url';

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const plugins = { react: [], solid: [solid()], svelte5: [svelte({ configFile: false })], svelte4: [svelte({ configFile: false })] };
for (const fw of Object.keys(plugins)) {
  await build({
    root: here(`./${fw}`),
    base: './',
    configFile: false,
    logLevel: 'error',
    plugins: plugins[fw],
    esbuild: fw === 'react' ? { jsx: 'automatic' } : undefined,
    publicDir: false,
    build: { outDir: here(`./dist/${fw}`), emptyOutDir: true, target: 'es2022' },
  });
}
