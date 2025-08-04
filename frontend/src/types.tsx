export enum MessageType {
    Websocket,
    FileDownload,
    Fetch,
    FetchResponse,
    Setup,
    Error,
    StoreSecret,
    RequestSecret,
    ClearSecret,
}

export interface Message {
    receiver_id?: string,
    type: MessageType,
    id?: string,
    data: FetchMessage | ResponseMessage | SetupMessage | ErrorMessage | Uint8Array | string | null
}

export interface FetchMessage {
    method: string,
    headers: [string, string][],
    body?: ArrayBuffer,
    url: string,
}

export interface ResponseMessage {
    status: number,
    statusText: string,
    headers: [string, string][],
    body: ArrayBuffer,
}

export interface SetupMessage {
    chargerID: string,
    port: number,
    secret: Uint8Array,
    debugMode: boolean,
}

export interface ErrorMessage {
    translation: string,
    format: unknown,
}

export interface ChargerKeys {
    id: string,
    charger_id: string,
    charger_pub: string,
    charger_address: string,
    web_private: number[],
    psk: number[],
    web_address: string,
}
