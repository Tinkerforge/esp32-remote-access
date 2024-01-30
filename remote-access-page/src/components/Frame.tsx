
import { Client } from 'wg-webclient';
import { StateUpdater, useState } from 'preact/hooks';
import { Component } from 'preact';
import { Message, MessageType } from '../types';
let data_url: [string, StateUpdater<string>];

export class Frame extends Component {
    constructor() {
        super();

        setTimeout(async () => {
            let response = await fetch("/wg/");
            let data = await response.blob();
            data_url[1](URL.createObjectURL(data))
        }, 500);
    }
    render() {
        data_url = useState("");
        let iframe = <></>;
        if (data_url[0] != "") {
            iframe = <iframe src={data_url[0]} height={600} width={1048} id="interface"></iframe>;
        }
        return (
            <div class="home">
                {iframe}
            </div>
        )
    }
}
