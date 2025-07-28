import { useTranslation } from "react-i18next";
import { Button, Modal } from "react-bootstrap";
import { StateDevice } from "./types";

interface DeleteDeviceModalProps {
    show: boolean;
    device: StateDevice;
    onConfirm: () => Promise<void>;
    onCancel: () => void;
}

export function DeleteDeviceModal({ show, device, onConfirm, onCancel }: DeleteDeviceModalProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });

    return (
        <Modal show={show} centered onHide={onCancel}>
            <Modal.Header id="delete-modal-header">
                {t("delete_modal_heading", { name: device.name })}
            </Modal.Header>
            <Modal.Body id="delete-modal-body">
                {t("delete_modal_body", { name: device.name })}
            </Modal.Body>
            <Modal.Footer>
                <Button
                    id="delete-confirm-button"
                    variant="danger"
                    onClick={async () => {
                        await onConfirm();
                    }}
                >
                    {t("remove")}
                </Button>
                <Button
                    id="delete-cancel-button"
                    variant="secondary"
                    onClick={onCancel}
                >
                    {t("close")}
                </Button>
            </Modal.Footer>
        </Modal>
    );
}
