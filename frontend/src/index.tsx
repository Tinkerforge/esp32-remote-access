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
import Container from "react-bootstrap/Container";
import Row from "react-bootstrap/Row";
import Card from "react-bootstrap/Card";
import Tabs from "react-bootstrap/Tabs";
import Tab from "react-bootstrap/Tab";
import 'bootstrap/dist/css/bootstrap.min.css';
import { ChargerList } from './pages/chargers.js';
import { ErrorAlert } from './components/Alert.js';
import { BACKEND } from './types.js';
import { AppState, loggedIn } from './utils.js';
import { Spinner } from 'react-bootstrap';
import { Recovery } from './pages/recovery.js';
import { useTranslation } from "react-i18next";
import Median from "median-js-bridge";


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
        loggedIn.value = AppState.LoggedIn;
    } else {
        loggedIn.value = AppState.LoggedOut;
    }
}

refresh_access_token();
setInterval(async () => {
    await refresh_access_token();
}, 1000 * 60 * 5);

export function App() {
    const {t} = useTranslation("", {useSuspense: false});

    if (!localStorage.getItem("secret_key") && loggedIn.value == AppState.LoggedIn) {
        logout(false);
    }

    switch (loggedIn.value) {
        case AppState.Loading:
            return <>
                <Row fluid className="align-content-center justify-content-center vh-100">
                    <Spinner animation='border' variant='primary'/>
                </Row>
            </>

        case AppState.LoggedOut:
            Median.sidebar.setItems({items: [], enabled: false, persist: true});
            return <>
                <ErrorAlert/>
                <Row fluid className="align-items-center vh-100">
                    <div class="d-flex justify-content-center">
                        <Card className="p-3 m-3 col-lg-6 col-xl-3">
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
                    </div>
                </Row>
            </>

        case AppState.LoggedIn:
            return (
                <>
                    <ErrorAlert/>
                    <CustomNavbar />
                    <LocationProvider>
                        <Router>
                            <Route path="/user" component={User} />
                            <Route default path="/chargers" component={ChargerList} />
                        </Router>
                    </LocationProvider>
                </>
            );
        // we need an extra recovery state, otherwise we would show the login/register page.
        case AppState.Recovery:
            return (<>
                <ErrorAlert/>
                <LocationProvider>
                    <Container fluid>
                        <Row fluid className="align-items-center vh-100">
                            <div class="d-flex justify-content-center">
                                <Router>
                                    <Route path="/recovery" component={Recovery} />
                                    <Route default component={NotFound} />
                                </Router>
                            </div>
                        </Row>
                    </Container>
                </LocationProvider>
            </>);
    }
}

render(<App />, document.getElementById('app'));
