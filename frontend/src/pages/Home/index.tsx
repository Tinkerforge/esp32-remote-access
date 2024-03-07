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

    return (
        data
    );
}
