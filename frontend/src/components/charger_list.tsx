import * as Base58 from "base58";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component } from "preact";
import { fetchClient, get_decrypted_secret, pub_key, secret } from "../utils";
import { Button, ButtonGroup, Card, Col, Collapse, Container, Dropdown, DropdownButton, Form, Modal, Row, Table } from "react-bootstrap";
import i18n from "../i18n";
import { ChevronDown, ChevronUp, Edit, Monitor, Trash2 } from "react-feather";
import { Circle } from "./Circle";
import Median from "median-js-bridge";
import { Dispatch, StateUpdater, useState } from "preact/hooks";
import { ChargersState } from "../pages/chargers";
import { useLocation } from "preact-iso";

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

interface ChargerListProps {
    parentState: ChargersState,
    setParentState: Dispatch<StateUpdater<ChargersState>>,
}

export class ChargerListComponent extends Component<ChargerListProps, ChargerListComponentState> {

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

        try {
            const noteBytes = Base64.toUint8Array(note);
            const decryptedNote = sodium.crypto_box_seal_open(noteBytes, pub_key, secret);
            const decoder = new TextDecoder();
            return decoder.decode(decryptedNote);
        } catch {
            return undefined;
        }

    }

    async updateChargers(that: any) {
        if (!secret) {
            await get_decrypted_secret();
        }
        try {
            const {data} = await fetchClient.GET("/charger/get_chargers", {credentials: "same-origin"})

            const chargers: Charger[] = data;
            const state_chargers = [];
            for (const charger of chargers) {
                let name = this.decrypt_name(charger.name);
                let note = this.decryptNote(charger.note);
                if (name === undefined || note === undefined) {
                    note = i18n.t("chargers.invalid_key");
                    charger.valid = false
                }
                const state_charger: StateCharger = {
                    id: charger.id,
                    uid: charger.uid,
                    name: name,
                    note: note,
                    status: charger.status,
                    port: charger.port,
                    valid: charger.valid,
                }
                state_chargers.push(state_charger);
            }
            this.setState({chargers: state_chargers});
        } catch (e) {
            const error = `${e}`;
            if (error.indexOf("Network") !== -1) {
                const updateChargers: StateCharger[] = [];
                for (const charger of this.state.chargers) {
                    charger.status = "Disconnected";
                    updateChargers.push(charger);
                }
                this.setState({chargers: updateChargers});
            } else {
                showAlert(error, "danger", "get_chargers");
            }
        }
    }

    componentWillUnmount() {
        clearInterval(this.updatingInterval);
    }

    async connect_to_charger(charger: StateCharger, route: (path: string, replace?: boolean) => void) {
        route(`/chargers/${charger.id}`);
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
        try {
            const decrypted_name =  sodium.crypto_box_seal_open(name_bytes, pub_key, secret);
            const decoder = new TextDecoder();
            return decoder.decode(decrypted_name);
        } catch {
            return undefined;
        }
    }

    connection_possible(charger: StateCharger) {
        let connection_possible = true;
        if (charger.status !== "Connected" || charger.valid === false) {
            connection_possible = false;
        }
        return connection_possible;
    }

    create_card(charger: StateCharger, split: String[], index: number, route: (path: string, replace?: boolean) => void) {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        const [expand, setExpand] = useState(false);
        return <>
            <Card className="my-2">
                <Card.Header onClick={async () => {
                    if (!this.connection_possible(charger)) {
                        return;
                    }
                    await this.connect_to_charger(charger, route);
                }} className="d-flex justify-content-between align-items-center p-2d5">
                    <Col xs="auto" className="d-flex">
                        {charger.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}
                    </Col>
                    <Col className="mx-3">
                        <h5 class="text-break" style="margin-bottom: 0;">{charger.name}</h5>
                    </Col>
                    <Col className="d-flex justify-content-end">
                        <Button className="me-2" variant="primary" disabled={!this.connection_possible(charger)} onClick={async () => {
                            await this.connect_to_charger(charger, route);
                        }}><Monitor/></Button>
                        <Button variant="danger" onClick={async (e) => {
                            e.stopPropagation();
                            this.removal_charger = charger;
                            this.setState({showDeleteModal: true});
                        }}><Trash2/></Button>
                    </Col>
                </Card.Header>
                <Card.Body>
                    <Row >
                        <Col xs="auto"><b>{t("mobile_charger_id")}</b></Col>
                        <Col className="text-end">{Base58.int_to_base58(charger.uid)}</Col>
                    </Row>
                    <hr style="margin-top: 5px;margin-bottom: 5px;"/>
                    <Row>
                        <Col xs="auto">
                            <Row>
                                <b>{t("note")}</b>
                            </Row>
                            <Row>
                                <Col className="p-0">
                                    <Button style="background-color:transparent;border:none;"
                                        onClick={() => {
                                            this.setState({
                                                showEditNoteModal: true,
                                                editNote: charger.note,
                                                editChargerIdx: index
                                            });
                                        }}>
                                        <Edit color="#333"/>
                                    </Button>
                                </Col>
                            </Row>
                        </Col>
                        <Col onClick={split.length <= 3 ? undefined : () => setExpand(!expand)}
                            style={{cursor: split.length <= 3 ? undefined : "pointer", whiteSpace: "pre-line", overflowWrap: "anywhere"}}>
                                <Row>
                                    <Col className="d-flex justify-content-end" style={{textAlign: "right"}} >
                                        <div>
                                            {split.slice(0, split.length <= 3 ? 3 : 2).join("\n")}
                                        </div>
                                    </Col>
                                </Row>
                                <Row>
                                    <Col className="d-flex justify-content-end" style={{textAlign: "right"}}>
                                        <Collapse in={expand}>
                                            <div>
                                                {split.slice(2).join("\n")}
                                            </div>
                                        </Collapse>
                                    </Col>
                                </Row>

                                <Row hidden={split.length <= 3}>
                                    <Col className="d-flex justify-content-end" >
                                    <a style={{fontSize: "14px", color: "blue", textDecoration: "underline"}}>
                                        {expand ? t("show_less") : t("show_more")}
                                    </a>
                                    </Col>
                                </Row>
                        </Col>
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
        const {route} = useLocation();

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
            const [expand, setExpand] = useState(false);
            const trimmed_note = charger.note.trim();
            const split = trimmed_note.split("\n");
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
                <td style={{width: "1%"}} class="align-middle">
                    <Button disabled={!this.connection_possible(charger)} id={`connect-${charger.name}`} onClick={async () => {
                        await this.connect_to_charger(charger, route);
                        }} variant="primary">
                        {t("connect")}
                    </Button>
                    <p style="color:red;" hidden={charger.valid}>
                        {t("no_keys")}
                    </p>
                </td>
                <td style={{width:"1%"}} class="align-middle">
                    <Button onClick={async () => {
                        this.removal_charger = charger;
                        this.setState({showDeleteModal: true})
                    }} variant="danger">
                        {t("remove")}
                    </Button>
                </td>
                <td class="align-middle">
                    <Container fluid>
                        <Row>
                            <Col className="d-flex align-items-center p-0" style={{whiteSpace: "pre-line", overflowWrap: "anywhere"}}>
                                <Container onClick={split.length <= 2 ? undefined : () => setExpand(!expand)} style={{cursor: split.length <= 1 ? undefined : "pointer"}}>
                                    <Row>
                                        <Col>
                                            <div>
                                                {split.slice(0, split.length <= 2 ? 2 : 1).join("\n")}
                                            </div>
                                        </Col>
                                    </Row>
                                    <Row>
                                        <Col>
                                            <Collapse in={expand}>
                                                <div style={{whiteSpace: "pre-wrap"}}>
                                                    {split.slice(1).join("\n")}
                                                </div>
                                            </Collapse>
                                        </Col>
                                    </Row>
                                    <Row hidden={split.length <= 2}>
                                        <Col>
                                        <a style={{fontSize: "14px", color: "blue", textDecoration: "underline"}}>
                                            {expand ? t("show_less") : t("show_more")}
                                        </a>
                                        </Col>
                                    </Row>
                                </Container>
                            </Col>
                            <Col className="p-0" sm="auto">
                                <Button style="background-color:transparent;border:none;"
                                        onClick={() => {
                                            this.setState({showEditNoteModal: true, editNote: charger.note, editChargerIdx: index});
                                            setExpand(false);
                                        }}>
                                    <Edit color="#333"/>
                                </Button>
                            </Col>
                        </Row>
                    </Container>
                </td>
            </tr>
            table_list.push(entry);
            card_list.push(this.create_card(charger, split, index, route));
        })

        return <>
            {/*Delete Charger Modal begin*/}
            <Modal show={this.state.showDeleteModal} centered onHide={() => this.setState({showDeleteModal: false})}>
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
            <Modal
                show={this.state.showEditNoteModal}
                centered
                onHide={() => this.setState({
                    showEditNoteModal: false,
                    editNote: "",
                    editChargerIdx: -1
                })}
            >
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
                        <Form.Control as="textarea" value={this.state.editNote} onChange={(e) => this.setState({editNote: (e.target as HTMLInputElement).value})}/>
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
                        <tr class="charger-head">
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
                                <Row>
                                    <Col>
                                        {t("charger_id")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("uid")}
                                    </Col>
                                </Row>
                            </th>
                            <th/>
                            <th/>
                            <th onClick={() => this.setSort("note")}>
                                <Container fluid>
                                    <Row>
                                        <Col>
                                            {t("note")}
                                        </Col>
                                        <Col xs="auto">
                                            {this.get_icon("note")}
                                        </Col>
                                    </Row>
                                </Container>
                            </th>
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
