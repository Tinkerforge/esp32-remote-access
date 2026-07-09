import { test, expect } from '@playwright/test';
import { testDomain, testPassword1, testUser1Email } from './common';

test('has title', async ({ page }) => {
  await page.goto(testDomain);

  await expect(page).toHaveTitle(/Remote Access/);
});

test('invalid register form', async ({ page }) => {
  await page.goto(testDomain);
  await page.getByRole('tab', { name: 'Register' }).click();
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('The name must not be empty')).toBeVisible();
  await expect(page.getByText('The email-address must not be')).toBeVisible();
  await expect(page.getByText('Must be at least')).toBeVisible();
});

test('register form validates password confirmation and legal acceptances before submit', async ({ page }) => {
  await page.goto(testDomain);
  await page.getByRole('tab', { name: 'Register' }).click();

  await page.getByPlaceholder('John Doe').fill('Validation User');
  await page.getByRole('textbox', { name: 'Email-address' }).fill(`validation-${Date.now()}@example.invalid`);
  await page.getByRole('textbox', { name: 'Password', exact: true }).fill('ValidPassword123!');
  await page.getByRole('textbox', { name: 'Confirm password' }).fill('DifferentPassword123!');

  await page.getByRole('button', { name: 'Register' }).click();

  await expect(page.getByText('Passwords do not match')).toBeVisible();
  await expect(page.locator('.form-check-input.is-invalid')).toHaveCount(2);

  await page.getByText('I have read, understood and I am accepting the privacy notice.').click();
  await expect(page.locator('.form-check-input.is-invalid')).toHaveCount(1);

  await page.getByText('I have read, understood and I am accepting the terms and conditions.').click();
  await expect(page.locator('.form-check-input.is-invalid')).toHaveCount(0);

  await page.getByRole('textbox', { name: 'Confirm password' }).fill('ValidPassword123!');
  await expect(page.getByText('Passwords do not match')).not.toBeVisible();
});

test('invalid login attempts', async ({ page }) => {
  await page.goto(testDomain);

  await page.getByRole('textbox', { name: 'Email' }).fill(testUser1Email);
  await page.getByRole('textbox', { name: 'Password' }).fill('wrong_password');
  await page.getByRole('button', { name: 'Login' }).click();
  await expect(page.getByText('Email-address or password wrong.')).toBeVisible();

  await page.getByRole('textbox', { name: 'Email' }).fill('nonexistent@example.com');
  await page.getByRole('textbox', { name: 'Password' }).fill(testPassword1);
  await page.getByRole('button', { name: 'Login' }).click();
  await expect(page.getByText('Email-address or password wrong.')).toBeVisible();
});

test('requests password reset from the login form', async ({ page }) => {
  await page.goto(testDomain);

  const recoveryEmail = `reset-${Date.now()}@example.invalid`;
  await page.getByRole('textbox', { name: 'Email' }).fill(recoveryEmail);
  await page.getByRole('link', { name: 'Password reset' }).click();

  const dialog = page.getByRole('dialog').filter({ hasText: 'Password reset' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: 'Email-address' })).toHaveValue(recoveryEmail);

  const recoveryResponse = page.waitForResponse((response) =>
    response.url().includes('/api/auth/start_recovery') &&
    response.request().method() === 'GET'
  );
  await dialog.getByRole('button', { name: 'Send' }).click();
  expect((await recoveryResponse).status()).toBe(200);

  await expect(dialog).not.toBeVisible();
  await expect(page.getByText('You should receive an email within the next few minutes.')).toBeVisible();
});
