import { Component } from 'preact';
import { Message, MessageType, SetupMessage } from '../types';
import Worker from '../worker?worker'
import { Button, Container, Row, Spinner } from 'react-bootstrap';
import { connected, setAppNavigation } from '../components/Navbar';
import {fetchClient, get_decrypted_secret, isDebugMode, pub_key, secret } from '../utils';
import Median from "median-js-bridge";
import i18n from '../i18n';
import { Dispatch, StateUpdater, useEffect } from 'preact/hooks';
import { showAlert } from '../components/Alert';
import { useLocation, useRoute } from 'preact-iso';
import { useTranslation } from 'react-i18next';
import { components } from '../schema';
import { Base64 } from 'js-base64';
import * as sodium from 'libsodium-wrappers';

// Extend the Window interface to include custom properties
declare global {
    interface Window {
        close: () => void;
        switchTo: (hash: string) => void;
    }
}

interface VirtualNetworkInterfaceSetParentState {
    parentState: Dispatch<StateUpdater<Partial<FrameState>>>,
}

class VirtualNetworkInterface {
    worker: Worker;
    abort: AbortController;
    id: string;
    path: string;
    setParentState: VirtualNetworkInterfaceSetParentState;
    chargerInfo: components["schemas"]["ChargerInfo"];
    debugMode: boolean;
    timeout: ReturnType<typeof setTimeout>;
    route: (path: string, replace?: boolean) => void;

    constructor(
        setParentState: VirtualNetworkInterfaceSetParentState,
        chargerInfo: components["schemas"]["ChargerInfo"],
        debugMode: boolean,
        route: (path: string, replace?: boolean) => void,
        path: string
    )
    {
        this.route = route;
        this.setParentState = setParentState;
        this.chargerInfo = chargerInfo;
        this.debugMode = debugMode;
        this.abort = new AbortController();
        this.path = path ? path : "";

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
            this.route("/devices");
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
        const data = msg.data as { translation: string, format?: Record<string, unknown> };
        showAlert(i18n.t(data.translation, data.format) as string, "danger");
        this.route("/devices");
    }

    // This handles Messages from the iframe/ Device-Webinterface
    iframeMessageHandler(e: MessageEvent) {
        const iframe = document.getElementById("interface") as HTMLIFrameElement;
        switch (e.data) {
            case "initIFrame":
                this.worker.postMessage("connect");
                return;

            case "webinterface_loaded":
                if (!iframe.contentWindow) {
                    throw new Error("IFrame contentWindow is null");
                }
                iframe.contentWindow.postMessage({
                    connection_id: this.id,
                });
                return;

            case "pauseWS":
                this.worker.postMessage("pauseWS");
                return;

            case "close":
                this.route("/devices");
                return;
        }
    }

    // This waits for the Worker to be done with the setup
    setupHandler(e: MessageEvent) {
        if (e.data === "started") {
            this.worker.onmessage = (e) => this.handleWorkerMessage(e);
            const message_data: SetupMessage = {
                chargerID: this.chargerInfo.id,
                port: this.chargerInfo.configured_port,
                secret: secret as Uint8Array,
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
                    iframe.src = `/wg-${this.id}/${this.path}`;
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
                Median.share.downloadFile({url, filename, open: true});
            }
        } else {
            const msg = e.data as Message;
            switch (msg.type) {
                case MessageType.Websocket:
                    const iframe = document.getElementById("interface") as HTMLIFrameElement;
                    const window = iframe.contentWindow;
                    if (!window) {
                        throw new Error("IFrame contentWindow is null");
                    }
                    window.postMessage(msg.data);
                    break;

                case MessageType.FileDownload:
                    const blob = new Blob([msg.data as Uint8Array]);
                    const url = URL.createObjectURL(blob)
                    if (Median.isNativeApp()) {
                        Median.share.downloadFile({url, filename: "out.pcap"});
                    } else {
                        const a = document.createElement("a");
                        a.href = url;
                        a.download = "out.pcap";
                        a.target = "_blank";
                        a.click();
                    }
                    break;

                case MessageType.FetchResponse:
                    if (!navigator.serviceWorker.controller) {
                        throw new Error("ServiceWorker controller is not available");
                    }
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

enum ConnectionState {
    Connecting,
    LoadingWebinterface,
}

interface FrameState {
    show_spinner: boolean,
    connection_state: ConnectionState,
}

export class Frame extends Component<{}, FrameState> {

    interface: VirtualNetworkInterface | undefined;
    route: (path: string, replace?: boolean) => void;

    constructor() {
        super();

        // eslint-disable-next-line @typescript-eslint/no-empty-function
        this.route = () => {}; // Placeholder, will be set in render
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
        }

        const that = this;
        // this is used by the app to close the remote connection via the native app menu.
        window.close = () => {
            if (that.interface) {
                clearTimeout(that.interface.timeout);
            }
            that.route("/devices");
            setAppNavigation();
            sessionStorage.removeItem("currentConnection");
        }

        // this is used by the app to change location via the native app menu.
        window.switchTo = (hash: string) => {
            const frame = document.getElementById("interface") as HTMLIFrameElement;

            // the iframe always has a contentWindow
            const frame_window = frame.contentWindow as Window;
            frame_window.location.hash = hash;
        }
    }

    componentWillUnmount() {
        if (this.interface && this.interface.cancel) {
            this.interface.cancel();
        }
        document.title = i18n.t("app_name");
        connected.value = false;
    }

    render() {
        const { route } = useLocation();
        const { params } = useRoute();
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "chargers"});

        useEffect(() => {
            this.route = route;
            setTimeout(async () => {
                await sodium.ready;

                if (!secret) {
                    await get_decrypted_secret();
                }

                const info = await fetchClient.POST("/charger/info", {
                    body: {charger: params.device},
                });

                if (info.error || !info.data) {
                    showAlert(t("not_connected"), "danger");
                    route("/devices", true);
                    return;
                }

                this.interface = new VirtualNetworkInterface({
                    parentState: (s) => this.setState(s),
                }, info.data, isDebugMode.value, this.route, params.path);

                if (Median.isNativeApp()) {
                    sessionStorage.setItem("currentConnection", info.data.id);
                }

                if (info.data.name) {
                    const nameBytes = Base64.toUint8Array(info.data.name);
                    const decryptedName = sodium.crypto_box_seal_open(nameBytes, pub_key as Uint8Array, secret as Uint8Array);
                    const name = sodium.to_string(decryptedName);
                    document.title = name;
                }

                connected.value = true;
            });

            return () => {
                document.title = i18n.t("app_name");
                connected.value = false;
            }
        }, []);

        const downLoadButton = isDebugMode.value ? <Row className="d-flex m-0">
                <Button variant='secondary' style={{borderRadius: 0}} class="m-0" onClick={() => {
                    if (this.interface) {
                        this.interface.downloadPcapLog();
                    }
                }}>Save Pcap log</Button>
            </Row> : null;
        return (
            <Container fluid className="d-flex flex-column h-100 p-0">
                <Row hidden={!this.state.show_spinner} className="align-content-center justify-content-center m-0 h-100">
                    <Spinner className="p-3" animation='border' variant='primary' />
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
                            window .close();
                        }}>{t("abort")}</Button>
                </Row>
                <Row className="flex-grow-1 m-0">
                    <iframe
                        class="p-0"
                        hidden={this.state.show_spinner}
                        width="100%"
                        height="100%"
                        id="interface" />
                </Row>
                {downLoadButton}
            </Container>
        )
    }
}
