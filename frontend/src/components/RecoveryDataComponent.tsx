import { Signal, useSignal } from "@preact/signals";
import { Base64 } from "js-base64";
import { Button, Modal, Form } from "react-bootstrap";
import { useTranslation } from "react-i18next";

interface RecoveryDataProps {
    email: string,
    secret: Uint8Array,
    show: Signal<boolean>,
}

export async function saveRecoveryData(secret: Uint8Array, email: string) {
    const secret_b64 = Base64.fromUint8Array(secret);
    const hash = await window.crypto.subtle.digest("SHA-256", new TextEncoder().encode(email + secret_b64));
    const backupData = {
        email,
        secret: secret_b64,
        hash: Base64.fromUint8Array(new Uint8Array(hash)),
    };

    const backupString = JSON.stringify(backupData);
    const file = new File([backupString], "RecoveryData", {
        type: "text/plain"
    });
    const a = document.createElement("a");
    const url = URL.createObjectURL(file);
    a.href = url;
    a.target = "_blank";
    a.download = `${email.replaceAll(".", "_").replaceAll("@", "_at_")}_my_warp_charger_com_recovery_data`;
    document.body.appendChild(a);
    a.click()
    URL.revokeObjectURL(url);
    a.remove();
}

export function RecoveryDataComponent(props: RecoveryDataProps) {
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "register"});
    const saved = useSignal(false);
    const confirmed = useSignal(false);

    return <Modal show={props.show.value} onHide={() => {
                    // Only allow closing if user has saved and confirmed
                    if (saved.value && confirmed.value) {
                        props.show.value = false;
                        window.location.replace("/");
                    }
                }}>
            <Modal.Header closeButton={saved.value && confirmed.value}>
                <Modal.Title>{t("save_recovery_data")}</Modal.Title>
            </Modal.Header>

            <Modal.Body>
                <p className="mb-3">{t("save_recovery_data_text")}</p>
                <div className="mb-3">
                    <Button 
                        variant="primary" 
                        size="lg"
                        className="w-100"
                        onClick={() => {
                            saveRecoveryData(props.secret, props.email);
                            saved.value = true;
                        }}>
                        {t("save")}
                    </Button>
                </div>
                {saved.value && (
                    <Form.Check
                        type="checkbox"
                        id="recovery-confirmation"
                        label={t("save_recovery_data_confirmation")}
                        checked={confirmed.value}
                        onChange={(e: any) => { confirmed.value = e.target.checked; }}
                        className="mt-3"
                    />
                )}
            </Modal.Body>

            <Modal.Footer>
                <Button 
                    variant={saved.value && confirmed.value ? "primary" : "secondary"} 
                    disabled={!saved.value || !confirmed.value}
                    onClick={() => {
                        if (saved.value && confirmed.value) {
                            props.show.value = false;
                        }
                    }}>
                    {t("close")}
                </Button>
            </Modal.Footer>
    </Modal>
}
