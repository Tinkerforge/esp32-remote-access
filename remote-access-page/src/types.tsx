export enum MessageType {
    Websocket,
    FileDownload
}

export interface Message {
    type: MessageType,
    data: string
}
