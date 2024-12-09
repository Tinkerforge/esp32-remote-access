import { signal } from "@preact/signals";
import Alert from "react-bootstrap/Alert";
import i18n from "../i18n";

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
        heading: heading ? heading : i18n.t("alert_default_text"),
    }
    window.scrollTo(0,0);
}

export function ErrorAlert() {
    return <Alert className="custom-alert" variant={state.value.variant} onClose={(a, b) => {
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
