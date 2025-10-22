/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

import { Message, MessageType, FetchMessage, ResponseMessage } from "./types";

declare const self: ServiceWorkerGlobalScope;
declare const __BUILD_TIMESTAMP__: string;

self.addEventListener("activate", (event) => {
    event.waitUntil(self.clients.claim());
});

self.addEventListener("install", (event) => {
    event.waitUntil(self.skipWaiting());
});

function handleWGRequest(event: FetchEvent) {
    let url = event.request.url.replace(self.location.origin, "");
    const headers1: [string, string][] = [];
    event.request.headers.forEach((val, key) => {
        headers1.push([key, val]);
    });
    if (event.request.headers.has("X-Connection-Id") || url.startsWith("/wg-")) {
        let receiverId = "";
        let chargerId: string | null = null;
        if (url.startsWith("/wg-")) {
            url = url.replace("/wg-", "");
            const first = url.indexOf("/");
            receiverId = url.substring(0, first);
            url = url.replace(receiverId, "");
            const parsedUrl = new URL(url, self.location.origin);
            chargerId = parsedUrl.searchParams.get("charger");
            parsedUrl.searchParams.delete("charger");
            url = parsedUrl.pathname;
        } else {
            receiverId = event.request.headers.get("X-Connection-Id") as string;
        }
        const promise: Promise<Response> = new Promise(async (resolve) => {
            if (chargerId) {
                const params = new URLSearchParams([
                    ['charger', chargerId]
                ]);
                const req = new Request(`/api/webinterface?${params.toString()}`, {cache: 'no-cache'});
                const response = await fetch(req, {
                    method: 'GET',
                    headers: new Headers(headers1)
                });
                if (response.status === 200) {
                    const ds = new DecompressionStream("gzip");
                    const stream = response.body?.pipeThrough(ds);
                    if (stream) {
                        const decompressedResponse = new Response(stream, {
                            status: response.status,
                            statusText: response.statusText,
                            headers: response.headers
                        });
                        resolve(decompressedResponse);
                        return;
                    }
                }
            }

            const id = crypto.randomUUID();
            const body = await event.request.arrayBuffer();
            const headers: [string, string][] = [];
            event.request.headers.forEach((val, key) => {
                headers.push([key, val]);
            });
            const message: FetchMessage = {
                method: event.request.method,
                headers,
                body: body.byteLength === 0 ? undefined : body,
                url
            };
            const msg: Message = {
                receiver_id: receiverId,
                id,
                type: MessageType.Fetch,
                data: message
            };
            self.addEventListener(id, (e: Event) => {
                const event = e as CustomEvent;
                resolve(event.detail);
            }, {once: true});
            const clients = await self.clients.matchAll();
            for (const client of clients) {
                client.postMessage(msg);
            }
        });
        event.respondWith(promise);
        return true;
    }
}

let lastAccessTokenRefresh = 0;
let responseCache: Response | null = null;

self.addEventListener("fetch", (event: FetchEvent) => {
    if (!handleWGRequest(event) && event.request.url.indexOf("/jwt_refresh") !== -1) {
        const now = Date.now();
        // In case the last access token refresh we lie to the client that the token was refreshed
        // This fixes multiple requests to refresh the token at once leading to users getting logged out
        if (now - lastAccessTokenRefresh < 1000 * 60 * 3 && responseCache) {
            event.respondWith(responseCache.clone());
        } else {
            const promise = new Promise<Response>(async (resolve) => {
                const response = await fetch(event.request);
                if (response.status === 200) {
                    lastAccessTokenRefresh = Date.now();
                    responseCache = response;
                } else {
                    responseCache = null;
                }
                resolve(response.clone());
            });
            event.respondWith(promise);
        }
    } else if (event.request.url.indexOf("/logout") !== -1) {
        lastAccessTokenRefresh = 0;
        responseCache = null;
    }
});

const SECRET_CACHE_NAME = 'secret-cache-v1';

async function storeSecretKeyInCache(secretKey: string): Promise<void> {
    const cache = await caches.open(SECRET_CACHE_NAME);
    const response = new Response(secretKey);
    await cache.put('secret-key', response);
}

async function getSecretKeyFromCache(): Promise<string | null> {
    try {
        const cache = await caches.open(SECRET_CACHE_NAME);
        const response = await cache.match('secret-key');
        if (response) {
            const secretKey = await response.text();
            return secretKey;
        }
    } catch (e) {
        console.error('Service Worker: Failed to get secretKey from cache:', e);
    }
    return null;
}

async function clearSecretKeyFromCache(): Promise<void> {
    try {
        const cache = await caches.open(SECRET_CACHE_NAME);
        await cache.delete('secret-key');
    } catch (e) {
        console.error('Service Worker: Failed to clear secretKey from cache:', e);
    }
}

function isIframeMessage(e: ExtendableMessageEvent): boolean {
    const source = e.source;
    if (source instanceof WindowClient && source.url.indexOf("wg-") !== -1) {
        return true;
    }
    return false;
}

self.addEventListener("message", async (e: ExtendableMessageEvent) => {
    if (isIframeMessage(e)) {
        console.warn("Service Worker ignoring message from invalid origin or iframe:", e.source);
        return;
    }

    if (typeof e.data === "string" && e.data === "version") {
        e.source?.postMessage(__BUILD_TIMESTAMP__);
    }

    const msg = e.data as Message;

    switch (msg.type) {
        case MessageType.FetchResponse:
            const resp_message = msg.data as ResponseMessage;
            const response = new Response(
                resp_message.body,
                {
                    status: resp_message.status,
                    statusText: resp_message.statusText,
                    headers: new Headers(resp_message.headers)
                }
            );

            const event = new CustomEvent(msg.id as string, {detail: response});
            self.dispatchEvent(event);
            break;

        case MessageType.StoreSecret:
            try {
                await storeSecretKeyInCache(msg.data as string);
                const responseMsg: Message = {
                    type: MessageType.StoreSecret,
                    data: "stored"
                };
                e.source?.postMessage(responseMsg);
            } catch (error) {
                const responseMsg: Message = {
                    type: MessageType.StoreSecret,
                    data: `error: ${(error as Error).message}`
                };
                e.source?.postMessage(responseMsg);
            }
            break;

        case MessageType.RequestSecret:
            const secretKey = await getSecretKeyFromCache();
            if (secretKey) {
                const responseMsg: Message = {
                    type: MessageType.RequestSecret,
                    data: secretKey
                };
                e.source?.postMessage(responseMsg);
            }
            break;

        case MessageType.ClearSecret:
            await clearSecretKeyFromCache();
            break;

        default:
            break;
    }
});
