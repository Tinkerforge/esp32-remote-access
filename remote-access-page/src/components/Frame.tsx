import { Component } from 'preact';
import { Message, MessageType } from '../types';
import Worker from '../worker?worker'

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

        this.worker.onmessage = (e: MessageEvent) => {
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
        window.addEventListener("message", (e: MessageEvent) => {
            if (e.data === "initIFrame") {
                this.worker.postMessage("connect");
                return;
            }
        });
    }

    // onload() {

    //     this.worker.onmessage = (e: MessageEvent) => {
    //         console.log("Got message from worker");
    //         const data = e.data;
    //         if (data.type === MessageType.Websocket) {
    //         } else if (data.type === MessageType.FileDownload) {
    //         }
    //     };

    // }

    render() {
        return (
            <div class="home">
                <iframe height={600} width={1048} id="interface"></iframe>
                <button onClick={() => {
                    console.log("trigger download");
                    this.worker.postMessage("download");
                }}>Download Pcap log</button>
            </div>
        )
    }
}
