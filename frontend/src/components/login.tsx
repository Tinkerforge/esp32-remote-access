import { Component } from "preact";
import Button from "react-bootstrap/Button"
import Form from "react-bootstrap/Form";
import { BACKEND } from "../types";
import { showAlert } from "./Alert";
import { generate_hash, get_salt_for_user } from "../utils";

interface LoginSchema {
    username: string,
    login_key: number[],
}

interface LoginState {
    username: string,
    password: string,
}

export class Login extends Component<{}, LoginState> {
    constructor() {
        super();
        this.state = {
            username: "",
            password: "",
        }
    }

    async onSubmit(e: SubmitEvent) {
        e.preventDefault();
        const form = e.target as any;
        if (form.checkValidity() === false) {
            e.stopPropagation();
        }

        let login_salt: Uint8Array;
        try {
            login_salt = await get_salt_for_user(this.state.username);
        } catch (e) {
            showAlert(e, "danger");
            return;
        }

        const login_key = await generate_hash(this.state.password, login_salt);

        const login_schema: LoginSchema = {
            username: this.state.username,
            login_key: [].slice.call(login_key)
        };

        let resp = await fetch(BACKEND + "/auth/login", {
            method: "POST",
            body: JSON.stringify(login_schema),
            headers: {
                "Content-Type": "application/json",
            },
            credentials: "include"
        });

        if (resp.status === 200) {
            sessionStorage.setItem("password", this.state.password);
            window.location.reload()
        } else {
            const body = await resp.text();
            const text = `Failed with status ${resp.status}: ${body}`;
            showAlert(text, "danger");
        }
    }

    render() {
        return(<>
            <Form onSubmit={async (e: SubmitEvent) => this.onSubmit(e)}>
                <Form.Group className="mb-3" controlId="loginEmail">
                    <Form.Label>Username</Form.Label>
                    <Form.Control type="text" placeholder="Username" value={this.state.username} onChange={(e) => {
                        this.setState({username: (e.target as HTMLInputElement).value});
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="loginPassword" >
                    <Form.Label>Password</Form.Label>
                    <Form.Control type="password" placeholder="Password" value={this.state.password} onChange={(e) => {
                        this.setState({password: (e.target as HTMLInputElement).value});
                    }} />
                </Form.Group>
                <Button variant="primary" type="submit">
                    Login
                </Button>
            </Form>
        </>)
    }
}
