import { test, expect, Page } from '@playwright/test';
import { login, mailiskClient, mailiskNameSpace, needCustomCert, testDomain, testPassword1, testPassword2, testUser1Email, testUser2, testUser2Email, testUserName1, testUserName2, testWallboxDomain, testWallboxUID } from './common';

test('has title', async ({ page }) => {
  await page.goto(testDomain);

  // Expect a title "to contain" a substring.
  await expect(page).toHaveTitle(/Remote Access/);
});

test('login', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
});


test('show account page', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByLabel('Name')).toHaveValue(testUserName1);
});

test('invalid register form', async ({ page }) => {
  await page.goto(testDomain);
  await page.getByRole('tab', { name: 'Register' }).click();
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('The name must not be empty')).toBeVisible();
  await expect(page.getByText('The email-address must not be')).toBeVisible();
  await expect(page.getByText('Must contain at least one')).toBeVisible();
});

test('invalid login attempts', async ({ page }) => {
  await page.goto(testDomain);

  // Test with wrong password
  await page.getByRole('textbox', { name: 'Email' }).fill(testUser1Email);
  await page.getByRole('textbox', { name: 'Password' }).fill('wrong_password');
  await page.getByRole('button', { name: 'Login' }).click();
  await expect(page.getByText('Email-address or password wrong.')).toBeVisible();

  // Test with non-existent email
  await page.getByRole('textbox', { name: 'Email' }).fill('nonexistent@example.com');
  await page.getByRole('textbox', { name: 'Password' }).fill(testPassword1);
  await page.getByRole('button', { name: 'Login' }).click();
  await expect(page.getByText('Email-address or password wrong.')).toBeVisible();
});

test('navigation and logout', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  // Test navigation to different pages
  await page.getByRole('link', { name: 'Token' }).click();
  await expect(page.getByRole('heading', { name: 'Create authorization token' })).toBeVisible();

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByRole('heading', { name: 'Account information' })).toBeVisible();

  // Test logout
  await page.getByRole('button', { name: 'Logout', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Login' })).toBeVisible();
});

test('token management', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Token' }).click();

  // Create a token
  await page.getByRole('textbox', { name: 'Name' }).fill('Test Token 1');
  await page.getByRole('button', { name: 'Create token' }).click();

  // Verify token appears in list
  await expect(page.getByText('Test Token 1')).toBeVisible();

  // Create another token
  await page.getByRole('textbox', { name: 'Name' }).clear();
  await page.getByRole('textbox', { name: 'Name' }).fill('Test Token 2');
  await page.getByRole('button', { name: 'Create token' }).click();

  // Verify both tokens exist
  await expect(page.getByText('Test Token 1')).toBeVisible();
  await expect(page.getByText('Test Token 2')).toBeVisible();

  // Delete a token
  await page.getByRole('button', { name: 'Delete' }).nth(0).click();
  await expect(page.getByText('Test Token 1')).not.toBeVisible();
  await expect(page.getByText('Test Token 2')).toBeVisible();
});

test('account information validation', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();

  // Test invalid name (empty)
  await page.getByLabel('Name').clear();
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByText('The name must not be empty')).toBeVisible();

  await page.getByLabel('Email-address').clear();
  await page.getByLabel('Email-address').fill(testUser1Email);
  await page.getByRole('button', { name: 'Save changes' }).click();
});

test('password change dialog validation', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await page.getByRole('button', { name: 'Change password' }).click();

  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByLabel('New password').fill('weak');
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('Must contain at least one')).toBeVisible();

  await page.getByLabel('Current password').clear();
  await page.getByLabel('Current password').fill('wrong_password');
  await page.getByLabel('New password').clear();
  await page.getByLabel('New password').fill('ValidPassword123!');
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await page.getByRole('button', { name: 'Close' }).click();
});

test('charger lifecycle', async ({ page }) => {
  test.slow();

  // Add charger
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('button', { name: 'Show' }).click();
  await page.getByRole('textbox', { name: 'Relay server hostname or IP' }).fill(testDomain.substring(8));
  if (needCustomCert) {
    await page.getByLabel('TLS certificate', { exact: true }).selectOption('0');
  } else {
    await page.getByLabel('TLS certificate', { exact: true }).selectOption('-1');
  }
  await page.getByRole('row', { name: 'of 5 users configured' }).getByRole('button').click();
  await page.getByRole('textbox', { name: 'Email address' }).click();
  await page.getByRole('textbox', { name: 'Email address' }).fill(testUser1Email);
  await page.getByLabel('Passwordonly used for the reg').fill(testPassword1);
  await page.getByLabel('Passwordonly used for the reg').press('Enter');
  await page.getByRole('button', { name: 'Reboot' }).click();

  // Connect to charger
  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();

  // Remove charger
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.getByRole('button', { name: 'Reboot' }).click();
  await page.waitForTimeout(6000);
  await page.goto(testDomain);
  await expect(page.getByText('No devices registered yet. Please connect your device to this account to get started.')).toBeVisible();
});

test('add charger with auth token', async ({page}) => {
  test.slow();
  await page.waitForTimeout(20_000);

  await login(page, testUser1Email, testPassword1);

  // Create token
  await page.getByRole('link', { name: 'Token' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Test');
  await page.getByRole('button', { name: 'Create token' }).click();
  const token = await page.getByRole('textbox').nth(1).inputValue();

  // Add charger
  await page.goto(testWallboxDomain);
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Event Log' }).click();
  await expect(page.getByPlaceholder('Loading event log...')).toContainText("Network connected");
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: 'of 5 users configured' }).getByRole('button').click();
  await page.getByLabel('Authorization method').selectOption('token');
  await page.getByLabel('Authorization token').fill(token);
  await page.getByRole('button', { name: 'Add' }).click();
  await page.getByRole('button', { name: 'Reboot' }).click();

  // Connect to charger
  await page.goto(testDomain);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();
});

test('change accountname', async ({page}) => {
  test.slow();

  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByLabel('Email-address')).toHaveValue(testUser1Email);
  await page.getByLabel('Email-address').fill(testUser2Email);
  await page.getByLabel('Name').fill(testUserName2);
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByLabel('Email-address')).toBeVisible();
  await page.getByLabel('Name').click();
  await page.getByRole('button', { name: 'Logout', exact: true }).click();


  const inbox = await mailiskClient.searchInbox(mailiskNameSpace, { to_addr_prefix:  testUser2, from_timestamp: (Date.now() / 1000) - 5 });
  if (!inbox.data || inbox.data.length === 0) {
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

// ===== TESTS AFTER THIS POINT CREATE NEW USERS OR USE DIFFERENT CREDENTIALS =====

test('change password', async ({page}) => {
  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await page.getByRole('button', { name: 'Change password' }).click();
  await page.getByLabel('Current password').click();
  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByLabel('New password').fill(testPassword2);
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('LoginRegisterEmail-')).toBeVisible();

  await login(page, testUser2Email, testPassword2);
});

test('connect to charger with new password', async ({page}) => {
  await login(page, testUser2Email, testPassword2);

  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();
});

test('remove charger', async ({page}) => {
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.getByRole('button', { name: 'Reboot' }).click();
  await page.waitForTimeout(6000);
  await login(page, testUser2Email, testPassword2);
  await expect(page.getByText('No devices registered yet. Please connect your device to this account to get started.')).toBeVisible();
});
