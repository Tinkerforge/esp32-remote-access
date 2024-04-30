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

import { render } from 'preact';
import { LocationProvider, Router, Route } from 'preact-iso';

import { Sidebar } from './components/sidebar.js';
import { Home } from './pages/Home/index.jsx';
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


async function refresh_access_token() {
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


export function App() {
    refresh_access_token();
    setTimeout(async () => {
        await refresh_access_token();
    }, 1000 * 60 * 5);

    switch (loggedIn.value) {
        case AppState.Loading:
            return <>
                <Spinner animation='border' variant='primary'/>
            </>

        case AppState.LoggedOut:
            return <>
                <ErrorAlert/>
                <Row fluid className="align-items-center vh-100">
                    <div class="d-flex col justify-content-center">
                        <Card className="p-3">
                            <Tabs
                                defaultActiveKey="login"
                                id="login-register-tab"
                                className="mb-3"
                            >
                                <Tab eventKey="login" title="Login">
                                    <Login />
                                </Tab>
                                <Tab eventKey="register" title="Register">
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
                    <LocationProvider>
                        <Container fluid>
                            <Row>
                                <Sidebar />
                                <main class="col-lg-10 col-md-9 ml-sm-auto px-md-4" >
                                    <Router>
                                        <Route path="/" component={Home} />
                                        <Route path="/user" component={User} />
                                        <Route path="/chargers" component={ChargerList} />
                                        <Route default component={NotFound} />
                                    </Router>
                                </main>
                            </Row>
                        </Container>
                    </LocationProvider>
                </>
            );
    }
}

render(<App />, document.getElementById('app'));
