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
import { BACKEND } from "../types";


interface UserState {
    id: string,
    name: string,
    email: string
}

interface State {
    isDirty: boolean
    user: UserState
}

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
        return (<>
            <Form onSubmit={this.submit}>
                <Form.Group className="pb-3" controlId="userId">
                    <Form.Label>User Id</Form.Label>
                    <Form.Control type="text" disabled value={this.state.user.id} />
                </Form.Group>
                <Form.Group className="pb-3" controlId="userEmail">
                    <Form.Label>Email</Form.Label>
                    <Form.Control type="email" value={this.state.user.email} onChange={(e) => {
                        this.setState({user: {...this.state.user, email: (e.target as HTMLInputElement).value}, isDirty: true});
                    }} />
                </Form.Group>
                <Form.Group className="pb-3" controlId="userName">
                    <Form.Label>Name</Form.Label>
                    <Form.Control type="text" value={this.state.user.name} onChange={(e) => {
                        this.setState({user: {...this.state.user, name: (e.target as HTMLInputElement).value}, isDirty: true});
                    }} />
                </Form.Group>
                <Button type="submit" variant="primary" disabled={!this.state.isDirty} className="mb-3" >Update</Button>
            </Form>
        </>)
    }
}

export function User() {
    const [show, setShow] = useState(false);
    const [currentPassword, setCurrentPassword] = useState("");
    const [newPassword, setNewPassword] = useState("");

    const handleClose = () => setShow(false);
    const handleShow = () => setShow(true);

    const submit = async (e: SubmitEvent) => {
        e.preventDefault();

        const payload = {
            old_pass: currentPassword,
            new_pass: newPassword
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
            handleClose();
        }
    };

    return (<>
        <UserComponent/>
        <Button variant="primary" onClick={handleShow}>
            Change Password
        </Button>

        <Modal show={show} onHide={handleClose}>
            <Form onSubmit={submit} >
                <Modal.Header>
                    <Modal.Title>
                        Change password
                    </Modal.Title>
                </Modal.Header>
                <Modal.Body>
                    <Form.Group className="pb-3" controlId="oldPassword">
                        <Form.Label>Current Password</Form.Label>
                        <Form.Control type="password" value={currentPassword} onChange={(e) => setCurrentPassword((e.target as HTMLInputElement).value)} />
                    </Form.Group>
                    <Form.Group className="pb-3" controlId="newPassword">
                        <Form.Label>New Password</Form.Label>
                        <Form.Control type="password" value={newPassword} onChange={(e) => setNewPassword((e.target as HTMLInputElement).value)} />
                    </Form.Group>
                </Modal.Body>
                <Modal.Footer>
                    <Button variant="secondary" onClick={handleClose}>
                        Close
                    </Button>
                    <Button variant="primary" type="submit">
                        Change Password
                    </Button>
                </Modal.Footer>
            </Form>
        </Modal>
    </>)
}
