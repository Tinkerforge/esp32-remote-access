import { MailiskClient } from "mailisk"

export const testDomain = process.env.TEST_DOMAIN
export const testUser = process.env.TEST_USER
export const testPassword = process.env.TEST_PASSWORD
export const mailiskClient = new MailiskClient({apiKey: process.env.MAILISK_API_KEY});
export const mailiskNameSpace = process.env.MAILISK_NAMESPACE;
