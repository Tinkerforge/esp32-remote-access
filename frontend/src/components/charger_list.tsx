import { signal } from "@preact/signals";
import * as Base58 from "base58";
import { chargerID, chargerPort } from "./Frame";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component } from "preact";
import { fetchClient, refresh_access_token } from "../utils";
import { Button, ButtonGroup, Card, Col, Container, Dropdown, DropdownButton, Form, Modal, Row, Table } from "react-bootstrap";
import i18n from "../i18n";
import { ChevronDown, ChevronUp, Edit, Monitor, Trash2 } from "react-feather";
import { Circle } from "./Circle";
import Median from "median-js-bridge";

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
    note: string,
    port: number,
    valid: boolean,
}

type SortColumn = "name" | "uid" | "status" | "none" | "note";

interface ChargerListComponentState {
    chargers: StateCharger[],
    showDeleteModal: boolean,
    showEditNoteModal: boolean,
    editNote: string,
    editChargerIdx: number,
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
            note: "",
        };
        this.state = {
            chargers: [],
            showDeleteModal: false,
            showEditNoteModal: false,
            editNote: "",
            editChargerIdx: 0,
            sortColumn: "none",
            sortSequence: "asc",
        };

        this.updateChargers(this);
        const that = this;
        this.updatingInterval = setInterval(() => that.updateChargers(that), 5000);
    }

    decryptNote(note?: string) {
        if (!note) {
            return "";
        }

        const noteBytes = Base64.toUint8Array(note);
        const decryptedNote = sodium.crypto_box_seal_open(noteBytes, pub_key, secret);
        const decoder = new TextDecoder();
        return decoder.decode(decryptedNote);
    }

    async updateChargers(that: any) {
        if (!secret) {
            await that.get_decrypted_secret();
        }
        fetchClient.GET("/charger/get_chargers", {credentials: "same-origin"}).then(async ({data, response}) => {
            if (response.status === 401) {
                await refresh_access_token();
                this.updateChargers;
                return;
            }
            const chargers: Charger[] = data;
            const state_chargers = [];
            for (const charger of chargers) {
                const state_charger: StateCharger = {
                    id: charger.id,
                    uid: charger.uid,
                    name: this.decrypt_name(charger.name),
                    note: this.decryptNote(charger.note),
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
                    <Col xs="auto" className="d-flex">
                        {charger.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}
                    </Col>
                    <Col className="mx-3">
                        <h5 class="text-break" style="margin-bottom: 0;">{charger.name}</h5>
                    </Col>
                    <Col className="d-flex justify-content-end">
                        <Button className="me-2" variant="primary" disabled={!this.connection_possible(charger)} onClick={async () => {
                            await this.connect_to_charger(charger);
                        }}><Monitor/></Button>
                        <Button variant="danger" onClick={async () => {
                            this.removal_charger = charger;
                            this.setState({showDeleteModal: true});
                        }}><Trash2/></Button>
                    </Col>
                </Card.Header>
                <Card.Body>
                    <Row >
                        <Col xs="3"><b>{t("mobile_charger_id")}</b></Col>
                        <Col xs="9" className="text-end">{Base58.int_to_base58(charger.uid)}</Col>
                    </Row>
                    <hr style="margin-top: 5px;margin-bottom: 5px;"/>
                    <Row>
                        <Col xs="3"><b>{t("note")}</b></Col>
                        <Col xs="9" className="text-end">{charger.note}</Col>
                    </Row>
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

    setMobileSort(column: SortColumn) {
        if (this.state.sortColumn !== column) {
            this.setState({sortColumn: column});
        } else {
            this.setState({sortColumn: "none"});
        }
    }

    getMobileSortName() {
        switch (this.state.sortColumn) {
            case "name":
                return i18n.t("chargers.charger_name");
            case "status":
                return i18n.t("chargers.status");
            case "uid":
                return i18n.t("chargers.charger_id");
            case "note":
                return i18n.t("chargers.note")
            default:
                return i18n.t("chargers.select_sorting");
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
                    <Col className="d-flex justify-content-center align-items-center">
                        {charger.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}
                    </Col>
                </td>
                <td class="align-middle">
                    {charger.name}
                </td>
                <td class="align-middle">
                    {Base58.int_to_base58(charger.uid)}
                </td>
                <td class="align-middle">
                        <Button style="background-color:transparent;border:none;" className="me-2"
                                onClick={() => this.setState({showEditNoteModal: true, editNote: charger.note, editChargerIdx: index})}>
                            <Edit color="#333"/>
                        </Button>
                        {charger.note}
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
                        this.setState({showDeleteModal: true})
                    }} variant="danger">
                        {t("remove")}
                    </Button>
                </td>
            </tr>
            table_list.push(entry);
            card_list.push(this.create_card(charger));
        })

        return <>
            {/*Delete Charger Modal begin*/}
            <Modal show={this.state.showDeleteModal} onHide={() => this.setState({showDeleteModal: false})}>
                <Modal.Header>
                    {t("delete_modal_heading", {name: this.removal_charger.name})}
                </Modal.Header>
                <Modal.Body>
                    {t("delete_modal_body", {name: this.removal_charger.name})}
                </Modal.Body>
                <Modal.Footer>
                    <Button variant="danger" onClick={async () => {
                        this.delete_charger();
                        this.setState({showDeleteModal: false});
                    }}>{t("remove")}</Button>
                    <Button variant="secondary" onClick={async () => {
                        this.setState({showDeleteModal: false});
                    }}>{t("close")}</Button>
                </Modal.Footer>
            </Modal>
            {/*Delete Charger Modal end*/}

            {/*Edit Note Modal begin*/}
            <Modal show={this.state.showEditNoteModal} onHide={() => this.setState({showEditNoteModal: false, editNote: "", editChargerIdx: -1})}>
                <Form onSubmit={async (e) => {
                    e.preventDefault();
                    const encryptedNote = sodium.crypto_box_seal(this.state.editNote, pub_key);
                    const b64Note = Base64.fromUint8Array(encryptedNote);

                    const {error} = await fetchClient.POST("/charger/update_note", {credentials: "same-origin", body: {note: b64Note, charger_id: this.state.chargers[this.state.editChargerIdx].id}});
                    if (error) {
                        showAlert(error, "danger", t("edit_note_failed"));
                    }

                    const chargers = this.state.chargers;
                    chargers[this.state.editChargerIdx].note = this.state.editNote;
                    this.setState({showEditNoteModal: false, editNote: "", editChargerIdx: -1, chargers: chargers});
                }}>
                    <Modal.Header>
                        {t("edit_note_heading")}
                    </Modal.Header>
                    <Modal.Body>
                        <Form.Control value={this.state.editNote} onChange={(e) => this.setState({editNote: (e.target as HTMLInputElement).value})}/>
                    </Modal.Body>
                    <Modal.Footer>
                        <Button variant="secondary" onClick={() => this.setState({showEditNoteModal: false, editNote: "", editChargerIdx: -1})}>
                            {t("decline")}
                        </Button>
                        <Button type="submit">
                            {t("accept")}
                        </Button>
                    </Modal.Footer>
                </Form>
            </Modal>
            {/*Edit Note Modal end*/}

            <Col className="d-none d-md-block">
                <Table striped hover>
                    <thead>
                        <tr>
                            <th onClick={() => this.setSort("status")}>
                                <Row>
                                    <Col className="align-content-end text-end">
                                        {this.get_icon("status")}
                                    </Col>
                                </Row>
                            </th>
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
                                <Row className="g-1">
                                    <Col>
                                        {t("charger_id")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("uid")}
                                    </Col>
                                </Row>
                            </th>
                            <th onClick={() => this.setSort("note")}>
                                <Row className="g1">
                                    <Col>
                                        {t("note")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("note")}
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
                <Col className={Median.isNativeApp() ? "mt-2" : undefined}>
                    <ButtonGroup>
                        <DropdownButton className="dropdown-btn" title={this.getMobileSortName()}>
                            <Dropdown.Item onClick={() => this.setMobileSort("name")}>{t("charger_name")}</Dropdown.Item>
                            <Dropdown.Item onClick={() => this.setMobileSort("uid")}>{t("charger_id")}</Dropdown.Item>
                            <Dropdown.Item onClick={() => this.setMobileSort("status")}>{t("status")}</Dropdown.Item>
                            <Dropdown.Item onClick={() => this.setMobileSort("note")}>{t("note")}</Dropdown.Item>
                        </DropdownButton>
                        <DropdownButton className="dropdown-btn" title={this.state.sortSequence == "asc" ? t("sorting_sequence_asc") : t("sorting_sequence_desc")}>
                            <Dropdown.Item onClick={() => this.setState({sortSequence: "asc"})}>{t("sorting_sequence_asc")}</Dropdown.Item>
                            <Dropdown.Item onClick={() => this.setState({sortSequence: "desc"})}>{t("sorting_sequence_desc")}</Dropdown.Item>
                        </DropdownButton>
                    </ButtonGroup>
                </Col>
                {card_list}
            </Container>
        </>
    }
}
