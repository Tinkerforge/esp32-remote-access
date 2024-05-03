import { test, expect, Page } from '@playwright/test';

test('has title', async ({ page }) => {
  await page.goto('https://192.168.1.44/');

  // Expect a title "to contain" a substring.
  await expect(page).toHaveTitle(/Remote Access/);
});

async function login(page: Page) {
  await page.goto('https://192.168.1.44/');
  await page.getByRole('textbox', { name: 'Email' }).click();
  await page.getByRole('textbox', { name: 'Email' }).fill('frederic@tinkerforge.com');
  await page.getByRole('textbox', { name: 'Email' }).press('Tab');
  await page.getByRole('textbox', { name: 'Password' }).fill('Test1234567890');
  await Promise.all([
    page.waitForResponse((resp) => {
      expect(resp.status()).toBe(200);
      return resp.url().includes("/api/auth/login")
    }, {timeout: 1000}),
    page.getByRole('button', { name: 'Login' }).click()
  ]);
}

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
