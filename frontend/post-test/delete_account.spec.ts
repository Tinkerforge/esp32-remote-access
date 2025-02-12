import test, { expect } from "@playwright/test";
import { login, testPassword2, testUser2Email } from "../tests/common";

test('delete account', async ({ page }) => {
    await login(page, testUser2Email, testPassword2);

    await page.getByRole('link', { name: 'User' }).click();
    await page.getByRole('button', { name: 'Delete account' }).click();
    await page.getByPlaceholder('Password').click();
    await page.getByPlaceholder('Password').fill(testPassword2);
    await page.getByPlaceholder('Password').press('Enter');
    await expect(page.getByText('LoginRegisterEmail-')).toBeVisible();
});
