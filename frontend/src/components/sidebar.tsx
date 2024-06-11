import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import { BACKEND } from "../types";
import { useTranslation } from "react-i18next";
import { Navbar } from "react-bootstrap";

export async function logout (e: Event) {
        e.preventDefault();

        await fetch(BACKEND + "/user/logout?logout_all=true", {
            credentials: "include",
        });

        window.location.reload();
    }

export function Sidebar() {
    const { url } = useLocation();
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "sidebar"})

    return (
        <Navbar expand="lg" className="bg-body-tertiary">
            <Navbar.Toggle aria-controls="basic-navbar-nav"/>
            <Navbar.Collapse id="navbar-nav">
                <Nav className="me-auto">
                    <Nav.Link href="/user" active={url === "/user"}>{t("user")}</Nav.Link>
                    <Nav.Link href="/chargers" active={url === "/chargers"}>{t("chargers")}</Nav.Link>
                </Nav>
                <Nav>
                    <Nav.Link onClick={logout} >{t("logout")}</Nav.Link>
                </Nav>
            </Navbar.Collapse>
        </Navbar>
    )
}
