import { test, expect } from '@playwright/test';
import { login, testPassword1, testUser1Email } from './common';
import { createToken, deleteToken, expectVisibleTokenOrder, testOrigin } from './token_helpers';

test('token management', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Token' }).click();

  const suffix = Date.now();
  const firstName = `Test Token 1 ${suffix}`;
  const secondName = `Test Token 2 ${suffix}`;

  await createToken(page, firstName);
  await createToken(page, secondName);

  try {
    await expect(page.getByText(firstName)).toBeVisible();
    await expect(page.getByText(secondName)).toBeVisible();

    await deleteToken(page, firstName);
    await expect(page.getByText(firstName)).not.toBeVisible();
    await expect(page.getByText(secondName)).toBeVisible();
  } finally {
    await deleteToken(page, secondName);
  }
});

test('token search filters existing tokens and shows empty search state', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
  await page.getByRole('link', { name: 'Token' }).click();

  const suffix = Date.now();
  const matchingName = `Search Alpha ${suffix}`;
  const otherName = `Search Beta ${suffix}`;

  await createToken(page, matchingName);
  await createToken(page, otherName);

  try {
    const searchInput = page.getByRole('searchbox', { name: 'Search tokens' });

    await searchInput.fill(matchingName);
    await expect(page.locator('.token-item', { hasText: matchingName })).toBeVisible();
    await expect(page.locator('.token-item', { hasText: otherName })).not.toBeVisible();

    await searchInput.fill('definitely-no-token-for-this-query');
    await expect(page.getByText('No tokens match your search.')).toBeVisible();

    await searchInput.fill('');
    await expect(page.locator('.token-item', { hasText: matchingName })).toBeVisible();
    await expect(page.locator('.token-item', { hasText: otherName })).toBeVisible();
  } finally {
    await deleteToken(page, matchingName);
    await deleteToken(page, otherName);
  }
});

test('token sorting supports name and creation-date ordering', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
  await page.getByRole('link', { name: 'Token' }).click();

  const suffix = Date.now();
  const alphaName = `AAA Sort ${suffix}`;
  const zuluName = `ZZZ Sort ${suffix}`;

  await createToken(page, zuluName);
  await page.waitForTimeout(1100);
  await createToken(page, alphaName);

  try {
    const sortSelect = page.getByLabel('Sort tokens');

    await sortSelect.selectOption('name-asc');
    await expectVisibleTokenOrder(page, [alphaName, zuluName]);

    await sortSelect.selectOption('name-desc');
    await expectVisibleTokenOrder(page, [zuluName, alphaName]);

    await sortSelect.selectOption('created-asc');
    await expectVisibleTokenOrder(page, [zuluName, alphaName]);

    await sortSelect.selectOption('created-desc');
    await expectVisibleTokenOrder(page, [alphaName, zuluName]);
  } finally {
    await deleteToken(page, alphaName);
    await deleteToken(page, zuluName);
  }
});

test('creating an unnamed reusable token auto-names it and updates status text', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
  await page.getByRole('link', { name: 'Token' }).click();

  const existingAutoNames = (await page.locator('.token-item h6').allTextContents()).map((name) => name.trim());
  await page.locator('#useOnce').click();
  await expect(page.getByText('This token can be used multiple times until manually deleted')).toBeVisible();

  const createResponse = page.waitForResponse((response) =>
    response.url().includes('/api/user/create_authorization_token') &&
    response.request().method() === 'POST'
  );
  await page.getByRole('button', { name: 'Create token' }).click();
  expect((await createResponse).status()).toBe(201);

  const getCreatedName = async () => {
    const names = (await page.locator('.token-item h6').allTextContents()).map((name) => name.trim());
    return names.find((name) => /^Token-\d+$/.test(name) && !existingAutoNames.includes(name)) ?? '';
  };
  await expect.poll(getCreatedName, { timeout: 5_000 }).toMatch(/^Token-\d+$/);

  const createdName = await getCreatedName();
  const createdToken = page.locator('.token-item', { hasText: createdName });
  await expect(createdToken).toBeVisible();
  await expect(createdToken.getByRole('button', { name: 'Reusable' })).toBeVisible();

  await deleteToken(page, createdName);
});

test('copies a generated token to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: testOrigin() });
  await login(page, testUser1Email, testPassword1);
  await page.getByRole('link', { name: 'Token' }).click();

  const tokenName = `Clipboard Token ${Date.now()}`;
  const tokenItem = await createToken(page, tokenName);
  const tokenValue = await tokenItem.getByRole('textbox').inputValue();

  try {
    await tokenItem.getByRole('button', { name: 'Copy' }).click();
    await expect(page.getByText('Token copied to clipboard')).toBeVisible();
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(tokenValue);
  } finally {
    await deleteToken(page, tokenName);
  }
});
