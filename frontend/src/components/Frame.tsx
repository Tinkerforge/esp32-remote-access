import { Component } from 'preact';
import { signal } from '@preact/signals';
import { Message, MessageType, SetupMessage } from '../types';
import Worker from '../worker?worker'

export let charger_info = signal({
    self_key: "",
    peer_key: "",
    psk: "",
    self_internal_ip: "",
    peer_internal_ip: "",
    key_id: "",
    port: 80,
});

export class Frame extends Component {

    worker: Worker;
    constructor() {
        super();

        this.worker = new Worker();

        navigator.serviceWorker.addEventListener("message", (e: MessageEvent) => {
            const msg = e.data as Message;
            if (msg.type) {
                this.worker.postMessage(msg);
            } else {
                console.log("Got unknown message from service worker!");
            }
        });

        const message_event = (e: MessageEvent) => {
            if (e.data === "ready") {
                const iframe = document.getElementById("interface") as HTMLIFrameElement;
                iframe.src = "/wg/";
            } else {
                const msg = e.data as Message;
                switch (msg.type) {
                    case MessageType.Websocket:
                        const iframe = document.getElementById("interface") as HTMLIFrameElement;
                        const window = iframe.contentWindow;
                        window.postMessage(msg.data);
                        break;

                    case MessageType.FileDownload:
                        const a = document.createElement("a");
                        const blob = new Blob([msg.data as Uint8Array]);
                        const url = URL.createObjectURL(blob)
                        a.href = url;
                        a.download = "out.pcap";
                        a.target = "_blank";
                        a.click();
                        break;

                    case MessageType.FetchResponse:
                        navigator.serviceWorker.controller.postMessage(msg);
                        break;
                }
            }
        };

        this.worker.onmessage = (e: MessageEvent) => {
            if (e.data === "started") {
                this.worker.onmessage = message_event;
                const message_data: SetupMessage = charger_info.value;
                const message: Message = {
                    type: MessageType.Setup,
                    data: message_data
                };

                this.worker.postMessage(message);
            }
        }

        window.addEventListener("message", (e: MessageEvent) => {
            if (e.data === "initIFrame") {
                this.worker.postMessage("connect");
                return;
            }
        });
    }

    componentWillUnmount() {
        this.worker.terminate();
    }

    render() {
        return (
            <>
                <iframe width="100%" height={screen.height} id="interface"></iframe>
                {/* <button onClick={() => {
                    this.worker.postMessage("download");
                }}>Download Pcap log</button> */}
            </>
        )
    }
}
