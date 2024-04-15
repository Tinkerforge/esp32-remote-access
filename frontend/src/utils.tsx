import { BACKEND } from "./types";
import { hash, ArgonType } from "argon2-browser";

export async function get_salt() {
    const resp = await fetch(`${BACKEND}/auth/generate_salt`, {
        method: "GET"
    });
    if (resp.status !== 200) {
        throw `Failed to get new salt with ${resp.status}: ${await resp.text()}`;
    }

    return new Uint8Array(await resp.arrayBuffer());
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
