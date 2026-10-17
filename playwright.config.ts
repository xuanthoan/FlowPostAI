import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: 'list',
  timeout: 60000,
  expect: { timeout: 10000 },
  use: {
    baseURL: 'http://127.0.0.1:1420',
    trace: 'on-first-retry',
  },
  // Tauri dev server must be running separately: npm run tauri:dev
  // For CI, uncomment webServer below:
  // webServer: {
  //   command: 'npm run tauri:dev',
  //   url: 'http://127.0.0.1:1420',
  //   reuseExistingServer: !process.env.CI,
  //   timeout: 120 * 1000,
  // },
  projects: [
    {
      name: 'tauri-e2e',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
