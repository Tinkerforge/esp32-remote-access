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
import { LocationProvider, Router, Route } from 'preact-iso';

import { CustomNavbar, logout } from './components/Navbar.js';
import { NotFound } from './pages/_404.jsx';
import { Login } from './components/login.js';
import { Register } from './components/register.js';
import { User } from './pages/user.js';
import Row from "react-bootstrap/Row";
import Card from "react-bootstrap/Card";
import Tabs from "react-bootstrap/Tabs";
import Tab from "react-bootstrap/Tab";
import { ChargerList } from './pages/chargers.js';
import { ErrorAlert } from './components/Alert.js';
import { BACKEND } from './utils';
import { AppState, loggedIn } from './utils.js';
import { Col, Spinner } from 'react-bootstrap';
import { Recovery } from './pages/recovery.js';
import { Trans, useTranslation } from "react-i18next";
import Median from "median-js-bridge";
import { Footer } from "./components/Footer";

import "./main.scss";

async function refresh_access_token() {
    if (window.location.pathname == "/recovery") {
        loggedIn.value = AppState.Recovery;
        return;
    }

    const resp = await fetch(BACKEND + "/auth/jwt_refresh", {
        method: "GET",
        credentials: "include"
    });

    if (resp.status == 200) {
        if (!localStorage.getItem("loginKey") || !localStorage.getItem("secretKey")) {
            logout(false);
        }
        loggedIn.value = AppState.LoggedIn;
    } else {
        localStorage.removeItem("loginKey");
        localStorage.removeItem("secretKey");
        loggedIn.value = AppState.LoggedOut;
    }
}

refresh_access_token();
setInterval(async () => {
    await refresh_access_token();
}, 1000 * 60 * 5);

localStorage.removeItem("secret_key");

export function App() {
    const {t} = useTranslation("", {useSuspense: false});

    switch (loggedIn.value) {
        case AppState.Loading:
            return <>
                <Row fluid className="align-content-center justify-content-center vh-100">
                    <Spinner animation='border' variant='primary'/>
                </Row>
            </>

        case AppState.LoggedOut:
            if (Median.isNativeApp()) {
                Median.sidebar.setItems({items: [], enabled: false, persist: true});
            }
            return <>
                <nav id="logo-nav" class="navbar navbar-expand-md navbar-dark sticky-top flex-md-nowrap p-0 pb-2 pt-2 ps-2">
                    <a href="/"><img class="pt-2 pb-2 pl-3" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAQ0AAAAjAQMAAAC0Dc25AAAABlBMVEXwH1b///+8ndbpAAAAAXRSTlMAQObYZgAAAE1JREFUOMtjeMDAwM7AwFDA/h8CDshDGQz1EPoHA3WVMEAA4wEogx1KM1iMKhlCSuiWXthhbpGBOQLmvOGihB1FCXKADwMl0Hj9QGslAM2l6A72PC0DAAAAAElFTkSuQmCC" style="max-width: calc(100vw - 72px);" alt="logo"/></a>
                </nav>
                <ErrorAlert/>
                <Row className="align-items-center justify-content-center flex-grow-1 gap-3 m-0 my-3">
                    <Card className="p-3 col-10 col-lg-5 col-xl-3">
                        <Trans i18nKey="description"><a target="__blank" href="https://docs.warp-charger.com/docs/remote_access" >link</a></Trans>
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
                    <ErrorAlert/>
                    <CustomNavbar />
                    <Col>
                        <LocationProvider>
                            <Router>
                                <Route path="/user" component={User} />
                                <Route default path="/chargers" component={ChargerList} />
                            </Router>
                        </LocationProvider>
                    </Col>
                    { Median.isNativeApp() ? <></> : <Footer /> }
                </>
            );
        // we need an extra recovery state, otherwise we would show the login/register page.
        case AppState.Recovery:
            return (<>
                <ErrorAlert/>
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

render(<App />, document.getElementById('app'));
