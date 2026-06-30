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

export async function waitForPasswordChargerRegistration(page: Page) {
  await expect(page.getByText('Preparing login')).toBeVisible({timeout: 30_000});
  await expect(page.getByText('Logging in account')).toBeVisible({timeout: 60_000});
  await expect(page.getByText('Preparing encryption')).toBeVisible({timeout: 60_000});
  await expect(page.getByText('Registering')).toBeVisible({timeout: 120_000});
}

export async function waitForTokenChargerRegistration(page: Page) {
    await expect(page.getByText('Registering')).toBeVisible({ timeout: 90_000 });
    await expect(page.getByText('Registering')).toBeHidden({ timeout: 90_000 });
}

export async function ensureChargerConnected(page: Page, email: string, password: string): Promise<void> {
  if (!testWallboxUID) {
    throw new Error('TEST_WALLBOX_UID environment variable is not set');
  }
  if (!testWallboxDomain) {
    throw new Error('TEST_WALLBOX_DOMAIN environment variable is not set');
  }

  await login(page, email, password);

  // Already connected — nothing to do.
  const chargerVisible = await page.locator('tbody').getByText(testWallboxUID).count() > 0;
  if (chargerVisible) {
    return;
  }

  // Create an auth token on the account.
  await page.getByRole('link', { name: 'Token' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Test');
  await page.getByRole('button', { name: 'Create token' }).click();
  const token = await page.getByRole('textbox').nth(1).inputValue();

  // Register the wallbox using the token.
  await page.goto(testWallboxDomain);
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Event Log' }).click();
  await expect(page.getByPlaceholder('Loading event log...')).toContainText("Connecting to Management WireGuard peer");
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: 'of 5 accounts configured' }).getByRole('button').click();
  await page.getByLabel('Authorization method').selectOption('token');
  await page.getByLabel('Authorization token').fill(token);
  await page.getByRole('button', { name: 'Add' }).click();
  await waitForTokenChargerRegistration(page);

  // Verify the wallbox shows up under the account and is online.
    await page.goto(testDomain);
    await expect(page.locator('tbody')).toContainText(testWallboxUID);
    await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  }

  export async function ensureChargerDisconnected(page: Page, email: string): Promise<void> {
    if (!testWallboxDomain || !email) {
      // Nothing to clean up if required env vars are missing.
      return;
    }

    await page.goto(testWallboxDomain + '/#status');
    await page.getByRole('button', { name: 'System' }).click();
    await page.getByRole('button', { name: 'Remote Access' }).click();

    // If the user row isn't there, the wallbox is already disconnected.
    const userRow = page.getByRole('row', { name: email });
    if (await userRow.count() === 0) {
      return;
    }

    // Click the action button (trash) for the user row, then save.
    await userRow.getByRole('button').click();
    await page.getByRole('button', { name: 'Save' }).click();

    // Give the wallbox a moment to apply the new configuration.
    await page.waitForTimeout(6000);
  }
