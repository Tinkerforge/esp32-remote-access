import * as Base58 from "base58";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";
import { Component } from "preact";
import { fetchClient, get_decrypted_secret, pub_key, secret } from "../utils";
import { Button, Container, Dropdown, DropdownButton, Form, Spinner } from "react-bootstrap";
import i18n from "../i18n";
import { useLocation } from "preact-iso";
import { Device, StateDevice, SortColumn, DeviceListState, Grouping } from "../components/device/types";
import { DeviceTable } from "../components/device/DeviceTable";
import { DeviceMobileView } from "../components/device/DeviceMobileView";
import { DeleteDeviceModal } from "../components/device/DeleteDeviceModal";
import { EditNoteModal } from "../components/device/EditNoteModal";
import { SearchInput } from "../components/device/SearchInput";
import { GroupingModal } from "../components/device/GroupingModal";

export class DeviceList extends Component<Record<string, never>, DeviceListState> {
    removalDevice: StateDevice;
    stateUpdateWs: WebSocket | null;
    isMounted: boolean;

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
            showGroupingModal: false,
            editNote: "",
            editChargerIdx: 0,
            sortColumn: "none",
            sortSequence: "asc",
            searchTerm: "",
            filteredDevices: [],
            groupings: [],
            selectedGroupingId: null,
            groupingSearchTerm: "",
            isLoading: true,
        };

        this.stateUpdateWs = null;
        this.isMounted = true;
        this.loadGroupings();
        this.connectStateUpdateWebSocket();
    }

    componentWillUnmount() {
        this.isMounted = false;
        if (this.stateUpdateWs) {
            this.stateUpdateWs.close();
            this.stateUpdateWs = null;
        }
    }

    async connectStateUpdateWebSocket() {
        if (!secret) {
            await get_decrypted_secret();
        }

        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/api/charger/get_devices`;

        try {
            this.stateUpdateWs = new WebSocket(wsUrl);

            this.stateUpdateWs.onmessage = (event) => {
                try {
                    const message = JSON.parse(event.data);
                    // Handle state_change message with full charger list
                    if (message.type === 'state_change' && Array.isArray(message.chargers)) {
                        console.log('Charger state changed, updating list');
                        this.processChargers(message.chargers as Device[]);
                    }
                    // Handle initial charger list (array without type wrapper)
                    else if (Array.isArray(message)) {
                        console.log('Received initial charger list');
                        this.processChargers(message as Device[]);
                    }
                } catch (e) {
                    console.error('Failed to parse state update message:', e);
                }
            };

            this.stateUpdateWs.onerror = (error) => {
                console.error('State update WebSocket error:', error);
            };

            this.stateUpdateWs.onclose = () => {
                if (this.isMounted) {
                    console.log('State update WebSocket closed, reconnecting in 5s...');
                    setTimeout(() => this.connectStateUpdateWebSocket(), 5000);
                }
            };
        } catch (e) {
            console.error('Failed to create state update WebSocket:', e);
            if (this.isMounted) {
                setTimeout(() => this.connectStateUpdateWebSocket(), 5000);
            }
        }
    }

    processChargers(devices: Device[]) {
        const stateDevices: StateDevice[] = [];
        for (const device of devices) {
            let name = this.decrypt_name(device.name);
            let note = this.decryptNote(device.note);
            if (name === undefined || note === undefined) {
                note = i18n.t("chargers.invalid_key");
                name = "";
                device.valid = false;
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
            };
            stateDevices.push(state_charger);
        }
        this.setSortedDevices(stateDevices);
        this.setState({ isLoading: false });
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

    async decryptGroupingName(name: string) {
        if (!pub_key || !secret) {
            await get_decrypted_secret();
        }

        if (!name) {
            return "";
        }
        const nameBytes = Base64.toUint8Array(name);
        try {
            // pub_key and secret are null-checked before this function is called
            const decryptedName = sodium.crypto_box_seal_open(nameBytes, pub_key as Uint8Array, secret as Uint8Array);
            const decoder = new TextDecoder();
            return decoder.decode(decryptedName);
        } catch {
            return undefined;
        }
    }

    async encryptGroupingName(name: string) {
        if (!pub_key || !secret) {
            await get_decrypted_secret();
        }

        if (!name) {
            return "";
        }
        try {
            // pub_key and secret are null-checked before this function is called
            const encryptedName = sodium.crypto_box_seal(name, pub_key as Uint8Array);
            return Base64.fromUint8Array(encryptedName);
        } catch {
            return undefined;
        }
    }

    async updateChargers() {
        if (!secret) {
            await get_decrypted_secret();
        }
        try {
            const { data, error, response } = await fetchClient.GET("/charger/get_devices", { credentials: "same-origin" })

            if (error || !data) {
                showAlert(i18n.t("chargers.loading_devices_failed", {status: response.status, response: error}), "danger");
                this.setState({ isLoading: false });
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
            this.setState({ isLoading: false });
        } catch (e) {
            const error = `${e}`;
            if (error.indexOf("Network") !== -1) {
                const updateDevices: StateDevice[] = [];
                for (const charger of this.state.devices) {
                    charger.status = "Disconnected";
                    updateDevices.push(charger);
                }
                this.setState({ devices: updateDevices, isLoading: false });
            } else {
                showAlert(error, "danger", "get_devices");
                this.setState({ isLoading: false });
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
            this.setState({ devices }, () => {
                this.applyFilters();
            });
        } else {
            showAlert(t("remove_error_text", { charger_id: Base58.int_to_base58(device.uid), status: response.status, text: error }), "danger");
        }
    }

    async loadGroupings() {
        try {
            const { data, error } = await fetchClient.GET("/grouping/list", {
                credentials: "same-origin"
            });

            if (error || !data) {
                console.error("Failed to load groupings:", error);
                return;
            }

            // Decrypt grouping names
            const decryptedGroupings = await Promise.all(data.groupings.map(async (grouping) => {
                const decryptedName = await this.decryptGroupingName(grouping.name);
                return {
                    ...grouping,
                    name: decryptedName !== undefined ? decryptedName : i18n.t("chargers.invalid_key")
                };
            }));

            this.setState({ groupings: decryptedGroupings });
        } catch (error) {
            console.error("Failed to load groupings:", error);
        }
    }

    handleGroupingsUpdated = (groupings: Grouping[]) => {
        this.setState({ groupings });
    }

    handleGroupingFilterChange = (groupingId: string | null) => {
        this.setState({ selectedGroupingId: groupingId }, () => {
            this.applyFilters();
        });
    }

    applyFilters() {
        let filtered = this.filterDevices(this.state.devices, this.state.searchTerm);

        // Apply grouping filter
        if (this.state.selectedGroupingId) {
            const grouping = this.state.groupings.find(g => g.id === this.state.selectedGroupingId);
            if (grouping) {
                filtered = filtered.filter(device => grouping.device_ids.includes(device.id));
            }
        }

        this.setState({ filteredDevices: filtered });
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

    filterDevices(devices: StateDevice[], searchTerm: string): StateDevice[] {
        if (!searchTerm.trim()) {
            return devices;
        }

        const lowerSearchTerm = searchTerm.toLowerCase().trim();
        return devices.filter(device => {
            return (
                device.name.toLowerCase().includes(lowerSearchTerm) ||
                device.id.toLowerCase().includes(lowerSearchTerm) ||
                device.uid.toString().includes(lowerSearchTerm) ||
                device.status.toLowerCase().includes(lowerSearchTerm) ||
                device.note.toLowerCase().includes(lowerSearchTerm) ||
                device.firmware_version.toLowerCase().includes(lowerSearchTerm)
            );
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

        this.setState({ devices }, () => {
            this.applyFilters();
        });
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
            showAlert(error, "danger", i18n.t("chargers.edit_note_failed"));
            return;
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

    handleSearchChange = (searchTerm: string) => {
        this.setState({ searchTerm }, () => {
            this.applyFilters();
        });
    }

    render() {
        const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
        const { route } = useLocation();

        // Show spinner while loading
        if (this.state.isLoading) {
            return (
                <Container fluid className="text-center mt-5">
                    <Spinner animation="border" role="status">
                        <span className="visually-hidden">{t("loading")}</span>
                    </Spinner>
                </Container>
            );
        }

        // Apply filtering: if search term or grouping filter is active, show filtered devices
        const devices = (this.state.filteredDevices.length > 0 || this.state.searchTerm || this.state.selectedGroupingId)
            ? this.state.filteredDevices
            : this.state.devices;

        const handleConnect = async (device: StateDevice) => {
            await this.connect_to_charger(device, route);
        };

        // Show empty state message if no devices at all
        if (this.state.devices.length === 0) {
            return (
                <Container fluid className="text-center mt-5">
                    <div className="text-muted">
                        <h5>{t("no_devices")}</h5>
                    </div>
                    <Button
                        variant="primary"
                        className="mt-3"
                        onClick={() => route("/tokens")}
                    >
                        {i18n.t("tokens.add_device")}
                    </Button>
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

                <GroupingModal
                    show={this.state.showGroupingModal}
                    devices={this.state.devices}
                    groupings={this.state.groupings}
                    onClose={() => this.setState({ showGroupingModal: false })}
                    encryptGroupingName={async (name: string) => this.encryptGroupingName(name)}
                    loadGroupings={async () => this.loadGroupings()}
                />

                <Container fluid>
                    <div className="d-flex justify-content-between align-items-center mb-3 flex-wrap gap-2 mt-3">
                        <div className="flex-grow-1 gap-2">
                            <SearchInput
                                searchTerm={this.state.searchTerm}
                                onSearchChange={this.handleSearchChange}
                            />
                        </div>
                        <div className="d-flex gap-2">
                            {this.state.groupings.length > 0 && (
                                <DropdownButton variant="outline-secondary" title={t("groupings")} className="w-auto"
                                >
                                    <div class="px-1">
                                        <Form.Control placeholder={t("search_groupings")}
                                            value={this.state.groupingSearchTerm}
                                            onChange={(e) => this.setState({ groupingSearchTerm: (e.target as HTMLInputElement).value })} />
                                    </div>
                                    <Dropdown.Item onClick={() => this.handleGroupingFilterChange(null)}>
                                        {t("all_devices")}
                                    </Dropdown.Item>
                                    {this.state.groupings.filter(grouping => grouping.name.toLowerCase().includes(this.state.groupingSearchTerm.toLowerCase())).map(grouping => (
                                        <Dropdown.Item key={grouping.id} disabled={grouping.id === this.state.selectedGroupingId} onClick={() => this.handleGroupingFilterChange(grouping.id)}>
                                            {grouping.name} ({grouping.device_ids.length})
                                        </Dropdown.Item>
                                    ))}
                                </DropdownButton>
                            )}
                            <Button
                                variant="primary"
                                onClick={() => this.setState({ showGroupingModal: true })}
                            >
                                {t("manage_groupings")}
                            </Button>
                        </div>
                    </div>
                </Container>

                {devices.length === 0 && (this.state.searchTerm || this.state.selectedGroupingId) ? (
                    <Container fluid className="text-center mt-5">
                        <div className="text-muted">
                            <h5>{t("no_devices_found")}</h5>
                        </div>
                    </Container>
                ) : (
                    <>
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
                            groupings={this.state.groupings}
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
                            groupings={this.state.groupings}
                        />
                    </>
                )}
            </>
        );
    }
}
