import test, { expect } from "@playwright/test";
import { login, testPassword, testUser1Email } from "../tests/common";

test('delete account', async ({ page }) => {
  await login(page, testUser1Email, testPassword);

    await page.getByRole('link', { name: 'User' }).click();
    await page.getByRole('button', { name: 'Delete account' }).click();
    await page.getByPlaceholder('Password').click();
    await page.getByPlaceholder('Password').fill(testPassword);
    await page.getByPlaceholder('Password').press('Enter');
    await expect(page.getByText('LoginRegisterEmail-')).toBeVisible();
});
