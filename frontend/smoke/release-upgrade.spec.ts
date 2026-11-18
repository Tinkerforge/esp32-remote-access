import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test, type APIRequestContext, type Page } from '@playwright/test';
import {
  configureChargerRelay, login, registerAccount, testDomain, testWallboxDomain,
  testWallboxUID, waitForTokenChargerRegistration,
} from '../tests/common';

const email = 'upgrade-smoke@example.test';

function setting(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`Missing ${name}. Run scripts/smoke-release-upgrade.sh --hardware.`);
  return value;
}

function stateFile(name: string) {
  return join(setting('SMOKE_STATE_DIR'), name);
}

interface RelaySettings {
  enable: boolean;
  relay_host: string;
  relay_port: number;
  cert_id: number;
  mtu?: number;
}

interface DeviceConfig extends RelaySettings {
  uuid: string;
  users: { id: number; uuid: string }[];
}

interface Backup {
  uid: string;
  userId: string;
  settings: RelaySettings;
}

async function deviceJson<T>(request: APIRequestContext, path: string): Promise<T> {
  const response = await request.get(new URL(path, testWallboxDomain).href, { timeout: 5000 });
  expect(response.ok(), `Device ${path} returned HTTP ${response.status()}`).toBe(true);
  return response.json();
}

async function devicePut(request: APIRequestContext, path: string, data: object) {
  const response = await request.put(new URL(path, testWallboxDomain).href, { data, timeout: 5000 });
  expect(response.ok(), `Device ${path} returned HTTP ${response.status()}`).toBe(true);
}

async function expectDevice(request: APIRequestContext, uid: string) {
  const device = await deviceJson<{ uid: string }>(request, '/info/name');
  expect(device.uid).toBe(uid);
}

function getDeviceConfig(request: APIRequestContext) {
  return deviceJson<DeviceConfig>(request, '/remote_access/config');
}

async function probe(page: Page) {
  const row = page.getByRole('row').filter({ has: page.getByText(testWallboxUID, { exact: true }) });
  const connect = row.getByRole('button', { name: 'Connect', exact: true });
  await expect(connect).toBeEnabled({ timeout: 120_000 });
  await connect.click();

  const iframe = page.locator('#interface');
  const status = iframe.contentFrame().getByRole('heading', { name: 'Status', exact: true });
  await expect(status).toBeVisible({ timeout: 45_000 });

  // The status page can be cached by the relay. Read the UID over the live
  // tunnel so seeing the page alone cannot make the smoke test pass.
  const src = await iframe.getAttribute('src');
  const tunnel = new URL(src ?? '', testDomain).pathname.split('/')[1];
  expect(tunnel).toMatch(/^wg-/);
  await expect.poll(() => page.evaluate(async (prefix) => {
    try {
      const response = await fetch(`/${prefix}/info/name`, { signal: AbortSignal.timeout(5000) });
      return response.ok ? (await response.json()).uid : null;
    } catch {
      return null;
    }
  }, tunnel), { timeout: 15_000 }).toBe(testWallboxUID);
}

test('hardware smoke: account', async ({ page }) => {
  await registerAccount(page, email, 'Upgrade Smoke', setting('SMOKE_ACCOUNT_PASSWORD'));
});

test('hardware smoke: register', async ({ page }) => {
  await expectDevice(page.request, testWallboxUID);
  const { users, enable, relay_host, relay_port, cert_id, mtu } = await getDeviceConfig(page.request);
  expect(users, 'Use a dedicated test device with no registered remote access accounts').toEqual([]);

  await login(page, email, setting('SMOKE_ACCOUNT_PASSWORD'));
  const account = await page.request.get(`${testDomain}/api/user/me`);
  expect(account.ok()).toBe(true);
  const { id: userId } = await account.json();
  expect(userId).toMatch(/^[0-9a-f-]{36}$/i);
  const backup: Backup = {
    uid: testWallboxUID,
    userId,
    settings: { enable, relay_host, relay_port, cert_id, mtu },
  };

  await page.getByRole('link', { name: 'Token', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Release upgrade smoke');
  await page.getByRole('button', { name: 'Create token', exact: true }).click();
  const token = await page.getByRole('textbox').nth(1).inputValue();

  await writeFile(stateFile('device-backup.json'), JSON.stringify(backup), { flag: 'wx', mode: 0o600 });
  await configureChargerRelay(page, { host: setting('SMOKE_RELAY_HOST'), port: setting('SMOKE_TCP_PORT') });
  await page.getByRole('row', { name: /0 of \d+ accounts configured/ }).getByRole('button').click();
  await page.getByLabel('Authorization method').selectOption('token');
  await page.getByLabel('Authorization token').fill(token);
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await waitForTokenChargerRegistration(page);

  const registered = await getDeviceConfig(page.request);
  expect(registered.users.map(user => user.uuid)).toContain(userId);
  expect(registered.uuid).toMatch(/^[0-9a-f-]{36}$/i);
  await writeFile(stateFile('registered-uuid'), registered.uuid);
  await page.goto(testDomain);
  await probe(page);
});

test('hardware smoke: probe', async ({ page }) => {
  await expectDevice(page.request, testWallboxUID);
  const registeredUuid = await readFile(stateFile('registered-uuid'), 'utf8');
  expect((await getDeviceConfig(page.request)).uuid).toBe(registeredUuid);
  await login(page, email, setting('SMOKE_ACCOUNT_PASSWORD'));
  await probe(page);
});

test('hardware smoke: cleanup', async ({ page }) => {
  const backup: Backup = JSON.parse(await readFile(stateFile('device-backup.json'), 'utf8'));
  await expectDevice(page.request, backup.uid);
  const { users } = await getDeviceConfig(page.request);
  const testUser = users.find(user => user.uuid === backup.userId);
  if (testUser) {
    await devicePut(page.request, '/remote_access/remove_user', { id: testUser.id });
    await expect.poll(async () => (await getDeviceConfig(page.request)).users.map(user => user.uuid))
      .not.toContain(backup.userId);
  }
  await devicePut(page.request, '/remote_access/config_update', { ...backup.settings, email: '' });
  await expect.poll(() => getDeviceConfig(page.request)).toMatchObject({ ...backup.settings });
});
