import solid from 'vite-plugin-solid';
import { defaultClientConditions, defaultServerConditions } from 'vite';
import { defineConfig } from 'vitest/config';

// Workspace packages resolve to their TypeScript sources.
const resolve = { conditions: ['deadhop-source', ...defaultClientConditions] };
const ssr = { resolve: { conditions: ['deadhop-source', ...defaultServerConditions] } };
const inline = { deps: { inline: [/solid-js/] } };

// Browser tests and server rendering run separately (see `npm test`): merging
// their coverage reports branches that exist only in one of the two builds.
export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/index.ts', 'src/props.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: { lines: 100, functions: 100, statements: 100, branches: 100 },
    },
    projects: [
      {
        plugins: [solid()],
        resolve: { conditions: ['deadhop-source', 'browser', ...defaultClientConditions] },
        ssr,
        test: { name: 'solid', environment: 'jsdom', include: ['test/**/*.test.tsx'], restoreMocks: true, server: inline },
      },
      {
        plugins: [solid({ ssr: true })],
        resolve,
        ssr,
        test: { name: 'solid-ssr', environment: 'node', include: ['test/**/*.ssr.tsx'], server: inline },
      },
    ],
  },
});
