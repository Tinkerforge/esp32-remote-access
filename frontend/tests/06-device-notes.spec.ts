import { test, expect } from '@playwright/test';
import { login, testPassword1, testUser1Email, testWallboxDomain, testWallboxUID } from './common';

test('edit charger note', async ({page}) => {
  test.slow();
  test.skip(!testWallboxDomain || !testWallboxUID, 'Requires TEST_WALLBOX_DOMAIN and TEST_WALLBOX_UID environment variables');

  await login(page, testUser1Email, testPassword1);
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  const row = page.getByRole('row', { name: testWallboxUID });
  await expect(row).toBeVisible();

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

  await editButton.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  const knownNote = 'Known pre-fill note ' + Date.now();
  await dialog.getByRole('textbox').fill(knownNote);
  await dialog.getByRole('button', { name: 'Accept' }).click();
  await expect(dialog).not.toBeVisible();
  await expect(row).toContainText(knownNote);

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

  await editButton.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('textbox').fill('Some note to clear');
  await dialog.getByRole('button', { name: 'Accept' }).click();
  await expect(dialog).not.toBeVisible();

  await editButton.click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('textbox').fill('');
  await dialog.getByRole('button', { name: 'Accept' }).click();
  await expect(dialog).not.toBeVisible();

  await page.reload();
  await expect(page.locator('tbody')).toContainText(testWallboxUID);

  await page.getByRole('row', { name: testWallboxUID }).locator('td.pe-0 button').click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('textbox')).toHaveValue('');
});
