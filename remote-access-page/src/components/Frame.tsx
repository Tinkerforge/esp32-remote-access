
import { WgClient } from 'wg-webclient';
import { StateUpdater, useState } from 'preact/hooks';
import { Component } from 'preact';

const secret = "EMx11sTpRVrReWObruImxwm3rxZMwSJWBqdIJRDPxHM=";
const peer = "AZmudADBwjZIF6vOEDnnzgVPmg/hI987RPllAM1wW2w=";
const wgClient = new WgClient(secret, peer);
let data_url: [string, StateUpdater<string>];

setTimeout(async () => {
	let event = await wgClient.get("/");
	window.addEventListener(event, (e) => {
		console.log(e.detail);
		const data = new Blob([e.detail]);
		const url = URL.createObjectURL(data);
		data_url[1](url);
		console.log(data)
	}, {once: true})
	// greet();
}, 5000);

export class Frame extends Component {
    render() {
        data_url = useState("");
        let iframe = <></>;
        if (data_url[0] != "") {
            iframe = <iframe src={data_url[0]}></iframe>;
        }
        return (
            <div class="home">
                {iframe}
            </div>
        )
    }
}
