import * as Base58 from "base58";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component, VNode } from "preact";
import { fetchClient, get_decrypted_secret, pub_key, secret } from "../utils";
import { Button, ButtonGroup, Card, Col, Collapse, Container, Dropdown, DropdownButton, Form, Modal, Row, Table } from "react-bootstrap";
import i18n from "../i18n";
import { ChevronDown, ChevronUp, Edit, Monitor, Trash2 } from "react-feather";
import { Circle } from "../components/Circle";
import Median from "median-js-bridge";
import { useState } from "preact/hooks";
import { useLocation } from "preact-iso";

interface Device {
    id: string,
    uid: number,
    name: string,
    note?: string | null,
    status: string,
    port: number,
    valid: boolean,
    last_state_change?: number | null,
}

interface StateDevice {
    id: string,
    uid: number,
    name: string,
    status: string,
    note: string,
    port: number,
    valid: boolean,
    last_state_change?: number | null,
}

type SortColumn = "name" | "uid" | "status" | "none" | "note" | "last_state_change";

interface DeviceListState {
    devices: StateDevice[],
    showDeleteModal: boolean,
    showEditNoteModal: boolean,
    editNote: string,
    editChargerIdx: number,
    sortColumn: SortColumn,
    sortSequence: "asc" | "desc"
}

export class DeviceList extends Component<{}, DeviceListState> {

    removalDevice: StateDevice;
    updatingInterval: any;
    constructor() {
        super();

        this.removalDevice = {
            id: "",
            uid: 0,
            name: "",
            status: "",
            port: 0,
            valid: true,
            note: "",
            last_state_change: null,
        };
        this.state = {
            devices: [],
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

    decryptNote(note?: string | null) {
        if (!note) {
            return "";
        }

        try {
            const noteBytes = Base64.toUint8Array(note);

            // pub_key and secret are null-checked before this function is called
            const decryptedNote = sodium.crypto_box_seal_open(noteBytes, pub_key as Uint8Array, secret as Uint8Array);
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
            const {data, error} = await fetchClient.GET("/charger/get_chargers", {credentials: "same-origin"})

            if (error || !data) {
                showAlert(i18n.t("chargers.loading_devices_failed"), "danger");
                return;
            }

            const devices: Device[] = data;
            const stateDevices = [];
            for (const device of devices) {
                let name = this.decrypt_name(device.name);
                let note = this.decryptNote(device.note);
                if (name === undefined || note === undefined) {
                    note = i18n.t("chargers.invalid_key");
                    name = "";
                    device.valid = false
                }
                const state_charger: StateDevice = {
                    id: device.id,
                    uid: device.uid,
                    name: name,
                    note: note,
                    status: device.status,
                    port: device.port,
                    valid: device.valid,
                    last_state_change: device.last_state_change,
                }
                stateDevices.push(state_charger);
            }
            this.setState({devices: stateDevices});
        } catch (e) {
            const error = `${e}`;
            if (error.indexOf("Network") !== -1) {
                const updateDevices: StateDevice[] = [];
                for (const charger of this.state.devices) {
                    charger.status = "Disconnected";
                    updateDevices.push(charger);
                }
                this.setState({devices: updateDevices});
            } else {
                showAlert(error, "danger", "get_chargers");
            }
        }
    }

    componentWillUnmount() {
        clearInterval(this.updatingInterval);
    }

    async connect_to_charger(device: StateDevice, route: (path: string, replace?: boolean) => void) {
        route(`/devices/${device.id}`);
    }

    async delete_charger() {
        const t = i18n.t;
        const device = this.removalDevice;
        const body = {
            charger: device.id
        };
        const {response, error} = await fetchClient.DELETE("/charger/remove", {body: body, credentials: "same-origin"});

        if (response.status === 200) {
            const devices = this.state.devices.filter((c) => c.id !== device.id);
            this.setState({devices});
        } else {
            showAlert(t("remove_error_text", {charger_id: Base58.int_to_base58(device.uid), status: response.status, text: error}), "danger");
        }
    }

    decrypt_name(name: string) {
        if (!name) {
            return "";
        }
        const name_bytes = Base64.toUint8Array(name);
        try {
            // pub_key and secret are null-checked before this function is called
            const decrypted_name =  sodium.crypto_box_seal_open(name_bytes, pub_key as Uint8Array, secret as Uint8Array);
            const decoder = new TextDecoder();
            return decoder.decode(decrypted_name);
        } catch {
            return undefined;
        }
    }

    formatLastStateChange(t: (key: string, options?: any) => string, timestamp?: number | null): string {
        if (!timestamp) {
            return "-";
        }

        const date = new Date(timestamp * 1000);
        const now = new Date();
        const diffMs = now.getTime() - date.getTime();
        const diffMinutes = Math.floor(diffMs / (1000 * 60));
        const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
        const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

        if (diffMinutes < 1) {
            return t("time_just_now");
        } else if (diffMinutes < 60) {
            return t("time_minutes_ago", { count: diffMinutes });
        } else if (diffHours < 24) {
            return t("time_hours_ago", { count: diffHours });
        } else if (diffDays < 7) {
            return t("time_days_ago", { count: diffDays });
        } else {
            return date.toLocaleDateString();
        }
    }

    connection_possible(device: StateDevice) {
        let connection_possible = true;
        if (device.status !== "Connected" || device.valid === false) {
            connection_possible = false;
        }
        return connection_possible;
    }

    create_card(device: StateDevice, split: String[], index: number, route: (path: string, replace?: boolean) => void) {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        const [expand, setExpand] = useState(false);
        return <>
            <Card className="my-2">
                <Card.Header onClick={async () => {
                    if (!this.connection_possible(device)) {
                        return;
                    }
                    await this.connect_to_charger(device, route);
                }} className="d-flex justify-content-between align-items-center p-2d5">
                    <Col xs="auto" className="d-flex">
                        {device.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}
                    </Col>
                    <Col className="mx-3">
                        <h5 class="text-break" style="margin-bottom: 0;">{device.name}</h5>
                    </Col>
                    <Col className="d-flex justify-content-end">
                        <Button className="me-2" variant="primary" disabled={!this.connection_possible(device)} onClick={async () => {
                            await this.connect_to_charger(device, route);
                        }}><Monitor/></Button>
                        <Button variant="danger" onClick={async (e) => {
                            e.stopPropagation();
                            this.removalDevice = device;
                            this.setState({showDeleteModal: true});
                        }}><Trash2/></Button>
                    </Col>
                </Card.Header>
                <Card.Body>
                    <Row >
                        <Col xs="auto"><b>{t("mobile_charger_id")}</b></Col>
                        <Col className="text-end">{Base58.int_to_base58(device.uid)}</Col>
                    </Row>
                    <hr style="margin-top: 5px;margin-bottom: 5px;"/>
                    <Row>
                        <Col xs="auto"><b>{t("last_state_change")}</b></Col>
                        <Col className="text-end">{this.formatLastStateChange(t, device.last_state_change)}</Col>
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
                                                editNote: device.note,
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
                    <p style="color:red;" hidden={device.valid}>{t("no_keys")}</p>
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
                return i18n.t("chargers.note");
            case "last_state_change":
                return i18n.t("chargers.last_state_change");
            default:
                return i18n.t("chargers.select_sorting");
        }
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});
        const {route} = useLocation();

        const table_list: VNode[] = [];
        const card_list: VNode[] = [];
        const devices = this.state.devices;
        devices.sort((a, b) => {
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
                default:
                    // Handle null/undefined values (like last_state_change)
                    if (first === null || first === undefined) {
                        ret = second === null || second === undefined ? 0 : 1;
                    } else if (second === null || second === undefined) {
                        ret = -1;
                    } else {
                        ret = (first as number) - (second as number);
                    }
                    break;
            }
            if (this.state.sortSequence === "asc") {
                return ret;
            } else {
                return ret * -1;
            }
        })

        // Show empty state message if no devices
        if (devices.length === 0) {
            return <Container fluid className="text-center mt-5">
                <div className="text-muted">
                    <h5>{t("no_devices")}</h5>
                </div>
            </Container>;
        }

        this.state.devices.forEach((charger, index) => {
            const [expand, setExpand] = useState(false);
            const trimmed_note = charger.note.trim();
            const split = trimmed_note.split("\n");
            const entry = <tr>
                <td class="align-middle text-center">
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
                <td class="align-middle text-center">
                    <div className="d-flex flex-row flex-md-wrap flex-lg-nowrap justify-content-center gap-2">
                        <Button disabled={!this.connection_possible(charger)} id={`connect-${charger.name}`} onClick={async () => {
                            await this.connect_to_charger(charger, route);
                            }} variant="primary">
                            {t("connect")}
                        </Button>
                        <Button onClick={async () => {
                            this.removalDevice = charger;
                            this.setState({showDeleteModal: true})
                        }} variant="danger">
                            {t("remove")}
                        </Button>
                    </div>
                    <p style="color:red;" hidden={charger.valid}>
                        {t("no_keys")}
                    </p>
                </td>
                <td class="align-middle text-center">
                    {this.formatLastStateChange(t, charger.last_state_change)}
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
            </tr>;
            table_list.push(entry);
            card_list.push(this.create_card(charger, split, index, route));
        })

        return <>
            {/*Delete Charger Modal begin*/}
            <Modal show={this.state.showDeleteModal} centered onHide={() => this.setState({showDeleteModal: false})}>
                <Modal.Header>
                    {t("delete_modal_heading", {name: this.removalDevice.name})}
                </Modal.Header>
                <Modal.Body>
                    {t("delete_modal_body", {name: this.removalDevice.name})}
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

                    // pub_key and secret are null-checked before this function is called
                    const encryptedNote = sodium.crypto_box_seal(this.state.editNote, pub_key as Uint8Array);
                    const b64Note = Base64.fromUint8Array(encryptedNote);

                    const {error} = await fetchClient.POST("/charger/update_note", {credentials: "same-origin", body: {note: b64Note, charger_id: this.state.devices[this.state.editChargerIdx].id}});
                    if (error) {
                        showAlert(error, "danger", t("edit_note_failed"));
                    }

                    const devices = this.state.devices;
                    devices[this.state.editChargerIdx].note = this.state.editNote;
                    this.setState({showEditNoteModal: false, editNote: "", editChargerIdx: -1, devices});
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
                <Table striped hover responsive>
                    <thead>
                        <tr class="charger-head">
                            <th onClick={() => this.setSort("status")}>
                                <Row className="m-0">
                                    <Col className="align-content-end text-end">
                                        {this.get_icon("status")}
                                    </Col>
                                </Row>
                            </th>
                            <th onClick={() => this.setSort("name")}>
                                <Row className="flex-nowrap m-0">
                                    <Col>
                                        {t("charger_name")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("name")}
                                    </Col>
                                </Row>
                            </th>
                            <th onClick={() => this.setSort("uid")}>
                                <Row className="flex-nowrap m-0">
                                    <Col>
                                        {t("charger_id")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("uid")}
                                    </Col>
                                </Row>
                            </th>
                            <th/>
                            <th onClick={() => this.setSort("last_state_change")}>
                                <Row className="flex-nowrap m-0">
                                    <Col>
                                        {t("last_state_change")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("last_state_change")}
                                    </Col>
                                </Row>
                            </th>
                            <th onClick={() => this.setSort("note")}>
                                <Row className="flex-nowrap m-0">
                                    <Col>
                                        {t("note")}
                                    </Col>
                                    <Col xs="auto">
                                        {this.get_icon("note")}
                                    </Col>
                                </Row>
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
                            <Dropdown.Item onClick={() => this.setMobileSort("last_state_change")}>{t("last_state_change")}</Dropdown.Item>
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
