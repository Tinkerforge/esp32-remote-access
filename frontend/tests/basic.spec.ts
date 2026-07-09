import { test, expect, Page } from '@playwright/test';
import { login, mailiskClient, mailiskNameSpace, needCustomCert, testDomain, testDomainForCharger, testPassword1, testPassword2, testUser1Email, testUser2, testUser2Email, testUserName1, testUserName2, testWallboxDomain, testWallboxUID, waitForPasswordChargerRegistration, waitForTokenChargerRegistration } from './common';

async function createToken(page: Page, name: string, useOnce = true) {
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

async function deleteToken(page: Page, name: string) {
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

async function expectVisibleTokenOrder(page: Page, expectedNames: string[]) {
  await expect.poll(() => visibleTokenNames(page, expectedNames), { timeout: 5_000 }).toEqual(expectedNames);
}

function testOrigin() {
  return new URL(testDomain).origin;
}


test('has title', async ({ page }) => {
  await page.goto(testDomain);

  await expect(page).toHaveTitle(/Remote Access/);
});

test('login', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
});


test('show account page', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);
  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByLabel('Name')).toHaveValue(testUserName1);
});

test('invalid register form', async ({ page }) => {
  await page.goto(testDomain);
  await page.getByRole('tab', { name: 'Register' }).click();
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('The name must not be empty')).toBeVisible();
  await expect(page.getByText('The email-address must not be')).toBeVisible();
  await expect(page.getByText('Must be at least')).toBeVisible();
});

test('register form validates password confirmation and legal acceptances before submit', async ({ page }) => {
  await page.goto(testDomain);
  await page.getByRole('tab', { name: 'Register' }).click();

  await page.getByPlaceholder('John Doe').fill('Validation User');
  await page.getByRole('textbox', { name: 'Email-address' }).fill(`validation-${Date.now()}@example.invalid`);
  await page.getByRole('textbox', { name: 'Password', exact: true }).fill('ValidPassword123!');
  await page.getByRole('textbox', { name: 'Confirm password' }).fill('DifferentPassword123!');

  await page.getByRole('button', { name: 'Register' }).click();

  await expect(page.getByText('Passwords do not match')).toBeVisible();
  await expect(page.locator('.form-check-input.is-invalid')).toHaveCount(2);

  await page.getByText('I have read, understood and I am accepting the privacy notice.').click();
  await expect(page.locator('.form-check-input.is-invalid')).toHaveCount(1);

  await page.getByText('I have read, understood and I am accepting the terms and conditions.').click();
  await expect(page.locator('.form-check-input.is-invalid')).toHaveCount(0);

  await page.getByRole('textbox', { name: 'Confirm password' }).fill('ValidPassword123!');
  await expect(page.getByText('Passwords do not match')).not.toBeVisible();
});


test('invalid login attempts', async ({ page }) => {
  await page.goto(testDomain);

  await page.getByRole('textbox', { name: 'Email' }).fill(testUser1Email);
  await page.getByRole('textbox', { name: 'Password' }).fill('wrong_password');
  await page.getByRole('button', { name: 'Login' }).click();
  await expect(page.getByText('Email-address or password wrong.')).toBeVisible();

  await page.getByRole('textbox', { name: 'Email' }).fill('nonexistent@example.com');
  await page.getByRole('textbox', { name: 'Password' }).fill(testPassword1);
  await page.getByRole('button', { name: 'Login' }).click();
  await expect(page.getByText('Email-address or password wrong.')).toBeVisible();
});

test('requests password reset from the login form', async ({ page }) => {
  await page.goto(testDomain);

  const recoveryEmail = `reset-${Date.now()}@example.invalid`;
  await page.getByRole('textbox', { name: 'Email' }).fill(recoveryEmail);
  await page.getByRole('link', { name: 'Password reset' }).click();

  const dialog = page.getByRole('dialog').filter({ hasText: 'Password reset' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: 'Email-address' })).toHaveValue(recoveryEmail);

  const recoveryResponse = page.waitForResponse((response) =>
    response.url().includes('/api/auth/start_recovery') &&
    response.request().method() === 'GET'
  );
  await dialog.getByRole('button', { name: 'Send' }).click();
  expect((await recoveryResponse).status()).toBe(200);

  await expect(dialog).not.toBeVisible();
  await expect(page.getByText('You should receive an email within the next few minutes.')).toBeVisible();
});

test('navigation and logout', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Token' }).click();
  await expect(page.getByRole('heading', { name: 'Create authorization token' })).toBeVisible();

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByRole('heading', { name: 'Account information' })).toBeVisible();

  await page.getByRole('button', { name: 'Logout', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Login' })).toBeVisible();
});

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

test('account information validation', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();

  await expect(page.getByLabel('Name')).toBeVisible();
  await page.getByLabel('Name').clear();
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByText('The name must not be empty')).toBeVisible();

  await page.getByLabel('Email-address').clear();
  await page.getByLabel('Email-address').fill(testUser1Email);
  await page.getByRole('button', { name: 'Save changes' }).click();
});

test('password change dialog validation', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await page.getByRole('button', { name: 'Change password' }).click();

  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill('weak');
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('Must be at least 8 characters long.')).toBeVisible();

  await page.getByLabel('Current password').clear();
  await page.getByLabel('Current password').fill('wrong_password');
  await page.getByRole('textbox', { name: 'New password', exact: true }).clear();
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill('ValidPassword123!');
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('Passwords do not match')).toBeVisible();
});

test('account local settings and account action modals', async ({ page }) => {
  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByRole('button', { name: 'Save changes' })).toBeDisabled();

  const debugSwitch = page.getByLabel('Debug mode');
  await debugSwitch.check();
  await expect(page.getByText(/Storage persisted:/)).toBeVisible();
  await expect.poll(() => page.evaluate(() => localStorage.getItem('debugMode'))).toBe('true');

  await debugSwitch.uncheck();
  await expect(page.getByText(/Storage persisted:/)).not.toBeVisible();
  await expect.poll(() => page.evaluate(() => localStorage.getItem('debugMode'))).toBeNull();

  await page.getByRole('button', { name: 'Save recovery file' }).click();
  const recoveryDialog = page.getByRole('dialog').filter({ hasText: 'Save recovery file' });
  await expect(recoveryDialog).toBeVisible();
  await recoveryDialog.getByRole('button', { name: 'Download' }).click();
  await expect(recoveryDialog.getByText('Wrong password. Could not decrypt secret.')).toBeVisible();
  await recoveryDialog.locator('.modal-footer').getByRole('button', { name: 'Close' }).click();
  await expect(recoveryDialog).not.toBeVisible();

  await page.getByRole('button', { name: 'Delete account' }).click();
  const deleteDialog = page.getByRole('dialog').filter({ hasText: 'Delete account' });
  await expect(deleteDialog).toBeVisible();
  await deleteDialog.getByPlaceholder('Password').fill('not-used-in-this-test');
  await expect(deleteDialog.getByPlaceholder('Password')).toHaveValue('not-used-in-this-test');

  await deleteDialog.getByRole('button', { name: 'Close' }).click();
  await expect(deleteDialog).not.toBeVisible();
});

test('charger lifecycle', async ({ page }) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('button', { name: 'Show' }).click();
  await page.getByRole('textbox', { name: 'Relay server hostname or IP' }).fill(testDomainForCharger);
  if (needCustomCert) {
    await page.getByLabel('TLS certificate', { exact: true }).selectOption('0');
  } else {
    await page.getByLabel('TLS certificate', { exact: true }).selectOption('-1');
  }
  await page.getByRole('row', { name: 'of 5 accounts configured' }).getByRole('button').click();
  await page.getByRole('textbox', { name: 'Email address' }).click();
  await page.getByRole('textbox', { name: 'Email address' }).fill(testUser1Email);
  await page.getByLabel('Passwordonly used for the reg').fill(testPassword1);
  await page.getByLabel('Passwordonly used for the reg').press('Enter');
  await waitForPasswordChargerRegistration(page);

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();

  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  // await page.getByRole('button', { name: 'Reboot' }).click();
  await page.waitForTimeout(6000);
  await page.goto(testDomain);
  await expect(page.getByText('No devices registered yet. Please connect your device to this account to get started.')).toBeVisible();
});

test('add charger with auth token', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Token' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Test');
  await page.getByRole('button', { name: 'Create token' }).click();
  const token = await page.getByRole('textbox').nth(1).inputValue();

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

  await page.goto(testDomain);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);
  await expect(page.locator('.bg-success').first()).toBeVisible({timeout: 100_000});
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Status' })).toBeVisible({timeout: 15_000});
  await page.locator('#interface').contentFrame().getByRole('button', { name: 'Close remote access' }).click();
});

test('edit charger note', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  const row = page.getByRole('row', { name: testWallboxUID });
  await expect(row).toBeVisible();

  // The Edit button is the only button in the note cell (the one with class `pe-0`).
  const editButton = row.locator('td.pe-0 button');
  await expect(editButton).toBeVisible();
  await editButton.click();

  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('textbox')).toBeVisible();

  const newNote = 'Test integration note ' + Date.now();
  await dialog.getByRole('textbox').fill(newNote);
  await dialog.getByRole('button', { name: 'Accept' }).click();

  await expect(dialog).not.toBeVisible();
  await expect(row).toContainText(newNote);
});

test('edit charger note and cancel', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  const row = page.getByRole('row', { name: testWallboxUID });
  const editButton = row.locator('td.pe-0 button');

  await editButton.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();

  const cancelledNote = 'Cancelled note ' + Date.now();
  await dialog.getByRole('textbox').fill(cancelledNote);
  await dialog.getByRole('button', { name: 'Decline' }).click();

  await expect(dialog).not.toBeVisible();
  await expect(row).not.toContainText(cancelledNote);
});

test('edit charger note modal pre-fills current note', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  const row = page.getByRole('row', { name: testWallboxUID });
  const editButton = row.locator('td.pe-0 button');

  // Set a known note first.
  await editButton.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  const knownNote = 'Known pre-fill note ' + Date.now();
  await dialog.getByRole('textbox').fill(knownNote);
  await dialog.getByRole('button', { name: 'Accept' }).click();
  await expect(dialog).not.toBeVisible();
  await expect(row).toContainText(knownNote);

  // Reopen the modal and verify it pre-fills with the saved note.
  await editButton.click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('textbox')).toHaveValue(knownNote);
});

test('edit charger note persists after reload', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  const row = page.getByRole('row', { name: testWallboxUID });
  const editButton = row.locator('td.pe-0 button');

  await editButton.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  const noteText = 'Persistence test note ' + Date.now();
  await dialog.getByRole('textbox').fill(noteText);
  await dialog.getByRole('button', { name: 'Accept' }).click();
  await expect(dialog).not.toBeVisible();

  await page.reload();
  await expect(page.locator('tbody')).toContainText(noteText);
});

test('clear charger note', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  const row = page.getByRole('row', { name: testWallboxUID });
  const editButton = row.locator('td.pe-0 button');

  // Set a known note first so we have something to clear.
  await editButton.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('textbox').fill('Some note to clear');
  await dialog.getByRole('button', { name: 'Accept' }).click();
  await expect(dialog).not.toBeVisible();

  // Clear the note by submitting an empty value.
  await editButton.click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('textbox').fill('');
  await dialog.getByRole('button', { name: 'Accept' }).click();
  await expect(dialog).not.toBeVisible();

  // Reload and verify the modal opens with an empty textarea,
  // confirming the cleared note was persisted on the server.
  await page.reload();
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  await page.getByRole('row', { name: testWallboxUID }).locator('td.pe-0 button').click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('textbox')).toHaveValue('');
});

test('recovery page working', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await page.getByRole('button', { name: 'Connect' }).click();
  const url = page.url();
  await page.goto(url + "/recovery");
  await expect(page.locator('#interface').contentFrame().getByRole('heading', { name: 'Firmware update' })).toBeVisible({timeout: 15_000});
  await expect(page.locator('#interface').contentFrame().getByRole('link', { name: 'logo' })).not.toBeVisible();
})

test('change accountname', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByLabel('Email-address')).toHaveValue(testUser1Email);
  await page.getByLabel('Email-address').fill(testUser2Email);
  await page.getByLabel('Name').fill(testUserName2);
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByLabel('Email-address')).toBeVisible();
  await page.getByLabel('Name').click();
  await page.getByRole('button', { name: 'Logout', exact: true }).click();


  const inbox = await mailiskClient?.searchInbox(mailiskNameSpace, { to_addr_prefix:  testUser2, from_timestamp: (Date.now() / 1000) - 5 });
  if (!inbox?.data || inbox.data.length === 0) {
      throw new Error("No emails found in inbox");
  }
  const firstEmail = inbox.data[0];
  if (!firstEmail || !firstEmail.text) {
      throw new Error("Email data is invalid");
  }
  const idx = firstEmail.text.indexOf(`[${testDomain}/api/auth/verify?`) + 1;
  if (idx === 0) {
      throw new Error("Failed to find verification URL in email");
  }
  const url = firstEmail.text.substring(idx, firstEmail.text.indexOf("]", idx));
  const response = await fetch(url);
  if (response.status !== 200) {
      throw new Error("Failed to verify email");
  }

  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await expect(page.getByRole('heading', { name: 'Account information' })).toBeVisible();
});

test('change password', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await login(page, testUser2Email, testPassword1);

  await page.getByRole('link', { name: 'Account' }).click();
  await page.getByRole('button', { name: 'Change password' }).click();
  await page.getByLabel('Current password').click();
  await page.getByLabel('Current password').fill(testPassword1);
  await page.getByRole('textbox', { name: 'New password', exact: true }).fill(testPassword2);
  await page.getByRole('textbox', { name: 'Confirm new password' }).fill(testPassword2);
  await page.getByRole('dialog').getByRole('button', { name: 'Change password' }).click();
  await expect(page.getByText('LoginRegisterEmail-')).toBeVisible();

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

test('remove charger', async ({page}) => {
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');
  await page.goto(testWallboxDomain + '/#status');
  await page.getByRole('button', { name: 'System' }).click();
  await page.getByRole('button', { name: 'Remote Access' }).click();
  await page.getByRole('row', { name: testUser1Email }).getByRole('button').click();
  await page.getByRole('button', { name: 'Save' }).click();
  // await page.getByRole('button', { name: 'Reboot' }).click();
  await page.waitForTimeout(6000);
  await login(page, testUser2Email, testPassword2);
  await expect(page.getByText('No devices registered yet. Please connect your device to this account to get started.')).toBeVisible();
});
