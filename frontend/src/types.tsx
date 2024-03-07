export enum MessageType {
    Websocket,
    FileDownload,
    Fetch,
    FetchResponse,
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

export const BACKEND = import.meta.env.VITE_BACKEND_URL;
