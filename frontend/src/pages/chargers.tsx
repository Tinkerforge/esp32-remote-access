import { Component } from "preact";
import { BACKEND } from "../types";
import { Button, Table } from "react-bootstrap";


interface Charger {
    id: string,
    name: string
}

interface ChargerListComponentState {
    chargers: Charger[]
}

class ChargerListComponent extends Component<{}, ChargerListComponentState> {

    constructor() {
        super();

        this.state = {
            chargers: []
        };

        fetch(BACKEND + "/charger/get_chargers", {
            credentials: "include"
        }).then(async (resp) => {
            console.log("response", resp);
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
                    console.log(resp);
                }}>Connect</Button></td>
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
    return <>
        <ChargerListComponent />
    </>
}