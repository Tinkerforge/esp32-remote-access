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
    data: any
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
    format: any,
}
