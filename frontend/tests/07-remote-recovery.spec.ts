import { test, expect } from '@playwright/test';
import { login, testPassword1, testUser1Email, testWallboxDomain, testWallboxUID } from './common';

test('recovery page working', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await page.getByRole('button', { name: 'Connect' }).click();
  const url = page.url();
  await page.goto(url + "/recovery");
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Firmware update' })).toBeVisible({timeout: 15_000});
  await expect(page.locator('#interface').contentFrame().getByRole('link', { name: 'logo' })).not.toBeVisible();
});
