import { defineConfig } from 'vitest/config';

// Real browser, real relay, real service, regtest bitcoind.
export default defineConfig({
  test: { environment: 'node', include: ['*.e2e.test.ts'], testTimeout: 180_000, hookTimeout: 180_000 },
});
