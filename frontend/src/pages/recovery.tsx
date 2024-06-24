import { Base64 } from "js-base64";
import { Button, Card, Form } from "react-bootstrap";
import { PASSWORD_PATTERN, concat_salts, generate_hash, generate_random_bytes, get_salt } from "../utils";
import { crypto_box_keypair, crypto_secretbox_KEYBYTES, crypto_secretbox_NONCEBYTES, crypto_secretbox_easy } from "libsodium-wrappers";
import { BACKEND } from "../types";
import { showAlert } from "../components/Alert";
import { useTranslation } from "react-i18next";
import { PasswordComponent } from "../components/password_component";
import { useState } from "preact/hooks";
import { useSignal } from "@preact/signals";
import { RecoveryDataComponent } from "../components/recovery_data_component";

interface RecoverySchema {
    new_encrypted_secret: number[],
    new_login_key: number[],
    new_login_salt: number[],
    new_secret_nonce: number[],
    new_secret_salt: number[],
    recovery_key: string,
    reused_secret: boolean,
}

export function Recovery() {
    const {t} = useTranslation();

    const params = new URLSearchParams(window.location.search);
    const [state, setState] = useState({
        recovery_key: params.get("token"),
        email: params.get("email"),
        new_password: "",
        passwordValid: true,
        fileValid: true,
        validated: false,
    });
    const secret = useSignal(new Uint8Array());
    const showModal = useSignal(false);

    const validateForm = () => {
        let ret = true;
        let passworValid = true;

        if (!PASSWORD_PATTERN.test(state.new_password)) {
            passworValid = false;
            ret = false;
        }

        if (!state.fileValid) {
            ret = false;
        }

        setState({...state, validated: true, passwordValid: passworValid});

        return ret;
    }

    const onSubmit = async (e: SubmitEvent) => {
        e.preventDefault();

        if (!validateForm()) {
            e.stopPropagation();
            return;
        }

        const salt1 = await get_salt();
        const secret_salt = concat_salts(salt1);
        const secret_key = await generate_hash(state.new_password, secret_salt, crypto_secretbox_KEYBYTES);

        const salt3 = await get_salt();
        const login_salt = concat_salts(salt3);
        const login_key = await generate_hash(state.new_password, login_salt);

        const secret_nonce = generate_random_bytes(crypto_secretbox_NONCEBYTES);

        let secret_reuse: boolean;
        let encrypted_secret: Uint8Array;
        if (secret.value.length == 0) {
            const key_pair = crypto_box_keypair();
            const new_secret = key_pair.privateKey;
            secret.value = new_secret;
            encrypted_secret = crypto_secretbox_easy(new_secret, secret_nonce, secret_key);
            secret_reuse = false;
        } else {
            encrypted_secret = crypto_secretbox_easy(secret.value, secret_nonce, secret_key);
            secret_reuse = true;
        }

        const payload: RecoverySchema = {
            new_encrypted_secret: [].slice.call(encrypted_secret),
            new_login_key: [].slice.call(login_key),
            new_login_salt: [].slice.call(login_salt),
            new_secret_nonce: [].slice.call(secret_nonce),
            new_secret_salt: [].slice.call(secret_salt),
            recovery_key: state.recovery_key,
            reused_secret: secret_reuse,
        }

        const resp = await fetch(`${BACKEND}/auth/recovery`, {
            method: "POST",
            body: JSON.stringify(payload),
            headers: {
                "Content-Type": "application/json"
            }
        });
        if (resp.status === 200) {
            showAlert("Your new password is set!", "success", "Success");
            showModal.value = true;
        } else {
            showAlert(`Failed to recover account with code ${resp.status}: ${await resp.text()}`, "danger");
        }
    }

    return <>
        <RecoveryDataComponent email={state.email} secret={secret.value} show={showModal} />

        <Form onSubmit={(e: SubmitEvent) => onSubmit(e)} noValidate>
            <Card>
                <Card.Header>
                    <Card.Title>
                        {t("recovery.recovery")}
                    </Card.Title>
                </Card.Header>
                <Card.Body>
                    <Form.Group className="mb-3" controlId="newPassword">
                        <Form.Label>
                            {t("recovery.new_password")}
                        </Form.Label>
                        <PasswordComponent isInvalid={!state.passwordValid}  onChange={(e) => {
                            setState({...state, new_password: (e.target as HTMLInputElement).value});
                        }} />
                    </Form.Group>
                    <Form.Group className="mb-3" controlId="recoveryFile">
                        <Form.Label>
                            {t("recovery.recovery_file")}
                        </Form.Label>
                        <Form.Control type="file" isInvalid={!state.fileValid} onChange={async (e) => {
                            const target = e.target as HTMLInputElement;
                            const recovery_file = target.files.item(0);

                            try {
                                const file_text = await recovery_file.text();
                                const file_object = JSON.parse(file_text);
                                if (!("email" in file_object) || !("secret" in file_object) || !("hash" in file_object)) {
                                    throw "Invalid data";
                                }
                                const hash = await window.crypto.subtle.digest("SHA-256", new TextEncoder().encode(file_object.email + file_object.secret));
                                const hash_string = Base64.fromUint8Array(new Uint8Array(hash));
                                if (hash_string != file_object.hash) {
                                    throw "Data has been modified";
                                }

                                secret.value = Base64.toUint8Array(file_object.secret);
                                setState({...state, fileValid: true, validated: true});
                            } catch (e) {
                                setState({...state, fileValid: false, validated: true});
                            }
                        }} />
                        <Form.Control.Feedback type="invalid">{t("recovery.invalid_file")}</Form.Control.Feedback>
                    </Form.Group>
                </Card.Body>
                <Card.Footer>
                    <Button type="submit" variant="primary">{t("recovery.submit")}</Button>
                </Card.Footer>
            </Card>
        </Form>
    </>
}
