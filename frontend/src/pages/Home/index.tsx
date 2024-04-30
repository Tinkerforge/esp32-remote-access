import * as cookie from 'cookie';
import { useState } from 'preact/hooks';

export function Home() {
    const [loggedIn, setLoggedIn] = useState(false);
    const cookies = cookie.parse(document.cookie);
    if ("access_token" in cookies) {
        setLoggedIn(true);
    }

    let data = <>
        Logged in
    </>;

    return (
        data
    );
}
