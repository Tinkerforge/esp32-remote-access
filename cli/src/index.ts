#!npx tsx

import { Command, Option } from "commander";
import { argon2Hash, FetchClient } from "./utils.js";
import { Base64 } from "js-base64";
import sodium from "libsodium-wrappers-sumo";
import { readFile, unlink, writeFile } from "node:fs";
import "dotenv/config";

interface Cache {
    cookie: string,
    secretKey: string,
    host: string,
}

const program = new Command();
program
    .version("1.0.0")
    .description("A simple CLI program to call a devices API through the Tinkerforge Remote Access");

program.command("login")
    .description("Logging in as a user. The login is preserved until logged out")
    .addOption(new Option("-e, --email <email>", "Email of user").env("EMAIL"))
    .addOption(new Option("-p, --password <password>", "Password of the user").env("PASSWORD"))
    .addOption(new Option("-h, --host <hostname>", "Hostname of the server. This will also be preserved").env("HOST"))
    .action(async (options) => {
        const host = options.hostname ? options.hostname : "tf-freddy";
        const email = options.email;
        if (!email) {
            console.error("Error: You need to specify an email. Either as environment variable or option");
            return;
        }
        const password = options.password ? options.password : process.env.PASSWORD;
        if (!password) {
            console.error("Error: You need to specify an password. Either as environment variable or option");
            return;
        }

        const fetchClient = new FetchClient(host);
        const getLoginSalt = await fetchClient.fetchClient.GET("/auth/get_login_salt", {params: {query: {email}}});
        if (getLoginSalt.error || !getLoginSalt.data) {
            console.error("Getting login salt returned an error:", getLoginSalt.error);
            return;
        }
        const loginKey = await argon2Hash(password, new Uint8Array(getLoginSalt.data));
        const { response } = await fetchClient.fetchClient.POST("/auth/login", {body: {email, login_key: [...loginKey]}});
        if (response.status !== 200) {
            console.error("Username or password wrong");
            return;
        }
        fetchClient.cookiesRef.cookies = fetchClient.parseCookies(response);
        const getSecret = await fetchClient.fetchClient.GET("/user/get_secret");
        if (getSecret.error || !getSecret.data) {
            console.error("Error while fetching secret: ", getSecret);
        }
        const secretKey = await argon2Hash(password, new Uint8Array(getSecret.data.secret_salt), sodium.crypto_secretbox_KEYBYTES);
        const cache: Cache = {
            cookie: fetchClient.cookiesRef.cookies,
            secretKey: Base64.fromUint8Array(secretKey),
            host
        };
        writeFile("cache", JSON.stringify(cache), () => {});
    })

program.parse(process.argv);
