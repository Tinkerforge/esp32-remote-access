import { Frame } from '../../components/Frame';
import * as cookie from 'cookie';
import { useState } from 'preact/hooks';
import { Card, Tab, Tabs } from 'react-bootstrap';
import { Register } from '../../components/register';
import { Login } from '../../components/login';

export function Home() {
    const [loggedIn, setLoggedIn] = useState(false);
    const cookies = cookie.parse(document.cookie);
    if ("access_token" in cookies) {
        console.log("token found");
        setLoggedIn(true);
    }
    console.log(document.cookie);

    let data = <>
        Logged in
    </>;

    if (!loggedIn) {
        data = <div class="col d-flex justify-content-center">
            <Card className="pt-3 ps-3 pe-3 pb-3">
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
    }

    return (
        data
    );
}
