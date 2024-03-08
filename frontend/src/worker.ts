import { Client } from "wg-webclient";
import { FetchMessage, Message, MessageType, ResponseMessage, SetupMessage } from "./types";

declare const self: DedicatedWorkerGlobalScope;

const tunnel_url = "wss://" + self.location.hostname + ":8081"
let wgClient = undefined;
self.postMessage("started");

self.addEventListener("message", async (e: MessageEvent) => {
    if (typeof e.data === "string") {
        switch (e.data) {
            case "connect":
                // wgClient.disconnect_ws();
                wgClient.start_ws();
                wgClient.on_message(async (msg: any) => {
                    self.postMessage({
                        type: 0,
                        data: msg
                    });
                });
                break;

            case "download":
                triggerDownload();
                break;
        }
    } else {
        const data = e.data as Message;
        switch (data.type) {
            case MessageType.Fetch:
                const req_data = data.data as FetchMessage;
                const request = new Request(req_data.url, {
                    method: req_data.method,
                    headers: new Headers(req_data.headers),
                    body: req_data.body,
                });
                let url = request.url.replace(self.location.origin, "");
                const response: Response = await wgClient.fetch(request, url);
                const headers: [string, string][] = [];
                response.headers.forEach((val, key) => {
                    headers.push([key, val]);
                })
                const response_msg: ResponseMessage = {
                    status: response.status,
                    statusText: response.statusText,
                    headers: headers,
                    body: await response.arrayBuffer()
                }

                const msg: Message = {
                    type: MessageType.FetchResponse,
                    id: data.id,
                    data: response_msg
                }
                self.postMessage(msg);
                break;

            case MessageType.Setup:
                const setup_data = data.data as SetupMessage;
                wgClient = new Client(setup_data.self_key, setup_data.peer_key, tunnel_url, setup_data.self_internal_ip, setup_data.peer_internal_ip);
                self.postMessage("ready");
                break;
        }
    }
});

function triggerDownload() {
    const msg = wgClient.get_pcap_log();
    self.postMessage({
        type: 1,
        data: msg
    });
}