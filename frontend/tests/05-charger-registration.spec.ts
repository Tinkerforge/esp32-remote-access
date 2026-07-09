import { test, expect } from '@playwright/test';
import { login, needCustomCert, testDomain, testDomainForCharger, testPassword1, testUser1Email, testWallboxDomain, testWallboxUID, waitForPasswordChargerRegistration, waitForTokenChargerRegistration } from './common';

test('charger lifecycle', async ({ page }) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('button', { name: 'Show' }).click();
  await page.getByRole('textbox', { name: 'Relay server hostname or IP' }).fill(testDomainForCharger);
  if (needCustomCert) {
    await page.getByLabel('TLS certificate', { exact: true }).selectOption('0');
  } else {
    await page.getByLabel('TLS certificate', { exact: true }).selectOption('-1');
  }
  await page.getByRole('row', { name: 'of 5 accounts configured' }).getByRole('button').click();
  await page.getByRole('textbox', { name: 'Email address' }).click();
  await page.getByRole('textbox', { name: 'Email address' }).fill(testUser1Email);
  await page.getByLabel('Passwordonly used for the reg').fill(testPassword1);
  await page.getByLabel('Passwordonly used for the reg').press('Enter');
  await waitForPasswordChargerRegistration(page);

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();

  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.waitForTimeout(6000);
  await page.goto(testDomain);
  await expect(page.getByText('No devices registered yet. Please connect your device to this account to get started.')).toBeVisible();
});

test('add charger with auth token', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Token' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Test');
  await page.getByRole('button', { name: 'Create token' }).click();
  const token = await page.getByRole('textbox').nth(1).inputValue();

  await page.goto(testWallboxDomain);
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Event Log' }).click();
  await expect(page.getByPlaceholder('Loading event log...')).toContainText("Connecting to Management WireGuard peer");
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: 'of 5 accounts configured' }).getByRole('button').click();
  await page.getByLabel('Authorization method').selectOption('token');
  await page.getByLabel('Authorization token').fill(token);
  await page.getByRole('button', { name: 'Add' }).click();
  await waitForTokenChargerRegistration(page);

  await page.goto(testDomain);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();
});
