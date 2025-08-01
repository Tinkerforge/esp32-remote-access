import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import { AppState, bc, fetchClient, FRONTEND_URL, loggedIn, pub_key, resetSecret, secret, clearSecretKeyFromServiceWorker } from "../utils";
import { useTranslation } from "react-i18next";
import { Navbar } from "react-bootstrap";
import Median from "median-js-bridge";
import i18n from "../i18n";
import { useState } from "preact/hooks";
import { Key, LogOut, Server, User } from "react-feather";
import { signal } from "@preact/signals";
import logo from "logo";
import { showAlert } from "./Alert";

export const connected = signal(false);

export async function logout(logout_all: boolean) {
        const { error } = await fetchClient.GET("/user/logout", {params:{query:{logout_all: logout_all}}, credentials: "same-origin"});

        if (logout_all && error) {
            showAlert(error, "danger");
            return;
        }

        resetSecret();
        localStorage.removeItem("loginSalt");
        await clearSecretKeyFromServiceWorker();

        loggedIn.value = AppState.LoggedOut;
        bc.postMessage("logout");
    }

export function setAppNavigation() {
    const items = [
        {
            label: i18n.t("navbar.chargers"),
            url: `${FRONTEND_URL}/devices`,
            icon: "fas fa-server"
        },
        {
            label: i18n.t("navbar.user"),
            url: `${FRONTEND_URL}/user`,
            icon: "fas fa-user"
        },
        {
            label: i18n.t("navbar.token"),
            url: `${FRONTEND_URL}/tokens`,
            icon: "fas fa-key"
        }
    ];
    Median.sidebar.setItems({ items: items, enabled: true, persist: true });
    return <></>;
}

export function CustomNavbar() {
    const { url } = useLocation();
    const [expanded, setExpanded] = useState(false);
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "navbar"});

    if (Median.isNativeApp()) {
        return setAppNavigation()
    }

    return (
        <Navbar id="remote_access_navbar" expanded={expanded} expand="md" hidden={connected.value} className="navbar-dark sticky-top flex-row flex-md-nowrap p-2 mb-2">
                <a href="/"><img class="pt-2 pb-2 ps-2" src={logo} style="max-width: calc(100vw - 80px); image-rendering: crisp-edges" alt="logo"/></a>
                <Navbar.Toggle onClick={() => setExpanded(!expanded)} id="navbar-toggler" aria-controls="basic-navbar-nav"/>
                <Navbar.Collapse id="navbar-nav" className="sidebar px-2 py-1">
                    <Nav className="me-auto ps-2">
                        <Nav.Link className="d-flex align-items-center" onClick={() => setExpanded(false)} href="/devices" active={url === "/chargers"}>
                            <Server/>
                            <p class="ms-1 mb-0">
                                {t("chargers")}
                            </p>
                        </Nav.Link>
                        <Nav.Link className="d-flex align-items-center" onClick={() => setExpanded(false)} href="/tokens" active={url === "/tokens"}>
                            <Key/>
                            <p class="ms-1 mb-0">
                                {t("token")}
                            </p>
                        </Nav.Link>
                        <Nav.Link className="d-flex align-items-center" onClick={() => setExpanded(false)} href="/user" active={url === "/user"}>
                            <User/>
                            <p class="ms-1 mb-0">
                                {t("user")}
                            </p>
                        </Nav.Link>
                    </Nav>
                    <hr class="d-block d-md-none my-1" style={{color: "#5a6268"}}/>
                    <Nav>
                        <Nav.Link className="d-flex align-items-center" onClick={(e) => {
                            e.preventDefault();
                            logout(false);
                        }}>
                            <LogOut/>
                            <p class="ms-1 mb-0">
                                {t("logout")}
                            </p>
                        </Nav.Link>
                    </Nav>
                </Navbar.Collapse>
        </Navbar>
    )
}
