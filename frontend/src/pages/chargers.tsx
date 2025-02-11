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
import { get_decrypted_secret, secret } from "../utils";
import { Row, Spinner } from "react-bootstrap";

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
    }, [state])

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
