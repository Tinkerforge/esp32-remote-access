import { test, expect } from '@playwright/test';
import { login, testPassword1, testUser1Email, testUserName1 } from './common';

test('account information validation', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: testUser1Email, exact: true }).click();

  const nameInput = page.getByLabel('Name', { exact: true });
  const saveButton = page.getByRole('button', { name: 'Save changes' });

  // The form is visible before the account information has finished loading.
  await expect(nameInput).toHaveValue(testUserName1);
  await nameInput.clear();
  await saveButton.click();
  await expect(page.getByText('The name must not be empty')).toBeVisible();

  await nameInput.fill(testUserName1);
  const [response] = await Promise.all([
    page.waitForResponse((resp) =>
      new URL(resp.url()).pathname === '/api/user/update_user' && resp.request().method() === 'PUT'
    ),
    saveButton.click(),
  ]);
  expect(response.status()).toBe(200);
  await expect(saveButton).toBeDisabled();
  await expect(nameInput).toHaveValue(testUserName1);
  await expect(page.getByText('The name must not be empty')).not.toBeVisible();
});

test('password change dialog validation', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: testUser1Email, exact: true }).click();
  await page.getByRole('button', { name: 'Change password' }).click();

  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill('weak');
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('Must be at least 8 characters long.')).toBeVisible();

  await page.getByLabel('Current password').clear();
  await page.getByLabel('Current password').fill('wrong_password');
  await page.getByRole('textbox', { name: 'New password', exact: true }).clear();
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill('ValidPassword123!');
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('Passwords do not match')).toBeVisible();
});

test('account local settings and account action modals', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: testUser1Email, exact: true }).click();
  await expect(page.getByRole('button', { name: 'Save changes' })).toBeDisabled();

  const debugSwitch = page.getByLabel('Debug mode');
  await debugSwitch.check();
  await expect(page.getByText(/Storage persisted:/)).toBeVisible();
  await expect.poll(() => page.evaluate(() => localStorage.getItem('debugMode'))).toBe('true');

  await debugSwitch.uncheck();
  await expect(page.getByText(/Storage persisted:/)).not.toBeVisible();
  await expect.poll(() => page.evaluate(() => localStorage.getItem('debugMode'))).toBeNull();

  await page.getByRole('button', { name: 'Save recovery file' }).click();
  const recoveryDialog = page.getByRole('dialog').filter({ hasText: 'Save recovery file' });
  await expect(recoveryDialog).toBeVisible();
  await recoveryDialog.getByRole('button', { name: 'Download' }).click();
  await expect(recoveryDialog.getByText('Wrong password. Could not decrypt secret.')).toBeVisible();
  await recoveryDialog.locator('.modal-footer').getByRole('button', { name: 'Close' }).click();
  await expect(recoveryDialog).not.toBeVisible();

  await page.getByRole('button', { name: 'Delete account' }).click();
  const deleteDialog = page.getByRole('dialog').filter({ hasText: 'Delete account' });
  await expect(deleteDialog).toBeVisible();
  await deleteDialog.getByPlaceholder('Password').fill('not-used-in-this-test');
  await expect(deleteDialog.getByPlaceholder('Password')).toHaveValue('not-used-in-this-test');

  await deleteDialog.getByRole('button', { name: 'Close' }).click();
  await expect(deleteDialog).not.toBeVisible();
});
