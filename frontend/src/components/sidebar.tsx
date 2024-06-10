import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import { BACKEND } from "../types";
import { useTranslation } from "react-i18next";

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
        <div class="collapse bg-light col-lg-2 col-md-3 d-md-block navbar-collapse sidebar">
            <Nav className="flex-column col-2">
                <Nav.Link href='/' active={url === "/"}>{t("home")}</Nav.Link>
                <Nav.Link href="/user" active={url === "/user"}>{t("user")}</Nav.Link>
                <Nav.Link href="/chargers" active={url === "/chargers"}>{t("chargers")}</Nav.Link>
                <Nav.Link onClick={logout} >{t("logout")}</Nav.Link>
            </Nav>
        </div>
    )
}
