// Compiled (DOM) build of the Solid components. The `solid` export condition
// points at the preserved JSX instead, for apps that compile Solid themselves (SSR).
import solid from 'vite-plugin-solid';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [solid()],
  build: {
    target: 'es2022',
    outDir: 'dist/solid',
    emptyOutDir: false,
    minify: false,
    lib: { entry: 'src/solid/index.ts', formats: ['es'], fileName: () => 'index.js' },
    rollupOptions: { external: [/^solid-js/, /^@deadhop\//] },
  },
});
