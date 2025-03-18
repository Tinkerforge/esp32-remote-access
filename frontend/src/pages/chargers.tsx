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

import { useEffect, useState } from "preact/hooks";
import { Frame } from "../components/Frame";
import { ChargerListComponent } from "../components/charger_list";
import { connected } from "../components/Navbar";
import Median from "median-js-bridge";
import { fetchClient, get_decrypted_secret, pub_key, secret } from "../utils";
import { Row, Spinner } from "react-bootstrap";
import { useLocation, useRoute } from "preact-iso";
import sodium from "libsodium-wrappers";
import { Base64 } from "js-base64";

export interface ChargersState {
    connected: boolean;
    connectedName: string;
    connectedId: string;
    connectedPort: number;
}

export function ChargerList() {
    const [state, setState] = useState<ChargersState>({
        connected: false,
        connectedName: "",
        connectedId: "",
        connectedPort: 0,
    })
    const [loaded, setLoaded] = useState(false);

    if (Median.isNativeApp() && !loaded) {
        setTimeout(async () => {
            if (!secret) {
                await get_decrypted_secret();
            }
            setLoaded(true);
            const currentConnection = sessionStorage.getItem("currentConnection");
            try {
                const currentConnectionObject: ChargersState = JSON.parse(currentConnection);
                if (currentConnectionObject.connected) {
                    setState(currentConnectionObject);
                }
            } catch {}
        });
    } else if (!loaded) {
        setLoaded(true);
    }

    useEffect(() => {
        connected.value = state.connected;
        document.title = state.connectedName == "" ?  "Remote Access" : state.connectedName;
    }, [state]);

    const { path, route } = useLocation();
    useEffect(() => {
        if (path !== "/chargers") {
            setTimeout(async () => {
                const split = path.split("/");
                const {error, data} = await fetchClient.POST("/charger/info", {body: {charger: split[2]}});
                if (error) {
                    route("/chargers", true);
                    return;
                }

                // Encounter possible chargers that were added before name encryption was implemented
                if (!data.name) {
                    setState({
                        connected: true,
                        connectedName: "",
                        connectedId: data.id,
                        connectedPort: data.configured_port,
                    });
                } else {
                    await sodium.ready;
                    if (!secret) {
                        await get_decrypted_secret();
                    }

                    const encryptedName = Base64.toUint8Array(data.name);
                    const binaryName = sodium.crypto_box_seal_open(encryptedName, pub_key, secret);
                    const decoder = new TextDecoder();
                    const name = decoder.decode(binaryName);
                    setState({
                        connected: true,
                        connectedName: name,
                        connectedId: data.id,
                        connectedPort: data.configured_port,
                    });
                }

                setLoaded(true);
            });
            setLoaded(false);
        } else {
            setState({
                connected: false,
                connectedName: "",
                connectedId: "",
                connectedPort: 0,
            });
        }
    }, [path]);

    if (!loaded) {
        return <Row className="align-content-center justify-content-center m-0 h-100">
            <Spinner className="p-3"animation='border' variant='primary'/>
        </Row>
    }
    if (!state.connected) {
        return <>
            <ChargerListComponent setParentState={setState} parentState={state} />
        </>
    } else {
        return <>
            <Frame setParentState={setState} parentState={state} />
        </>
    }
}
