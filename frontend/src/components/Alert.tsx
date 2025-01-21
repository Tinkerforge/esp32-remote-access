import { signal } from "@preact/signals";
import Alert from "react-bootstrap/Alert";
import i18n from "../i18n";

const state = signal({
    text: "",
    show: false,
    variant: "",
    heading: "",
});

let alertTimeout: number | undefined;

const clearAlertTimeout = () => {
    if (alertTimeout) {
        window.clearTimeout(alertTimeout);
        alertTimeout = undefined;
    }
};

export function showAlert(text: string, variant: string, heading?: string, timeout_ms?: number) {
    clearAlertTimeout();

    state.value = {
        text: text,
        show: true,
        variant: variant,
        heading: heading ? heading : i18n.t("alert_default_text"),
    }
    window.scrollTo(0,0);

    if (timeout_ms) {
        alertTimeout = window.setTimeout(() => {
            state.value = {
                text: "",
                show: false,
                variant: "",
                heading: "",

            };
        }, timeout_ms);
    }
}

export function ErrorAlert() {
    return <Alert className="custom-alert small" variant={state.value.variant} onClose={() => {
        clearAlertTimeout();
        state.value = {
            text: "",
            show: false,
            variant: "",
            heading: "",
        }
    }} show={state.value.show} id="errorAlert" dismissible>
        <Alert.Heading>{state.value.heading}</Alert.Heading>
        <p className="mb-0">{state.value.text}</p>
    </Alert>
}
