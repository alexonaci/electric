import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: [`test/e2e/**/*.test.ts`],
    globalSetup: `test/support/global-setup.ts`,
    fileParallelism: false,
    testTimeout: 30000,
    hookTimeout: 30000,
    reporters: [`default`],
  },
})
