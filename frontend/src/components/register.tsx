import { Component } from "preact";
import { Button, Form, FormGroup, Modal } from "react-bootstrap"
import { BACKEND } from "../types";
import { showAlert } from "./Alert";
import { PASSWORD_PATTERN, generate_hash, generate_random_bytes, get_salt } from "../utils";
import { Base64 } from "js-base64";
import sodium from "libsodium-wrappers";
import { useTranslation } from "react-i18next";
import i18n from "../i18n";

interface RegisterSchema {
    name: string,
    email: string,
    login_key: number[],
    login_salt: number[],
    secret: number[],
    secret_nonce: number[],
    secret_salt: number[],
}

interface RegisterState {
    accepted: boolean,
    password: string,
    passwordValid: boolean,
    name: string,
    nameValid: boolean,
    email: string,
    emailValid: boolean,
    checkBoxChecked: boolean,
    checkBoxValid: boolean,
    recoverySafed: boolean,
    encryptedSecret: Uint8Array,
    secret: Uint8Array,
    showModal: boolean,
    validated: boolean,
}

export class Register extends Component<{}, RegisterState> {
    constructor() {
        super();
        this.state = {
            accepted: false,
            password: "",
            passwordValid: true,
            name: "",
            nameValid: true,
            email: "",
            emailValid: true,
            checkBoxChecked: false,
            checkBoxValid: true,
            recoverySafed: false,
            encryptedSecret: new Uint8Array(),
            secret: new Uint8Array(),
            showModal: false,
            validated: false,
        }
    }

    checkPassword() {

        const res = PASSWORD_PATTERN.test(this.state.password);
        if (!res) {
            this.setState({passwordValid: false});
        } else {
            this.setState({passwordValid: true});
        }
        return res;
    }

    checkForm() {
        let ret = true;
        if (!this.checkPassword()) {
           ret = false;
        }

        if (this.state.email.length === 0) {
            this.setState({emailValid: false});
            ret = false;
        } else {
            this.setState({emailValid: true});
        }

        if (this.state.name.length === 0) {
            this.setState({nameValid: false});
            ret = false;
        } else {
            this.setState({nameValid: true});
        }

        if (!this.state.checkBoxChecked) {
            this.setState({checkBoxValid: false});
            ret = false;
        } else {
            this.setState({checkBoxValid: true});
        }

        this.setState({validated: true});

        return ret;
    }

    async onSubmit(e: SubmitEvent) {
        e.preventDefault()

        if (!this.checkForm()) {
            e.stopPropagation();
            return;
        }

        let secret_salt: Uint8Array;
        try {
            secret_salt = await get_salt();
        } catch (e) {
            showAlert(e, "danger");
            return;
        }

        const this_secret_salt = generate_random_bytes(24);

        const combined_secret_salt = new Uint8Array(secret_salt.length + this_secret_salt.length);
        combined_secret_salt.set(secret_salt);
        combined_secret_salt.set(this_secret_salt, secret_salt.length);
        const secret_key = await generate_hash(this.state.password, combined_secret_salt, sodium.crypto_secretbox_KEYBYTES);

        const keypair = sodium.crypto_box_keypair();

        const secret_nonce = generate_random_bytes(sodium.crypto_secretbox_NONCEBYTES);
        const encrypted_secret = sodium.crypto_secretbox_easy(keypair.privateKey, secret_nonce, secret_key);

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
            secret: [].slice.call(new Uint8Array(encrypted_secret)),
            secret_nonce: [].slice.call(secret_nonce),
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
            showAlert(text, "success", i18n.t("alert_default_success"));
        } else {
            const body = await resp.text();
            const text = `Failed with status ${resp.status}: ${body}`;
            showAlert(text, "danger");
            return;
        }

        this.setState({encryptedSecret: new Uint8Array(encrypted_secret), showModal: true, secret: keypair.privateKey});

    }

    async saveRecoveryData() {
        const secret_b64 = Base64.fromUint8Array(this.state.secret);
        const hash = await window.crypto.subtle.digest("SHA-256", new TextEncoder().encode(this.state.email + secret_b64));
        const backupData = {
            email: this.state.email,
            secret: secret_b64,
            hash: Base64.fromUint8Array(new Uint8Array(hash)),
        };

        const backupString = JSON.stringify(backupData);
        const file = new File([backupString], "RecpveryData", {
            type: "text/plain"
        });
        const a = document.createElement("a");
        const url = URL.createObjectURL(file);
        a.href = url;
        a.download = "RecoveryData";
        document.body.appendChild(a);
        a.click()

        this.setState({recoverySafed: true});
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "register"})

        return (<>
            <Modal show={this.state.showModal} onHide={() => this.setState({showModal: false})}>
                <Modal.Dialog>
                    <Modal.Header closeButton>
                        <Modal.Title>{t("save_recovery_data")}</Modal.Title>
                    </Modal.Header>

                    <Modal.Body>
                        <p className="mb-3">{t("save_recovery_data_text")}</p>
                        <Button variant="primary" onClick={() => this.saveRecoveryData()}>{t("save")}</Button>
                    </Modal.Body>

                    <Modal.Footer>
                        <Button variant={this.state.recoverySafed ? "primary" : "danger"} onClick={() => {
                            this.setState({showModal: false});
                        }}>{t("close")}</Button>
                    </Modal.Footer>
                </Modal.Dialog>
            </Modal>

            <Form onSubmit={(e: SubmitEvent) => this.onSubmit(e)} validated={this.state.validated} noValidate>
                <Form.Group className="mb-3" controlId="registerName">
                    <Form.Label>{t("name")}</Form.Label>
                    <Form.Control type="text" placeholder="John Doe" value={this.state.name} isInvalid={!this.state.nameValid} onChange={(e) => {
                        this.setState({name: (e.target as HTMLInputElement).value})
                    }} />
                    <Form.Control.Feedback type="invalid">
                        {t("name_error_message")}
                    </Form.Control.Feedback>
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerEmail">
                    <Form.Label>{t("email")}</Form.Label>
                    <Form.Control type="email" placeholder={t("email")} value={this.state.email} isInvalid={!this.state.emailValid} onChange={(e) => {
                        this.setState({email: (e.target as HTMLInputElement).value})
                    }} />
                    <Form.Control.Feedback type="invalid">
                        {t("email_error_message")}
                    </Form.Control.Feedback>
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerPassword">
                    <Form.Label>{t("password")}</Form.Label>
                    <Form.Control required isInvalid={!this.state.passwordValid} type="password" placeholder={t("password")} value={this.state.password} onChange={(e) => {
                        this.setState({password: (e.target as HTMLInputElement).value});
                    }} />
                    <Form.Control.Feedback type="invalid">
                        {t("password_error_message")}
                    </Form.Control.Feedback>
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerSubmit">
                    <Form.Check type="checkbox" label={t("accecpt_privacy_notice")} isInvalid={!this.state.checkBoxValid} onClick={() => this.setState({checkBoxChecked: !this.state.checkBoxChecked})}/>
                </Form.Group>
                <Button variant="primary" type="submit">
                    {t("register")}
                </Button>
            </Form>
        </>)
    }
}
