import { expect, Page } from "@playwright/test";
import { MailiskClient } from "mailisk"

export const testDomain = process.env.TEST_DOMAIN;
export const testUserName = process.env.TEST_USER_NAME;
export const testPassword = process.env.TEST_PASSWORD;

export const mailiskClient = new MailiskClient({apiKey: process.env.MAILISK_API_KEY});
export const mailiskNameSpace = process.env.MAILISK_NAMESPACE;
export const testUser1 = process.env.MAILISK_USER1;
export const testUser1Email = `${testUser1}@${mailiskNameSpace}.mailisk.net`;
export const testWallboxDomain = process.env.TEST_WALLBOX_DOMAIN;
export const testWallboxUID = process.env.TEST_WALLBOX_UID;

export async function login(page: Page, email: string, password: string) {
  await page.goto(testDomain);
  await page.getByRole('textbox', { name: 'Email' }).click();
  await page.getByRole('textbox', { name: 'Email' }).fill(email);
  await page.getByRole('textbox', { name: 'Email' }).press('Tab');
  await page.getByRole('textbox', { name: 'Password' }).fill(password);
  await Promise.all([
    page.waitForResponse((resp) => {
      expect(resp.status()).toBe(200);
      return resp.url().includes("/api/auth/login")
    }, {timeout: 1000}),
    page.getByRole('button', { name: 'Login' }).click(),
  ]);
}
