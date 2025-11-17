import { Component } from "preact";
import { Button, Form } from "react-bootstrap"
import { showAlert } from "./Alert";
import { fetchClient, generate_hash, generate_random_bytes, get_salt } from "../utils";
import sodium from "libsodium-wrappers";
import { Trans, useTranslation } from "react-i18next";
import i18n from "../i18n";
import { PasswordComponent } from "./PasswordComponent";
import { RecoveryDataComponent } from "./RecoveryDataComponent";
import { ResendVerification } from "./ResendVerification";
import { Signal, signal } from "@preact/signals";
import { privacy_notice, terms_of_use } from "links";

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
    confirmPassword: string,
    confirmPasswordValid: boolean,
    name: string,
    nameValid: boolean,
    email: string,
    emailValid: boolean,
    termsAndConditionsChecked: boolean,
    termsAndConditionsValid: boolean,
    acceptPrivacyChecked: boolean,
    acceptPrivacyValid: boolean,
    recoverySafed: boolean,
    encryptedSecret: Uint8Array,
    secret: Uint8Array,
    registrationSuccess: boolean,
}

export class Register extends Component<Record<string, never>, RegisterState> {
    constructor() {
        super();
        this.state = {
            accepted: false,
            password: "",
            passwordValid: true,
            confirmPassword: "",
            confirmPasswordValid: true,
            name: "",
            nameValid: true,
            email: "",
            emailValid: true,
            termsAndConditionsChecked: false,
            termsAndConditionsValid: true,
            acceptPrivacyChecked: false,
            acceptPrivacyValid: true,
            recoverySafed: false,
            encryptedSecret: new Uint8Array(),
            secret: new Uint8Array(),
            registrationSuccess: false,
        }

        this.showModal = signal(false);
    }

    showModal: Signal<boolean>;

    async checkPassword() {
        let res = true;

        const state = this.state as RegisterState;

        if (this.state.password.length < 8) {
            state.passwordValid = false;
            res = false;
        } else {
            state.passwordValid = true;
        }

        const passwordsMatch = this.state.password === this.state.confirmPassword;
        if (!passwordsMatch) {
            state.confirmPasswordValid = false;
            res = false;
        } else {
            state.confirmPasswordValid = true;
        }

        const promise = new Promise<boolean>((resolve) => {
            this.setState(state, () => {
                resolve(res);
            });
        });

        return promise;
    }

    async checkForm() {
        let ret = true;
        if (!await this.checkPassword()) {
           ret = false;
        }

        const state = this.state as RegisterState;
        if (state.email.length === 0) {
            state.emailValid = false;
            ret = false;
        } else {
            state.emailValid = true;
        }

        if (state.name.length === 0) {
            state.nameValid = false;
            ret = false;
        } else {
            state.nameValid = true;
        }

        if (!state.acceptPrivacyChecked) {
            state.acceptPrivacyValid = false;
            ret = false;
        } else {
            state.acceptPrivacyValid = true;
        }

        if (!state.termsAndConditionsChecked) {
            state.termsAndConditionsValid = false;
            ret = false;
        } else {
            state.termsAndConditionsValid = true;
        }

        this.setState(state);

        return ret;
    }

    async onSubmit(e: SubmitEvent) {
        e.preventDefault()

        if (! await this.checkForm()) {
            e.stopPropagation();
            return;
        }

        let secret_salt: Uint8Array;
        try {
            secret_salt = await get_salt();
        } catch (e: unknown) {
            showAlert(String(e), "danger");
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
        } catch (e: unknown) {
            showAlert(String(e), "danger");
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

        const {response, error} = await fetchClient.POST("/auth/register",
            {
                body: login_data,
                headers: {
                    "X-Lang": i18n.language
                }
            })
        if (response.status === 201) {
            showAlert(i18n.t("register.registration_successful"), "success", "register", i18n.t("alert_default_success"));
        } else {
            const text = `Failed with status ${response.status}: ${error}`;
            showAlert(text, "danger");
            return;
        }
        this.setState({
            encryptedSecret: new Uint8Array(encrypted_secret),
            secret: keypair.privateKey,
            registrationSuccess: true,
        });
        this.showModal.value = true; // keep existing behavior of showing recovery modal
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "register"})

        const form = <Form onSubmit={(e: SubmitEvent) => this.onSubmit(e)} noValidate>
                <Form.Group className="mb-3" controlId="registerName">
                    <Form.Label>{t("name")}</Form.Label>
                    <Form.Control name="Name" type="text" placeholder="John Doe" value={this.state.name} isInvalid={!this.state.nameValid} onChange={(e) => {
                        const newName = (e.target as HTMLInputElement).value;
                        this.setState({
                            name: newName,
                            nameValid: newName.length > 0
                        });
                    }} />
                    <Form.Control.Feedback type="invalid">
                        {t("name_error_message")}
                    </Form.Control.Feedback>
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerEmail">
                    <Form.Label>{t("email")}</Form.Label>
                    <Form.Control type="email" placeholder={t("email")} value={this.state.email} isInvalid={!this.state.emailValid} onChange={(e) => {
                        const newEmail = (e.target as HTMLInputElement).value;
                        this.setState({
                            email: newEmail,
                            emailValid: newEmail.length > 0
                        });
                    }} />
                    <Form.Control.Feedback type="invalid">
                        {t("email_error_message")}
                    </Form.Control.Feedback>
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerPassword">
                    <Form.Label>{t("password")}</Form.Label>
                    <PasswordComponent
                        value={this.state.password}
                        showStrength={true}
                        onChange={(e) => {
                            this.setState({password: e}, async () => {
                                if (!this.state.confirmPasswordValid || !this.state.passwordValid) {
                                    await this.checkPassword();
                                }
                            });
                        }}
                        isInvalid={!this.state.passwordValid}
                        invalidMessage={t("password_error_message")} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerConfirmPassword">
                    <Form.Label>{t("confirm_password")}</Form.Label>
                    <PasswordComponent value={this.state.confirmPassword} isInvalid={!this.state.confirmPasswordValid} onChange={(e) => {
                        this.setState({confirmPassword: e}, () => {
                            if (!this.state.confirmPasswordValid) {
                                this.checkPassword();
                            }
                        });
                    }}
                    invalidMessage={t("confirm_password_error_message")} />
                </Form.Group>
                <Form.Group className="mb-3" onClick={() => {
                    const newChecked = !this.state.acceptPrivacyChecked;
                    this.setState({
                        acceptPrivacyChecked: newChecked,
                        acceptPrivacyValid: newChecked
                    });
                }}>
                    <Form.Check checked={this.state.acceptPrivacyChecked} type="checkbox" label={<Trans i18nKey="register.accept_privacy_notice" ><a target="__blank" href={privacy_notice}>link</a></Trans>} isInvalid={!this.state.acceptPrivacyValid} />
                </Form.Group>
                <Form.Group className="mb-3" onClick={() => {
                    const newChecked = !this.state.termsAndConditionsChecked;
                    this.setState({
                        termsAndConditionsChecked: newChecked,
                        termsAndConditionsValid: newChecked
                    });
                }}>
                    <Form.Check checked={this.state.termsAndConditionsChecked} type="checkbox" label={<Trans i18nKey="register.accept_terms_and_conditions" ><a target="__blank" href={terms_of_use}>link</a></Trans>} isInvalid={!this.state.termsAndConditionsValid} />
                </Form.Group>
                <Button variant="primary" type="submit">
                    {t("register")}
                </Button>
            </Form>;

        return (<>
            <RecoveryDataComponent email={this.state.email} secret={this.state.secret} show={this.showModal} />

            { !this.state.registrationSuccess && form}

            { this.state.registrationSuccess && this.state.email && <ResendVerification email={this.state.email} /> }
        </>)
    }
}
