import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defaultClientConditions, defaultServerConditions } from 'vite';
import { defineConfig } from 'vitest/config';

// Workspace packages resolve to their TypeScript sources.
const resolve = { conditions: ['deadhop-source', ...defaultClientConditions] };
const ssr = { resolve: { conditions: ['deadhop-source', ...defaultServerConditions] } };

// Browser tests and server rendering run separately (see `npm test`): merging
// their coverage reports branches that exist only in one of the two builds.
// The Svelte 4 components also run on the real Svelte 4 runtime: test-svelte4/.
export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,svelte}'],
      exclude: ['src/**/index.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: { lines: 100, functions: 100, statements: 100, branches: 100 },
    },
    projects: [
      { plugins: [svelte(), svelteTesting()], resolve, ssr, test: { name: 'svelte', environment: 'jsdom', include: ['test/**/*.test.ts'], restoreMocks: true } },
      { plugins: [svelte()], resolve, ssr, test: { name: 'svelte-ssr', environment: 'node', include: ['test/**/*.ssr.ts'] } },
    ],
  },
});
