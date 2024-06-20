import { Component } from "preact";
import Button from "react-bootstrap/Button"
import Form from "react-bootstrap/Form";
import { BACKEND } from "../types";
import { showAlert } from "./Alert";
import { generate_hash, get_salt_for_user } from "../utils";
import { Modal } from "react-bootstrap";
import { useTranslation } from "react-i18next";
import { PasswordComponent } from "./password_component";

interface LoginSchema {
    email: string,
    login_key: number[],
}

interface LoginState {
    email: string,
    password: string,
    show_modal: boolean,
}

export class Login extends Component<{}, LoginState> {
    constructor() {
        super();
        this.state = {
            email: "",
            password: "",
            show_modal: false,
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
            login_salt = await get_salt_for_user(this.state.email);
        } catch (e) {
            showAlert(e, "danger");
            return;
        }

        const login_key = await generate_hash(this.state.password, login_salt);

        const login_schema: LoginSchema = {
            email: this.state.email,
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
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "login"});
        return(<>
            <Modal show={this.state.show_modal} onHide={() => this.setState({show_modal: false})}>
                <Modal.Dialog>
                    <Modal.Header closeButton>
                        <Modal.Title>
                            {t("password_recovery")}
                        </Modal.Title>
                    </Modal.Header>
                    <Form onSubmit={async (e: SubmitEvent) => {
                        e.preventDefault();
                        const resp = await fetch(`${BACKEND}/auth/start_recovery?email=${this.state.email}`);
                        if (resp.status != 200) {
                            this.setState({show_modal: false});
                            showAlert(t("error_alert_text", {status: resp.status, text: await resp.text(), interpolation: {escapeValue: false}}), "danger");
                        } else {
                            showAlert(t("success_alert_text"), "success", t("success_alert_heading"));
                            this.setState({show_modal: false});
                        }
                    }}>
                        <Modal.Body>
                            <Form.Group className="mb-3" controlId="startRecoveryEmail">
                                <Form.Label>{t("email")}</Form.Label>
                                <Form.Control type="text" placeholder={t("email")} value={this.state.email} onChange={(e) => {
                                    this.setState({email: (e.target as HTMLInputElement).value});
                                }} />
                            </Form.Group>
                        </Modal.Body>
                        <Modal.Footer>
                            <Button variant="primary" type="submit">
                                {t("send")}
                            </Button>
                            <Button variant="secondary" type="button" onClick={() => this.setState({show_modal: false})}>
                                {t("close")}
                            </Button>
                        </Modal.Footer>
                    </Form>
                </Modal.Dialog>
            </Modal>

            <Form onSubmit={async (e: SubmitEvent) => this.onSubmit(e)}>
                <Form.Group className="mb-3" controlId="loginEmail">
                    <Form.Label>{t("email")}</Form.Label>
                    <Form.Control type="text" placeholder={t("email")} value={this.state.email} onChange={(e) => {
                        this.setState({email: (e.target as HTMLInputElement).value});
                    }} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="loginPassword" >
                    <Form.Label>{t("password")}</Form.Label>
                    <PasswordComponent onChange={(e) => {
                        this.setState({password: (e.target as HTMLInputElement).value});
                    }}/>
                </Form.Group>
                <Button variant="primary" type="submit">
                    {t("login")}
                </Button>
                <a className="col mb-3 ms-3" href="" onClick={(e) => {
                        e.preventDefault();
                        this.setState({show_modal: true});
                    }}>{t("password_recovery")}</a>
            </Form>
        </>)
    }
}
