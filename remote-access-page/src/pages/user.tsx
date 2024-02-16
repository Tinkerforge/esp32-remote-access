import { Component } from "preact";
import Form from "react-bootstrap/Form"
import Button from "react-bootstrap/Button";


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

    render() {
        return (<>
            <Form>
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
                <Button type="submit" variant="primary" disabled={!this.state.isDirty} >Update</Button>
            </Form>
        </>)
    }
}

export function User() {
    return (<UserComponent/>)
}
