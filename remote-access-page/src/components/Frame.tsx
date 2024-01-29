
import { Client } from 'wg-webclient';
import { StateUpdater, useState } from 'preact/hooks';
import { Component } from 'preact';
import { Message, MessageType } from '../types';
// import Worker from '../fetch_sw?worker';


// const secret = "+ATK0a+6eX1w4cZ/ueDLfJGa/er8VfCfnon/9I7Hd2s=";
// const peer = "Ev900s9ZPaBFYR0qQqmv4n2zYzOH69XPqsISPf3GXD4=";
// const wgClient = new Client(secret, peer, "ws://localhost:8081");
let data_url: [string, StateUpdater<string>];

// setTimeout(() => {
//     wgClient.download_pcap_log();
// }, 10000)

// setInterval(() => {
//     wgClient.fetch("/evse/state", "GET").then(async (e: Response) => {
//         const data = await e.json();
//         console.log(data);
//     });
// }, 500);

export class Frame extends Component {
    constructor() {
        super();

        setTimeout(async () => {
            let response = await fetch("/evse/state");
            console.log(await response.json());

            const req = new XMLHttpRequest();
            req.addEventListener("load", (e) => {
                console.log(e);
            });
            req.open("GET", "http://localhost/evse/state");
            req.send();
        }, 500);

        // if ("serviceWorker" in navigator) {
        //     console.log(import.meta.env.MODE);
        //     navigator.serviceWorker.register(
        //         import.meta.env.MODE === 'production' ? '/worker.js' : '/dev-sw.js?dev-sw',
        //         { type: import.meta.env.MODE === 'production' ? 'classic' : 'module' }
        //     ).then((v) => {
        //         console.log(v);
        //         fetch("/esve/state");
        //     })

        // }
        // const worker = new Worker();

        // console.log("frame constructor");
        // worker.addEventListener("message", async (e: MessageEvent) => {

        //     let test = await fetch("/evse/state");
        //     console.log("test", await test);
        //     const data = e.data as Message;
        //     console.log("got message", data);
        //     switch (data.type) {
        //         case MessageType.FileDownload:
        //             const a = document.createElement("a");
        //             a.href = data.data;
        //             a.download = "log.pcap";
        //             a.target = "_blank";
        //             a.click();
        //             break;
        //     }
        // });


        // wgClient.start_ws();

        // wgClient.on_message((e: string) => {
        //     const iframe = document.getElementById("interface") as HTMLIFrameElement;
        //     iframe.contentWindow.postMessage(e, "*");
        //     // console.log("got message", e);
        // })

        // }, {once: true})

        // const data = {
        //     "tasks": [
        //       {
        //         "trigger": [
        //           5,
        //           {
        //             "tag_type": 2,
        //             "tag_id": "04:52:40:1A:25:55:80"
        //           }
        //         ],
        //         "action": [
        //           2,
        //           {
        //             "topic": "test",
        //             "payload": "bla",
        //             "retain": false,
        //             "use_prefix": false
        //           }
        //         ]
        //       },
        //       {
        //         "trigger": [
        //           3,
        //           {
        //             "topic_filter": "bla",
        //             "payload": "",
        //             "retain": false,
        //             "use_prefix": false
        //           }
        //         ],
        //         "action": [
        //           7,
        //           {
        //             "tag_type": 2,
        //             "tag_id": "04:52:40:1A:25:55:80",
        //             "action": 0
        //           }
        //         ]
        //       }
        //     ]
        //   }
        // const body = new TextEncoder().encode(JSON.stringify(data));

        // let put = wgClient.fetch("/automation/config", "PUT", body);
        // window.addEventListener(put, (e: CustomEvent) => {
        //     console.log("put:", new TextDecoder().decode(e.detail.body()));
        // });
    }
    render() {
        data_url = useState("");
        let iframe = <></>;
        if (data_url[0] != "") {
            // window.fetch = async (...args) => {
            //     const response = await wgClient.fetch(...args);
            //     return response;
            // }
            iframe = <iframe src={data_url[0]} height={600} width={1048} id="interface"></iframe>;
        }
        return (
            <div class="home">
                {iframe}
            </div>
        )
    }
}
