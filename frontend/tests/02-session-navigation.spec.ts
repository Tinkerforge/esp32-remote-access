import { test, expect } from '@playwright/test';
import { login, testPassword1, testUser1Email, testUserName1 } from './common';

test('login', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
});

test('show account page', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByLabel('Name')).toHaveValue(testUserName1);
});

test('navigation and logout', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Token' }).click();
  await expect(page.getByRole('heading', { name: 'Create authorization token' })).toBeVisible();

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByRole('heading', { name: 'Account information' })).toBeVisible();

  await page.getByRole('button', { name: 'Logout', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Login' })).toBeVisible();
});
