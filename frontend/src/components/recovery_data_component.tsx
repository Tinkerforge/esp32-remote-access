import { Signal, useSignal } from "@preact/signals";
import { Base64 } from "js-base64";
import { Button, Modal } from "react-bootstrap";
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
        email: email,
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

    return <Modal show={props.show.value} onHide={() => {
                    props.show.value = false;
                    window.location.replace("/");
                }}>
            <Modal.Header closeButton>
                <Modal.Title>{t("save_recovery_data")}</Modal.Title>
            </Modal.Header>

            <Modal.Body>
                <p className="mb-3">{t("save_recovery_data_text")}</p>
                <Button variant="primary" onClick={() => {
                    saveRecoveryData(props.secret, props.email);
                    saved.value = true;
                }}>{t("save")}</Button>
            </Modal.Body>

            <Modal.Footer>
                <Button variant={saved.value ? "primary" : "danger"} onClick={() => {
                    props.show.value = false;
                    window.location.replace("/");
                }}>{t("close")}</Button>
            </Modal.Footer>
    </Modal>
}
