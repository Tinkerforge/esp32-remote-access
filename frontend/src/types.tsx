export enum MessageType {
    Websocket,
    FileDownload,
    Fetch,
    FetchResponse,
    Setup,
}

export interface Message {
    type: MessageType,
    id?: string,
    data: any
}

export interface FetchMessage {
    method: string,
    headers: [string, string][],
    body?: ArrayBuffer,
    url: string
}

export interface ResponseMessage {
    status: number,
    statusText: string,
    headers: [string, string][],
    body: ArrayBuffer,
}

export interface SetupMessage {
    self_key: string,
    peer_key: string,
    psk: string,
    self_internal_ip: string,
    peer_internal_ip: string,
    key_id: string,
}

export const BACKEND = import.meta.env.VITE_BACKEND_URL;
