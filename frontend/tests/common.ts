import { expect, Page } from "@playwright/test";
import { MailiskClient } from "mailisk";

function getOptionalEnvVar(name: string, defaultValue: string = ""): string {
  return process.env[name] || defaultValue;
}

export const testDomain = getOptionalEnvVar('TEST_DOMAIN', 'http://localhost:3000');
export const testPort = getOptionalEnvVar('TEST_PORT', '');
export const testDomainForCharger = testDomain.replace(/^https?:\/\//, '').replace(testPort, '');
export const testUserName1 = getOptionalEnvVar('TEST_USER_NAME1', getOptionalEnvVar('TEST_USER', 'testuser1'));
export const testUserName2 = getOptionalEnvVar('TEST_USER_NAME2', 'testuser2');
export const testPassword1 = getOptionalEnvVar('TEST_PASSWORD1', getOptionalEnvVar('TEST_PASSWORD', 'testpass1'));
export const testPassword2 = getOptionalEnvVar('TEST_PASSWORD2', 'testpass2');

// Mailisk configuration - optional for email testing
const mailiskApiKey = getOptionalEnvVar('MAILISK_API_KEY');
const mailiskNamespaceValue = getOptionalEnvVar('MAILISK_NAMESPACE');
const testUser1Value = getOptionalEnvVar('MAILISK_USER1');
const testUser2Value = getOptionalEnvVar('MAILISK_USER2');

export const mailiskClient = mailiskApiKey ? new MailiskClient({apiKey: mailiskApiKey}) : null as MailiskClient | null;
export const mailiskNameSpace = mailiskNamespaceValue;
export const testUser1 = testUser1Value;
export const testUser2 = testUser2Value;
export const testUser1Email = testUser1Value && mailiskNamespaceValue ? `${testUser1Value}@${mailiskNamespaceValue}.mailisk.net` : '';
export const testUser2Email = testUser2Value && mailiskNamespaceValue ? `${testUser2Value}@${mailiskNamespaceValue}.mailisk.net` : '';
export const testWallboxDomain = getOptionalEnvVar('TEST_WALLBOX_DOMAIN');
export const testWallboxUID = getOptionalEnvVar('TEST_WALLBOX_UID');

// Helper function to ensure mailisk is configured when needed
export function ensureMailiskConfigured() {
  if (!mailiskClient) {
    throw new Error('Mailisk is not configured. Please set MAILISK_API_KEY and MAILISK_NAMESPACE environment variables.');
  }
  return mailiskClient;
}

export const needCustomCert = process.env.NEED_CUSTOM_CERT === "true";

export async function login(page: Page, email: string, password: string) {
  await page.goto(testDomain);
  await page.getByRole('textbox', { name: 'Email' }).click();
  await page.getByRole('textbox', { name: 'Email' }).fill(email);
  await page.getByRole('textbox', { name: 'Email' }).press('Tab');
  await page.getByRole('textbox', { name: 'Password' }).fill(password);
  await page.getByRole('button', { name: 'Login' }).click();
  await page.waitForResponse((resp) => {
    expect(resp.status()).toBe(200);
    return resp.url().includes("/api/auth/login")
  });
}
