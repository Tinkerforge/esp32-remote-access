import { test, expect, Page } from '@playwright/test';
import { login, testDomain, testPassword, testUser1Email, testUserName, testWallboxDomain, testWallboxUID } from './common';

test('has title', async ({ page }) => {
  await page.goto(testDomain);

  // Expect a title "to contain" a substring.
  await expect(page).toHaveTitle(/Remote Access/);
});

test('login', async ({ page }) => {
  await login(page, testUser1Email, testPassword);
});


test('show user page', async ({ page }) => {
  await login(page, testUser1Email, testPassword);
  await page.getByRole('link', { name: 'User' }).click();
  await expect(page.getByLabel('Name')).toHaveValue(testUserName);
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
  await page.getByRole('row', { name: 'of 5 users config­ured.' }).getByRole('button').click();
  await page.getByLabel('Email ad­dress').click();
  await page.getByLabel('Email ad­dress').fill(testUser1Email);
  await page.getByLabel('Email ad­dress').press('Tab');
  await page.getByLabel('Passwordonly used for the reg').fill(testPassword);
  await page.getByLabel('Passwordonly used for the reg').press('Enter');
  await page.getByRole('button', { name: 'Reboot' }).click();

  // Connect to charger
  await login(page, testUser1Email, testPassword);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible();
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();

  // Remove charger
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.getByRole('button', { name: 'Reboot' }).click();
  await page.goto(testDomain);
  await expect(page.getByText('NameDevice-IDNoteSortAscending')).toBeVisible();
});

test('charger lifecycle with auth token', async ({page}) => {
  test.slow();
  await page.waitForTimeout(10_000);

  await login(page, testUser1Email, testPassword);

  // Create token
  await page.getByRole('link', { name: 'Token' }).click();
  await page.getByRole('button', { name: 'Create token' }).click();
  const token = await page.getByRole('textbox').inputValue();

  // Add charger
  await page.goto(testWallboxDomain);
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Event Log' }).click();
  await expect(page.getByPlaceholder('Loading event log...')).toContainText("Network connected");
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: 'of 5 users config­ured.' }).getByRole('button').click();
  await page.getByLabel('Autho­ri­za­tion method').selectOption('token');
  await page.getByLabel('Autho­ri­za­tion token').click();
  await page.getByLabel('Autho­ri­za­tion token').fill(token);
  await page.getByRole('button', { name: 'Add' }).click();
  await page.getByRole('button', { name: 'Reboot' }).click();

  // Connect to charger
  await page.goto(testDomain);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible();
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();

  // Remove charger
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.getByRole('button', { name: 'Reboot' }).click();
  await page.goto(testDomain);
  await expect(page.getByText('NameDevice-IDNoteSortAscending')).toBeVisible();
});

test('change username', async ({page}) => {

});
