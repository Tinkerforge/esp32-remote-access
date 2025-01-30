import { test, expect, Page } from '@playwright/test';
import { login, testDomain, testPassword, testUser } from './common';

test('has title', async ({ page }) => {
  await page.goto(testDomain);

  // Expect a title "to contain" a substring.
  await expect(page).toHaveTitle(/Remote Access/);
});

test('login', async ({ page }) => {
  await login(page);
});

test('list chargers', async ({ page }) => {
  await login(page);
  await page.getByRole('link', { name: 'Chargers' }).click();
  await expect(page.locator('tbody')).toContainText('esp32-YwQ');
});

test('connect to charger', async ({ page }) => {
  await login(page);
  await page.getByRole('link', { name: 'Chargers' }).click();
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.frameLocator('#interface').getByRole('textbox')).toHaveValue('esp32-YwQ');
})

test('show user page', async ({ page }) => {
  await login(page);
  await page.getByRole('link', { name: 'User' }).click();
  await expect(page.getByLabel('Name')).toHaveValue('fred');
});

test('invalid register form', async ({ page }) => {
  await page.goto(testDomain);
  await page.getByRole('tab', { name: 'Register' }).click();
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('The name must not be empty')).toBeVisible();
  await expect(page.getByText('The email-address must not be')).toBeVisible();
  await expect(page.getByText('Must contain at least one')).toBeVisible();
});
