/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

import { Component } from "preact";
import Form from "react-bootstrap/Form"
import Button from "react-bootstrap/Button";
import Modal from "react-bootstrap/Modal";
import { useState } from "preact/hooks";
import { BACKEND } from "../utils";
import { PASSWORD_PATTERN, concat_salts, generate_hash, generate_random_bytes, get_salt, get_salt_for_user } from "../utils";
import sodium from "libsodium-wrappers";
import { logout } from "../components/Navbar";
import { useTranslation } from "react-i18next";
import { Card, Container } from "react-bootstrap";
import { signal } from "@preact/signals";
import { PasswordComponent } from "../components/password_component";
import i18n from "../i18n";
import { showAlert } from "../components/Alert";
import { Base64 } from "js-base64";


interface UserState {
    id: string,
    name: string,
    email: string
}

interface State {
    isDirty: boolean
    user: UserState
}

let email = "";

class UserComponent extends Component<{}, State> {
    constructor() {
        super();

        const state = {
            id: "",
            name: "",
            email: "",
        };

        this.state = {
            isDirty: false,
            user: state,
        }

        fetch(BACKEND + "/user/me", {
            credentials: "include"
        }).then(async (r) => {
            if (r.status === 200) {
                const user: UserState = await r.json();
                email = user.email;
                this.setState({user: user, isDirty: false});
            } else {
                console.log("Got answer:", r);
            }
        })
    }

    submit = async (e: SubmitEvent) => {
        e.preventDefault();
        const resp = await fetch(BACKEND + "/user/update_user", {
            method: "PUT",
            credentials: "include",
            headers: {
                "Content-Type": "application/json"
            },
            body: JSON.stringify(this.state.user)
        })

        if (resp.status === 200) {
            window.location.reload();
        }
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "user"});
        return (<>
            <Form onSubmit={this.submit}>
                <Form.Group className="pb-3" controlId="userId">
                    <Form.Label>{t("user_id")}</Form.Label>
                    <Form.Control type="text" disabled value={this.state.user.id} />
                </Form.Group>
                <Form.Group className="pb-3" controlId="userEmail">
                    <Form.Label>{t("email")}</Form.Label>
                    <Form.Control type="email" value={this.state.user.email} onChange={(e) => {
                        this.setState({user: {...this.state.user, email: (e.target as HTMLInputElement).value}, isDirty: true});
                    }} />
                </Form.Group>
                <Form.Group className="pb-3" controlId="userName">
                    <Form.Label>{t("name")}</Form.Label>
                    <Form.Control type="text" value={this.state.user.name} onChange={(e) => {
                        this.setState({user: {...this.state.user, name: (e.target as HTMLInputElement).value}, isDirty: true});
                    }} />
                </Form.Group>
                <Button type="submit" variant="primary" disabled={!this.state.isDirty} className="mb-3" >{t("change")}</Button>
            </Form>
        </>)
    }
}

export function User() {
    const [showPasswordReset, setShowPasswordReset] = useState(false);
    const [deleteUser, setDeleteUser] = useState({show: false, password: "", password_valid: true});
    const [currentPassword, setCurrentPassword] = useState("");
    const [currentPasswordIsValid, setCurrentPasswordIsValid] = useState(true);
    const [newPassword, setNewPassword] = useState("");
    const [newPasswordIsValid, setNewPasswordIsValid] = useState(true);
    const validated = signal(false);

    const handleUpdatePasswordClose = () => setShowPasswordReset(false);
    const handleUpdatePasswordShow = () => setShowPasswordReset(true);
    const handleDelteUserClose = () => setDeleteUser({...deleteUser, show: false});
    const handleDeleteUserShow = () => setDeleteUser({...deleteUser, show: true});

    const checkPasswords = () => {
        let ret = true;
        if (!PASSWORD_PATTERN.test(newPassword)) {
            setNewPasswordIsValid(false);
            ret = false;
        } else {
            setNewPasswordIsValid(true);
        }

        if (currentPassword.length === 0) {
            setCurrentPasswordIsValid(false);
            ret = false;
        } else {
            setCurrentPasswordIsValid(true);
        }

        validated.value = true;

        return ret;
    }

    const submitUpdatePassword = async (e: SubmitEvent) => {
        e.preventDefault();

        if (!checkPasswords()) {
            e.stopPropagation();
            return;
        }

        const secret_resp = await fetch(BACKEND + "/user/get_secret", {
            method: "GET",
            credentials: "include",
        })

        const {
            secret,
            secret_nonce,
            secret_salt
        } = await secret_resp.json();

        const secret_key = await generate_hash(currentPassword, new Uint8Array(secret_salt), sodium.crypto_secretbox_KEYBYTES);
        const decrypted_secret = sodium.crypto_secretbox_open_easy(new Uint8Array(secret), new Uint8Array(secret_nonce), secret_key);

        const salt1 = await get_salt();
        const new_secret_salt = concat_salts(salt1);
        const new_secret_key = await generate_hash(newPassword, new_secret_salt, sodium.crypto_secretbox_KEYBYTES);

        const new_secret_nonce = generate_random_bytes(sodium.crypto_secretbox_NONCEBYTES);
        const new_encrypted_secret = sodium.crypto_secretbox_easy(decrypted_secret, new_secret_nonce, new_secret_key);

        const login_salt = await get_salt_for_user(email);
        const login_key = await generate_hash(currentPassword, login_salt);

        const salt3 = await get_salt();
        const new_login_salt = concat_salts(salt3);
        const new_login_key = await generate_hash(newPassword, new_login_salt);

        const payload = {
            old_login_key: [].slice.call(login_key),
            new_login_key: [].slice.call(new_login_key),
            new_login_salt: [].slice.call(new_login_salt),
            new_secret_nonce: [].slice.call(new_secret_nonce),
            new_secret_salt: [].slice.call(new_secret_salt),
            new_encrypted_secret: [].slice.call(new Uint8Array(new_encrypted_secret)),
        };

        const resp = await fetch(BACKEND + "/user/update_password", {
            credentials: "include",
            method: "PUT",
            headers: {
                "Content-Type": "application/json"
            },
            body: JSON.stringify(payload)
        });
        if (resp.status === 200) {
            logout(true);
            handleUpdatePasswordClose();
        }
    };

    const submitDeleteUser = async (e: SubmitEvent) => {
        e.preventDefault();

        const t = i18n.t;

        const loginSaltBs64 = window.localStorage.getItem("loginKey");
        const loginSalt = Base64.toUint8Array(loginSaltBs64);
        const loginKey = await generate_hash(deleteUser.password, loginSalt)

        const resp = await fetch(BACKEND + "/user/delete", {
            credentials: "include",
            method: "DELETE",
            headers: {
                "Content-Type": "application/json"
            },
            body: JSON.stringify({login_key: [].slice.call(loginKey)})
        });

        if (resp.status === 200) {
            location.reload();
        } else  if (resp.status === 400) {
            setDeleteUser({...deleteUser, password_valid: false})
        } else {
            showAlert(`${t("alert_default_text")}: ${resp.status} ${await resp.text()}`, "danger")
            handleDelteUserClose();
        }
    }

    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "user"});

    return (<>
        <Container fluid>
            <Card className="p-3 my-3">
            <UserComponent/>
            <Button variant="primary" className="col col-sm-6 col-md-4 col-lg-3 col-xl-2 mb-3" onClick={handleUpdatePasswordShow}>
                {t("change_password")}
            </Button>

            <Button variant="primary" className="col col-sm-6 col-md-4 col-lg-3 col-xl-2 mb-3" onClick={() => logout(true)}>
                {t("logout_all")}
            </Button>
            <Button variant="danger" className="col col-sm-6 col-md-4 col-lg-3 col-xl-2" onClick={handleDeleteUserShow}>
                {t("delete_user")}
            </Button>
            </Card>
        </Container>

        {/* Delete user modal */}
        <Modal show={deleteUser.show} onHide={handleDelteUserClose} centered>
            <Form onSubmit={submitDeleteUser} validated={validated.value} noValidate>
                <Modal.Header>
                    <Modal.Title>
                        {t("delete_user")}
                    </Modal.Title>
                </Modal.Header>
                <Modal.Body>
                    <Form.Group className="pb-3" controlId="deleteUserPassword">
                        <Form.Label>{t("password")}</Form.Label>
                        <PasswordComponent onChange={(e) => setDeleteUser({...deleteUser, password: (e.target as HTMLInputElement).value})} isInvalid={!deleteUser.password_valid} invalidMessage={t("password_invalid")} />
                    </Form.Group>
                </Modal.Body>
                <Modal.Footer>
                    <Button variant="secondary" onClick={handleDelteUserClose}>
                        {t("close")}
                    </Button>
                    <Button variant="danger" type="submit">
                        {t("delete_user")}
                    </Button>
                </Modal.Footer>
            </Form>
        </Modal>

        {/* Reset password modal */}
        <Modal show={showPasswordReset} onHide={handleUpdatePasswordClose} centered>
            <Form onSubmit={submitUpdatePassword} validated={validated.value} noValidate>
                <Modal.Header>
                    <Modal.Title>
                        {t("change_password")}
                    </Modal.Title>
                </Modal.Header>
                <Modal.Body>
                    <Form.Group className="pb-3" controlId="oldPassword">
                        <Form.Label>{t("current_password")}</Form.Label>
                        <PasswordComponent isInvalid={!currentPasswordIsValid} onChange={(e) => {
                            setCurrentPassword((e.target as HTMLInputElement).value);
                        }} />
                    </Form.Group>
                    <Form.Group className="pb-3" controlId="newPassword">
                        <Form.Label>{t("new_password")}</Form.Label>
                        <PasswordComponent onChange={(e) => setNewPassword((e.target as HTMLInputElement).value)} isInvalid={!newPasswordIsValid} invalidMessage={t("new_password_error_message")} />
                    </Form.Group>
                </Modal.Body>
                <Modal.Footer>
                    <Button variant="secondary" onClick={handleUpdatePasswordClose}>
                        {t("close")}
                    </Button>
                    <Button variant="primary" type="submit">
                        {t("change_password")}
                    </Button>
                </Modal.Footer>
            </Form>
        </Modal>
    </>)
}
