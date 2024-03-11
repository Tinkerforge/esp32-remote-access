import { Button } from "react-bootstrap";
import { BACKEND } from "../types";


export function Test() {
    return (<>
        <Button variant="primary" onClick={async () => {
            const resp = await fetch(
                BACKEND + "/management",
                {
                    method: "PUT",
                    credentials: "include",
                    body: JSON.stringify({
                        id: "asdafa"
                    }),
                    headers: {
                        "Content-Type": "application/json",
                    }
                }
            )
        }}>Click me</Button>
    </>)
}
