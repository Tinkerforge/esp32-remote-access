import { signal } from "@preact/signals";
import * as Base58 from "base58";
import { chargerID, chargerPort } from "./Frame";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component } from "preact";
import { fetchClient } from "../utils";
import { Button, Card, Col, Container, Modal, Table } from "react-bootstrap";
import i18n from "../i18n";
import { Monitor, Trash2 } from "react-feather";
import { Circle } from "./Circle";

interface Charger {
    id: string,
    uid: number,
    name: number[],
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

interface ChargerListComponentState {
    chargers: StateCharger[],
    showModal: boolean
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
        const {data, error} = await fetchClient.GET("/user/get_secret", {credentials: "same-origin"});
        if (error) {
            showAlert(t("chargers.loading_secret_failed", {status: get_secret_resp.status, response: await get_secret_resp.text()}), "danger");
            return;
        }
        const encoded_key = localStorage.getItem("secretKey");
        const secret_key = Base64.toUint8Array(encoded_key);
        secret = sodium.crypto_secretbox_open_easy(new Uint8Array(data.secret), new Uint8Array(data.secret_nonce), secret_key);
        pub_key = sodium.crypto_scalarmult_base(secret);
    }

    async connect_to_charger(charger: StateCharger) {
        const t = i18n.t;

        const {response} = await fetchClient.GET("/charger/get_key", {params:{query:{cid: charger.id}}, credentials: "same-origin"});
        if (response.status !== 200) {
            showAlert(t("chargers.connect_error_text", {charger_id: Base58.int_to_base58(charger.id), status: response.status, response: await response.text()}), "danger");
            return;
        }

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
        const {response} = await fetchClient.DELETE("/charger/remove", {body: body, credentials: "same-origin"});

        if (response.status === 200) {
            const chargers = this.state.chargers.filter((c) => c.id !== charger.id);
            this.setState({chargers: chargers});
        } else {
            showAlert(t("remove_error_text", {charger_id: Base58.int_to_base58(charger.id), status: response.status, text: await response.text()}), "danger");
        }
    }

    decrypt_name(name: number[]) {
        if (!name) {
            return "";
        }
        const decrypted_name =  sodium.crypto_box_seal_open(new Uint8Array(name), pub_key, secret);
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

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        const table_list = [];
        const card_list = [];
        this.state.chargers.forEach((charger, index) => {
            const entry = <tr>
                <td>{charger.name}</td>
                <td>{Base58.int_to_base58(charger.uid)}</td>
                <td>{charger.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}</td>
                <td><Button disabled={!this.connection_possible(charger)} id={`connect-${charger.name}`} onClick={async () => {
                    await this.connect_to_charger(charger);
                }} variant="primary">{t("connect")}</Button><p style="color:red;" hidden={charger.valid}>{t("no_keys")}</p></td>
                <td><Button onClick={async () => {
                    this.removal_charger = charger;
                    this.setState({showModal: true})
                }} variant="danger">{t("remove")}</Button></td>
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
                            <th>{t("charger_name")}</th>
                            <th>{t("charger_id")}</th>
                            <th>{t("status")}</th>
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
