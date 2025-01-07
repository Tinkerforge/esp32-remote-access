import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import { fetchClient, FRONTEND_URL } from "../utils";
import { useTranslation } from "react-i18next";
import { Navbar } from "react-bootstrap";
import Median from "median-js-bridge";
import i18n from "../i18n";
import { useState } from "preact/hooks";
import { LogOut, Server, User } from "react-feather";
import { signal } from "@preact/signals";

export const connected = signal(false);

export async function logout(logout_all: boolean) {
        await fetchClient.GET("/user/logout", {params:{query:{logout_all: logout_all}}, credentials: "same-origin"});

        localStorage.removeItem("loginSalt");
        localStorage.removeItem("secretKey");

        window.location.reload();
    }

export function setAppNavigation() {
    const items = [{
        label: i18n.t("navbar.chargers"),
        url: `${FRONTEND_URL}/chargers`,
        icon: "fas fa-server"
    },
    {
        label: i18n.t("navbar.user"),
        url: `${FRONTEND_URL}/user`,
        icon: "fas fa-user"
    }]
    Median.sidebar.setItems({items: items, enabled: true, persist: true});
    return <></>
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
                <a href="/"><img class="pt-2 pb-2 pl-3" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAQ0AAAAjAQMAAAC0Dc25AAAABlBMVEXwH1b///+8ndbpAAAAAXRSTlMAQObYZgAAAE1JREFUOMtjeMDAwM7AwFDA/h8CDshDGQz1EPoHA3WVMEAA4wEogx1KM1iMKhlCSuiWXthhbpGBOQLmvOGihB1FCXKADwMl0Hj9QGslAM2l6A72PC0DAAAAAElFTkSuQmCC" style="max-width: calc(100vw - 80px); image-rendering: crisp-edges" alt="logo"/></a>
                <Navbar.Toggle onClick={() => setExpanded(!expanded)} id="navbar-toggler" aria-controls="basic-navbar-nav"/>
                <Navbar.Collapse id="navbar-nav" className="sidebar px-2 py-1">
                    <Nav className="me-auto">
                        <Nav.Link className="d-flex align-items-center" onClick={() => setExpanded(false)} href="/user" active={url === "/user"}>
                            <User/>
                            <p class="ms-1 mb-0">
                                {t("user")}
                            </p>
                        </Nav.Link>
                        <Nav.Link className="d-flex align-items-center" onClick={() => setExpanded(false)} href="/chargers" active={url === "/chargers"}>
                            <Server/>
                            <p class="ms-1 mb-0">
                                {t("chargers")}
                            </p>
                        </Nav.Link>
                    </Nav>
                    <hr class="d-block d-md-none my-1" />
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
