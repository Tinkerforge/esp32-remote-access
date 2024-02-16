import { useLocation } from "preact-iso";
import Nav from "react-bootstrap/Nav";
import Navbar from "react-bootstrap/Navbar";

export function Sidebar() {
    const { url } = useLocation();

    return (
        <div class="collapse bg-light col-lg-2 col-md-3 d-md-block navbar-collapse sidebar">
            <Nav className="flex-column col-2">
                <Nav.Link href='/' active={url === "/"}>Home</Nav.Link>
                <Nav.Link href='/404' active={url === "/404"}>404</Nav.Link>
            </Nav>
        </div>
    )
}
