import { Message, MessageType, FetchMessage, ResponseMessage } from "./types";

declare const self: ServiceWorkerGlobalScope;

self.addEventListener("fetch", async (event: FetchEvent) => {
    let url = event.request.url.replace(self.location.origin, "");
    if (url.startsWith("/wg/")) {
        url = url.replace("/wg", "");
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
