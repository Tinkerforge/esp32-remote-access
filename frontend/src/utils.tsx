import { signal } from "@preact/signals";
import { hash, ArgonType } from "argon2-browser";
import createClient, { Middleware } from "openapi-fetch";
import type { paths } from "./schema";
import sodium from "libsodium-wrappers";
import { logout } from "./components/Navbar";
import i18n from "./i18n";
import { showAlert } from "./components/Alert";
import { Base64 } from "js-base64";
import { Message, MessageType } from "./types";
import Median from "median-js-bridge";

export async function get_salt() {
    const {data, response} = await fetchClient.GET("/auth/generate_salt");
    if (response.status !== 200) {
        throw `Failed to get new salt with ${response.status}: ${await response.text()}`;
    }
    if (!data) throw "No salt data returned";
    return new Uint8Array(data);
}

export async function get_salt_for_user(email: string) {
    const {data, error} = await fetchClient.GET("/auth/get_login_salt", {params: {query: {email}}});
    if (error || !data) {
        throw `Failed to get login_salt for user ${email}: ${error}`;
    }
    return new Uint8Array(data);
}

export async function generate_hash(pass: string, salt: Uint8Array | string, len?: number) {
    const password_hash = await hash({
        pass,
        salt,
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
    async onResponse({request, response}) {
        // Ingnore jwt refresh route since it will cause a deadlock when failing
        if (request.url.indexOf("/jwt_refresh") !== -1) {
            return undefined;
        }

        if (response.status === 401 && !auth_already_failed) {
            await refresh_access_token();
            return await fetch(request);
        }
            return response;

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
            const hasLoginSalt = localStorage.getItem("loginSalt");
            const hasSecret = await getSecretKeyFromServiceWorker();
            if (!hasLoginSalt || !hasSecret) {
                logout(false);
            }
            loggedIn.value = AppState.LoggedIn;
        } else {
            auth_already_failed = true;
            resetSecret();
            localStorage.removeItem("loginSalt");
            await clearSecretKeyFromServiceWorker();
            loggedIn.value = AppState.LoggedOut;
        }
    } catch (e) {
        //This means we are logged in but the refresh failed
        const hasLoginSalt = localStorage.getItem("loginSalt");
        const hasSecret = await getSecretKeyFromServiceWorker();
        if (hasLoginSalt && hasSecret) {
            loggedIn.value = AppState.LoggedIn;
        } else {
            resetSecret();
        }
        console.error(e);
    }
}

export let secret: Uint8Array | null = null;
export let pub_key: Uint8Array | null = null;

// Service Worker communication functions
export async function storeSecretKeyInServiceWorker(secretKey: string): Promise<void> {
    if (!navigator.serviceWorker.controller) {
        return;
    }

    const msg: Message = {
        type: MessageType.StoreSecret,
        data: secretKey
    };
    navigator.serviceWorker.controller.postMessage(msg);
}

let gettingSecretInProgress = false;
let secretKeyPromise: Promise<string | null> | null = null;
let retries = 0;

export async function getSecretKeyFromServiceWorker(): Promise<string | null> {
    if (gettingSecretInProgress) {
        return secretKeyPromise;
    }

    secretKeyPromise = new Promise(async (resolve) => {
        if (!navigator.serviceWorker.controller && retries < 3) {
            console.error("No service worker controller found. Retrying...");
            retries++;
            resolve(await getSecretKeyFromServiceWorker());
            return;
        } else if (!navigator.serviceWorker.controller) {
            console.error("No service worker controller found after retries.");
            resolve(null);
            return;
        } else if (retries >= 3) {
            console.error("Max retries reached without service worker controller.");
            resolve(null);
            return;
        }
        gettingSecretInProgress = true;

        const timeout = setTimeout(async () => {
            console.error("Service Worker: Failed to get secretKey within timeout. Retrying...");
            if (!appSleeps || !Median.isNativeApp()) {
                gettingSecretInProgress = false;
                retries++;
                resolve(await getSecretKeyFromServiceWorker());
            }
        }, 5000);
        retries = 0;

        const handleMessage = (event: MessageEvent) => {
            const msg = event.data as Message;
            if (msg.type === MessageType.StoreSecret) {
                clearTimeout(timeout);
                navigator.serviceWorker.removeEventListener('message', handleMessage);
                gettingSecretInProgress = false;
                resolve(msg.data as string);
            }
        };

        navigator.serviceWorker.addEventListener('message', handleMessage);

        const requestMsg: Message = {
            type: MessageType.RequestSecret,
            data: null
        };
        navigator.serviceWorker.controller.postMessage(requestMsg);
    });

    return secretKeyPromise;
}

export async function clearSecretKeyFromServiceWorker(): Promise<void> {
    if (!navigator.serviceWorker.controller) {
        return;
    }

    const msg: Message = {
        type: MessageType.ClearSecret,
        data: null
    };
    navigator.serviceWorker.controller.postMessage(msg);
}

export async function get_decrypted_secret() {
    await sodium.ready;
    const t = i18n.t;
    const {data, error, response} = await fetchClient.GET("/user/get_secret", {credentials: "same-origin"});
    const status = response.status;
    if (error || !data) {
        showAlert(t("chargers.loading_secret_failed", {status, response: error}), "danger");
        return;
    }
    const encoded_key = await getSecretKeyFromServiceWorker();
    if (!encoded_key) {
        showAlert(t("chargers.loading_secret_failed", {status: 'no_key', response: 'No secretKey in service worker cache'}), "danger");
        return;
    }
    const secret_key = Base64.toUint8Array(encoded_key);
    secret = sodium.crypto_secretbox_open_easy(new Uint8Array(data.secret), new Uint8Array(data.secret_nonce), secret_key);
    pub_key = secret ? sodium.crypto_scalarmult_base(secret) : null;
}

export function resetSecret() {
    secret = null;
    pub_key = null;
    // Also clear the secret from service worker cache
    clearSecretKeyFromServiceWorker().catch((e: unknown) => console.warn("Failed to clear secret from service worker:", e));
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
