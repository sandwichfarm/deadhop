import { defaultClientConditions, defaultServerConditions } from 'vite';
import { defineConfig } from 'vitest/config';

// Workspace packages resolve to their TypeScript sources.
const resolve = { conditions: ['deadhop-source', ...defaultClientConditions] };
const ssr = { resolve: { conditions: ['deadhop-source', ...defaultServerConditions] } };

// Browser tests and server rendering run separately (see `npm test`): merging
// their coverage reports branches that exist only in one of the two builds.
export default defineConfig({
  esbuild: { jsx: 'automatic' },
  test: {
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/index.ts', 'src/props.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: { lines: 100, functions: 100, statements: 100, branches: 100 },
    },
    projects: [
      { resolve, ssr, esbuild: { jsx: 'automatic' }, test: { name: 'react', environment: 'jsdom', include: ['test/**/*.test.tsx'], restoreMocks: true } },
      { resolve, ssr, esbuild: { jsx: 'automatic' }, test: { name: 'react-ssr', environment: 'node', include: ['test/**/*.ssr.tsx'] } },
    ],
  },
});
