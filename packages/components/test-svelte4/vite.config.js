import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

const here = (p) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  plugins: [svelte({ configFile: false, compilerOptions: {} })],
  resolve: {
    // The components and shared scenarios live outside this project: make sure
    // they get this project's Svelte 4 and test library, not the workspace's Svelte 5.
    dedupe: ['svelte', '@testing-library/svelte'],
    conditions: ['cvmpay-source', 'browser'],
  },
  server: { fs: { allow: [here('../..')] } },
  test: {
    environment: 'jsdom',
    include: ['*.test.js'],
    restoreMocks: true,
  },
});
