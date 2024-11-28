import { signal } from "@preact/signals";
import * as Base58 from "base58";
import { chargerID, chargerPort } from "./Frame";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component } from "preact";
import { fetchClient } from "../utils";
import { Button, Card, Col, Container, Modal, Row, Table } from "react-bootstrap";
import i18n from "../i18n";
import { ChevronDown, ChevronUp, Minus, Monitor, Trash2 } from "react-feather";
import { Circle } from "./Circle";

interface Charger {
    id: string,
    uid: number,
    name: string,
    note?: string,
    status: string,
    port: number,
    valid: boolean,
}

interface StateCharger {
    id: string,
    uid: number,
    name: string,
    status: string,
    port: number,
    valid: boolean,
}

type SortColumn = "name" | "uid" | "status" | "none";

interface ChargerListComponentState {
    chargers: StateCharger[],
    showModal: boolean,
    sortColumn: SortColumn,
    sortSequence: "asc" | "desc"
}

export const connected = signal(false);
export const connected_to = signal("");

export let secret: Uint8Array;
export let pub_key: Uint8Array

export class ChargerListComponent extends Component<{}, ChargerListComponentState> {

    removal_charger: StateCharger;
    updatingInterval: any;
    constructor() {
        super();

        this.removal_charger = {
            id: "",
            uid: 0,
            name: "",
            status: "",
            port: 0,
            valid: true,
        };
        this.state = {
            chargers: [],
            showModal: false,
            sortColumn: "none",
            sortSequence: "asc",
        };

        this.updateChargers(this);
        const that = this;
        this.updatingInterval = setInterval(() => that.updateChargers(that), 5000);
    }

    async updateChargers(that: any) {
        if (!secret) {
            await that.get_decrypted_secret();
        }
        fetchClient.GET("/charger/get_chargers", {credentials: "same-origin"}).then(async ({data}) => {
            const chargers: Charger[] = data;
            const state_chargers = [];
            for (const charger of chargers) {
                const state_charger: StateCharger = {
                    id: charger.id,
                    uid: charger.uid,
                    name: this.decrypt_name(charger.name),
                    status: charger.status,
                    port: charger.port,
                    valid: charger.valid,
                }
                state_chargers.push(state_charger);
            }
            this.setState({chargers: state_chargers});
        })
    }

    componentWillUnmount() {
        clearInterval(this.updatingInterval);
    }

    async get_decrypted_secret() {
        await sodium.ready;
        const t = i18n.t;
        const {data, error, response} = await fetchClient.GET("/user/get_secret", {credentials: "same-origin"});
        if (error) {
            showAlert(t("chargers.loading_secret_failed", {status: response.status, response: error}), "danger");
            return;
        }
        const encoded_key = localStorage.getItem("secretKey");
        const secret_key = Base64.toUint8Array(encoded_key);
        secret = sodium.crypto_secretbox_open_easy(new Uint8Array(data.secret), new Uint8Array(data.secret_nonce), secret_key);
        pub_key = sodium.crypto_scalarmult_base(secret);
    }

    async connect_to_charger(charger: StateCharger) {
        chargerID.value = charger.id;
        chargerPort.value = charger.port;
        connected_to.value = charger.name;
        connected.value = true;
    }

    async delete_charger() {
        const t = i18n.t;
        const charger = this.removal_charger;
        const body = {
            charger: charger.id
        };
        const {response, error} = await fetchClient.DELETE("/charger/remove", {body: body, credentials: "same-origin"});

        if (response.status === 200) {
            const chargers = this.state.chargers.filter((c) => c.id !== charger.id);
            this.setState({chargers: chargers});
        } else {
            showAlert(t("remove_error_text", {charger_id: Base58.int_to_base58(charger.id), status: response.status, text: error}), "danger");
        }
    }

    decrypt_name(name: string) {
        if (!name) {
            return "";
        }
        const name_bytes = Base64.toUint8Array(name);
        const decrypted_name =  sodium.crypto_box_seal_open(name_bytes, pub_key, secret);
        const decoder = new TextDecoder();
        return decoder.decode(decrypted_name);
    }

    connection_possible(charger: StateCharger) {
        let connection_possible = true;
        if (charger.status !== "Connected" || charger.valid === false) {
            connection_possible = false;
        }
        return connection_possible;
    }

    create_card(charger: StateCharger) {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        return <>
            <Card className="my-2">
                <Card.Header onClick={async () => {
                    if (!this.connection_possible(charger)) {
                        return;
                    }
                    await this.connect_to_charger(charger);
                }} className="d-flex justify-content-between align-items-center p-2d5">
                    <h5 class="text-break" style="margin-bottom: 0;">{charger.name}</h5>
                    <div style="white-space: nowrap; vertical-align: middle;">
                        <Button className="me-2" variant="primary" disabled={!this.connection_possible(charger)} onClick={async () => {
                            await this.connect_to_charger(charger);
                        }}><Monitor/></Button>
                        <Button variant="danger" onClick={async () => {
                            this.removal_charger = charger;
                            this.setState({showModal: true});
                        }}><Trash2/></Button>
                    </div>
                </Card.Header>
                <Card.Body>
                    <table class="table" style="margin-bottom: 0;">
                        <tr>
                            <td><b>{t("charger_id")}</b></td>
                            <td>{Base58.int_to_base58(charger.uid)}</td>
                        </tr>
                        <tr>
                            <td><b>{t("status")}</b></td>
                            <td>{charger.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}</td>
                        </tr>
                    </table>
                    <p style="color:red;" hidden={charger.valid}>{t("no_keys")}</p>
                </Card.Body>
            </Card>
        </>
    }

    get_icon(column: SortColumn) {
        if (this.state.sortColumn !== column) {
            // Updown Icon
            return <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-chevrons-down"><polyline points="7 14 12 19 17 14"></polyline><polyline points="7 10 12 5 17 10"></polyline></svg>;
        } else if (this.state.sortSequence === "asc") {
            return <ChevronDown/>;
        } else {
            return <ChevronUp/>;
        }
    }

    setSort(column: SortColumn) {
        if (this.state.sortColumn !== column) {
            this.setState({...this.state, sortColumn: column, sortSequence: "asc"});
        } else if (this.state.sortSequence === "asc") {
            this.setState({...this.state, sortSequence: "desc"});
        } else {
            this.setState({...this.state, sortColumn: "none", sortSequence: "asc"});
        }
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        const table_list = [];
        const card_list = [];
        const chargers = this.state.chargers;
        chargers.sort((a, b) => {
            let sortColumn = this.state.sortColumn;
            if (sortColumn === "none") {
                sortColumn = "name";
            }
            let ret: number;
            const first = a[sortColumn];
            const second = b[sortColumn];
            switch (typeof first) {
                case "string":
                    ret = first.localeCompare(second as string);
                    break;
                case "number":
                    ret = first - (second as number);
                    break;
            }
            if (this.state.sortSequence === "asc") {
                return ret;
            } else {
                return ret * -1;
            }
        })
        this.state.chargers.forEach((charger, index) => {
            const entry = <tr>
                <td class="align-middle">
                    {charger.name}
                </td>
                <td class="align-middle">
                    {Base58.int_to_base58(charger.uid)}
                </td>
                <td class="align-middle">
                    {charger.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}
                </td>
                <td class="align-middle">
                    <Button disabled={!this.connection_possible(charger)} id={`connect-${charger.name}`} onClick={async () => {
                        await this.connect_to_charger(charger);
                        }} variant="primary">
                        {t("connect")}
                    </Button>
                    <p style="color:red;" hidden={charger.valid}>
                        {t("no_keys")}
                    </p>
                </td>
                <td class="align-middle">
                    <Button onClick={async () => {
                        this.removal_charger = charger;
                        this.setState({showModal: true})
                    }} variant="danger">
                        {t("remove")}
                    </Button>
                </td>
            </tr>
            table_list.push(entry);
            card_list.push(this.create_card(charger));
        })

        return <>
            <Modal show={this.state.showModal} onHide={() => this.setState({showModal: false})}>
                <Modal.Header>
                    {t("delete_modal_heading", {name: this.removal_charger.name})}
                </Modal.Header>
                <Modal.Body>
                    {t("delete_modal_body", {name: this.removal_charger.name})}
                </Modal.Body>
                <Modal.Footer>
                    <Button variant="danger" onClick={async () => {
                        this.delete_charger();
                        this.setState({showModal: false});
                    }}>{t("remove")}</Button>
                    <Button variant="secondary" onClick={async () => {
                        this.setState({showModal: false});
                    }}>{t("close")}</Button>
                </Modal.Footer>
            </Modal>
            <Col className="d-none d-md-block">
                <Table striped hover>
                    <thead>
                        <tr>
                            <th onClick={() => this.setSort("name")}>
                                <Row>
                                    <Col>
                                        {t("charger_name")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("name")}
                                    </Col>
                                </Row>
                            </th>
                            <th onClick={() => this.setSort("uid")}>
                                <Row>
                                    <Col>
                                        {t("charger_id")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("uid")}
                                    </Col>
                                </Row>
                            </th>
                            <th onClick={() => this.setSort("status")}>
                                <Row>
                                    <Col>
                                        {t("status")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("status")}
                                    </Col>
                                </Row>
                                </th>
                            <th />
                            <th />
                        </tr>
                    </thead>
                    <tbody>
                        {table_list}
                    </tbody>
                </Table>
            </Col>
            <Container fluid className="d-md-none">
                {card_list}
            </Container>
        </>
    }
}
