import { signal } from "@preact/signals";
import { Base64 } from "js-base64";
import { Button, Card, Form } from "react-bootstrap";
import { concat_salts, generate_hash, generate_random_bytes, get_salt } from "../utils";
import { crypto_box_SECRETKEYBYTES, crypto_box_keypair, crypto_box_seal, crypto_secretbox_KEYBYTES, crypto_secretbox_NONCEBYTES, crypto_secretbox_easy } from "libsodium-wrappers";
import { BACKEND } from "../types";
import { showAlert } from "../components/Alert";

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
    const params = new URLSearchParams(window.location.search);
    const state = signal({
        recovery_key: params.get("token"),
        new_password: "",
        secret: new Uint8Array(),
    });

    const onSubmit = async (e: SubmitEvent) => {
        e.preventDefault();

        const salt1 = await get_salt();
        const secret_salt = concat_salts(salt1);
        const secret_key = await generate_hash(state.value.new_password, secret_salt, crypto_secretbox_KEYBYTES);

        const salt3 = await get_salt();
        const login_salt = concat_salts(salt3);
        const login_key = await generate_hash(state.value.new_password, login_salt);

        const secret_nonce = generate_random_bytes(crypto_secretbox_NONCEBYTES);

        let secret_reuse: boolean;
        let encrypted_secret: Uint8Array;
        if (state.value.secret.length == 0) {
            const key_pair = crypto_box_keypair();
            const new_secret = key_pair.privateKey;
            encrypted_secret = crypto_secretbox_easy(new_secret, secret_nonce, secret_key);
            secret_reuse = false;
            window.location.replace("/");
        } else {
            encrypted_secret = crypto_secretbox_easy(state.value.secret, secret_nonce, secret_key);
            secret_reuse = true;
        }

        const payload: RecoverySchema = {
            new_encrypted_secret: [].slice.call(encrypted_secret),
            new_login_key: [].slice.call(login_key),
            new_login_salt: [].slice.call(login_salt),
            new_secret_nonce: [].slice.call(secret_nonce),
            new_secret_salt: [].slice.call(secret_salt),
            recovery_key: state.value.recovery_key,
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
        } else {
            showAlert(`Failed to recover account with code ${resp.status}: ${await resp.text()}`, "danger");
        }
    }

    return <>
        <Form onSubmit={(e: SubmitEvent) => onSubmit(e)}>
            <Card>
                <Card.Header>
                    <Card.Title>
                        Account Recovery
                    </Card.Title>
                </Card.Header>
                <Card.Body>
                    <Form.Group className="mb-3" controlId="newPassword">
                        <Form.Label>
                            New Password
                        </Form.Label>
                        <Form.Control type="password" placeholder="New Password" value={state.value.new_password} onChange={(e) => {
                            state.value.new_password = (e.target as HTMLInputElement).value;
                        }} />
                    </Form.Group>
                    <Form.Group className="mb-3" controlId="recoveryFile">
                        <Form.Label>
                            Recovery File
                        </Form.Label>
                        <Form.Control aria-invalid="true" type="file" onChange={async (e) => {
                            const target = e.target as HTMLInputElement;
                            const recovery_file = target.files.item(0);

                            try {
                                const file_text = await recovery_file.text();
                                const file_object = JSON.parse(file_text);
                                const hash = await window.crypto.subtle.digest("SHA-256", new TextEncoder().encode(file_object.email + file_object.secret));
                                const hash_string = Base64.fromUint8Array(new Uint8Array(hash));
                                if (hash_string != file_object.hash) {
                                    throw "Data has been modified";
                                }

                                state.value.secret = Base64.toUint8Array(file_object.secret);
                                target.reportValidity = () => {
                                    return true;
                                }
                            } catch (e) {
                                console.log(e);
                                target.reportValidity = () => {
                                    return false;
                                }
                            }
                        }} />
                        <Form.Control.Feedback type="invalid">File is invalid</Form.Control.Feedback>
                    </Form.Group>
                </Card.Body>
                <Card.Footer>
                    <Button type="submit" variant="primary">Submit</Button>
                </Card.Footer>
            </Card>
        </Form>
    </>
}
