import { signal } from "@preact/signals";
import Alert from "react-bootstrap/Alert";
import i18n from "../i18n";

interface AlertItem {
    id: string;
    text: string;
    variant: string;
    heading: string;
    timeoutId?: number;
}

const alerts = signal<AlertItem[]>([]);

const clearAlertTimeout = (id: string) => {
    const alert = alerts.value.find(a => a.id === id);
    if (alert?.timeoutId) {
        window.clearTimeout(alert.timeoutId);
    }
};

export async function showAlert(text: string, variant: "danger" | "success" | "warning", id?: string, heading?: string, timeout_ms?: number) {
    if (text.indexOf("Failed to fetch") !== -1) {
        console.warn("Alert suppressed due to 'Failed to fetch' message");
        return;
    }

    const hash = await crypto.subtle.digest("SHA-1", new TextEncoder().encode(text + variant + (heading || ""))).then(buf => {
        return new TextDecoder().decode(buf);
    });
    id = id ? id : hash;
    const alert: AlertItem = {
        id,
        text,
        variant,
        heading: heading || i18n.t("alert_default_text"),
    };

    alerts.value = alerts.value.filter(a => {
        if (a.id === id) {
            clearTimeout(a.timeoutId);
            return false;
        }
        return true;
    });

    if (timeout_ms) {
        alert.timeoutId = window.setTimeout(() => {
            alerts.value = alerts.value.filter(a => a.id !== id);
        }, timeout_ms);
    }

    alerts.value = [...alerts.value, alert];
    window.scrollTo(0, 0);
}

export function ErrorAlert() {
    return <div className="alert-container m-2">
        {alerts.value.map(alert => (
            <Alert
                key={alert.id}
                className="custom-alert"
                variant={alert.variant}
                onClose={() => {
                    clearAlertTimeout(alert.id);
                    alerts.value = alerts.value.filter(a => a.id !== alert.id);
                }}
                dismissible
            >
                <Alert.Heading>{alert.heading}</Alert.Heading>
                <p className="mb-0">{alert.text}</p>
            </Alert>
        ))}
    </div>;
}
