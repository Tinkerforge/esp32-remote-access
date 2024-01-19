
import { Client } from 'wg-webclient';
import { StateUpdater, useState } from 'preact/hooks';
import { Component } from 'preact';

const secret = "EFHaYB4PvohMsO7VqxNQFyQhw6uKq6PD0FpjhZrCMkI=";
const peer = "T1gy5yRSwYlSkjxAfnk/koNhlRyxsrFhdGW87LY1cxM=";
const wgClient = new Client(secret, peer);
let data_url: [string, StateUpdater<string>];

setTimeout(() => {
    wgClient.download_pcap_log();
}, 10000)

export class Frame extends Component {
    constructor() {
        super();

        wgClient.fetch("/", "GET").then(async (e: Response) => {
            const data = await e.blob();
            const url = URL.createObjectURL(data);
            data_url[1](url);
        });

        wgClient.fetch("/evse/state", "GET").then(async (e: Response) => {
            const data = await e.json();
            console.log(data);
        });

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
            iframe = <iframe src={data_url[0]} height={600} width={1048}></iframe>;
        }
        return (
            <div class="home">
                {iframe}
            </div>
        )
    }
}
