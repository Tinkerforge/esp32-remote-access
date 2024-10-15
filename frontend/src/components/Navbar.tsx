import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import { BACKEND, FRONTEN_URL } from "../utils";
import { useTranslation } from "react-i18next";
import { Navbar } from "react-bootstrap";
import { connected } from "./charger_list";
import Median from "median-js-bridge";
import i18n from "../i18n";

export async function logout(logout_all: boolean) {
        await fetch(`${BACKEND}/user/logout?logout_all=${logout_all ? "true" : "false"}`, {
            credentials: "include",
        });

        localStorage.removeItem("loginKey");
        localStorage.removeItem("secret_key");

        window.location.reload();
    }

export function setAppNavigation() {
    const items = [{
        label: i18n.t("navbar.chargers"),
        url: `${FRONTEN_URL}/chargers`,
        icon: "fas fa-server"
    },
    {
        label: i18n.t("navbar.user"),
        url: `${FRONTEN_URL}/user`,
        icon: "fas fa-user"
    }]
    Median.sidebar.setItems({items: items, enabled: true, persist: true});
    return <></>
}

export function CustomNavbar() {
    const { url } = useLocation();
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "navbar"})

    if (Median.isNativeApp()) {
        return setAppNavigation()
    }


    return (
        <Navbar id="remote_access_navbar" expand="md" hidden={connected.value} className="navbar-dark sticky-top flex-row flex-md-nowrap p-2 mb-2">
                <a href="/"><img class="pt-2 pb-2 pl-3" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAQ0AAAAjAQMAAAC0Dc25AAAABlBMVEXwH1b///+8ndbpAAAAAXRSTlMAQObYZgAAAE1JREFUOMtjeMDAwM7AwFDA/h8CDshDGQz1EPoHA3WVMEAA4wEogx1KM1iMKhlCSuiWXthhbpGBOQLmvOGihB1FCXKADwMl0Hj9QGslAM2l6A72PC0DAAAAAElFTkSuQmCC" style="max-width: calc(100vw - 80px); image-rendering: crisp-edges" alt="logo"/></a>
                <Navbar.Toggle className="navbar-toggler" aria-controls="basic-navbar-nav"/>
                <Navbar.Collapse id="navbar-nav" className="navbar-extend p-2">
                    <Nav className="me-auto">
                        <Nav.Link href="/user" active={url === "/user"}>{t("user")}</Nav.Link>
                        <Nav.Link href="/chargers" active={url === "/chargers"}>{t("chargers")}</Nav.Link>
                    </Nav>
                    <Nav>
                        <Nav.Link onClick={(e) => {
                            e.preventDefault();
                            logout(true);
                        }}>{t("logout")}</Nav.Link>
                    </Nav>
                </Navbar.Collapse>
        </Navbar>
    )
}
