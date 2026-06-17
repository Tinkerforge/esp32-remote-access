export interface Device {
    id: string,
    uid: number,
    name: string,
    note?: string | null,
    status: string,
    port: number,
    valid: boolean,
    last_state_change?: number | null,
    firmware_version: string,
}

export interface StateDevice {
    id: string,
    uid: number,
    name: string,
    status: string,
    note: string,
    port: number,
    valid: boolean,
    last_state_change?: number | null,
    firmware_version: string,
    // Hostname/IP of the device on the local network. Set for two kinds of
    // devices, and is the sole marker that a device is reachable on the LAN:
    //   1. Standalone local devices that are not yet paired with the cloud
    //      account. They are identified by an empty `id`; cloud-only
    //      actions (remove, edit note) are hidden in the UI for them.
    //   2. Cloud-paired devices that are *also* reachable on the LAN. They
    //      gain a "local" badge in the UI while the cloud management actions 
    //      stay available.
    // Unset for cloud-only devices. The WARP app's discovery bridge uses
    // `host` to route the user to the device directly.
    host?: string,
}

export type SortColumn = "name" | "uid" | "status" | "none" | "note" | "last_state_change" | "firmware_version";

// Selects how to reach a device. `"default"` keeps the legacy behavior of
// preferring the local network when available and falling back to the cloud.
// `"local"` / `"cloud"` force the respective path (with a graceful fallback to
// the other if the chosen one is not actually reachable).
export type ConnectVia = "default" | "local" | "cloud";

export interface Grouping {
    id: string,
    name: string,
    device_ids: string[],
}

export interface DeviceListState {
    devices: StateDevice[],
    showDeleteModal: boolean,
    showEditNoteModal: boolean,
    showGroupingModal: boolean,
    editNote: string,
    editChargerIdx: number,
    sortColumn: SortColumn,
    sortSequence: "asc" | "desc",
    searchTerm: string,
    filteredDevices: StateDevice[],
    groupings: Grouping[],
    selectedGroupingId: string | null,
    groupingSearchTerm: string,
    isLoading: boolean,
    // Devices discovered on the local network, kept separate from the cloud-paired
    // `devices` list so the two sources can update independently. They are merged
    // into the rendered list together with `devices`.
    localDevices: StateDevice[],
    // The pristine cloud-paired device list, kept separate from `devices` so a
    // local-discovery update can re-merge with the original cloud entries
    // (instead of re-deriving them from the already-merged `devices` list).
    cloudDevices: StateDevice[],
}
