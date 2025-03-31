import createClient, { Client, ClientMethod, InitParam, Middleware } from "openapi-fetch";
import type { paths } from "./schema.js";
import { Algorithm, hash, Options } from "@node-rs/argon2";
import { Base64 } from "js-base64";
import sodium from "libsodium-wrappers-sumo";
import { writeFile } from "node:fs";

export interface Cache {
    cookies: string,
    secretKey: string,
    host: string,
}

export class FetchClient {
    auth_already_failed: boolean;
    fetchClient: Client<paths, `${string}/${string}`>;
    cookies: string;
    cacheRequest: Request | undefined;
    secretKey: string;
    host: string;

    constructor(cache: Cache) {
        this.auth_already_failed = false;
        this.fetchClient = createClient<paths>({baseUrl: `https://${cache.host}/api`});
        this.cookies = cache.cookies;
        this.cacheRequest = undefined;
        this.secretKey = cache.secretKey;
        this.host = cache.host;

        const that = this;
        const AuthMiddleware: Middleware = {
            async onResponse({request, response, options}) {
                // Ingnore jwt refresh route since it will cause a deadlock when failing
                if (request.url.indexOf("/jwt_refresh") !== -1) {
                    return undefined;
                }

                if (response.status === 401 && !that.auth_already_failed) {
                    await that.refresh_access_token();
                    if (that.cacheRequest) {
                        request = that.cacheRequest;
                    }
                    that.cacheRequest = undefined;
                    request.headers.set("cookie", that.cookies);
                    return await fetch(request);
                } else {
                    that.cacheRequest = undefined;
                    return response;
                }
            }
        };
        const SetCookieMiddleware: Middleware = {
            onRequest({request}) {
                if (request.url.indexOf("/jwt_refresh") === -1) {
                    that.cacheRequest = request.clone();
                }
                request.headers.set("cookie", that.cookies);
                return request;
            }
        }
        this.fetchClient.use(AuthMiddleware);
        this.fetchClient.use(SetCookieMiddleware);
    }

    private async refresh_access_token() {
        const {error, response} = await this.fetchClient.GET("/auth/jwt_refresh");

        if (!error) {
            this.cookies = this.parseCookies(response);
            const cache: Cache = {
                cookies: this.cookies,
                secretKey: this.secretKey,
                host: this.host,
            };
            writeFile("cache", JSON.stringify(cache), () => {});
        }
    }

    parseCookies(response: Response) {
        const cookies = response.headers.getSetCookie();
        return cookies.map((entry) => {
            const parts = entry.split(';');
            const cookiePart = parts[0];
            return cookiePart;
        }).join(';');
    }
}

export async function argon2Hash(password: string, salt: Uint8Array, length: number = 24) {
        const argon2Options: Options = {
            parallelism: 1,
            timeCost: 2,
            outputLen: length,
            memoryCost: 19 * 1024,
            algorithm: Algorithm.Argon2id,
            salt,
        };
        const argon2Hash = await hash(password, argon2Options);
        const split = argon2Hash.split("$");
        return Base64.toUint8Array(split[split.length - 1]);
}

export async function getDecryptedSecret(secretKey: string, fetchClient: FetchClient) {
    await sodium.ready;
    const getSecret = await fetchClient.fetchClient.GET("/user/get_secret");
    if (getSecret.error || !getSecret.data) {
        throw (`Error while fetching secret: ${getSecret}`);
    }
    const decodedSecretKey = Base64.toUint8Array(secretKey);
    const secret = sodium.crypto_secretbox_open_easy(new Uint8Array(getSecret.data.secret), new Uint8Array(getSecret.data.secret_nonce), decodedSecretKey);
    const pub = sodium.crypto_scalarmult_base(secret);
    return {
        pub,
        secret
    }
}
