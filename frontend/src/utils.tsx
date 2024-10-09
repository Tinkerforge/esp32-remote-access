import { signal } from "@preact/signals";
import { hash, ArgonType } from "argon2-browser";
import i18n from "./i18n";
import * as passwdqc from "passwdqc";

export async function get_salt() {
    const resp = await fetch(`${BACKEND}/auth/generate_salt`, {
        method: "GET"
    });
    if (resp.status !== 200) {
        throw `Failed to get new salt with ${resp.status}: ${await resp.text()}`;
    }
    const json = await resp.text();
    const data = JSON.parse(json);

    return new Uint8Array(data);
}

export async function get_salt_for_user(email: string) {
    const resp = await fetch(`${BACKEND}/auth/get_login_salt?email=${email}`, {
        method: "GET"
    });
    if (resp.status !== 200) {
        throw `Failed to get login_salt for user ${email}: ${await resp.text()}`;
    }
    const json = await resp.text();
    const data = JSON.parse(json);

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

interface PwCheckParams {
    passwd: string,
    login?: string,
    oldPass?: string,
}

export function checkPassword(params: PwCheckParams) {
    const pms = {
        min: [passwdqc.INT_MAX, 24, 11, 8, 7],
        max: passwdqc.INT_MAX,
        passphrase_words: 3,
        match_length: 4,
        similar_deny: 1,
        random_bits: 47,
        flags: 3,
        retry: 3,
    };
    return passwdqc.check(params.passwd, params.oldPass, params.login, undefined, pms);
}

export function translatePwError(err: string) {
    const t = i18n.t;
    switch(err) {
        case passwdqc.REASON_SAME:
            return t("passwd.same");
        case passwdqc.REASON_SIMILAR:
            return t("passwd.similar");
        case passwdqc.REASON_SHORT:
            return t("passwd.short");
        case passwdqc.REASON_SIMPLESHORT:
            return t("passwd_simpleshort");
        case passwdqc.REASON_SIMPLE:
            return t("passwd.simple");
        default:
            return err
    }
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
