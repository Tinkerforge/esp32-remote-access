import { StateUpdater, useState, useEffect } from 'preact/hooks';
import { Component } from 'preact';
let data_url: [string, StateUpdater<string>];

export class Frame extends Component {
    constructor() {
        super();
    }

    onload() {
        window.addEventListener("message", (e: MessageEvent) => {
            if (e.data === "initIFrame") {
                navigator.serviceWorker.controller.postMessage("connect");
                return;
            }
        });


        navigator.serviceWorker.addEventListener("message", (e: MessageEvent) => {
            const iframe = document.getElementById("interface") as HTMLIFrameElement;
            const window = iframe.contentWindow;
            window.postMessage(e.data);
        });

    }

    render() {
        data_url = useState("");
        return (
            <div class="home">
                <iframe src="/wg/" onLoad={this.onload} height={600} width={1048} id="interface"></iframe>
            </div>
        )
    }
}
