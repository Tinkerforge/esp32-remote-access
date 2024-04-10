import { signal } from "@preact/signals";
import Alert from "react-bootstrap/Alert";

const state = signal({
    text: "",
    show: false,
    variant: ""
});

export function showAlert(text: string, variant: string) {
    state.value = {
        text: text,
        show: true,
        variant: variant,
    }
}

export function ErrorAlert() {
    return <Alert variant={state.value.variant} onClose={(a, b) => {
        state.value = {
            text: "",
            show: false,
            variant: "",
        }
    }} show={state.value.show} id="errorAlert" dismissible>
        <Alert.Heading>An Error occured!</Alert.Heading>
        <p>{state.value.text}</p>
    </Alert>
}
