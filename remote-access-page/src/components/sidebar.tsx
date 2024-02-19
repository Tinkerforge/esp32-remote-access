import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";

export function Sidebar() {
    const { url } = useLocation();

    const logout = async (e: Event) => {
        e.preventDefault();

        await fetch("http://localhost:8081/auth/logout", {
            credentials: "include"
        });

        window.location.reload();
    }

    return (
        <div class="collapse bg-light col-lg-2 col-md-3 d-md-block navbar-collapse sidebar">
            <Nav className="flex-column col-2">
                <Nav.Link href='/' active={url === "/"}>Home</Nav.Link>
                <Nav.Link href="/user" active={url === "/user"}>User</Nav.Link>
                <Nav.Link onClick={logout} >Logout</Nav.Link>
            </Nav>
        </div>
    )
}
