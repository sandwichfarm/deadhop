import { defineConfig } from 'vitest/config';

// End-to-end tests talk to a real relay, the real service and regtest bitcoind.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['e2e/**/*.e2e.test.ts'],
    testTimeout: 120_000,
    hookTimeout: 60_000,
  },
});
