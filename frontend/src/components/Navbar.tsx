import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import { BACKEND } from "../types";
import { useTranslation } from "react-i18next";
import { Button, Container, Navbar, Row } from "react-bootstrap";
import { connected, connected_to } from "./charger_list";
import Median from "median-js-bridge";

export async function logout(logout_all: boolean) {
        await fetch(`${BACKEND}/user/logout?logout_all=${logout_all ? "true" : "false"}`, {
            credentials: "include",
        });

        window.location.reload();
    }

export function CustomNavbar() {
    const { url } = useLocation();
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "navbar"})

    if (Median.isNativeApp()) {
        const items = [{
            label: t("chargers"),
            url: "https://mystaging.warp-charger.com/chargers",
            icon: "fas fa-server"
        },
        {
            label: t("user"),
            url: "https://mystaging.warp-charger.com/user",
            icon: "fas fa-user"
        }]
        Median.sidebar.setItems({items: items, enabled: true, persist: true});
        return <></>
    }

    const nav = <>
        <Navbar.Toggle aria-controls="basic-navbar-nav"/>
        <Navbar.Collapse id="navbar-nav">
            <Nav className="me-auto">
                <Nav.Link href="/user" active={url === "/user"}>{t("user")}</Nav.Link>
                <Nav.Link href="/chargers" active={url === "/chargers"}>{t("chargers")}</Nav.Link>
            </Nav>
            <Nav hidden={connected.value}>
                <Nav.Link onClick={(e) => {
                    e.preventDefault();
                    logout(true);
                }}>{t("logout")}</Nav.Link>
            </Nav>
        </Navbar.Collapse>
    </>

    return (
        <Navbar expand="lg" className="bg-body-tertiary">
            <Container fluid>
                    {connected_to.value}
                    {connected.value ? <></> : nav}
                    <a style="color: #ff0000" class="pe-2">Prerelease</a>
                    <Button variant="primary"
                    onClick={() => {
                        connected.value = false;
                        connected_to.value = "";
                    }} hidden={!connected.value}>{t("close")}</Button>
            </Container>
        </Navbar>
    )
}
