import { useTranslation } from "react-i18next";
import { Button, Form, Modal } from "react-bootstrap";

interface EditNoteModalProps {
    show: boolean;
    note: string;
    onNoteChange: (note: string) => void;
    onSubmit: (e: Event) => Promise<void>;
    onCancel: () => void;
}

export function EditNoteModal({ show, note, onNoteChange, onSubmit, onCancel }: EditNoteModalProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });

    return (
        <Modal
            show={show}
            centered
            onHide={onCancel}
        >
            <Form onSubmit={onSubmit}>
                <Modal.Header>
                    {t("edit_note_heading")}
                </Modal.Header>
                <Modal.Body>
                    <Form.Control
                        as="textarea"
                        value={note}
                        onChange={(e) => onNoteChange((e.target as HTMLInputElement).value)}
                    />
                </Modal.Body>
                <Modal.Footer>
                    <Button variant="secondary" onClick={onCancel}>
                        {t("decline")}
                    </Button>
                    <Button type="submit">
                        {t("accept")}
                    </Button>
                </Modal.Footer>
            </Form>
        </Modal>
    );
}
