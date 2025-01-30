import { test } from "@playwright/test";
import { testDomain, testPassword, testUser, mailiskClient, mailiskNameSpace } from "./common";


test('register', async ({ page }) => {
    await page.goto(testDomain);
    await page.getByRole('tab', { name: 'Register' }).click();
    await page.getByRole('textbox', { name: 'Email-address' }).click();
    await page.getByRole('textbox', { name: 'Email-address' }).fill(testUser);
    await page.getByPlaceholder('John Doe').click();
    await page.getByPlaceholder('John Doe').fill('TestUser');
    await page.getByRole('textbox', { name: 'Password' }).click();
    await page.getByRole('textbox', { name: 'Password' }).fill(testPassword);
    await page.getByText('I have read, understood and I am accepting the privacy notice.').click();
    await page.getByText('I have read, understood and I am accepting the terms and conditions.').click();
    await page.getByRole('button', { name: 'Register' }).click();
    await page.getByText('Close').click();
    const inbox = await mailiskClient.searchInbox(mailiskNameSpace);
    const idx = inbox.data[0].text.indexOf("[https://tf-freddy/api/auth/verify?") + 1;
    if (idx === -1) {
        throw new Error("Failed to verify email");
    }
    const url = inbox.data[0].text.substring(idx, inbox.data[0].text.indexOf("]", idx));
    const response = await fetch(url);
    if (response.status !== 200) {
        throw new Error("Failed to verify email");
    }
  });
