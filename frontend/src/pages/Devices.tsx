import * as Base58 from "base58";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component } from "preact";
import { fetchClient, get_decrypted_secret, pub_key, secret } from "../utils";
import { Container } from "react-bootstrap";
import i18n from "../i18n";
import { useLocation } from "preact-iso";
import { Device, StateDevice, SortColumn, DeviceListState } from "../components/device/types";
import { DeviceTable } from "../components/device/DeviceTable";
import { DeviceMobileView } from "../components/device/DeviceMobileView";
import { DeleteDeviceModal } from "../components/device/DeleteDeviceModal";
import { EditNoteModal } from "../components/device/EditNoteModal";

export class DeviceList extends Component<{}, DeviceListState> {
    removalDevice: StateDevice;
    updatingInterval: ReturnType<typeof setInterval>;

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
            firmware_version: "",
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

        this.updateChargers();
        this.updatingInterval = setInterval(() => this.updateChargers(), 5000);
    }

    componentWillUnmount() {
        clearInterval(this.updatingInterval);
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

    decrypt_name(name: string) {
        if (!name) {
            return "";
        }
        const name_bytes = Base64.toUint8Array(name);
        try {
            // pub_key and secret are null-checked before this function is called
            const decrypted_name = sodium.crypto_box_seal_open(name_bytes, pub_key as Uint8Array, secret as Uint8Array);
            const decoder = new TextDecoder();
            return decoder.decode(decrypted_name);
        } catch {
            return undefined;
        }
    }

    async updateChargers() {
        if (!secret) {
            await get_decrypted_secret();
        }
        try {
            const { data, error, response } = await fetchClient.GET("/charger/get_chargers", { credentials: "same-origin" })

            if (error || !data) {
                showAlert(i18n.t("chargers.loading_devices_failed", {status: response.status, response: error}), "danger");
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
                    name,
                    note,
                    status: device.status,
                    port: device.port,
                    valid: device.valid,
                    last_state_change: device.last_state_change,
                    firmware_version: device.firmware_version,
                }
                stateDevices.push(state_charger);
            }
            this.setSortedDevices(stateDevices);
        } catch (e) {
            const error = `${e}`;
            if (error.indexOf("Network") !== -1) {
                const updateDevices: StateDevice[] = [];
                for (const charger of this.state.devices) {
                    charger.status = "Disconnected";
                    updateDevices.push(charger);
                }
                this.setState({ devices: updateDevices });
            } else {
                showAlert(error, "danger", "get_chargers");
            }
        }
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
        const { response, error } = await fetchClient.DELETE("/charger/remove", { body, credentials: "same-origin" });

        if (response.status === 200) {
            const devices = this.state.devices.filter((c) => c.id !== device.id);
            this.setState({ devices });
        } else {
            showAlert(t("remove_error_text", { charger_id: Base58.int_to_base58(device.uid), status: response.status, text: error }), "danger");
        }
    }

    formatLastStateChange(t: (key: string, options?: Record<string, unknown>) => string, timestamp?: number | null): string {
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
        }
            return date.toLocaleDateString();

    }

    connection_possible(device: StateDevice) {
        let connection_possible = true;
        if (device.status !== "Connected" || device.valid === false) {
            connection_possible = false;
        }
        return connection_possible;
    }

    setSort(column: SortColumn) {
        let newSortColumn: SortColumn;
        let newSortSequence: "asc" | "desc";

        if (this.state.sortColumn !== column) {
            newSortColumn = column;
            newSortSequence = "asc";
        } else if (this.state.sortSequence === "asc") {
            newSortColumn = column;
            newSortSequence = "desc";
        } else {
            newSortColumn = "none";
            newSortSequence = "asc";
        }

        this.setState({
            ...this.state,
            sortColumn: newSortColumn,
            sortSequence: newSortSequence
        }, () => {
            this.setSortedDevices([...this.state.devices]);
        });
    }

    setMobileSort(column: SortColumn) {
        let newSortColumn: SortColumn;

        if (this.state.sortColumn !== column) {
            newSortColumn = column;
        } else {
            newSortColumn = "none";
        }

        this.setState({ sortColumn: newSortColumn }, () => {
            this.setSortedDevices([...this.state.devices]);
        });
    }

    setSortedDevices(devices: StateDevice[]) {
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
            }
                return ret * -1;

        });
        this.setState({ devices });
    }

    handleDelete = (device: StateDevice) => {
        this.removalDevice = device;
        this.setState({ showDeleteModal: true });
    }

    handleEditNote = (device: StateDevice, index: number) => {
        this.setState({
            showEditNoteModal: true,
            editNote: device.note,
            editChargerIdx: index
        });
    }

    handleDeleteConfirm = async () => {
        await this.delete_charger();
        this.setState({ showDeleteModal: false });
    }

    handleDeleteCancel = () => {
        this.setState({ showDeleteModal: false });
    }

    handleEditNoteSubmit = async (e: Event) => {
        e.preventDefault();

        // pub_key and secret are null-checked before this function is called
        const encryptedNote = sodium.crypto_box_seal(this.state.editNote, pub_key as Uint8Array);
        const b64Note = Base64.fromUint8Array(encryptedNote);

        const { error } = await fetchClient.POST("/charger/update_note", {
            credentials: "same-origin",
            body: { note: b64Note, charger_id: this.state.devices[this.state.editChargerIdx].id }
        });

        if (error) {
            const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
            showAlert(error, "danger", t("edit_note_failed"));
        }

        const devices = this.state.devices;
        devices[this.state.editChargerIdx].note = this.state.editNote;
        this.setState({ showEditNoteModal: false, editNote: "", editChargerIdx: -1, devices });
    }

    handleEditNoteCancel = () => {
        this.setState({
            showEditNoteModal: false,
            editNote: "",
            editChargerIdx: -1
        });
    }

    render() {
        const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
        const { route } = useLocation();
        const devices = this.state.devices;

        const handleConnect = async (device: StateDevice) => {
            await this.connect_to_charger(device, route);
        };

        // Show empty state message if no devices
        if (devices.length === 0) {
            return (
                <Container fluid className="text-center mt-5">
                    <div className="text-muted">
                        <h5>{t("no_devices")}</h5>
                    </div>
                </Container>
            );
        }

        return (
            <>
                <DeleteDeviceModal
                    show={this.state.showDeleteModal}
                    device={this.removalDevice}
                    onConfirm={this.handleDeleteConfirm}
                    onCancel={this.handleDeleteCancel}
                />

                <EditNoteModal
                    show={this.state.showEditNoteModal}
                    note={this.state.editNote}
                    onNoteChange={(note) => this.setState({ editNote: note })}
                    onSubmit={this.handleEditNoteSubmit}
                    onCancel={this.handleEditNoteCancel}
                />

                <DeviceTable
                    devices={devices}
                    sortColumn={this.state.sortColumn}
                    sortSequence={this.state.sortSequence}
                    onSort={(column) => this.setSort(column)}
                    onConnect={handleConnect}
                    onDelete={this.handleDelete}
                    onEditNote={this.handleEditNote}
                    connectionPossible={(device) => this.connection_possible(device)}
                    formatLastStateChange={(t, timestamp) => this.formatLastStateChange(t, timestamp)}
                />

                <DeviceMobileView
                    devices={devices}
                    sortColumn={this.state.sortColumn}
                    sortSequence={this.state.sortSequence}
                    onMobileSort={(column) => this.setMobileSort(column)}
                    onSortSequenceChange={(sequence) => this.setState({ sortSequence: sequence }, () => {
                        // Re-sort the devices after state update
                        this.setSortedDevices([...this.state.devices]);
                    })}
                    onConnect={handleConnect}
                    onDelete={this.handleDelete}
                    onEditNote={this.handleEditNote}
                    connectionPossible={(device) => this.connection_possible(device)}
                    formatLastStateChange={(t, timestamp) => this.formatLastStateChange(t, timestamp)}
                />
            </>
        );
    }
}
