import { signal } from "@preact/signals";
import * as Base58 from "base58";
import { charger_info } from "./Frame";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component } from "preact";
import { BACKEND } from "../types";
import { Button, Card, Col, Container, Modal, Table } from "react-bootstrap";
import i18n from "../i18n";
import { Monitor, Trash2 } from "react-feather";

interface Charger {
    id: number,
    name: string,
    status: string,
    port: number,
}

interface ChargerListComponentState {
    chargers: Charger[],
    showModal: boolean
}

export const connected = signal(false);
export const connected_to = signal("");

export class ChargerListComponent extends Component<{}, ChargerListComponentState> {

    removal_charger: Charger;
    constructor() {
        super();

        this.removal_charger = {
            id: 0,
            name: "",
            status: "",
            port: 0,
        };
        this.state = {
            chargers: [],
            showModal: false,
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

    async connect_to_charger(charger: Charger) {
        const t = i18n.t;
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
            showAlert(t("connect_error_text", {charger_id: Base58.int_to_base58(charger.id), status: resp.status, text: await resp.text()}), "danger");
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
    }

    async delete_charger() {
        const t = i18n.t;
        const charger = this.removal_charger;
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
        } else {
            showAlert(t("remove_error_text", {charger_id: Base58.int_to_base58(charger.id), status: resp.status, text: await resp.text()}), "danger");
        }
    }

    create_card(charger: Charger) {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        return <>
            <Card className="mb-2">
                <Card.Header className="d-flex justify-content-between align-items-center p-2d5">
                    <h5 class="text-break" style="margin-bottom: 0;">{charger.name}</h5>
                    <div style="white-space: nowrap; vertical-align: middle;">
                        <Button className="me-2" variant="primary" disabled={charger.status !== "Connected"} onClick={async () => {
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
                            <td>{Base58.int_to_base58(charger.id)}</td>
                        </tr>
                        <tr>
                            <td><b>{t("status")}</b></td>
                            <td>{charger.status === "Disconnected" ? t("status_disconnected") : t("status_connected")}</td>
                        </tr>
                    </table>
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
                <td>{index}</td>
                <td>{charger.name}</td>
                <td>{Base58.int_to_base58(charger.id)}</td>
                <td>{charger.status === "Disconnected" ? t("status_disconnected") : t("status_connected")}</td>
                <td><Button disabled={charger.status !== "Connected"} id={`connect-${charger.name}`} onClick={async () => {
                    await this.connect_to_charger(charger);
                }} variant="primary">{t("connect")}</Button></td>
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
