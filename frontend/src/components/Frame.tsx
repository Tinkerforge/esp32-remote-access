import { Component } from 'preact';
import { signal } from '@preact/signals';
import { Message, MessageType, SetupMessage } from '../types';
import Worker from '../worker?worker'
import { Row, Spinner } from 'react-bootstrap';
import { secret } from './charger_list';
import { setAppNavigation } from './Navbar';
import { enableLogging, refresh_access_token } from '../utils';
import Median from "median-js-bridge";
import i18n from '../i18n';
import { ChargersState } from '../pages/chargers';
import { Dispatch, StateUpdater } from 'preact/hooks';

interface FrameProps {
    parentState: ChargersState,
    setParentState: Dispatch<StateUpdater<ChargersState>>,
}

interface FrameState {
    show_spinner: boolean,
}

export class Frame extends Component<FrameProps, FrameState> {

    worker: Worker;
    show_spinner = signal(true);
    id: string;
    abort: AbortController;
    constructor() {
        super();

        this.abort = new AbortController();

        this.id = crypto.randomUUID();
        navigator.serviceWorker.addEventListener("message", (e: MessageEvent) => {
            const msg = e.data as Message;
            if (msg.receiver_id === this.id) {
                this.worker.postMessage(msg);
            }
        });

        this.startWorker();

        if (Median.isNativeApp()) {
            const t = i18n.t;
            Median.sidebar.setItems({
                enabled: true,
                persist: true,
                items: [
                    {
                        label: t("app.close_remote_access"),
                        url: "javascript:window.close()"
                    }
                ]
            })
        }

        // used by the app to detect a resumed app
        window.addEventListener("appResumed", async () => {
            this.worker.terminate();
            this.worker = null;
            await refresh_access_token();
            this.startWorker();
            this.show_spinner.value = true;

            const t = i18n.t;
            Median.sidebar.setItems({
                enabled: true,
                persist: true,
                items: [
                    {
                        label: t("app.close_remote_access"),
                        url: "javascript:window.close()"
                    }
                ]
            })
        }, {signal: this.abort.signal});

        // this is used by the app to close the remote connection via the native app menu.
        (window as any).close = () => {
            this.props.setParentState({
                connected: false,
                connectedId: "",
                connectedName: "",
                connectedPort: 0,
            });
            setAppNavigation();
        }

        // this is used by the app to change location via the native app menu.
        (window as any).switchTo = (hash: string) => {
            const frame = document.getElementById("interface") as HTMLIFrameElement;
            const frame_window = frame.contentWindow;
            frame_window.location.hash = hash;
        }
    }

    handleErrorMessage(msg: Message) {
        this.props.setParentState({
            connected: false,
            connectedId: "",
            connectedName: "",
            connectedPort: 0,
        });
    }

    startWorker() {
        this.worker = new Worker();
        const message_event = (e: MessageEvent) => {
            if (typeof e.data === "string") {
                switch (e.data) {
                    case "ready":
                        const iframe = document.getElementById("interface") as HTMLIFrameElement;
                        iframe.src = `/wg-${this.id}/`;
                        break;
                    case "closed":
                        this.worker.terminate();
                        break;
                }
            } else if (e.data.unresolved) {
                const msg = JSON.stringify(e.data.msg);
                const blob = new Blob([msg]);
                const url = URL.createObjectURL(blob);
                const filename = `warp_charger_error_${Date.now()}.json`;
                if (Median.isNativeApp()) {
                    Median.share.downloadFile({url: url, filename: filename, open: true});
                }
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

                    case MessageType.Error:
                        this.handleErrorMessage(msg);
                        break;
                }
            }
        };

        this.worker.onmessage = (e: MessageEvent) => {
            if (e.data === "started") {
                this.worker.onmessage = message_event;
                const message_data: SetupMessage = {
                    chargerID: this.props.parentState.connectedId,
                    port: this.props.parentState.connectedPort,
                    secret: secret
                };
                const message: Message = {
                    type: MessageType.Setup,
                    data: message_data
                };

                this.worker.postMessage(message);

                if (enableLogging) {
                    this.worker.postMessage("enableLogging");
                }
            } else if (e.data.type) {
                const msg = e.data as Message;
                switch (msg.type) {
                    case MessageType.Error:
                        this.handleErrorMessage(msg);
                        break;

                    default:
                        break;
                }
            }
        }

        window.addEventListener("message", (e: MessageEvent) => {
            const iframe = document.getElementById("interface") as HTMLIFrameElement;
            switch (e.data) {
                case "initIFrame":
                    this.worker.postMessage("connect");
                    return;

                case "webinterface_loaded":
                    this.show_spinner.value = false;
                    iframe.contentWindow.postMessage({
                        connection_id: this.id,
                    });
                    return;

                case "pauseWS":
                    this.worker.postMessage("pauseWS");
                    return;

                case "close":
                    this.props.setParentState({
                        connected: false,
                        connectedId: "",
                        connectedName: "",
                        connectedPort: 0,
                    });
                    return;
            }
        });

        // this is used by the app to close the remote connection via the native app menu.
        (window as any).close = () => {
            this.props.setParentState({
                connected: false,
                connectedId: "",
                connectedName: "",
                connectedPort: 0,
            });
            setAppNavigation();
        }

        // this is used by the app to change location via the native app menu.
        (window as any).switchTo = (hash: string) => {
            const frame = document.getElementById("interface") as HTMLIFrameElement;
            const frame_window = frame.contentWindow;
            frame_window.location.hash = hash;
        }

        window.addEventListener("keydown", (e: KeyboardEvent) => {
            if (e.ctrlKey && e.altKey && e.code === "KeyP") {
                this.worker.postMessage("download");
            } else if(e.ctrlKey && e.altKey && e.shiftKey && e.code === "KeyR") {
                const iframe = document.getElementById("interface") as HTMLIFrameElement;
                iframe.src = `/wg-${this.id}/recovery`;
            }
        })
    }

    componentWillUnmount() {
        this.worker.postMessage("close");
        document.title = "Remote Access";
        this.abort.abort();
    }

    render() {
        return (
            <>
                <Row hidden={!this.show_spinner.value} className="align-content-center justify-content-center m-0 h-100">
                    <Spinner className="p-3"animation='border' variant='primary'/>
                </Row>
                <iframe hidden={this.show_spinner.value} width="100%" height="100%" id="interface"></iframe>
                {/* <button onClick={() => {
                    this.worker.postMessage("download");
                }}>Download Pcap log</button> */}
            </>
        )
    }
}
