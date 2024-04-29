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

interface Charger {
    id: number,
    name: string
}

interface ChargerListComponentState {
    chargers: Charger[],
}

const connected = signal(false);

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
        const crypto = window.crypto.subtle;
        const password = sessionStorage.getItem("password");
        const secret_hash = await generate_hash(password, new Uint8Array(secret_data.secret_salt), 32);
        const secret_key = await crypto.importKey("raw", secret_hash, {name: "AES-CBC"}, false, ["decrypt"]);
        const secret = await crypto.decrypt(
                {
                    name: "AES-CBC",
                    iv: new Uint8Array(secret_data.secret_iv)
                },
                secret_key,
                new Uint8Array(secret_data.secret)
            );

        const wg_decrypt_key = await crypto.importKey("raw", secret, {name: "AES-CBC"}, false, ["decrypt"]);
        const web_private = await crypto.decrypt(
            {
                name: "AES-CBC",
                iv: new Uint8Array(keys.web_private_iv)
            },
            wg_decrypt_key,
            new Uint8Array(keys.web_private)
        )
        const decoder = new TextDecoder();
        const web_private_string = decoder.decode(web_private);

        return web_private_string;
    }

    render() {
        const list = [];
        this.state.chargers.forEach((charger, index) => {
            const entry = <tr>
                <td>{index}</td>
                <td>{charger.name}</td>
                <td>{Base58.int_to_base58(charger.id)}</td>
                <td><Button onClick={async () => {

                    const get_secret_resp = await fetch(BACKEND + "/user/get_secret", {
                        credentials: "include"
                    });

                    const resp = await fetch(BACKEND + "/charger/get_key?cid=" + charger.id, {
                        credentials: "include"
                    });
                    const json = await resp.json();

                    const web_private = await this.decrypt_keys(json, await get_secret_resp.json());

                    charger_info.value = {
                        self_key: web_private,
                        self_internal_ip: json.web_address,
                        peer_key: json.charger_pub,
                        peer_internal_ip: json.charger_address,
                        key_id: json.id,
                    }

                    console.log(charger_info.value);

                    connected.value = true;
                }} variant="primary">Connect</Button></td>
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
                variant="danger">Remove</Button></td>
            </tr>
            list.push(entry);
        })

        return <>
            <Table striped hover>
                <thead>
                    <tr>
                        <th>#</th>
                        <th>Charger Name</th>
                        <th>Charger Id</th>
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
            <Button variant="primary"
                    onClick={() => {
                        connected.value = false;
                    }}>Close</Button>
        </>
    }
}
