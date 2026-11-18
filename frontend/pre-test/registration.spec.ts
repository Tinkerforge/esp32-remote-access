import { test } from "@playwright/test";
import { registerAccount, testPassword1, testUserName1, testUser1Email, verifyEmail } from "../tests/common";


test('register', async ({ page }) => {
    test.slow();
    const sentAfter = Math.floor(Date.now() / 1000);
    await registerAccount(page, testUser1Email, testUserName1, testPassword1);

    // Wait for the recovery modal to appear and complete the recovery data flow
    await page.locator('.modal').getByRole('button', { name: 'Save' }).click();
    await page.locator('.modal').getByRole('checkbox', { name: 'I have downloaded and safely stored the recovery file' }).check();
    await page.locator('.modal-footer').getByRole('button', { name: 'Close' }).click();

    await verifyEmail(page, testUser1Email, sentAfter);
});
