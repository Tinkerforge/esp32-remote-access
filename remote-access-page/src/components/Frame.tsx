
import { Client } from 'wg-webclient';
import { StateUpdater, useState } from 'preact/hooks';
import { Component } from 'preact';

const secret = "EMx11sTpRVrReWObruImxwm3rxZMwSJWBqdIJRDPxHM=";
const peer = "AZmudADBwjZIF6vOEDnnzgVPmg/hI987RPllAM1wW2w=";
const wgClient = new Client(secret, peer);
let data_url: [string, StateUpdater<string>];

setTimeout(async () => {
    let event = wgClient.fetch("/", "GET");
    window.addEventListener(event, (e: CustomEvent) => {
        const data = new Blob([e.detail]);
        const url = URL.createObjectURL(data);
        data_url[1](url);
    }, {once: true})
}, 1000);

setTimeout(() => {
    wgClient.download_pcap_log();
}, 10000)

export class Frame extends Component {
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
