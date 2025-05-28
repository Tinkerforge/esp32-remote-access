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

test('charger lifecycle', async ({ page }) => {
  test.slow();

  // Add charger
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('button', { name: 'Show' }).click();
  await page.getByLabel('Relay server hostname').fill(testDomain.substring(8));
  if (needCustomCert) {
    await page.getByLabel('TLS cer­tif­i­cate', { exact: true }).selectOption('0');
  } else {
    await page.getByLabel('TLS cer­tif­i­cate', { exact: true }).selectOption('-1');
  }
  await page.getByRole('row', { name: 'of 5 users config­ured.' }).getByRole('button').click();
  await page.getByLabel('Email ad­dress').click();
  await page.getByLabel('Email ad­dress').fill(testUser1Email);
  await page.getByLabel('Email ad­dress').press('Tab');
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
  await expect(page.getByText('NameDevice-IDNoteSortAscending')).toBeVisible();
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
  await page.getByRole('row', { name: 'of 5 users config­ured.' }).getByRole('button').click();
  await page.getByLabel('Autho­ri­za­tion method').selectOption('token');
  await page.getByLabel('Autho­ri­za­tion token').fill(token);
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
  await expect(page.getByText('NameDevice-IDNoteSortAscending')).toBeVisible();
});
