import { Component } from "preact";
import Form from "react-bootstrap/Form"
import Button from "react-bootstrap/Button";
import Modal from "react-bootstrap/Modal";
import { useState } from "preact/hooks";


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

        fetch("http://localhost:8081/user/me", {
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
        const resp = await fetch("http://localhost:8081/user/update_user", {
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

        const resp = await fetch("http://localhost:8081/user/update_password", {
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
