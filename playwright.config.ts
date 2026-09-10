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
      // skeleton.spec.ts exercises the phone H5 over a mock WS — desktop app
      // specs (desktop.spec.ts) run under this project.
      testIgnore: '**/skeleton.spec.ts',
    },
    {
      name: 'teleprompter',
      // 8791: the H5's real port (8787) is the desktop LAN server's port AND
      // collides with a long-running local tool on 127.0.0.1:8787 — e2e
      // previews must not fight the desktop's production port.
      use: { baseURL: 'http://localhost:8791' },
      // The desktop surface needs the 1420 preview + Tauri IPC mock; running
      // desktop.spec.ts against the H5 origin would test the wrong app.
      testIgnore: '**/desktop.spec.ts',
    },
  ],
  webServer: [
    {
      command: 'pnpm --filter @nextalk/desktop preview --port 1420 --strictPort',
      url: 'http://localhost:1420',
      reuseExistingServer: true,
    },
    {
      command: 'pnpm --filter @nextalk/teleprompter preview --port 8791 --strictPort',
      url: 'http://localhost:8791',
      reuseExistingServer: true,
    },
  ],
});
