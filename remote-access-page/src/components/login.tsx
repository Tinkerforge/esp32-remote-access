import { Component } from "preact";
import Button from "react-bootstrap/Button"
import Form from "react-bootstrap/Form";

interface LoginSchema {
    email: string,
    password: string,
}


export class Login extends Component<{}, LoginSchema> {
    constructor() {
        super();
        this.state = {
            email: "",
            password: "",
        }
    }


    render() {
        return(<>
            <Form onSubmit={async (e: SubmitEvent) => {
                e.preventDefault();
                const form = e.target as any;
                if (form.checkValidity() === false) {
                    e.stopPropagation();
                }

                let resp = await fetch("http://localhost:8081/auth/login", {
                    method: "POST",
                    body: JSON.stringify(this.state),
                    headers: {
                        "Content-Type": "application/json",
                    },
                    credentials: "include"
                });

                if (resp.status === 200) {
                    window.location.reload()
                }
            }}>
                <Form.Group className="mb-3" controlId="loginEmail">
                    <Form.Label>Email</Form.Label>
                    <Form.Control type="email" placeholder="Email" onChange={(e) => {
                        this.setState({email: (e.target as HTMLInputElement).value});
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="loginPassword" >
                    <Form.Label>Password</Form.Label>
                    <Form.Control type="password" placeholder="Password" onChange={(e) => {
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
