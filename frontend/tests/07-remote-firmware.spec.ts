import { stat } from 'node:fs/promises';
import { test, expect, type Page } from '@playwright/test';
import {
  connectToChargerRemotely, login, readBootIdThroughTunnel, testPassword1, testUser1Email, testWallboxUID,
} from './common';

const firmwarePath = process.env.TEST_FIRMWARE_PATH ?? '';

test.skip(!testWallboxUID || !firmwarePath, 'Requires TEST_WALLBOX_UID and TEST_FIRMWARE_PATH');

function waitForFirmwarePost(page: Page, tunnel: string, endpoint: string, timeout: number) {
  return page.waitForResponse((response) => {
    const request = response.request();
    const url = new URL(response.url());
    return url.origin === new URL(page.url()).origin && request.method() === 'POST' && (
      url.pathname === `${tunnel}${endpoint}` ||
      (url.pathname === endpoint && request.headers()['x-connection-id'] === tunnel.slice('/wg-'.length))
    );
  }, { timeout });
}

for (const mode of ['normal', 'recovery'] as const) {
  test(`upload firmware through the ${mode} remote access interface`, async ({ page }) => {
    test.setTimeout(420_000);
    expect((await stat(firmwarePath)).isFile(), `Firmware path must point to a file: ${firmwarePath}`).toBe(true);

    await login(page, testUser1Email, testPassword1);
    await connectToChargerRemotely(page, 100_000);
    if (mode === 'recovery') {
      await page.waitForURL(/\/(devices|chargers)\/[^/]+$/);
      await page.goto(`${page.url()}/recovery`);
    }

    const iframe = page.locator('#interface');
    const device = iframe.contentFrame();
    if (mode === 'normal') {
      await expect(device.getByRole('heading', { name: 'Status', exact: true })).toBeVisible({ timeout: 45_000 });
      await device.getByRole('button', { name: 'System', exact: true }).click();
      await device.getByRole('button', { name: 'Firmware Update', exact: true }).click();
    }
    await expect(device.getByRole('heading', { name: /^firmware update$/i })).toBeVisible({ timeout: 45_000 });

    const src = (await iframe.getAttribute('src')) ?? '';
    const tunnel = new URL(src, page.url()).pathname.split('/').slice(0, 2).join('/');
    expect(tunnel).toMatch(/^\/wg-[^/]+$/);

    const identity = await page.evaluate(async (prefix) => {
      const response = await fetch(`${prefix}/info/name`, { signal: AbortSignal.timeout(5000) });
      return response.ok ? (await response.json()).uid : null;
    }, tunnel);
    expect(identity, 'The upload targets the configured test charger').toBe(testWallboxUID);

    let bootIdBefore: number | null = null;
    await expect.poll(async () => {
      bootIdBefore = await readBootIdThroughTunnel(page, tunnel);
      return bootIdBefore;
    }, { message: 'Read the initial boot ID through remote access', timeout: 15_000 }).not.toBeNull();

    const fileInput = mode === 'normal' ? device.getByLabel('Manual update') : device.locator('#firmware');
    await fileInput.setInputFiles(firmwarePath);
    const uploadButton = device.getByRole('button', {
      name: mode === 'normal' ? 'Install' : 'Upload firmware',
      exact: true,
    });
    const upload = waitForFirmwarePost(page, tunnel, '/flash_firmware', 150_000);
    const [response] = await Promise.all([
      upload,
      (async () => {
        if (mode === 'normal') {
          const [check] = await Promise.all([
            waitForFirmwarePost(page, tunnel, '/check_firmware', 20_000),
            uploadButton.click(),
          ]);
          if (check.status() === 400) {
            const downgrade = device.getByRole('dialog').filter({ hasText: 'Firmware downgrade' });
            await downgrade.getByRole('button', { name: 'Downgrade firmware', exact: true }).click();
          } else {
            expect(check.status(), 'The charger accepted the firmware preflight check').toBe(200);
          }
        } else {
          await uploadButton.click();
        }
      })(),
    ]);
    expect(response.status(), 'The charger accepted the firmware upload').toBe(200);

    await expect.poll(async () => {
      const bootId = await readBootIdThroughTunnel(page, tunnel);
      return bootId !== null && bootId !== bootIdBefore;
    }, {
      message: 'The original remote access tunnel responds after the firmware reboot',
      timeout: 120_000,
      intervals: [1000, 2000, 5000],
    }).toBe(true);
  });
}
