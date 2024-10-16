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

import { Client, set_pcap_logging } from "wg-webclient";
import { FetchMessage, Message, MessageType, ResponseMessage, SetupMessage } from "./types";
import sodium from "libsodium-wrappers";

declare const self: DedicatedWorkerGlobalScope;

const tunnel_url = import.meta.env.VITE_BACKEND_WS_URL + "/ws?key_id="
let wgClient: Client | undefined = undefined;
let setup_data: SetupMessage;

self.addEventListener("message", async (e: MessageEvent) => {
    if (typeof e.data === "string") {
        switch (e.data) {
            case "connect":
                // wgClient.disconnect_inner_ws();
                wgClient.start_inner_ws((msg: string) => {
                    self.postMessage({
                        type: 0,
                        data: msg
                    });
                });
                break;

            case "close":
                wgClient.disconnect_inner_ws();
                self.postMessage("closed");
                break;

            case "pauseWS":
                wgClient.disconnect_inner_ws();
                break;

            case "enableLogging":
                set_pcap_logging(true);
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

                // Filter username and password from url
                const regex = /[^:/]+:[^/@]+/;
                const credetials = req_data.url.match(regex);
                let username = undefined;
                let password = undefined;
                if (credetials) {
                    const split = credetials[0].split(":");
                    username = split[0];
                    password = split[1];
                }

                // Replace username and password in url.
                req_data.url = req_data.url.replace(/^((?:http|ws)s?:\/\/)[^:]+:[^@]+@/, "$1");

                const request = new Request(req_data.url, {
                    method: req_data.method,
                    headers: new Headers(req_data.headers),
                    body: req_data.body,
                });
                let url = request.url.replace(self.location.origin, "");
                const response: Response = await wgClient.fetch(request, url, username, password);
                const headers: [string, string][] = [];
                response.headers.forEach((val, key) => {
                    headers.push([key, val]);
                })

                const response_msg: ResponseMessage = {
                    status: response.status,
                    statusText: response.statusText,
                    headers: headers,
                    body: await response.arrayBuffer(),
                }

                const msg: Message = {
                    type: MessageType.FetchResponse,
                    id: data.id,
                    data: response_msg
                }
                self.postMessage(msg);
                break;

            case MessageType.Setup:
                await sodium.ready;
                setup_data = data.data as SetupMessage;
                await start_connection(setup_data);
                self.postMessage("ready");
                break;
        }
    }
});

function disconnect_cb() {
    setTimeout(async () => {
        start_connection(setup_data);
    }, 1000);
}

async function start_connection(setup_data: SetupMessage) {
    let resp: Response;
    try {
        resp = await fetch(`${import.meta.env.VITE_BACKEND_URL}/charger/get_key?cid=${setup_data.chargerID}`, {
            credentials: "same-origin",
        });
    } catch (e) {
        disconnect_cb();
        return;
    }
    const keys = await resp.json();
    const decrypted_keys = decrypt_keys(keys, setup_data.secret);

    wgClient = new Client(
        decrypted_keys.web_private_string,
        keys.charger_pub,
        decrypted_keys.psk,
        tunnel_url + keys.id,
        keys.web_address,
        keys.charger_address,
        setup_data.port,
        disconnect_cb,
    );
}

function decrypt_keys(keys: any, secret: Uint8Array) {
    const public_key = sodium.crypto_scalarmult_base(secret);
    const web_private = sodium.crypto_box_seal_open(new Uint8Array(keys.web_private), public_key, secret);
    const decoder = new TextDecoder();
    const web_private_string = decoder.decode(web_private);
    const psk = sodium.crypto_box_seal_open(new Uint8Array(keys.psk), public_key, secret);
    const psk_string = decoder.decode(psk);

    return {
        web_private_string: web_private_string,
        psk: psk_string
    };
}

function triggerDownload() {
    const msg = wgClient.get_pcap_log();
    self.postMessage({
        type: 1,
        data: msg
    });
}

self.postMessage("started");
