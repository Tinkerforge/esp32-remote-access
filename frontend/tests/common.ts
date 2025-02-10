import { expect, Page } from "@playwright/test";
import { MailiskClient } from "mailisk"

export const testDomain = process.env.TEST_DOMAIN;
export const testUser = process.env.TEST_USER;
export const testUserName = process.env.TEST_USER_NAME;
export const testPassword = process.env.TEST_PASSWORD;
export const mailiskClient = new MailiskClient({apiKey: process.env.MAILISK_API_KEY});
export const mailiskNameSpace = process.env.MAILISK_NAMESPACE;
export const testWallboxDomain = process.env.TEST_WALLBOX_DOMAIN;
export const testWallboxUID = process.env.TEST_WALLBOX_UID;

export async function login(page: Page) {
  await page.goto(testDomain);
  await page.getByRole('textbox', { name: 'Email' }).click();
  await page.getByRole('textbox', { name: 'Email' }).fill(testUser);
  await page.getByRole('textbox', { name: 'Email' }).press('Tab');
  await page.getByRole('textbox', { name: 'Password' }).fill(testPassword);
  await Promise.all([
    page.waitForResponse((resp) => {
      expect(resp.status()).toBe(200);
      return resp.url().includes("/api/auth/login")
    }, {timeout: 1000}),
    page.getByRole('button', { name: 'Login' }).click(),
  ]);
}
