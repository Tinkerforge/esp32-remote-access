import { Component } from "preact";
import { Button, Form } from "react-bootstrap"
import { showAlert } from "./Alert";
import { PASSWORD_PATTERN, fetchClient, generate_hash, generate_random_bytes, get_salt } from "../utils";
import sodium from "libsodium-wrappers";
import { Trans, useTranslation } from "react-i18next";
import i18n from "../i18n";
import { PasswordComponent } from "./password_component";
import { RecoveryDataComponent } from "./recovery_data_component";
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
}

export class Register extends Component<{}, RegisterState> {
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
        }

        this.showModal = signal(false);
    }

    showModal: Signal<boolean>;

    checkPassword() {
        let res = true;

        const passwordPatternValid = PASSWORD_PATTERN.test(this.state.password);
        if (!passwordPatternValid) {
            this.setState({passwordValid: false});
            res = false;
        } else {
            this.setState({passwordValid: true});
        }

        const passwordsMatch = this.state.password === this.state.confirmPassword;
        if (!passwordsMatch) {
            this.setState({confirmPasswordValid: false});
            res = false;
        } else {
            this.setState({confirmPasswordValid: true});
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

        if (!this.state.acceptPrivacyChecked) {
            this.setState({acceptPrivacyValid: false});
            ret = false;
        } else {
            this.setState({acceptPrivacyValid: true});
        }

        if (!this.state.termsAndConditionsChecked) {
            this.setState({termsAndConditionsValid: false});
            ret = false;
        } else {
            this.setState({termsAndConditionsValid: true});
        }

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
        } catch (e: any) {
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
        } catch (e: any) {
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

        const {response, error} = await fetchClient.POST("/auth/register",
            {
                body: login_data,
                headers: {
                    "X-Lang": i18n.language
                }
            })
        if (response.status === 201) {
            showAlert(i18n.t("register.registration_successful"), "success", "registe", i18n.t("alert_default_success"));
        } else {
            const text = `Failed with status ${response.status}: ${error}`;
            showAlert(text, "danger");
            return;
        }

        this.setState({encryptedSecret: new Uint8Array(encrypted_secret), secret: keypair.privateKey});
        this.showModal.value = true;
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "register"})

        return (<>
            <RecoveryDataComponent email={this.state.email} secret={this.state.secret} show={this.showModal} />

            <Form onSubmit={(e: SubmitEvent) => this.onSubmit(e)} noValidate>
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
                    <PasswordComponent isInvalid={!this.state.passwordValid} onChange={(e) => {
                        this.setState({password: e}, () => {
                            if (!this.state.confirmPasswordValid || !this.state.passwordValid) {
                                this.checkPassword();
                            }
                        });
                    }}
                    invalidMessage={t("password_error_message")} />
                </Form.Group>
                <Form.Group className="mb-3" controlId="registerConfirmPassword">
                    <Form.Label>{t("confirm_password")}</Form.Label>
                    <PasswordComponent isInvalid={!this.state.confirmPasswordValid} onChange={(e) => {
                        this.setState({confirmPassword: e}, () => {
                            if (!this.state.confirmPasswordValid || !this.state.passwordValid) {
                                this.checkPassword();
                            }
                        });
                    }}
                    invalidMessage={t("confirm_password_error_message")} />
                </Form.Group>
                <Form.Group className="mb-3" onClick={() => this.setState({acceptPrivacyChecked: !this.state.acceptPrivacyChecked})}>
                    <Form.Check checked={this.state.acceptPrivacyChecked} type="checkbox" label={<Trans i18nKey="register.accept_privacy_notice" ><a target="__blank" href={privacy_notice}>link</a></Trans>} isInvalid={!this.state.acceptPrivacyValid}/>
                </Form.Group>
                <Form.Group className="mb-3" onClick={() => this.setState({termsAndConditionsChecked: !this.state.termsAndConditionsChecked})}>
                    <Form.Check checked={this.state.termsAndConditionsChecked} type="checkbox" label={<Trans i18nKey="register.accept_terms_and_conditions" ><a target="__blank" href={terms_of_use}>link</a></Trans>} isInvalid={!this.state.termsAndConditionsValid}/>
                </Form.Group>
                <Button variant="primary" type="submit">
                    {t("register")}
                </Button>
            </Form>
        </>)
    }
}
