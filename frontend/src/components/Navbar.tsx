import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import { BACKEND } from "../types";
import { useTranslation } from "react-i18next";
import { Button, Container, Navbar } from "react-bootstrap";
import { connected } from "../pages/chargers";

export async function logout(logout_all: boolean) {
        await fetch(`${BACKEND}/user/logout?logout_all=${logout_all ? "true" : "false"}`, {
            credentials: "include",
        });

        window.location.reload();
    }

export function CustomNavbar() {
    const { url } = useLocation();
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "navbar"})

    return (
        <Navbar expand="lg" className="bg-body-tertiary">
            <Container fluid>
                <Navbar.Toggle aria-controls="basic-navbar-nav" hidden={connected.value}/>
                <Navbar.Collapse id="navbar-nav" hidden={connected.value}>
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
                    <a style="color: #ff0000" class="pe-2">Prerelease</a>
                    <Button variant="primary"
                    onClick={() => {
                        connected.value = false;
                    }} hidden={!connected.value}>{t("close")}</Button>
            </Container>
        </Navbar>
    )
}
