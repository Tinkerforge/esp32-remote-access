import { Component } from "preact";
import { Button, Form, FormGroup } from "react-bootstrap"
import { BACKEND } from "../types";
import { showAlert } from "./Alert";
import { generate_hash, generate_random_bytes, get_salt } from "../utils";

interface RegisterSchema {
    name: string,
    email: string,
    login_key: number[],
    login_salt: number[],
    secret: number[],
    secret_iv: number[],
    secret_salt: number[],
}

interface RegisterState {
    accepted: boolean,
    password: string,
    name: string,
    email: string,
}

export class Register extends Component<{}, RegisterState> {
    constructor() {
        super();
        this.state = {
            accepted: false,
            password: "",
            name: "",
            email: "",
        }
    }

    async onSubmit(e: SubmitEvent) {
        e.preventDefault()
        const form = e.currentTarget as any;
        if (form.checkValidity() === false) {
            e.stopPropagation();
        }

        let secret_salt: Uint8Array;
        try {
            secret_salt = await get_salt();
        } catch (e) {
            showAlert(e, "danger");
            return;
        }

        const this_secret_salt = generate_random_bytes(24);
        const secret = generate_random_bytes(16);

        const combined_secret_salt = new Uint8Array(secret_salt.length + this_secret_salt.length);
        combined_secret_salt.set(secret_salt);
        combined_secret_salt.set(this_secret_salt, secret_salt.length);
        const secret_hash = await generate_hash(this.state.password, combined_secret_salt, 16);

        const crypto = window.crypto.subtle;
        const key = await crypto.importKey("raw", secret_hash, {name: "AES-CBC"}, false, ["encrypt"]);

        const secret_iv = generate_random_bytes(16);
        const encrypted_secret = await crypto.encrypt(
            {
                name: "AES-CBC",
                iv: secret_iv
            },
            key,
            secret
        );

        let login_salt: Uint8Array;
        try {
            login_salt = await get_salt();
        } catch (e) {
            showAlert(e, "danger");
            return;
        }

        const this_login_salt = generate_random_bytes(24);
        const combined_login_salt = new Uint8Array(login_salt.length + this_login_salt.length);
        combined_login_salt.set(login_salt);
        combined_login_salt.set(this_login_salt, login_salt.length);

        const login_key = await generate_hash(this.state.password, combined_login_salt);

        const login_data: RegisterSchema = {
            name: this.state.name,
            email: this.state.email,
            login_key: [].slice.call(login_key),
            login_salt: [].slice.call(combined_login_salt),
            secret: [].slice.call(encrypted_secret),
            secret_iv: [].slice.call(secret_iv),
            secret_salt: [].slice.call(combined_secret_salt),
        }

        const resp = await fetch(BACKEND + "/auth/register", {
            method: "POST",
            body: JSON.stringify(login_data),
            headers: {
                "Content-Type": "application/json",
            }
        });
        if (resp.status === 201) {
            const text = "Registration was successful, you should receive an email in the next couple of minutes.";
            showAlert(text, "success");
        } else {
            const body = await resp.text();
            const text = `Failed with status ${resp.status}: ${body}`;
            showAlert(text, "danger");
        }
    }

    render() {
        return (<>
            <Form onSubmit={(e: SubmitEvent) => this.onSubmit(e)}>
                <Form.Group className="mb-3" controlId="registerName">
                    <Form.Label>Name</Form.Label>
                    <Form.Control type="text" placeholder="John Doe" value={this.state.name} onChange={(e) => {
                        this.setState({name: (e.target as HTMLInputElement).value})
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerEmail">
                    <Form.Label>Email</Form.Label>
                    <Form.Control type="email" placeholder="Email" value={this.state.email} onChange={(e) => {
                        this.setState({email: (e.target as HTMLInputElement).value})
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerPassword">
                    <Form.Label>Password</Form.Label>
                    <Form.Control type="password" placeholder="Password" value={this.state.password} onChange={(e) => {
                        this.setState({password: (e.target as HTMLInputElement).value});
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerSubmit">
                    <Form.Check type="checkbox" label="Accept privacy notice" required/>
                </Form.Group>
                <Button variant="primary" type="submit">
                    Register
                </Button>
            </Form>
        </>)
    }
}
