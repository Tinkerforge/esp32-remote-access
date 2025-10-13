/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

import "./i18n";
import { render } from 'preact';
import { LocationProvider, Router, Route, lazy } from 'preact-iso';

import { connected, CustomNavbar } from './components/Navbar.js';
import { NotFound } from './pages/_404.jsx';
import { Login } from './components/Login.js';
import { Register } from './components/Register.js';
import Row from "react-bootstrap/Row";
import Card from "react-bootstrap/Card";
import Tabs from "react-bootstrap/Tabs";
import Tab from "react-bootstrap/Tab";
import { ErrorAlert } from './components/Alert.js';
import { isDebugMode, refresh_access_token } from './utils';
import { AppState, loggedIn } from './utils.js';
import { Col, Form, Spinner } from 'react-bootstrap';
import Modal from "react-bootstrap/Modal";
import Button from "react-bootstrap/Button";
import { Recovery } from './pages/Recovery.js';
import { Trans, useTranslation } from "react-i18next";
import Median from "median-js-bridge";
import { Footer } from "./components/Footer";
import favicon from "favicon";
import logo from "logo";
import { Message, MessageType } from './types';
import { startVersionChecking } from './versionChecker';

import "./styles/main.scss";
import { docs } from "links";
import { useEffect, useState } from "preact/hooks";

if (isDebugMode.value) {
    addEventListener("unhandledrejection", (event) => {
        const stack = event.reason.stack.split("\n");

        const evt = {
            message: event.reason.message,
            stack
        }
        const msg = JSON.stringify(evt);
        const blob = new Blob([msg]);
        const url = URL.createObjectURL(blob);
        const filename = `warp_charger_error_${Date.now()}.json`
        if (Median.isNativeApp()) {
            Median.share.downloadFile({url, filename, open: true});
        }
    });
}

const icon: HTMLLinkElement | null = document.querySelector('link[rel="icon"]');
if (icon) {
    icon.href = favicon;
}
let refreshInterval: NodeJS.Timeout | undefined;
const refreshMinutes = (Math.random() * (5 -3) + 3);

const Tokens = lazy(() => import('./pages/Tokens.js').then(m => m.Tokens));
const User = lazy(() => import('./pages/User.js').then(m => m.User));
const DeviceList = lazy(() => import('./pages/Devices.js').then(m => m.DeviceList));
const Frame = lazy(() => import('./pages/Frame.js').then(m => m.Frame));

export function App() {
    const {t} = useTranslation("", {useSuspense: false});

    const [modalConfig, setModalConfig] = useState({ show: false, rememberChoice: false });

    const requestPersistence = async () => {
        try {
            const storageManager: StorageManager | undefined = (navigator as unknown as { storage?: StorageManager }).storage;
            const granted = storageManager && typeof storageManager.persist === "function"
                ? await storageManager.persist()
                : false;
            if (!granted) {
                console.warn("Storage persistence not granted. You may be logged out from time to time.");
            }
        } catch (e) {
            console.warn("Failed to request storage persistence", e);
        } finally {
            setModalConfig({...modalConfig, show: false });
        }
    };

    if (window.navigator.userAgent.indexOf("Firefox") !== -1 && localStorage.getItem("persistedChoice") !== "true") {
        useEffect(() => {
            (async () => {
                try {
                    const storageManager: StorageManager | undefined = (navigator as unknown as { storage?: StorageManager }).storage;
                    const persisted = storageManager && typeof storageManager.persisted === "function"
                        ? await storageManager.persisted()
                        : false;
                    if (!persisted) {
                        setModalConfig({...modalConfig, show: true });
                    }
                } catch (e) {
                    console.warn("Failed to check storage persistence", e);
                }
            })();
        }, []);
    } else {
        useEffect(() => {
            (async () => {
                const storageManager: StorageManager | undefined = (navigator as unknown as { storage?: StorageManager }).storage;
                const persisted = storageManager && typeof storageManager.persisted === "function"
                    ? await storageManager.persisted()
                    : false;
                if (!persisted) {
                    await requestPersistence();
                }
            })();
        }, []);
    }

    useEffect(() => {
        if (!connected.value) {
            document.title = t("app_name");
        }
    }, [connected.value, t]);

    useEffect(() => {
        if (window.parent !== window) {
            console.warn("Warp Charger is running in an iframe, closing window");
            const errorMessage = t("iframe.error_message");
            window.parent.postMessage({ type: "error", message: errorMessage }, "*");
            return;
        }
        refresh_access_token();
        startVersionChecking(10);
    }, []);
    useEffect(() => {
        if (loggedIn.value === AppState.LoggedIn) {
            refreshInterval = setInterval(async () => {
                await refresh_access_token();
            }, 1000 * 60 * refreshMinutes);

        } else {
            clearInterval(refreshInterval);
        }
    }, [loggedIn.value])

    // Migrate secret from localStorage to service worker
    useEffect(() => {
        const secretFromLocalStorage = localStorage.getItem("secretKey");
        if (secretFromLocalStorage) {
            localStorage.removeItem("secretKey");
            const msg: Message = {
                type: MessageType.StoreSecret,
                data: secretFromLocalStorage
            };
            navigator.serviceWorker.controller?.postMessage(msg);
        }
    }, []);

    // Prepare persistence explanation modal to render alongside any app state
    const persistModal = (
        <Modal show={modalConfig.show} onHide={() => setModalConfig({...modalConfig, show: false})} backdrop="static" keyboard={false} centered>
            <Modal.Header closeButton>
                <Modal.Title>{t("storage_persistence.title", { defaultValue: "Keep you signed in" })}</Modal.Title>
            </Modal.Header>
            <Modal.Body>
                {t("storage_persistence.body")}
                <div className="mt-3"
                     onClick={() => setModalConfig({...modalConfig, rememberChoice: !modalConfig.rememberChoice})}>
                    <Form.Check
                        type="checkbox"
                        label={t("storage_persistence.remember_choice")}
                        checked={modalConfig.rememberChoice}
                    />
                </div>
            </Modal.Body>
            <Modal.Footer>
                <Button variant="secondary" onClick={() => {
                    setModalConfig({...modalConfig, show: false});
                    if (modalConfig.rememberChoice) {
                        localStorage.setItem("persistedChoice", "true");
                    }
                }}>
                    {t("not_now")}
                </Button>
                <Button variant="primary" onClick={requestPersistence}>
                    {t("allow")}
                </Button>
            </Modal.Footer>
        </Modal>
    );

    if (!window.ServiceWorker) {
        return <Row fluid className="align-content-center justify-content-center vh-100">
            {t("no_service_worker")}
        </Row>
    }

    switch (loggedIn.value) {
        case AppState.Loading:
            return <>
                <Row fluid className="align-content-center justify-content-center vh-100">
                    <Spinner animation='border' variant='primary' />
                </Row>
            </>

        case AppState.LoggedOut:
            if (Median.isNativeApp()) {
                Median.sidebar.setItems({items: [], enabled: false, persist: false});
            }
            return <>
                <nav hidden={Median.isNativeApp()} id="logo-nav" class="navbar navbar-expand-md navbar-dark sticky-top flex-md-nowrap p-0 pb-2 pt-2 ps-2">
                    <a href="/"><img class="pt-2 pb-2 ps-2" src={logo} style="max-width: calc(100vw - 72px);" alt="logo" /></a>
                </nav>
                <ErrorAlert />
                <Row className="align-items-center justify-content-center flex-grow-1 gap-3 m-0 my-3">
                    <Card className="p-3 col-10 col-lg-5 col-xl-3">
                        <Trans i18nKey="description"><a target="__blank" href={docs} >link</a></Trans>
                    </Card>
                    <Card className="p-3 col-10 col-lg-5 col-xl-3">
                        <Tabs
                            defaultActiveKey="login"
                            id="login-register-tab"
                            className="mb-3"
                        >
                            <Tab eventKey="login" title={t("login.login")}>
                                <Login />
                            </Tab>
                            <Tab eventKey="register" title={t("register.register")}>
                                <Register />
                            </Tab>
                        </Tabs>
                    </Card>
                </Row>
                { Median.isNativeApp() ? <></> : <Footer /> }
            </>

        case AppState.LoggedIn:
            return (
                <>
                    <ErrorAlert />
                    <Col>
                        <LocationProvider>
                            <CustomNavbar />
                            <Router onRouteChange={() => {
                                connected.value = false;
                                // Ensure title is reset to translated app name when navigating away from a device connection
                                document.title = t("app_name");
                            }}>
                                <Route path="/tokens" component={Tokens} />
                                <Route path="/user" component={User} />
                                {/* Dont break any bookmarks that users could have created */}
                                <Route path="/chargers" component={DeviceList} />
                                <Route path="/chargers/:device/:path*" component={Frame} />
                                <Route default component={DeviceList} />
                                <Route path="/devices/:device/:path*" component={Frame} />
                            </Router>
                        </LocationProvider>
                    </Col>
                    { Median.isNativeApp() ? <></> : <Footer /> }
                    { import.meta.env?.MODE === 'test' ? null : persistModal }
                </>
            );
        // we need an extra recovery state, otherwise we would show the login/register page.
        case AppState.Recovery:
            return (<>
                <ErrorAlert />
                <LocationProvider>
                    <Row className="align-items-center justify-content-center flex-grow-1 gap-3 m-0 my-3">
                        <Router>
                            <Route path="/recovery" component={Recovery} />
                            <Route default component={NotFound} />
                        </Router>
                    </Row>
                </LocationProvider>
                { Median.isNativeApp() ? <></> : <Footer /> }
            </>);
    }
}

// The app div will alway be present
render(<App />, document.getElementById("app") as HTMLElement);
