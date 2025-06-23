import { signal } from "@preact/signals";
import { hash, ArgonType } from "argon2-browser";
import createClient, { Middleware } from "openapi-fetch";
import type { paths } from "./schema";
import sodium from "libsodium-wrappers";
import { logout } from "./components/Navbar";
import i18n from "./i18n";
import { showAlert } from "./components/Alert";
import { Base64 } from "js-base64";

export async function get_salt() {
    const {data, response} = await fetchClient.GET("/auth/generate_salt");
    if (response.status !== 200) {
        throw `Failed to get new salt with ${response.status}: ${await response.text()}`;
    }
    if (!data) throw "No salt data returned";
    return new Uint8Array(data);
}

export async function get_salt_for_user(email: string) {
    const {data, error} = await fetchClient.GET("/auth/get_login_salt", {params: {query: {email: email}}});
    if (error || !data) {
        throw `Failed to get login_salt for user ${email}: ${error}`;
    }
    return new Uint8Array(data);
}

export async function generate_hash(pass: string, salt: Uint8Array | string, len?: number) {
    const password_hash = await hash({
        pass: pass,
        salt: salt,
        // Takes about 1.5 seconds on a Nexus 4
        time: 2, // the number of iterations
        mem: 19 * 1024, // used memory, in KiB
        hashLen: len ? len : 24, // desired hash length
        parallelism: 1, // desired parallelism (it won't be computed in parallel, however)
        type: ArgonType.Argon2id,
    });

    return password_hash.hash;
}

export function generate_random_bytes(len: number) {
    const arr = new Uint8Array(len);
    crypto.getRandomValues(arr);

    return arr;
}

export function concat_salts(salt1: Uint8Array) {
    const salt2 = generate_random_bytes(24);
    const concat = new Uint8Array(salt1.length + salt2.length);
    concat.set(salt1);
    concat.set(salt2, salt1.length);

    return concat;
}

export enum AppState {
    Loading,
    LoggedIn,
    LoggedOut,
    Recovery,
}

export const loggedIn = signal(AppState.Loading);

export const PASSWORD_PATTERN = /(?=.*\d)(?=.*[a-z])(?=.*[A-Z]).{8,}/;
export const BACKEND = import.meta.env.VITE_BACKEND_URL;
export const FRONTEND_URL = import.meta.env.VITE_FRONTEND_URL;

let auth_already_failed = false;
export const fetchClient = createClient<paths>({baseUrl: BACKEND});
const AuthMiddleware: Middleware = {
    async onResponse({request, response, options}) {
        // Ingnore jwt refresh route since it will cause a deadlock when failing
        if (request.url.indexOf("/jwt_refresh") !== -1) {
            return undefined;
        }

        if (response.status === 401 && !auth_already_failed) {
            await refresh_access_token();
            return await fetch(request);
        } else {
            return response;
        }
    }
}
fetchClient.use(AuthMiddleware);

export async function refresh_access_token() {
    if (window.location.pathname == "/recovery") {
        loggedIn.value = AppState.Recovery;
        return;
    }

    try {
        const {error, response} = await window.navigator.locks.request("refreshLock", async () => {
            const resp = await fetchClient.GET("/auth/jwt_refresh", {credentials: "same-origin"});
            return resp;
        });

        if (!error || response.status === 502) {
            if (!localStorage.getItem("loginSalt") || !localStorage.getItem("secretKey")) {
                logout(false);
            }
            loggedIn.value = AppState.LoggedIn;
        } else {
            auth_already_failed = true;
            resetSecret();
            localStorage.removeItem("loginSalt");
            localStorage.removeItem("secretKey");
            loggedIn.value = AppState.LoggedOut;
        }
    } catch (e) {
        resetSecret();
        //This means we are logged in but the refresh failed
        if (localStorage.getItem("loginSalt") && localStorage.getItem("secretKey")) {
            loggedIn.value = AppState.LoggedIn;
        }
        console.error(e);
    }
}

export let secret: Uint8Array | null = null;
export let pub_key: Uint8Array | null = null;

export async function get_decrypted_secret() {
    await sodium.ready;
    const t = i18n.t;
    const {data, error, response} = await fetchClient.GET("/user/get_secret", {credentials: "same-origin"});
    const status = response.status;
    if (error || !data) {
        showAlert(t("chargers.loading_secret_failed", {status, response: error}), "danger");
        return;
    }
    const encoded_key = localStorage.getItem("secretKey");
    if (!encoded_key) {
        showAlert(t("chargers.loading_secret_failed", {status: 'no_key', response: 'No secretKey in localStorage'}), "danger");
        return;
    }
    const secret_key = Base64.toUint8Array(encoded_key);
    secret = sodium.crypto_secretbox_open_easy(new Uint8Array(data.secret), new Uint8Array(data.secret_nonce), secret_key);
    pub_key = secret ? sodium.crypto_scalarmult_base(secret) : null;
}

export function resetSecret() {
    secret = null;
    pub_key = null;
}

export const isDebugMode = signal(false);
const debug = localStorage.getItem("debugMode");
if (debug) {
    isDebugMode.value = true;
}

window.addEventListener("appReload", () => {
    // Sometime the appSleeps value seems not beeing set. To encounter this check if the lastAlive was
    // set during the timout of the wireguard connection
    if (appSleeps || Date.now() - lastAlive >= 1000 * 60 * 2) {
        window.location.reload();
    }
});

let appSleeps = false;
let lastAlive = Date.now();
setInterval(() => {
    const now = Date.now();
    if (now - lastAlive > 5000) {
        appSleeps = true;
    }
    lastAlive = now;
}, 2000);

// Broadcast channel to sync the app state between tabs
export const bc = new BroadcastChannel("sync");
bc.onmessage = (event) => {
    switch (event.data) {
        case "login":
            window.location.reload();
            break;
        case "logout":
            resetSecret();
            loggedIn.value = AppState.LoggedOut;
            break;
    }
}
