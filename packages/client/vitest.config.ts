import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    restoreMocks: true,
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/test-helpers.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: { lines: 100, functions: 100, statements: 100, branches: 100 },
    },
  },
});
