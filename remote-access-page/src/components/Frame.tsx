import { StateUpdater, useState, useEffect } from 'preact/hooks';
import { Component } from 'preact';
import { MessageType } from '../types';
let data_url: [string, StateUpdater<string>];


export class Frame extends Component {

    worker: ServiceWorkerRegistration
    constructor() {
        super();

        this.cleanRegisterWorker();

        window.addEventListener("beforeunload", () => {
            navigator.serviceWorker.controller.postMessage("close");
            this.worker.unregister();
        })

        navigator.serviceWorker.onmessage = (e: MessageEvent) => {
            if (e.data === "ready") {
                const iframe = document.getElementById("interface") as HTMLIFrameElement;
                iframe.onload = this.onload;
                iframe.src = "/wg/";
            }
        };
    }

    async cleanRegisterWorker() {
        const regs = await navigator.serviceWorker.getRegistrations();
        for (let reg of regs) {
            reg.unregister();
        }
        this.worker = await navigator.serviceWorker.register("/worker.js");
    }

    onload() {
        window.addEventListener("message", (e: MessageEvent) => {
            if (e.data === "initIFrame") {
                navigator.serviceWorker.controller.postMessage("connect");
                return;
            }
        });

        navigator.serviceWorker.onmessage = (e: MessageEvent) => {
            const data = e.data;
            if (data.type === MessageType.Websocket) {
            const iframe = document.getElementById("interface") as HTMLIFrameElement;
            const window = iframe.contentWindow;
                window.postMessage(data.data);
            } else if (data.type === MessageType.FileDownload) {
                const a = document.createElement("a");
                const blob = new Blob([data.data as Uint8Array]);
                const url = URL.createObjectURL(blob)
                a.href = url;
                a.download = "out.pcap";
                a.target = "_blank";
                a.click();
            }
        };

    }

    render() {
        data_url = useState("");
        return (
            <div class="home">
                <iframe height={600} width={1048} id="interface"></iframe>
                <button onClick={() => {
                    console.log("trigger download");
                    navigator.serviceWorker.controller.postMessage("download");
                }}> Download Pcap log</button>
            </div>
        )
    }
}
