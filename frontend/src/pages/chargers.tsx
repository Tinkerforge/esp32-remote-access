import { Component } from "preact";
import { BACKEND } from "../types";
import { Button, Table } from "react-bootstrap";
import { Frame, charger_info } from "../components/Frame";
import { signal } from "@preact/signals";


interface Charger {
    id: string,
    name: string
}

interface ChargerListComponentState {
    chargers: Charger[]
}

const connected = signal(false);

class ChargerListComponent extends Component<{}, ChargerListComponentState> {

    constructor() {
        super();

        this.state = {
            chargers: []
        };

        fetch(BACKEND + "/charger/get_chargers", {
            credentials: "include"
        }).then(async (resp) => {
            this.setState({chargers: await resp.json()});
        });
    }

    render() {
        const list = [];
        this.state.chargers.forEach((charger, index) => {
            const entry = <tr>
                <td>{index}</td>
                <td>{charger.name}</td>
                <td>{charger.id}</td>
                <td><Button onClick={async () => {
                    const resp = await fetch(BACKEND + "/charger/get_key?cid=" + charger.id, {
                        credentials: "include"
                    });
                    const json = await resp.json();
                    charger_info.value = {
                        self_key: json.web_private,
                        self_internal_ip: json.web_address,
                        peer_key: json.charger_pub,
                        peer_internal_ip: json.charger_address,
                        key_id: json.id,
                    }

                    connected.value = true;
                }}>Connect</Button></td>
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
                className="btn-danger">Remove</Button></td>
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
