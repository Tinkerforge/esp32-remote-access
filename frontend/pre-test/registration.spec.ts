import { test } from "@playwright/test";
import { testDomain, testPassword1, testUserName1, testUser1Email, verifyEmail } from "../tests/common";


test('register', async ({ page }) => {
    test.slow();
    await page.goto(testDomain);
    await page.getByRole('tab', { name: 'Register' }).click();
    await page.getByRole('textbox', { name: 'Email-address' }).click();
    await page.getByRole('textbox', { name: 'Email-address' }).fill(testUser1Email);
    await page.getByPlaceholder('John Doe').click();
    await page.getByPlaceholder('John Doe').fill(testUserName1);
    await page.getByRole('textbox', { name: 'Password', exact: true }).fill(testPassword1);
    await page.getByRole('textbox', { name: 'Confirm password' }).fill(testPassword1);
    await page.getByText('I have read, understood and I am accepting the privacy notice.').click();
    await page.getByText('I have read, understood and I am accepting the terms and conditions.').click();
    const sentAfter = Math.floor(Date.now() / 1000);
    await page.getByRole('button', { name: 'Register' }).click();

    // Wait for the recovery modal to appear and complete the recovery data flow
    await page.locator('.modal').getByRole('button', { name: 'Save' }).click();
    await page.locator('.modal').getByRole('checkbox', { name: 'I have downloaded and safely stored the recovery file' }).check();
    await page.locator('.modal-footer').getByRole('button', { name: 'Close' }).click();

    await verifyEmail(page, testUser1Email, sentAfter);
});
