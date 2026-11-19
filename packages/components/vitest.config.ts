import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import solid from 'vite-plugin-solid';
import { defaultClientConditions, defaultServerConditions } from 'vite';
import { defineConfig } from 'vitest/config';

// Workspace packages resolve to their TypeScript sources.
const resolve = { conditions: ['cvmpay-source', ...defaultClientConditions] };
const ssr = { resolve: { conditions: ['cvmpay-source', ...defaultServerConditions] } };

export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx,svelte}'],
      exclude: ['src/**/index.ts', 'src/**/props.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: { lines: 100, functions: 100, statements: 100, branches: 100 },
    },
    projects: [
      {
        resolve,
        ssr,
        esbuild: { jsx: 'automatic' },
        test: { name: 'react', environment: 'jsdom', include: ['test/react/**/*.test.tsx'], restoreMocks: true },
      },
      {
        resolve,
        ssr,
        esbuild: { jsx: 'automatic' },
        test: { name: 'react-ssr', environment: 'node', include: ['test/react/**/*.ssr.tsx'] },
      },
      {
        plugins: [solid()],
        resolve: { ...resolve, conditions: ['cvmpay-source', 'browser', ...defaultClientConditions] },
        ssr,
        test: { name: 'solid', environment: 'jsdom', include: ['test/solid/**/*.test.tsx'], restoreMocks: true, server: { deps: { inline: [/solid-js/] } } },
      },
      {
        plugins: [solid({ ssr: true })],
        resolve,
        ssr,
        test: { name: 'solid-ssr', environment: 'node', include: ['test/solid/**/*.ssr.tsx'], server: { deps: { inline: [/solid-js/] } } },
      },
      {
        plugins: [svelte(), svelteTesting()],
        resolve,
        ssr,
        test: { name: 'svelte', environment: 'jsdom', include: ['test/svelte*/**/*.test.ts'], restoreMocks: true },
      },
      {
        plugins: [svelte()],
        resolve,
        ssr,
        test: { name: 'svelte-ssr', environment: 'node', include: ['test/svelte*/**/*.ssr.ts'] },
      },
    ],
  },
});
