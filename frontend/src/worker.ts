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
import { FetchMessage, Message, MessageType, ResponseMessage, SetupMessage, ChargerKeys } from "./types";
import sodium from "libsodium-wrappers";

declare const self: DedicatedWorkerGlobalScope;

const tunnel_url = `/api/ws?key_id=`
let wgClient: Client | undefined;
let setup_data: SetupMessage;

function isFailedToFetch(reason: unknown): boolean {
    if (!reason) return false;
    if (typeof reason === 'string') return reason.includes('Failed to fetch');
    if (reason instanceof Error) return reason.message.includes('Failed to fetch');
    return false;
}

self.addEventListener("unhandledrejection", (event) => {
    if (isFailedToFetch(event.reason)) {
        event.preventDefault();
        return;
    }

    const stack = event.reason.stack.split("\n");

    const evt = {
        message: event.reason.message,
        stack
    }
    self.postMessage({unresolved: true, msg: evt});
});

self.addEventListener("message", async (e: MessageEvent) => {
    if (typeof e.data === "string") {
        switch (e.data) {
            case "connect":
                if (wgClient) {
                    wgClient.start_inner_ws((msg: string) => {
                        self.postMessage({
                            type: 0,
                            data: msg
                        });
                    });
                } else {
                    self.postMessage({
                        type: MessageType.Error,
                        data: {
                            translation: "wgclient.not_initialized",
                            format: undefined,
                        },
                    });
                }
                break;

            case "close":
                if (wgClient) {
                    wgClient.disconnect_inner_ws();
                } else {
                    self.postMessage({
                        type: MessageType.Error,
                        data: {
                            translation: "wgclient.not_initialized",
                            format: undefined,
                        },
                    });
                }
                self.postMessage("closed");
                break;

            case "pauseWS":
                if (wgClient) {
                    wgClient.disconnect_inner_ws();
                } else {
                    self.postMessage({
                        type: MessageType.Error,
                        data: {
                            translation: "wgclient.not_initialized",
                            format: undefined,
                        },
                    });
                }
                break;

            case "download":
                if (wgClient) {
                    triggerDownload();
                } else {
                    self.postMessage({
                        type: MessageType.Error,
                        data: {
                            translation: "wgclient.not_initialized",
                            format: undefined,
                        },
                    });
                }
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
                let username;
                let password;
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
                const url = request.url.replace(self.location.origin, "");
                if (!wgClient) {
                    const msg: Message = {
                        type: MessageType.Error,
                        id: data.id,
                        data: {
                            translation: "wgclient.not_initialized",
                            format: undefined,
                        },
                    };
                    self.postMessage(msg);
                    return;
                }
                const response: Response = await wgClient.fetch(request, url, username, password);
                const headers: [string, string][] = [];
                response.headers.forEach((val, key) => {
                    headers.push([key, val]);
                })

                const response_msg: ResponseMessage = {
                    status: response.status,
                    statusText: response.statusText,
                    headers,
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
                set_pcap_logging(setup_data.debugMode);
                break;
        }
    }
});

function disconnect_cb() {
    setTimeout(async () => {
        start_connection(setup_data);
    }, 1000);
}

function connect_cb() {
    self.postMessage("ready");
}

async function start_connection(setup_data: SetupMessage) {
    let keys: ChargerKeys;
    const url = `/api/charger/get_key?cid=${  setup_data.chargerID}`;
    try {
        const resp = await fetch(url, {credentials: "same-origin"});
        if (resp.status === 404) {
            const msg: Message = {
                type: MessageType.Error,
                data: {
                    translation: "chargers.all_keys_in_use",
                    format: undefined,
                },
            }
            self.postMessage(msg);
            return;
        } else if (resp.status !== 200) {
            const msg: Message = {
                type: MessageType.Error,
                data: {
                    translation: "chargers.loading_keys_failed",
                    format: {
                        status: resp.status,
                        response: resp.text(),
                    },
                },
            }
            self.postMessage(msg);
            return;
        }
        keys = await resp.json();
    } catch {
        setTimeout(() => {
            start_connection(setup_data);
        }, 1000);
        return;
    }
    const decrypted_keys = decrypt_keys(keys, setup_data.secret);

    wgClient = new Client(
        decrypted_keys.web_private_string,
        keys.charger_pub,
        decrypted_keys.psk,
        tunnel_url + keys.id,
        keys.web_address,
        keys.charger_address,
        setup_data.port,
        setup_data.mtu,
        disconnect_cb,
        connect_cb,
    );
}

function decrypt_keys(keys: ChargerKeys, secret: Uint8Array) {
    const public_key = sodium.crypto_scalarmult_base(secret);
    const web_private = sodium.crypto_box_seal_open(new Uint8Array(keys.web_private), public_key, secret);
    const decoder = new TextDecoder();
    const web_private_string = decoder.decode(web_private);
    const psk = sodium.crypto_box_seal_open(new Uint8Array(keys.psk), public_key, secret);
    const psk_string = decoder.decode(psk);

    return {
        web_private_string,
        psk: psk_string
    };
}

function triggerDownload() {
    if (!wgClient) return;
    const msg = wgClient.get_pcap_log();
    self.postMessage({
        type: 1,
        data: msg
    });
}

self.postMessage("started");
