export interface Device {
    id: string,
    uid: number,
    name: string,
    note?: string | null,
    status: string,
    port: number,
    valid: boolean,
    last_state_change?: number | null,
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
}

export type SortColumn = "name" | "uid" | "status" | "none" | "note" | "last_state_change";

export interface DeviceListState {
    devices: StateDevice[],
    showDeleteModal: boolean,
    showEditNoteModal: boolean,
    editNote: string,
    editChargerIdx: number,
    sortColumn: SortColumn,
    sortSequence: "asc" | "desc"
}
