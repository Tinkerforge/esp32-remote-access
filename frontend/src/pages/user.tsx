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
import { fetchClient } from "../utils";
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
    email: string,
    has_old_charger: boolean,
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
            has_old_charger: false,
        };

        this.state = {
            isDirty: false,
            user: state,
        }

        fetchClient.GET("/user/me", {credentials: "same-origin"}).then(({data, error, response}) => {
            if (data) {
                this.setState({user: data});
            } else if (error) {
                showAlert(i18n.t("user.get_user_failed", {status: response.status, response: error}), "danger");
            }
        });
    }

    submit = async (e: SubmitEvent) => {
        e.preventDefault();
        const {response, error} = await fetchClient.PUT("/user/update_user", {body: this.state.user, credentials: "same-origin"});
        if (response.status === 200) {
            window.location.reload();
        } else if (error) {
            showAlert(i18n.t("user.update_user_failed", {status: response.status, response: error}), "danger");
        }
    }

    render() {
        const {t} = useTranslation("", {useSuspense: false, keyPrefix: "user"});
        return (<>
            <Form onSubmit={this.submit}>
                <Form.Group className="pb-3" controlId="userId">
                    <Form.Label className="text-muted">{t("user_id")}</Form.Label>
                    <Form.Control type="text" disabled value={this.state.user.id} className="bg-light" />
                </Form.Group>
                <Form.Group className="pb-3" controlId="userEmail">
                    <Form.Label className="text-muted">{t("email")}</Form.Label>
                    <Form.Control type="email" value={this.state.user.email} onChange={(e) => {
                        this.setState({user: {...this.state.user, email: (e.target as HTMLInputElement).value}, isDirty: true});
                    }} disabled={this.state.user.has_old_charger} />
                    {this.state.user.has_old_charger &&
                        <Form.Text className="text-danger">
                            {t("email_change_disabled")}
                        </Form.Text>
                    }
                </Form.Group>
                <Form.Group className="pb-3" controlId="userName">
                    <Form.Label className="text-muted">{t("name")}</Form.Label>
                    <Form.Control type="text" value={this.state.user.name} onChange={(e) => {
                        this.setState({user: {...this.state.user, name: (e.target as HTMLInputElement).value}, isDirty: true});
                    }} />
                </Form.Group>
                <Button type="submit" variant="primary" disabled={!this.state.isDirty}>
                    {t("save_changes")}
                </Button>
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

        const {data, error, response} = await fetchClient.GET("/user/get_secret", {credentials: "same-origin"});
        if (error) {
            showAlert(i18n.t("user.update_password_failed", {status: response.status, response: error}), "danger");
            return;
        }

        const {
            secret,
            secret_nonce,
            secret_salt
        } = data;

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

        {
            const {response, error} = await fetchClient.PUT("/user/update_password", {body: payload, credentials: "same-origin"});
            if (response.status === 200) {
                logout(true);
                handleUpdatePasswordClose();
            } else {
                showAlert(i18n.t("user.update_password_failed", {status: response.status, response: error}), "danger");
            }
        }
    };

    const submitDeleteUser = async (e: SubmitEvent) => {
        e.preventDefault();

        const t = i18n.t;

        const loginSaltBs64 = window.localStorage.getItem("loginSalt");
        const loginSalt = Base64.toUint8Array(loginSaltBs64);
        const loginKey = await generate_hash(deleteUser.password, loginSalt)

        const {response, error} = await fetchClient.DELETE("/user/delete", {
            credentials: "include",
            body: {
                login_key: [].slice.call(loginKey)
            }
        });

        if (response.status === 200) {
            location.reload();
        } else  if (response.status === 400) {
            setDeleteUser({...deleteUser, password_valid: false})
        } else {
            showAlert(i18n.t("delete_user", {status: response.status, response: error}), "danger")
            handleDelteUserClose();
        }
    }

    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "user"});

    return (<>
        <Container>
            <Card className="my-4">
                <Card.Header className="pb-2">
                    <h5 className="mb-0">{t("profile_information")}</h5>
                </Card.Header>
                <Card.Body>
                    <UserComponent/>
                </Card.Body>
                <Card.Header className="border-top pb-2">
                    <h5 className="mb-0">{t("account_actions")}</h5>
                </Card.Header>
                <Card.Body className="pt-3">
                    <div className="d-flex flex-wrap gap-3">
                        <Button variant="outline-primary" onClick={handleUpdatePasswordShow}>
                            {t("change_password")}
                        </Button>
                        <Button variant="outline-warning" onClick={() => logout(true)}>
                            {t("logout_all")}
                        </Button>
                        <Button variant="outline-danger" onClick={handleDeleteUserShow}>
                            {t("delete_user")}
                        </Button>
                    </div>
                </Card.Body>
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
                    <Button variant="outline-secondary" onClick={handleDelteUserClose}>
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
                    <Button variant="outline-secondary" onClick={handleUpdatePasswordClose}>
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
