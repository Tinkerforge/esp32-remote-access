import { expect, Page } from '@playwright/test';
import { testDomain } from './common';

export async function createToken(page: Page, name: string, useOnce = true) {
  await page.getByRole('textbox', { name: 'Name' }).fill(name);

  const useOnceSwitch = page.locator('#useOnce');
  if ((await useOnceSwitch.isChecked()) !== useOnce) {
    await useOnceSwitch.click();
  }

  const createResponse = page.waitForResponse((response) =>
    response.url().includes('/api/user/create_authorization_token') &&
    response.request().method() === 'POST'
  );
  await page.getByRole('button', { name: 'Create token' }).click();
  expect((await createResponse).status()).toBe(201);

  const tokenItem = page.locator('.token-item', { hasText: name });
  await expect(tokenItem).toBeVisible();
  return tokenItem;
}

export async function deleteToken(page: Page, name: string) {
  const searchInput = page.getByRole('searchbox', { name: 'Search tokens' });
  if (await searchInput.count() > 0) {
    await searchInput.fill('');
  }

  const tokenItem = page.locator('.token-item', { hasText: name });
  if (await tokenItem.count() === 0) {
    return;
  }

  const deleteResponse = page.waitForResponse((response) =>
    response.url().includes('/api/user/delete_authorization_token') &&
    response.request().method() === 'DELETE'
  );
  await tokenItem.getByRole('button', { name: 'Delete' }).click();
  expect((await deleteResponse).status()).toBe(200);
  await expect(tokenItem).not.toBeVisible();
}

async function visibleTokenNames(page: Page, names: string[]) {
  return page.locator('.token-item h6').evaluateAll((elements, expectedNames) => {
    const expected = new Set(expectedNames);
    return elements
      .map((element) => element.textContent?.trim() ?? '')
      .filter((name) => expected.has(name));
  }, names);
}

export async function expectVisibleTokenOrder(page: Page, expectedNames: string[]) {
  await expect.poll(() => visibleTokenNames(page, expectedNames), { timeout: 5_000 }).toEqual(expectedNames);
}

export function testOrigin() {
  return new URL(testDomain).origin;
}
