import { test, expect } from '@playwright/test';
import { login, mailiskClient, mailiskNameSpace, testDomain, testPassword1, testPassword2, testUser1Email, testUser2, testUser2Email, testUserName2, testWallboxDomain, testWallboxUID } from './common';

test('change accountname', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByLabel('Email-address')).toHaveValue(testUser1Email);
  await page.getByLabel('Email-address').fill(testUser2Email);
  await page.getByLabel('Name').fill(testUserName2);
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByLabel('Email-address')).toBeVisible();
  await page.getByLabel('Name').click();
  await page.getByRole('button', { name: 'Logout', exact: true }).click();

  const inbox = await mailiskClient?.searchInbox(mailiskNameSpace, { to_addr_prefix:  testUser2, from_timestamp: (Date.now() / 1000) - 5 });
  if (!inbox?.data || inbox.data.length === 0) {
      throw new Error("No emails found in inbox");
  }
  const firstEmail = inbox.data[0];
  if (!firstEmail || !firstEmail.text) {
      throw new Error("Email data is invalid");
  }
  const idx = firstEmail.text.indexOf(`[${testDomain}/api/auth/verify?`) + 1;
  if (idx === 0) {
      throw new Error("Failed to find verification URL in email");
  }
  const url = firstEmail.text.substring(idx, firstEmail.text.indexOf("]", idx));
  const response = await fetch(url);
  if (response.status !== 200) {
      throw new Error("Failed to verify email");
  }

  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByRole('heading', { name: 'Account information' })).toBeVisible();
});

test('change password', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await page.getByRole('button', { name: 'Change password' }).click();
  await page.getByLabel('Current password').click();
  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill(testPassword2);
  await page.getByRole('textbox', { name: 'Confirm new password' }).fill(testPassword2);
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('LoginRegisterEmail-')).toBeVisible();

  await login(page, testUser2Email, testPassword2);
});

test('connect to charger with new password', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await login(page, testUser2Email, testPassword2);

  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();
});

test('remove charger', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.waitForTimeout(6000);
  await login(page, testUser2Email, testPassword2);
  await expect(page.getByText('No devices registered yet. Please connect your device to this account to get started.')).toBeVisible();
});
