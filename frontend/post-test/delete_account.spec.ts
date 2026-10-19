import test, { expect } from "@playwright/test";
import { login, testPassword2, testUser2Email } from "../tests/common";

test('delete account', async ({ page }) => {
    await login(page, testUser2Email, testPassword2);

    await page.getByRole('link', { name: testUser2Email, exact: true }).click();
    await page.getByRole('button', { name: 'Delete account' }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByLabel('Password', { exact: true }).fill(testPassword2);
    const deleteResponse = page.waitForResponse((response) =>
        new URL(response.url()).pathname === '/api/user/delete' && response.request().method() === 'DELETE'
    );
    await dialog.getByRole('button', { name: 'Delete account', exact: true }).click();
    expect((await deleteResponse).status()).toBe(200);
    await expect(page.getByRole('button', { name: 'Login', exact: true })).toBeVisible();
});
