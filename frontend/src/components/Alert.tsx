import { signal } from "@preact/signals";
import Alert from "react-bootstrap/Alert";

const state = signal({
    text: "",
    show: false,
    variant: "",
    heading: "",
});

export function showAlert(text: string, variant: string, heading?: string) {
    state.value = {
        text: text,
        show: true,
        variant: variant,
        heading: heading ? heading : "An Error occured!",
    }
}

export function ErrorAlert() {
    return <Alert variant={state.value.variant} onClose={(a, b) => {
        state.value = {
            text: "",
            show: false,
            variant: "",
            heading: "",
        }
    }} show={state.value.show} id="errorAlert" dismissible>
        <Alert.Heading>{state.value.heading}</Alert.Heading>
        <p>{state.value.text}</p>
    </Alert>
}
