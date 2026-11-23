import { test, expect } from '@playwright/test';
import { connectToChargerRemotely, login, readBootIdThroughTunnel, testPassword1, testPassword2, testUser1Email, testUser2Email, testUserName2, testWallboxDomain, testWallboxUID, verifyEmail } from './common';

test.describe.configure({ mode: 'serial' });

test('change accountname', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: testUser1Email, exact: true }).click();
  await expect(page.getByLabel('Email-address')).toHaveValue(testUser1Email);
  await page.getByLabel('Email-address').fill(testUser2Email);
  await page.getByLabel('Name').fill(testUserName2);
  const sentAfter = Math.floor(Date.now() / 1000);
  const updateResponse = page.waitForResponse((response) =>
    new URL(response.url()).pathname === '/api/user/update_user' && response.request().method() === 'PUT'
  );
  await page.getByRole('button', { name: 'Save changes' }).click();
  expect((await updateResponse).status(), 'The destination test email must not already belong to another account').toBe(200);
  await expect(page.getByRole('button', { name: 'Save changes' })).toBeDisabled();
  await expect(page.getByLabel('Email-address')).toHaveValue(testUser2Email);
  await expect(page.getByRole('link', { name: testUser2Email, exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Logout', exact: true }).click();

  await verifyEmail(page, testUser2Email, sentAfter);

  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: testUser2Email, exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Account information' })).toBeVisible();
});

test('change password', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: testUser2Email, exact: true }).click();
  await page.getByRole('button', { name: 'Change password' }).click();
  await page.getByLabel('Current password').click();
  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill(testPassword2);
  await page.getByRole('textbox', { name: 'Confirm new password' }).fill(testPassword2);
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByRole('button', { name: 'Login', exact: true })).toBeVisible();

  await login(page, testUser2Email, testPassword2);
});

test('connect to charger with new password', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await login(page, testUser2Email, testPassword2);

  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();
});

test('an open remote access session reconnects after the charger reboots', async ({ page }) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID');
  test.setTimeout(240_000);

  const device = new URL(testWallboxDomain);
  const info = await page.request.get(new URL('/info/name', device).href, { timeout: 5000 });
  expect(info.ok(), 'Reading the configured charger identity').toBe(true);
  expect((await info.json()).uid).toBe(testWallboxUID);

  await login(page, testUser2Email, testPassword2);
  await connectToChargerRemotely(page, 30_000);

  const iframe = page.locator('#interface');
  const status = iframe.contentFrame().getByRole('heading', { name: 'Status', exact: true });
  await expect(status).toBeVisible({ timeout: 45_000 });

  const src = (await iframe.getAttribute('src')) ?? '';
  const tunnel = new URL(src, page.url()).pathname.split('/').slice(0, 2).join('/');
  expect(tunnel).toMatch(/^\/wg-[^/]+$/);
  const pageUrl = page.url();
  const documentStart = await page.evaluate(() => performance.timeOrigin);

  let bootIdBefore: number | null = null;
  await expect.poll(async () => {
    bootIdBefore = await readBootIdThroughTunnel(page, tunnel);
    return bootIdBefore;
  }, { message: 'Read the boot ID over the open tunnel', timeout: 15_000 }).not.toBeNull();

  // Reboot over the LAN so the remote tab receives no navigation or UI input.
  const reboot = await page.request.put(new URL('/reboot', device).href, {
    data: 'null',
    headers: { 'Content-Type': 'application/json' },
    timeout: 5000,
  });
  expect(reboot.ok(), 'The charger accepted the reboot command').toBe(true);

  await expect.poll(async () => {
    const bootId = await readBootIdThroughTunnel(page, tunnel);
    return bootId !== null && bootId !== bootIdBefore;
  }, {
    message: 'The original browser tunnel responds with a new boot ID',
    timeout: 120_000,
    intervals: [1000, 2000, 5000],
  }).toBe(true);

  await expect(status).toBeVisible({ timeout: 15_000 });
  await expect(iframe).toHaveAttribute('src', src);
  await expect(page).toHaveURL(pageUrl);
  expect(await page.evaluate(() => performance.timeOrigin), 'The remote tab stayed open').toBe(documentStart);
});

test('remove charger', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await page.goto(`${testWallboxDomain}/#status`);
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  // The charger can refresh the displayed email when it reconnects to the relay.
  const accountRow = page.getByRole('row', { name: testUser1Email })
    .or(page.getByRole('row', { name: testUser2Email }));
  await accountRow.getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  await page.waitForTimeout(6000);
  await login(page, testUser2Email, testPassword2);
  await expect(page.getByText('No devices registered yet. Please connect your device to this account to get started.')).toBeVisible();
});
