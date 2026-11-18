import { defineConfig, devices } from '@playwright/test';
import dotenv from 'dotenv';

dotenv.config({ path: '.env.local' });

export default defineConfig({
  testDir: '.',
  testMatch: 'release-upgrade.spec.ts',
  timeout: 240_000,
  workers: 1,
  reporter: 'list',
  use: {
    ...devices['Desktop Chrome'],
    // The browser uses loopback. The charger validates the LAN hostname.
    ignoreHTTPSErrors: true,
    // Chromium applies the exception to service-worker requests and uses
    // HTTP/1.1 for the relay's WebSocket upgrades.
    launchOptions: { args: ['--ignore-certificate-errors', '--disable-http2'] },
    actionTimeout: 15_000,
    navigationTimeout: 15_000,
  },
});
