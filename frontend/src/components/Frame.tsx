import { Component } from 'preact';
import { Message, MessageType, SetupMessage } from '../types';
import Worker from '../worker?worker'
import { Row, Spinner } from 'react-bootstrap';
import { setAppNavigation } from './Navbar';
import {isDebugMode, secret } from '../utils';
import Median from "median-js-bridge";
import i18n from '../i18n';
import { ChargersState } from '../pages/chargers';
import { Dispatch, StateUpdater } from 'preact/hooks';

interface VirtualNetworkInterfaceSetParentState {
    chargersState: Dispatch<StateUpdater<ChargersState>>,
    parentState: Dispatch<StateUpdater<FrameState>>,
}

class VirtualNetworkInterface {
    worker: Worker;
    abort: AbortController;
    id: string;
    setParentState: VirtualNetworkInterfaceSetParentState;
    connectionInfo: ChargersState;
    debugMode: boolean;

    constructor(setParentState: VirtualNetworkInterfaceSetParentState, connectionInfo: ChargersState, debugMode: boolean) {
        this.setParentState = setParentState;
        this.connectionInfo = connectionInfo;
        this.debugMode = debugMode;
        this.abort = new AbortController();

        this.worker = new Worker();
        this.id = crypto.randomUUID();
        navigator.serviceWorker.addEventListener("message", (e: MessageEvent) => {
            const msg = e.data as Message;
            if (msg.receiver_id === this.id) {
                this.worker.postMessage(msg);
            }
        }, {signal: this.abort.signal});

        this.worker.onmessage = (e) => this.setupHandler(e);

        window.addEventListener("message", (e) => this.iframeMessageHandler(e), {signal: this.abort.signal});
        window.addEventListener("keydown", (e) => this.keyDownHandler(e), {signal: this.abort.signal});
    }

    cancel() {
        this.worker.postMessage("close");
        this.worker.terminate();
        this.abort.abort();
    }

    handleErrorMessage(msg: Message) {
        this.setParentState.chargersState({
            connected: false,
            connectedId: "",
            connectedName: "",
            connectedPort: 0,
        });
    }

    // This handles Messages from the iframe/ Device-Webinterface
    iframeMessageHandler(e: MessageEvent) {
        const iframe = document.getElementById("interface") as HTMLIFrameElement;
        switch (e.data) {
            case "initIFrame":
                this.worker.postMessage("connect");
                return;

            case "webinterface_loaded":
                this.setParentState.parentState({show_spinner: false});
                iframe.contentWindow.postMessage({
                    connection_id: this.id,
                });
                return;

            case "pauseWS":
                this.worker.postMessage("pauseWS");
                return;

            case "close":
                this.setParentState.chargersState({
                    connected: false,
                    connectedId: "",
                    connectedName: "",
                    connectedPort: 0,
                });
                return;
        }
    }

    // This waits for the Worker to be done with the setup
    setupHandler(e: MessageEvent) {
        if (e.data === "started") {
            this.worker.onmessage = (e) => this.handleWorkerMessage(e);
            const message_data: SetupMessage = {
                chargerID: this.connectionInfo.connectedId,
                port: this.connectionInfo.connectedPort,
                secret: secret,
                debugMode: this.debugMode,
            };
            const message: Message = {
                type: MessageType.Setup,
                data: message_data
            };

            this.worker.postMessage(message);
        } else if (e.data.type) {
            const msg = e.data as Message;
            switch (msg.type) {
                case MessageType.Error:
                    // this.handleErrorMessage(msg);
                    break;

                default:
                    break;
            }
        }
    }

    // This handles the Message coming from the Charger once the setup is done
    handleWorkerMessage(e: MessageEvent) {
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
    }

    keyDownHandler(e: KeyboardEvent) {
        if (e.ctrlKey && e.altKey && e.code === "KeyP") {
            this.worker.postMessage("download");
        } else if(e.ctrlKey && e.altKey && e.shiftKey && e.code === "KeyR") {
            const iframe = document.getElementById("interface") as HTMLIFrameElement;
            iframe.src = `/wg-${this.id}/recovery`;
        }
    }
}

interface FrameProps {
    parentState: ChargersState,
    setParentState: Dispatch<StateUpdater<ChargersState>>,
}

interface FrameState {
    show_spinner: boolean,
}

export class Frame extends Component<FrameProps, FrameState> {

    interface: VirtualNetworkInterface;
    id: string;
    constructor() {
        super();

        this.state = {
            show_spinner: true,
        };

        // this.props is not initialized in the constructor. So we set it afterwards.
        setTimeout(() => {
            this.interface = new VirtualNetworkInterface({
                    parentState: (s) => this.setState(s),
                    chargersState: (s) => this.props.setParentState(s),
                },
                this.props.parentState,
                isDebugMode.value);
        });

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

            setTimeout(() => sessionStorage.setItem("currentConnection", JSON.stringify(this.props.parentState)))
        }

        // this is used by the app to close the remote connection via the native app menu.
        (window as any).close = () => {
            this.props.setParentState({
                connected: false,
                connectedId: "",
                connectedName: "",
                connectedPort: 0,
            });
            setAppNavigation();
            sessionStorage.removeItem("currentConnection");
        }

        // this is used by the app to change location via the native app menu.
        (window as any).switchTo = (hash: string) => {
            const frame = document.getElementById("interface") as HTMLIFrameElement;
            const frame_window = frame.contentWindow;
            frame_window.location.hash = hash;
        }
    }

    componentWillUnmount() {
        if (this.interface && this.interface.cancel) {
            this.interface.cancel();
        }
    }

    render() {
        return (
            <>
                <Row hidden={!this.state.show_spinner} className="align-content-center justify-content-center m-0 h-100">
                    <Spinner className="p-3"animation='border' variant='primary'/>
                </Row>
                <iframe hidden={this.state.show_spinner} width="100%" height="100%" id="interface"></iframe>
                {/* <button onClick={() => {
                    this.worker.postMessage("download");
                }}>Download Pcap log</button> */}
            </>
        )
    }
}
