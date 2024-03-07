import { render } from 'preact';
import { LocationProvider, Router, Route } from 'preact-iso';

import { Sidebar } from './components/sidebar.js';
import { Home } from './pages/Home/index.jsx';
import { NotFound } from './pages/_404.jsx';
import { Login } from './components/login.js';
import { Register } from './components/register.js';
import { FrameFunction } from './components/Frame.js';
import { User } from './pages/user.js';
import Container from "react-bootstrap/Container";
import Row from "react-bootstrap/Row";
import Card from "react-bootstrap/Card";
import Tabs from "react-bootstrap/Tabs";
import Tab from "react-bootstrap/Tab";
import * as cookie from "cookie";
import 'bootstrap/dist/css/bootstrap.min.css';
import { ChargerList } from './pages/chargers.js';


export function App() {

    const cookies = cookie.parse(document.cookie);
    if (!("access_token" in cookies)) {
        return <Row fluid className="align-items-center vh-100">
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
    }

    return (
        <LocationProvider>
            <Container fluid>
                <Row>
                    <Sidebar />
                    <main class="col-lg-10 col-md-9 ml-sm-auto px-md-4" >
                        <Router>
                            <Route path="/" component={Home} />
                            <Route path="/user" component={User} />
                            <Route path="/frame" component={FrameFunction} />
                            <Route path="/chargers" component={ChargerList} />
                            <Route default component={NotFound} />
                        </Router>
                    </main>
                </Row>
            </Container>
        </LocationProvider>
    );
}

render(<App />, document.getElementById('app'));
