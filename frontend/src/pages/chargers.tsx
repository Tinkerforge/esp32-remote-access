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

import { Component } from "preact";
import { BACKEND } from "../types";
import { Button, Table } from "react-bootstrap";
import { Frame, charger_info } from "../components/Frame";
import { signal } from "@preact/signals";
import * as Base58 from "base58";
import { generate_hash } from "../utils";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";

interface Charger {
    id: number,
    name: string,
    status: string,
    port: number,
}

interface ChargerListComponentState {
    chargers: Charger[],
}

export const connected = signal(false);
export const connected_to = signal("");

class ChargerListComponent extends Component<{}, ChargerListComponentState> {
    constructor() {
        super();

        this.state = {
            chargers: [],
        };

        fetch(BACKEND + "/charger/get_chargers", {
            credentials: "include"
        }).then(async (resp) => {
            this.setState({chargers: await resp.json()});
        });
    }

    async decrypt_keys(keys: any, secret_data: any) {
        const encoded_key = localStorage.getItem("secret_key");
        const secret_key = Base64.toUint8Array(encoded_key);
        const secret = sodium.crypto_secretbox_open_easy(new Uint8Array(secret_data.secret), new Uint8Array(secret_data.secret_nonce), secret_key);

        const public_key = sodium.crypto_scalarmult_base(new Uint8Array(secret));
        const web_private = sodium.crypto_box_seal_open(new Uint8Array(keys.web_private), public_key, new Uint8Array(secret));
        const decoder = new TextDecoder();
        const web_private_string = decoder.decode(web_private);
        const psk = sodium.crypto_box_seal_open(new Uint8Array(keys.psk), public_key, new Uint8Array(secret));
        const psk_string = decoder.decode(psk);

        return {
            web_private_string: web_private_string,
            psk: psk_string
        };
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        const list = [];
        this.state.chargers.forEach((charger, index) => {
            const entry = <tr>
                <td>{index}</td>
                <td>{charger.name}</td>
                <td>{Base58.int_to_base58(charger.id)}</td>
                <td>{charger.status === "Disconnected" ? t("status_disconnected") : t("status_connected")}</td>
                <td><Button disabled={charger.status !== "Connected"} onClick={async () => {

                    const get_secret_resp = await fetch(BACKEND + "/user/get_secret", {
                        credentials: "include"
                    });
                    if (get_secret_resp.status !== 200) {
                        showAlert(t("connect_error_text", {charger_id: Base58.int_to_base58(charger.id), status: get_secret_resp.status, text: await get_secret_resp.text()}), "danger");
                        return;
                    }

                    const resp = await fetch(BACKEND + "/charger/get_key?cid=" + charger.id, {
                        credentials: "include"
                    });
                    if (resp.status !== 200) {
                        showAlert(t("connect_error_text", {charger_id: Base58.int_to_base58(charger.id), status: get_secret_resp.status, text: await get_secret_resp.text()}), "danger");
                        return;
                    }

                    const json = await resp.json();

                    const ret = await this.decrypt_keys(json, await get_secret_resp.json());

                    charger_info.value = {
                        self_key: ret.web_private_string,
                        psk: ret.psk,
                        self_internal_ip: json.web_address,
                        peer_key: json.charger_pub,
                        peer_internal_ip: json.charger_address,
                        key_id: json.id,
                        port: charger.port,
                    }

                    connected_to.value = charger.name;
                    connected.value = true;
                }} variant="primary">{t("connect")}</Button></td>
                <td><Button onClick={async () => {
                    const body = {
                        charger: charger.id
                    };
                    const resp = await fetch(BACKEND + "/charger/remove", {
                        method: "DELETE",
                        credentials: "include",
                        body: JSON.stringify(body),
                        headers: {
                            "Content-Type": "application/json"
                        }
                    });

                    if (resp.status === 200) {
                        const chargers = this.state.chargers.filter((c) => c.id !== charger.id);
                        this.setState({chargers: chargers});
                    }
                }}
                variant="danger">{t("remove")}</Button></td>
            </tr>
            list.push(entry);
        })

        return <>
            <Table striped hover>
                <thead>
                    <tr>
                        <th>#</th>
                        <th>{t("charger_name")}</th>
                        <th>{t("charger_id")}</th>
                        <th>{t("status")}</th>
                        <th />
                        <th />
                        <th />
                    </tr>
                </thead>
                <tbody>
                    {list}
                </tbody>
            </Table>
        </>
    }
}

export function ChargerList() {

    if (!connected.value) {
        return <>
            <ChargerListComponent />
        </>
    } else {
        return <>
            <Frame />
        </>
    }
}
