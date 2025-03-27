import createClient, { Client, ClientMethod, InitParam, Middleware } from "openapi-fetch";
import type { paths } from "./schema.js";
import { Algorithm, hash, Options } from "@node-rs/argon2";
import { Base64 } from "js-base64";


export class FetchClient {
    auth_already_failed: boolean;
    fetchClient: Client<paths, `${string}/${string}`>;
    cookiesRef: {cookies: string};

    constructor(host: string, cookies: string = "") {
        this.auth_already_failed = false;
        this.fetchClient = createClient<paths>({baseUrl: `https://${host}/api`});
        this.cookiesRef = {cookies};

        const that = this;
        const AuthMiddleware: Middleware = {
            async onResponse({request, response, options}) {
                // Ingnore jwt refresh route since it will cause a deadlock when failing
                if (request.url.indexOf("/jwt_refresh") !== -1) {
                    return undefined;
                }

                if (response.status === 401 && !that.auth_already_failed) {
                    await that.refresh_access_token();
                    return await fetch(request);
                } else {
                    return response;
                }
            }
        };
        const cookiesRef = this.cookiesRef;
        const SetCookieMiddleware: Middleware = {
            onRequest({request}) {
                request.headers.set("cookie", cookiesRef.cookies);
                return request;
            }
        }
        this.fetchClient.use(AuthMiddleware);
        this.fetchClient.use(SetCookieMiddleware);
    }

    private async refresh_access_token() {
        const {error, response} = await this.fetchClient.GET("/auth/jwt_refresh", {credentials: "same-origin"});

        if (!error) {
            this.cookiesRef.cookies = this.parseCookies(response);
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
