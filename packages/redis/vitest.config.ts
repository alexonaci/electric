import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: [`test/**/*.test.ts`],
    exclude: [`test/e2e/**/*.test.ts`],
    testTimeout: 30000,
    coverage: {
      provider: `istanbul`,
      reporter: [`text`, `json`, `html`, `lcov`],
      include: [`**/src/**`],
    },
    reporters: [`default`],
  },
})
