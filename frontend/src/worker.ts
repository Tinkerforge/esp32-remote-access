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

const tunnel_url = `/api/ws?key_id=`;
let wgClient: Client | undefined;
let setup_data: SetupMessage;
let connectionGeneration = 0;
let innerRetry: ReturnType<typeof setTimeout> | undefined;
let reconnectRetry: ReturnType<typeof setTimeout> | undefined;
let setupAbort: AbortController | undefined;
const CONNECTION_TIMEOUT_MS = 10_000;

function cancelInnerRetry() {
    clearTimeout(innerRetry);
    innerRetry = undefined;
}

function disposeClient() {
    cancelInnerRetry();
    if (wgClient) {
        wgClient.disconnect();
        wgClient.free();
        wgClient = undefined;
    }
}

function stopConnection() {
    ++connectionGeneration;
    setupAbort?.abort();
    clearTimeout(reconnectRetry);
    disposeClient();
}

function reportTimeout() {
    self.postMessage({ type: MessageType.Error, data: {
        translation: "wgclient.connection_timeout", format: undefined,
    } });
}

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
    self.postMessage({ unresolved: true, msg: evt });
});

function start_inner_ws_with_retry(cb: (msg: string) => void) {
    cancelInnerRetry();
    const client = wgClient;
    const generation = connectionGeneration;
    const deadline = Date.now() + CONNECTION_TIMEOUT_MS;
    if (!client) return;
    const attempt = () => {
        if (client !== wgClient || generation !== connectionGeneration) return;
        if (client.peer_discovery_complete()) {
            client.start_inner_ws(cb);
            return;
        }
        if (client.peer_discovery_failed() || Date.now() >= deadline) {
            stopConnection();
            reportTimeout();
            return;
        }
        innerRetry = setTimeout(attempt, 200);
    };
    attempt();
}

self.addEventListener("message", async (e: MessageEvent) => {
    if (typeof e.data === "string") {
        switch (e.data) {
            case "connect":
                if (wgClient) {
                    start_inner_ws_with_retry((msg: string) => {
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
                stopConnection();
                self.postMessage("closed");
                break;

            case "pauseWS":
                cancelInnerRetry();
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
                stopConnection();
                const setupGeneration = connectionGeneration;
                await sodium.ready;
                if (setupGeneration !== connectionGeneration) return;
                setup_data = data.data as SetupMessage;
                set_pcap_logging(setup_data.debugMode);
                await start_connection(setup_data);
                break;
        }
    }
});

async function start_connection(setup_data: SetupMessage, deadline = Date.now() + CONNECTION_TIMEOUT_MS) {
    stopConnection();
    const generation = connectionGeneration;
    const controller = new AbortController();
    setupAbort = controller;
    const timeout = setTimeout(() => controller.abort(), Math.max(0, deadline - Date.now()));
    let keys: ChargerKeys;
    const url = `/api/charger/get_key?cid=${setup_data.chargerID}`;
    try {
        const resp = await fetch(url, { credentials: "same-origin", signal: controller.signal });
        if (generation !== connectionGeneration) return;
        if (resp.status === 404) {
            self.postMessage({ type: MessageType.Error, data: {
                translation: "chargers.all_keys_in_use", format: undefined,
            } });
            return;
        } else if (resp.status !== 200) {
            const response = await resp.text();
            if (generation !== connectionGeneration) return;
            self.postMessage({ type: MessageType.Error, data: {
                translation: "chargers.loading_keys_failed", format: { status: resp.status, response },
            } });
            return;
        }
        keys = await resp.json();
    } catch {
        if (generation !== connectionGeneration) return;
        if (Date.now() >= deadline) { reportTimeout(); return; }
        reconnectRetry = setTimeout(() => {
            if (generation === connectionGeneration) void start_connection(setup_data, deadline);
        }, 1000);
        return;
    } finally {
        clearTimeout(timeout);
    }
    if (generation !== connectionGeneration) return;
    const decrypted_keys = decrypt_keys(keys, setup_data.secret);
    // Probe the authenticated user tunnel itself. Legacy registrations retain
    // their IPv4 pair as fallback; the public/management family is irrelevant.
    const peerAddress = keys.charger_address.split("/")[0];
    const normalizedPeer = peerAddress.includes(":") ? new URL(`http://[${peerAddress}]/`).hostname : "";
    // Existing fe80::1 registrations also use multicast probes after upgrade.
    const discoveryToken = normalizedPeer === "[ff02::1]" || normalizedPeer === "[fe80::1]";
    const discoverPeer = discoveryToken || !peerAddress.includes(":");
    let localAddress = keys.web_address;
    if (discoveryToken) {
        const bytes = crypto.getRandomValues(new Uint8Array(16));
        bytes[0] = 0xfd;
        localAddress = Array.from({ length: 8 }, (_, i) =>
            ((bytes[2 * i] << 8) | bytes[2 * i + 1]).toString(16)).join(":") + "/128";
    }
    const client = new Client(
        decrypted_keys.web_private_string,
        keys.charger_pub,
        decrypted_keys.psk,
        tunnel_url + keys.id,
        localAddress,
        keys.charger_address,
        setup_data.port,
        setup_data.mtu,
        () => {
            if (generation !== connectionGeneration || wgClient !== client) return;
            cancelInnerRetry();
            reconnectRetry = setTimeout(() => {
                if (generation === connectionGeneration) void start_connection(setup_data);
            }, 1000);
        },
        () => {
            if (generation === connectionGeneration && wgClient === client) self.postMessage("ready");
        },
        discoverPeer,
        keys.ipv4_fallback?.web_address,
        keys.ipv4_fallback?.charger_address,
    );
    wgClient = client;
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
