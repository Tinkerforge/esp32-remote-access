import { test } from "@playwright/test";
import { testDomain, testPassword1, testUser1, mailiskClient, mailiskNameSpace, testUserName1, testUser1Email } from "../tests/common";


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
    await page.getByRole('button', { name: 'Register' }).click();

    // Wait for the recovery modal to appear and complete the recovery data flow
    await page.locator('.modal').getByRole('button', { name: 'Save' }).click();
    await page.locator('.modal').getByRole('checkbox', { name: 'I have downloaded and safely stored the recovery file' }).check();
    await page.locator('.modal-footer').getByRole('button', { name: 'Close' }).click();

    if (!mailiskClient) {
        throw new Error('Mailisk client is not configured');
    }
    const inbox = await mailiskClient.searchInbox(mailiskNameSpace, { to_addr_prefix:  testUser1, from_timestamp: (Date.now() / 1000) - 5 });

    if (!inbox.data || inbox.data.length === 0) {
        throw new Error("No emails found in inbox");
    }

    const emailText = inbox.data[0].text;
    if (!emailText) {
        throw new Error("Email text is empty");
    }

    const idx = emailText.indexOf(`[${testDomain}/api/auth/verify?`) + 1;
    if (idx === 0) {
        throw new Error("Failed to find verification URL in email");
    }
    const url = emailText.substring(idx, emailText.indexOf("]", idx));
    const response = await fetch(url);
    if (response.status !== 200) {
        throw new Error("Failed to verify email");
    }
});
