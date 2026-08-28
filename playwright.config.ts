import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  fullyParallel: false,
  retries: 0,
  reporter: [['list']],
  use: {
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'desktop',
      use: { baseURL: 'http://localhost:1420' },
    },
    {
      name: 'teleprompter',
      use: { baseURL: 'http://localhost:8787' },
    },
  ],
  webServer: [
    {
      command: 'pnpm --filter @nextalk/desktop preview --port 1420 --strictPort',
      url: 'http://localhost:1420',
      reuseExistingServer: true,
    },
    {
      command: 'pnpm --filter @nextalk/teleprompter preview --port 8787 --strictPort',
      url: 'http://localhost:8787',
      reuseExistingServer: true,
    },
  ],
});
