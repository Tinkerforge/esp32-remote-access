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

import { Client } from "wg-webclient";
import { BACKEND, FetchMessage, Message, MessageType, ResponseMessage, SetupMessage } from "./types";

declare const self: DedicatedWorkerGlobalScope;

const tunnel_url = import.meta.env.VITE_BACKEND_WS_URL + "/ws?key_id="
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
                wgClient = new Client(setup_data.self_key, setup_data.peer_key, setup_data.psk, tunnel_url + setup_data.key_id, setup_data.self_internal_ip, setup_data.peer_internal_ip, setup_data.port);
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
