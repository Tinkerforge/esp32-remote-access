import { test, expect, Page, Locator, Dialog } from '@playwright/test';
import { login, testUser2Email, testPassword2, testWallboxUID, testWallboxDomain, ensureChargerConnected, ensureChargerDisconnected } from './common';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function openGroupingModal(page: Page): Promise<Locator> {
    await page.getByRole('button', { name: 'Manage Groups' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    return dialog;
}

async function closeGroupingModal(dialog: Locator): Promise<void> {
    await dialog.locator('.modal-footer button.btn-secondary', { hasText: 'Close' }).click();
    await expect(dialog).not.toBeVisible();
}

function getGroupingRow(dialog: Locator, name: string): Locator {
    return dialog.locator('.list-group-item', { hasText: name });
}

function getEditButtonForGrouping(row: Locator): Locator {
    return row.locator('button.btn-outline-primary');
}

function getDeleteButtonForGrouping(row: Locator): Locator {
    return row.locator('button.btn-outline-danger');
}

function getSetAsDefaultCheckbox(row: Locator): Locator {
    return row.locator('input[type="checkbox"]');
}

function getDeviceCheckbox(dialog: Locator, deviceUid: string): Locator {
    return dialog.locator('.form-check', { hasText: deviceUid }).locator('input[type="checkbox"]');
}

async function deleteAllGroupings(page: Page): Promise<void> {
    const dialog = await openGroupingModal(page);

    const dialogHandler = (d: Dialog) => d.accept().catch(() => {});
    page.on('dialog', dialogHandler);

    try {
        let prevCount = await dialog.locator('.list-group-item').count();
        let attempts = 0;

        while (prevCount > 0 && attempts < 50) {
            // Click the first delete button
            const firstDeleteBtn = dialog.locator('.list-group-item').first().locator('button.btn-outline-danger');
            await firstDeleteBtn.click();

            // Wait for the list to update (one fewer item)
            await expect(dialog.locator('.list-group-item')).toHaveCount(prevCount - 1, { timeout: 10_000 });
            prevCount--;
            attempts++;
        }
    } finally {
        page.off('dialog', dialogHandler);
    }

    await closeGroupingModal(dialog);
}

/**
  * Wait for a grouping-related API call to complete.
  * Matches POST /grouping/create, PUT /grouping/edit, DELETE /grouping/delete,
  * POST /grouping/add_device, and DELETE /grouping/remove_device.
  */
function waitForGroupingApi(page: Page, method: string, endpoint: string) {
    return page.waitForResponse(
        (r) => r.url().includes(`/api${endpoint}`) && r.request().method() === method,
        { timeout: 15_000 }
    );
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

test.describe('GroupingModal', () => {
        test.beforeAll(async ({ browser }) => {
                test.slow();
                test.skip(!testWallboxUID, 'Requires TEST_WALLBOX_UID environment variable');
                test.skip(!testWallboxDomain, 'Requires TEST_WALLBOX_DOMAIN environment variable');
                test.skip(!testUser2Email, 'Requires MAILISK_USER2 and MAILISK_NAMESPACE environment variables');

                // Ensure a charger for testing is connected to the user2 account before
                // any test runs. If the wallbox is already registered, this is a no-op;
                // otherwise it registers one via the auth-token flow.
                const context = await browser.newContext();
                const page = await context.newPage();
                try {
                    await ensureChargerConnected(page, testUser2Email, testPassword2);
                } finally {
                    await context.close();
                }
            });

            test.afterAll(async ({ browser }) => {
                test.slow();
                test.skip(!testWallboxUID, 'Requires TEST_WALLBOX_UID environment variable');
                test.skip(!testWallboxDomain, 'Requires TEST_WALLBOX_DOMAIN environment variable');
                test.skip(!testUser2Email, 'Requires MAILISK_USER2 and MAILISK_NAMESPACE environment variables');

                // Remove the wallbox from the user2 account so that subsequent test
                // runs (or manual exploration) start from a known state. If the wallbox
                // is already disconnected this is a no-op.
                const context = await browser.newContext();
                const page = await context.newPage();
                try {
                    await ensureChargerDisconnected(page, testUser2Email);
                } finally {
                    await context.close();
                }
            });

    test.beforeEach(async ({ page }) => {
        test.slow();
        test.skip(!testWallboxUID, 'Requires TEST_WALLBOX_UID environment variable');
        test.skip(!testWallboxDomain, 'Requires TEST_WALLBOX_DOMAIN environment variable');
        test.skip(!testUser2Email, 'Requires MAILISK_USER2 and MAILISK_NAMESPACE environment variables');

        await login(page, testUser2Email, testPassword2);

        // The login() helper only awaits the /api/auth/login response; the page
        // then has to fetch the secret salt, decrypt the secret key, and switch
        // to the devices view. Wait for the device list to actually render and
        // the wallbox row to appear before continuing.
        await expect(page.locator('tbody')).toContainText(testWallboxUID!, { timeout: 30_000 });

        // Clean up any existing groupings so each test starts from a clean state
        await deleteAllGroupings(page);
    });

    test('opens modal and shows empty state', async ({ page }) => {
        const dialog = await openGroupingModal(page);
        await expect(dialog.getByText('No groups created yet')).toBeVisible();
        await expect(dialog.getByRole('button', { name: 'Create Group' })).toBeVisible();
        await closeGroupingModal(dialog);
    });

    test('closes modal via X button', async ({ page }) => {
        const dialog = await openGroupingModal(page);
        // The X close button is a <button class="btn-close" aria-label="Close">
        // in the modal header. The footer also has a "Close" button, so we
        // disambiguate by targeting the X button specifically.
        await dialog.locator('.btn-close').click();
        await expect(dialog).not.toBeVisible();
    });

    test('creates a new grouping with a device', async ({ page }) => {
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();

        // Form should appear
        await expect(dialog.getByPlaceholder('Enter group name...')).toBeVisible();

        const name = `Test Group ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(name);

        // Select the wallbox device
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        // Save and wait for API calls
        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        const addDeviceResponse = waitForGroupingApi(page, 'POST', '/grouping/add_device');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;
        await addDeviceResponse;

        // Should return to list view with the new grouping
        await expect(dialog.locator('.list-group-item', { hasText: name })).toBeVisible();
        await closeGroupingModal(dialog);
    });

    test('cancels grouping creation', async ({ page }) => {
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();

        await dialog.getByPlaceholder('Enter group name...').fill('Will be cancelled');
        await dialog.getByRole('button', { name: 'Cancel' }).click();

        // Should return to list view with no groupings
        await expect(dialog.getByText('No groups created yet')).toBeVisible();
        await closeGroupingModal(dialog);
    });

    test('edits a grouping name', async ({ page }) => {
        // First create a grouping
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const originalName = `Original ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(originalName);
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;

        // Now edit it
        const row = getGroupingRow(dialog, originalName);
        await getEditButtonForGrouping(row).click();

        // Form should be pre-filled with the original name
        await expect(dialog.getByPlaceholder('Enter group name...')).toHaveValue(originalName);

        const newName = `Edited ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(newName);

        const editResponse = waitForGroupingApi(page, 'PUT', '/grouping/edit');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await editResponse;

        // Old name should be gone, new name should be visible
        await expect(dialog.locator('.list-group-item', { hasText: originalName })).not.toBeVisible();
        await expect(dialog.locator('.list-group-item', { hasText: newName })).toBeVisible();

        await closeGroupingModal(dialog);
    });

    test('deletes a grouping', async ({ page }) => {
        // First create a grouping
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const name = `To Delete ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(name);
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;

        // Delete it (auto-accept the confirm dialog)
        page.on('dialog', (d) => d.accept().catch(() => {}));
        const row = getGroupingRow(dialog, name);
        const deleteResponse = waitForGroupingApi(page, 'DELETE', '/grouping/delete');
        await getDeleteButtonForGrouping(row).click();
        await deleteResponse;

        await expect(dialog.locator('.list-group-item', { hasText: name })).not.toBeVisible();
        await closeGroupingModal(dialog);
    });

    test('does not delete when confirm is cancelled', async ({ page }) => {
        // First create a grouping
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const name = `Keep Me ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(name);
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;

        // Try to delete but dismiss the confirm dialog
        page.on('dialog', (d) => d.dismiss().catch(() => {}));
        const row = getGroupingRow(dialog, name);
        await getDeleteButtonForGrouping(row).click();

        // The grouping should still be visible
        await expect(row).toBeVisible();
        await closeGroupingModal(dialog);
    });

    test('sets grouping as default via per-row checkbox', async ({ page }) => {
        // Create a grouping (not default)
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const name = `Default Test ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(name);
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;

        // The new grouping should not show the Default badge yet
        const row = getGroupingRow(dialog, name);
        await expect(row.locator('.badge', { hasText: 'Default' })).not.toBeVisible();

        // Click the per-row set-as-default checkbox
        const setDefaultResponse = waitForGroupingApi(page, 'PUT', '/grouping/edit');
        await getSetAsDefaultCheckbox(row).click();
            await setDefaultResponse;
            await getSetAsDefaultCheckbox(row).isChecked();

        // Should now show the Default badge
        await expect(row.locator('.badge', { hasText: 'Default' })).toBeVisible();

        await closeGroupingModal(dialog);
    });

    test('shows replaces message when setting as default in edit form', async ({ page }) => {
        // Create first grouping and set as default
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const firstName = `First Default ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(firstName);
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;

        // Set as default via per-row checkbox
        const firstRow = getGroupingRow(dialog, firstName);
        const setDefaultResponse = waitForGroupingApi(page, 'PUT', '/grouping/edit');
        await getSetAsDefaultCheckbox(firstRow).click();
            await setDefaultResponse;
            await getSetAsDefaultCheckbox(firstRow).isChecked();

        // Create second grouping
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const secondName = `Second ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(secondName);
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const createResponse2 = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse2;

        // Edit the second grouping and check "Set as default"
        const secondRow = getGroupingRow(dialog, secondName);
        await getEditButtonForGrouping(secondRow).click();

        // The "Set as default" checkbox in the edit form has id "set-as-default"
        const setAsDefaultCheckbox = dialog.locator('#set-as-default');
        await setAsDefaultCheckbox.check();

        // The replaces message should appear, mentioning the first grouping's name
        await expect(dialog.getByText(`This will replace "${firstName}" as the default group.`)).toBeVisible();

        // Cancel
        await dialog.getByRole('button', { name: 'Cancel' }).click();
        await closeGroupingModal(dialog);
    });

    test('searches devices in edit form', async ({ page }) => {
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();

        // The device search input should be visible
        const searchInput = dialog.getByPlaceholder('Search devices');
        await expect(searchInput).toBeVisible();

        // The wallbox should be visible initially
        await expect(dialog.locator('.form-check', { hasText: testWallboxUID! })).toBeVisible();

        // Type a search query that doesn't match
        await searchInput.fill('nonexistent-device-xyz');
        await expect(dialog.locator('.form-check', { hasText: testWallboxUID! })).not.toBeVisible();

        // Clear and verify the device is back
        await searchInput.fill('');
        await expect(dialog.locator('.form-check', { hasText: testWallboxUID! })).toBeVisible();

        // Cancel
        await dialog.getByRole('button', { name: 'Cancel' }).click();
        await closeGroupingModal(dialog);
    });

    test('validates required group name', async ({ page }) => {
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();

        // The name input has the `required` attribute, so the browser blocks
        // submission. Check the validity state.
        const nameInput = dialog.getByPlaceholder('Enter group name...');
        const isInvalid = await nameInput.evaluate((el: HTMLInputElement) => !el.checkValidity());
        expect(isInvalid).toBe(true);

        await dialog.getByRole('button', { name: 'Cancel' }).click();
        await closeGroupingModal(dialog);
    });

    test('adds and removes devices from grouping', async ({ page }) => {
        // Create a grouping with no devices
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const name = `Device Test ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(name);

        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;

        // Verify it starts with 0 devices
        let row = getGroupingRow(dialog, name);
        await expect(row).toContainText('0');

        // Edit and add a device
        await getEditButtonForGrouping(row).click();
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const addDeviceResponse = waitForGroupingApi(page, 'POST', '/grouping/add_device');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await addDeviceResponse;

        // Should now show 1 device
        row = getGroupingRow(dialog, name);
        await expect(row).toContainText('1');

        // Edit and remove the device
        await getEditButtonForGrouping(row).click();
        await getDeviceCheckbox(dialog, testWallboxUID!).uncheck();

        const removeDeviceResponse = waitForGroupingApi(page, 'DELETE', '/grouping/remove_device');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await removeDeviceResponse;

        // Should now show 0 devices
        row = getGroupingRow(dialog, name);
        await expect(row).toContainText('0');

        await closeGroupingModal(dialog);
    });

    test('persists grouping after page reload', async ({ page }) => {
        // Create a grouping
        const dialog = await openGroupingModal(page);
        await dialog.getByRole('button', { name: 'Create Group' }).click();
        const name = `Persistence Test ${Date.now()}`;
        await dialog.getByPlaceholder('Enter group name...').fill(name);
        await getDeviceCheckbox(dialog, testWallboxUID!).check();

        const createResponse = waitForGroupingApi(page, 'POST', '/grouping/create');
        await dialog.getByRole('button', { name: 'Save' }).click();
        await createResponse;

        await closeGroupingModal(dialog);

        // Reload and verify the grouping still exists
        await page.reload();
        await expect(page.locator('tbody')).toContainText(testWallboxUID!);

        const dialog2 = await openGroupingModal(page);
        await expect(dialog2.locator('.list-group-item', { hasText: name })).toBeVisible();
        await closeGroupingModal(dialog2);
    });
});
