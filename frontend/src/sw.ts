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

self.addEventListener("fetch", async (event: FetchEvent) => {
    let url = event.request.url.replace(self.location.origin, "");
    const headers1: [string, string][] = [];
    event.request.headers.forEach((val, key) => {
        headers1.push([key, val]);
    });
    if (event.request.headers.has("X-Connection-Id") || url.startsWith("/wg-")) {
        let receiver_id = "";
        if (url.startsWith("/wg-")) {
            url = url.replace("/wg-", "");
            const first = url.indexOf("/");
            receiver_id = url.substring(0, first);
            url = url.replace(receiver_id, "");
        } else {
            receiver_id = event.request.headers.get("X-Connection-Id");
        }
        const promise: Promise<Response> = new Promise(async (resolve, reject) => {
            const id = crypto.randomUUID();
            const body = await event.request.arrayBuffer();
            const headers: [string, string][] = [];
            event.request.headers.forEach((val, key) => {
                headers.push([key, val]);
            });
            const fetch: FetchMessage = {
                method: event.request.method,
                headers: headers,
                body: body.byteLength === 0 ? undefined : body,
                url: url
            };
            const msg: Message = {
                receiver_id: receiver_id,
                id: id,
                type: MessageType.Fetch,
                data: fetch
            };
            self.addEventListener(id, (e: CustomEvent) => {
                resolve(e.detail);
            }, {once: true});
            const clients = await self.clients.matchAll();
            for (const client of clients) {
                client.postMessage(msg);
            }
        });
        event.respondWith(promise);
    }
});


self.addEventListener("activate", () => {
    self.clients.claim();
});

self.addEventListener("message", (e) => {
    const msg = e.data as Message;
    if (msg.type === MessageType.FetchResponse) {
        const resp_message = msg.data as ResponseMessage;
        const response = new Response(
            resp_message.body,
            {
                status: resp_message.status,
                statusText: resp_message.statusText,
                headers: new Headers(resp_message.headers)
            }
        );
        const event = new CustomEvent(msg.id, {detail: response});
        self.dispatchEvent(event);
    }
});
