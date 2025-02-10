import { test, expect, Page } from '@playwright/test';
import { login, testDomain, testPassword, testUser, testUserName, testWallboxDomain, testWallboxUID } from './common';

test('has title', async ({ page }) => {
  await page.goto(testDomain);

  // Expect a title "to contain" a substring.
  await expect(page).toHaveTitle(/Remote Access/);
});

test('login', async ({ page }) => {
  await login(page);
});


test('show user page', async ({ page }) => {
  await login(page);
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
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('cell').nth(1).click();
  await page.getByLabel('Email ad­dress').click();
  await page.getByLabel('Email ad­dress').fill(testUser);
  await page.getByLabel('Email ad­dress').press('Tab');
  await page.getByLabel('Passwordonly used for the reg').fill(testPassword);
  await page.getByLabel('Passwordonly used for the reg').press('Enter');
  await page.getByRole('button', { name: 'Reboot' }).click();
  await page.goto(testDomain);
  await page.getByRole('textbox', { name: 'Email-address' }).click();
  await page.getByRole('textbox', { name: 'Email-address' }).fill(testUser);
  await page.getByRole('textbox', { name: 'Email-address' }).press('Tab');
  await page.getByRole('textbox', { name: 'Password' }).fill(testPassword);
  await page.getByRole('textbox', { name: 'Password' }).press('Enter');
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success')).toBeVisible();
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().locator('#P0-91')).toBeVisible({timeout: 10_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.getByRole('button', { name: 'Reboot' }).click();
  await page.goto(testDomain);
  await expect(page.getByText('NameDevice-IDNoteSortAscending')).toBeVisible();
});
