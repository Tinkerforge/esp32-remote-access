import { test, expect } from '@playwright/test';
import { login, testPassword1, testPassword2, testUser1Email, testUser2Email, testUserName2, testWallboxDomain, testWallboxUID, verifyEmail } from './common';

test.describe.configure({ mode: 'serial' });

test('change accountname', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: testUser1Email, exact: true }).click();
  await expect(page.getByLabel('Email-address')).toHaveValue(testUser1Email);
  await page.getByLabel('Email-address').fill(testUser2Email);
  await page.getByLabel('Name').fill(testUserName2);
  const sentAfter = Math.floor(Date.now() / 1000);
  const updateResponse = page.waitForResponse((response) =>
    new URL(response.url()).pathname === '/api/user/update_user' && response.request().method() === 'PUT'
  );
  await page.getByRole('button', { name: 'Save changes' }).click();
  expect((await updateResponse).status(), 'The destination test email must not already belong to another account').toBe(200);
  await expect(page.getByRole('button', { name: 'Save changes' })).toBeDisabled();
  await expect(page.getByLabel('Email-address')).toHaveValue(testUser2Email);
  await expect(page.getByRole('link', { name: testUser2Email, exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Logout', exact: true }).click();

  await verifyEmail(page, testUser2Email, sentAfter);

  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: testUser2Email, exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Account information' })).toBeVisible();
});

test('change password', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: testUser2Email, exact: true }).click();
  await page.getByRole('button', { name: 'Change password' }).click();
  await page.getByLabel('Current password').click();
  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill(testPassword2);
  await page.getByRole('textbox', { name: 'Confirm new password' }).fill(testPassword2);
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByRole('button', { name: 'Login', exact: true })).toBeVisible();

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
