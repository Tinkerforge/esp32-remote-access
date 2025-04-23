import { Component } from 'preact';
import { Message, MessageType, SetupMessage } from '../types';
import Worker from '../worker?worker'
import { Button, Container, Row, Spinner } from 'react-bootstrap';
import { setAppNavigation } from './Navbar';
import {isDebugMode, secret } from '../utils';
import Median from "median-js-bridge";
import i18n from '../i18n';
import { ChargersState } from '../pages/chargers';
import { Dispatch, StateUpdater, useEffect } from 'preact/hooks';
import { showAlert } from './Alert';
import { useLocation } from 'preact-iso';
import { useTranslation } from 'react-i18next';

interface VirtualNetworkInterfaceSetParentState {
    chargersState: Dispatch<StateUpdater<ChargersState>>,
    parentState: Dispatch<StateUpdater<Partial<FrameState>>>,
}

class VirtualNetworkInterface {
    worker: Worker;
    abort: AbortController;
    id: string;
    setParentState: VirtualNetworkInterfaceSetParentState;
    connectionInfo: ChargersState;
    debugMode: boolean;
    timeout: any;
    route: (path: string, replace?: boolean) => void;

    constructor(
        setParentState: VirtualNetworkInterfaceSetParentState,
        connectionInfo: ChargersState,
        debugMode: boolean,
        route: (path: string, replace?: boolean) => void)
    {
        this.route = route;
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
        this.timeout = setTimeout(() => {
            setParentState.chargersState({
                connected: false,
                connectedId: "",
                connectedName: "",
                connectedPort: 0,
            });
            this.route("/chargers");
            showAlert(i18n.t("chargers.connection_timeout_text"), "danger", "network", i18n.t("chargers.connection_timeout"));
        }, 30_000)

        window.addEventListener("message", (e) => this.iframeMessageHandler(e), {signal: this.abort.signal});
        window.addEventListener("keydown", (e) => this.keyDownHandler(e), {signal: this.abort.signal});
    }

    cancel() {
        this.worker.postMessage("close");
        this.worker.terminate();
        this.abort.abort();
    }

    handleErrorMessage(msg: Message) {
        showAlert(i18n.t(msg.data.translation, msg.data.format) as string, "danger");
        this.setParentState.chargersState({
            connected: false,
            connectedId: "",
            connectedName: "",
            connectedPort: 0,
        });
        this.route("/chargers");
    }

    // This handles Messages from the iframe/ Device-Webinterface
    iframeMessageHandler(e: MessageEvent) {
        const iframe = document.getElementById("interface") as HTMLIFrameElement;
        switch (e.data) {
            case "initIFrame":
                this.worker.postMessage("connect");
                return;

            case "webinterface_loaded":
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
                location.pathname = "/chargers";
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
                    this.handleErrorMessage(msg);
                    break;

                default:
                    break;
            }
        }
    }

    downloadPcapLog() {
        this.worker.postMessage("download");
    }

    // This handles the Message coming from the Charger once the setup is done
    handleWorkerMessage(e: MessageEvent) {
        if (typeof e.data === "string") {
            switch (e.data) {
                case "ready":
                    this.setParentState.parentState({connection_state: ConnectionState.LoadingWebinterface});
                    const iframe = document.getElementById("interface") as HTMLIFrameElement;
                    const path = window.location.pathname;
                    const split = path.split("/");
                    const newPath = split.slice(3).join("/");
                    iframe.src = `/wg-${this.id}/${newPath}`;
                    iframe.addEventListener("load", () => {
                        clearTimeout(this.timeout);
                        this.setParentState.parentState({show_spinner: false});
                    });
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
                    const blob = new Blob([msg.data as Uint8Array]);
                    const url = URL.createObjectURL(blob)
                    if (Median.isNativeApp()) {
                        Median.share.downloadFile({url: url, filename: "out.pcap"});
                    } else {
                        const a = document.createElement("a");
                        a.href = url;
                        a.download = "out.pcap";
                        a.target = "_blank";
                        a.click();
                    }
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

enum ConnectionState {
    Connecting,
    LoadingWebinterface,
}

interface FrameState {
    show_spinner: boolean,
    connection_state: ConnectionState,
}

export class Frame extends Component<FrameProps, FrameState> {

    interface: VirtualNetworkInterface;
    id: string;
    route: (path: string, replace?: boolean) => void;

    constructor() {
        super();

        this.state = {
            show_spinner: true,
            connection_state: ConnectionState.Connecting,
        };

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

        const that = this;
        // this is used by the app to close the remote connection via the native app menu.
        (window as any).close = () => {
            this.props.setParentState({
                connected: false,
                connectedId: "",
                connectedName: "",
                connectedPort: 0,
            });
            clearTimeout(that.interface.timeout);
            this.route("/chargers");
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
        const { route } = useLocation();
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});

        useEffect(() => {
            this.route = route;
            this.interface = new VirtualNetworkInterface({
                    parentState: (s) => this.setState(s),
                    chargersState: (s) => this.props.setParentState(s),
                },
                this.props.parentState,
                isDebugMode.value,
                this.route,
            );
        }, []);

        const downLoadButton = isDebugMode.value ? <Row className="d-flex m-0">
                <Button variant='secondary' style={{borderRadius: 0}} class="m-0" onClick={() => {
                    this.interface.downloadPcapLog();
                }}>Save Pcap log</Button>
            </Row> : null;
        return (
            <Container fluid className="d-flex flex-column h-100 p-0">
                <Row hidden={!this.state.show_spinner} className="align-content-center justify-content-center m-0 h-100">
                    <Spinner className="p-3" animation='border' variant='primary'/>
                    <div className="text-center mt-2">
                      {this.state.connection_state === ConnectionState.Connecting ?
                        i18n.t("chargers.connecting") :
                        i18n.t("chargers.loading_webinterface")}
                    </div>
                    <Button className="col-lg-1 col-md-2 col-sm-3 col-6 mt-3"
                        variant="warning"
                        type="button"
                        onClick={(e) => {
                            e.stopPropagation();
                            e.preventDefault();
                            (window as any).close();
                        }}>{t("abort")}</Button>
                </Row>
                <Row className="flex-grow-1 m-0">
                    <iframe class="p-0" hidden={this.state.show_spinner} width="100%" height="100%" id="interface"></iframe>
                </Row>
                {downLoadButton}
            </Container>
        )
    }
}
