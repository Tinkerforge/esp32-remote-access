import { Base64 } from "js-base64";
import { Button, Card, Form, Modal } from "react-bootstrap";
import { AppState, PASSWORD_PATTERN, concat_salts, fetchClient, generate_hash, generate_random_bytes, get_salt, loggedIn } from "../utils";
import { crypto_box_keypair, crypto_secretbox_KEYBYTES, crypto_secretbox_NONCEBYTES, crypto_secretbox_easy } from "libsodium-wrappers";
import { showAlert } from "../components/Alert";
import { useTranslation } from "react-i18next";
import { PasswordComponent } from "../components/PasswordComponent";
import { useEffect, useState } from "preact/hooks";
import { useSignal } from "@preact/signals";
import { RecoveryDataComponent } from "../components/RecoveryDataComponent";
import { useLocation } from "preact-iso";

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
    const { route, query } = useLocation();

    useEffect(() => {
        fetchClient.POST("/check_expiration", {body: {token: query.token, token_type: "Recovery"}})
            .then(({data}) => {
                if (!data) {
                    showAlert(t("recovery.token_expired"), "danger");
                    loggedIn.value = AppState.LoggedOut;
                    route("/", true);
                }
            })
    }, [])

    const [state, setState] = useState({
        recovery_key: query.token as string,
        email: query.email as string,
        new_password: "",
        confirm_password: "",
        passwordValid: true,
        confirmPasswordValid: true,
        fileValid: true,
        validated: false,
    });

    const secret = useSignal<Uint8Array>(new Uint8Array());
    const showModal = useSignal(false);
    const showNoFileWarning = useSignal(false);
    const acknowledgeNoFile = useSignal(false);

    const validateForm = () => {
        let ret = true;
        let passworValid = true;
        let confirmValid = true;

        if (!PASSWORD_PATTERN.test(state.new_password)) {
            passworValid = false;
            ret = false;
        }

        if (state.confirm_password !== state.new_password) {
            confirmValid = false;
            ret = false;
        }

        // If user touched file input and it became invalid, block submit.
        if (!state.fileValid) {
            ret = false;
        }

        setState({...state, validated: true, passwordValid: passworValid, confirmPasswordValid: confirmValid});

        return ret;
    }

    const executeRecovery = async () => {
        const salt1 = await get_salt();
        const secret_salt = concat_salts(salt1);
        const secret_key = await generate_hash(state.new_password, secret_salt, crypto_secretbox_KEYBYTES);

        const salt3 = await get_salt();
        const login_salt = concat_salts(salt3);
        const login_key = await generate_hash(state.new_password, login_salt);

        const secret_nonce = generate_random_bytes(crypto_secretbox_NONCEBYTES);

        let secret_reuse: boolean;
        let encrypted_secret: Uint8Array;
        if (secret.value.length === 0) {
            const key_pair = crypto_box_keypair();
            const new_secret = key_pair.privateKey;
            secret.value = new Uint8Array(new_secret);
            encrypted_secret = crypto_secretbox_easy(new_secret, secret_nonce, secret_key);
            secret_reuse = false;
        } else {
            encrypted_secret = crypto_secretbox_easy(secret.value as Uint8Array, secret_nonce, secret_key);
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

        const {response, error} = await fetchClient.POST("/auth/recovery", {body: payload});
        if (response.status === 200) {
            showAlert("Your new password is set!", "success", "recovery", "Success");
            showModal.value = true;
        } else {
            showAlert(`Failed to recover account with code ${response.status}: ${error}`, "danger");
        }
    }

    const onSubmit = async (e: SubmitEvent) => {
        e.preventDefault();

        if (!validateForm()) {
            e.stopPropagation();
            return;
        }

        // If no recovery file was provided, ask for explicit confirmation first.
        if (secret.value.length === 0) {
            showNoFileWarning.value = true;
            return;
        }

        await executeRecovery();
    }

    return <>
        {/* Strong confirmation if no recovery file is provided */}
        <Modal show={showNoFileWarning.value} onHide={() => { showNoFileWarning.value = false; }} centered>
            <Modal.Header closeButton>
                <Modal.Title>
                    {t("recovery.no_file_warning_heading")}
                </Modal.Title>
            </Modal.Header>
            <Modal.Body>
                <p>{t("recovery.no_file_warning_body")}</p>
                <Form.Check
                    type="checkbox"
                    id="acknowledge-no-file"
                    label={t("recovery.no_file_warning_ack")}
                    checked={acknowledgeNoFile.value}
                    onChange={(e: React.ChangeEvent<HTMLInputElement>) => { acknowledgeNoFile.value = e.currentTarget.checked; }}
                />
            </Modal.Body>
            <Modal.Footer>
                <Button variant="outline-secondary" onClick={() => { showNoFileWarning.value = false; }}>
                    {t("recovery.no_file_warning_cancel")}
                </Button>
                <Button variant="danger" disabled={!acknowledgeNoFile.value} onClick={async () => { showNoFileWarning.value = false; acknowledgeNoFile.value = false; await executeRecovery(); }}>
                    {t("recovery.no_file_warning_proceed")}
                </Button>
            </Modal.Footer>
        </Modal>

        <RecoveryDataComponent email={state.email} secret={secret.value as Uint8Array} show={showModal} />

        <Card className="p-0 col-10 col-lg-5 col-xl-3">
            <Form onSubmit={(e: SubmitEvent) => onSubmit(e)} noValidate>
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
                            setState({...state, new_password: e});
                        }} />
                    </Form.Group>
                    <Form.Group className="mb-3" controlId="confirmPassword">
                        <Form.Label>
                            {t("recovery.confirm_password")}
                        </Form.Label>
                        <PasswordComponent isInvalid={!state.confirmPasswordValid} invalidMessage={t("recovery.confirm_password_error_message")} onChange={(e) => {
                            setState({...state, confirm_password: e});
                        }} />
                    </Form.Group>
                    <Form.Group className="mb-3" controlId="recoveryFile">
                        <Form.Label>
                            {t("recovery.recovery_file")}
                        </Form.Label>
                        <Form.Control type="file" isInvalid={!state.fileValid} onChange={async (e) => {
                            const target = e.target as HTMLInputElement;
                            if (!target.files) {
                                return;
                            }
                            const recovery_file = target.files.item(0);
                            if (!recovery_file) {
                                setState({...state, fileValid: false, validated: true});
                                return;
                            }

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

                                secret.value = new Uint8Array(Base64.toUint8Array(file_object.secret));
                                setState({...state, fileValid: true, validated: true});
                            } catch {
                                setState({...state, fileValid: false, validated: true});
                            }
                        }} />
                        <Form.Control.Feedback type="invalid">{t("recovery.invalid_file")}</Form.Control.Feedback>
                    </Form.Group>
                </Card.Body>
                <Card.Footer>
                    <Button type="submit" variant="primary">{t("recovery.submit")}</Button>
                </Card.Footer>
            </Form>
        </Card>
    </>
}
