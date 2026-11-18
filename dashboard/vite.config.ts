import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  // Relative asset URLs: the build can be served from any path or opened from a static host.
  base: './',
  build: { target: 'es2022', sourcemap: false },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
    restoreMocks: true,
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,svelte}'],
      exclude: ['src/**/*.test.ts', 'src/test-helpers.ts', 'src/vite-env.d.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: { lines: 100, functions: 100, statements: 100, branches: 100 },
    },
  },
});
