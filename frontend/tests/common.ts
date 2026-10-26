import { expect, Page } from "@playwright/test";
import { MailiskClient } from "mailisk";

function getOptionalEnvVar(name: string, defaultValue: string = ""): string {
  return process.env[name] || defaultValue;
}

export function getChargerRelaySettings(domain: string, port: string = '') {
  const url = new URL(domain);
  return {
    host: url.hostname,
    // The charger's relay connection uses TLS, so its default port is 443.
    port: port.replace(/^:/, '') || url.port || '443',
  };
}

export const testDomain = getOptionalEnvVar('TEST_DOMAIN', 'http://localhost:3000');
export const testPort = getOptionalEnvVar('TEST_PORT', '');
const chargerRelaySettings = getChargerRelaySettings(testDomain, testPort);
export const testDomainForCharger = chargerRelaySettings.host;
export const testPortForCharger = chargerRelaySettings.port;
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

export async function configureChargerRelay(page: Page) {
  await page.goto(`${testWallboxDomain}/#status`);
  await page.getByRole('button', { name: 'System', exact: true }).click();
  await page.getByRole('button', { name: 'Remote Access', exact: true }).click();
  await page.getByRole('button', { name: 'Show', exact: true }).click();
  await page.getByRole('textbox', { name: 'Relay server hostname or IP' }).fill(testDomainForCharger);
  await page.getByRole('spinbutton', { name: 'Relay server port', exact: true }).fill(testPortForCharger);
  await page.getByLabel('TLS certificate', { exact: true }).selectOption(needCustomCert ? '0' : '-1');

  const [response] = await Promise.all([
    page.waitForResponse((resp) =>
      new URL(resp.url()).pathname === '/remote_access/config_update' && resp.request().method() === 'PUT'
    ),
    page.getByRole('button', { name: 'Save', exact: true }).click(),
  ]);
  expect(response.ok()).toBe(true);
}

export async function login(page: Page, email: string, password: string) {
  await page.goto(testDomain);
  const loginForm = page.getByRole('tabpanel', { name: 'Login', exact: true });
  await loginForm.getByLabel('Email-address', { exact: true }).fill(email);
  await loginForm.getByLabel('Password', { exact: true }).fill(password);
  const [response] = await Promise.all([
    page.waitForResponse((resp) =>
      new URL(resp.url()).pathname === '/api/auth/login' && resp.request().method() === 'POST'
    ),
    loginForm.getByRole('button', { name: 'Login', exact: true }).click(),
  ]);
  expect(response.status()).toBe(200);
  await expect(page.getByRole('button', { name: 'Logout', exact: true })).toBeVisible();
}

export async function verifyEmail(page: Page, email: string, sentAfter: number) {
  const inbox = await ensureMailiskConfigured().searchInbox(mailiskNameSpace, {
    to_addr_prefix: email,
    from_timestamp: sentAfter,
  });
  const marker = `[${testDomain}/api/auth/verify?`;
  const text = inbox.data.find((message) => message.text?.includes(marker))?.text;
  if (!text) {
    throw new Error('No verification email found for the test account');
  }

  const start = text.indexOf(marker) + 1;
  const url = text.substring(start, text.indexOf(']', start));
  const response = await page.goto(url);
  expect(response?.status()).toBe(200);
  await expect(page.getByRole('button', { name: 'Login', exact: true })).toBeVisible();
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
  await configureChargerRelay(page);
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
