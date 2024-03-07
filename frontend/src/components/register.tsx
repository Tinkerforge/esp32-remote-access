import { Component } from "preact";
import { Button, Form, FormGroup } from "react-bootstrap"
import { BACKEND } from "../types";

interface RegisterSchema {
    name: string,
    email: string,
    password: string,
}

interface RegisterState {
    data: RegisterSchema,
    accepted: boolean
}

export class Register extends Component<{}, RegisterState> {

    constructor() {
        super();
        this.state = {
            data: {
                name: "",
                email: "",
                password: "",
            },
            accepted: false
        }
    }

    onSubmit(e: SubmitEvent) {
    }

    render() {
        const data = this.state.data;
        return (<>
            <Form onSubmit={(e: SubmitEvent) => {
                e.preventDefault()
                const form = e.currentTarget as any;
                if (form.checkValidity() === false) {
                    e.stopPropagation();
                    console.log("invalid");
                }

                fetch(BACKEND + "/auth/register", {
                    method: "POST",
                    body: JSON.stringify(this.state.data),
                    headers: {
                        "Content-Type": "application/json",
                    }
                })
            }}>
                <Form.Group className="mb-3" controlId="registerName">
                    <Form.Label>Name</Form.Label>
                    <Form.Control type="text" placeholder="John Doe" value={data.name} onChange={(e) => {
                        console.log(e.target);
                        this.setState({data: {...this.state.data, name: (e.target as HTMLInputElement).value}})
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerEmail">
                    <Form.Label>Email</Form.Label>
                    <Form.Control type="email" placeholder="Email" value={data.email} onChange={(e) => {
                        this.setState({data: {...this.state.data, email: (e.target as HTMLInputElement).value}})
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerPassword">
                    <Form.Label>Password</Form.Label>
                    <Form.Control type="password" placeholder="Password" value={data.password} onChange={(e) => {
                        this.setState({data: {...this.state.data, password: (e.target as HTMLInputElement).value}})
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
