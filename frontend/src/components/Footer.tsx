import { Col, Container, Row } from "react-bootstrap";
import { useTranslation } from "react-i18next";
import { privacy_notice, terms_of_use, imprint } from "links";
import { signal } from "@preact/signals";

export const connectedDeviceNote = signal("");

export function Footer() {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "footer" });
    const note = connectedDeviceNote.value.trim().split(/\r?\n/, 1)[0];
    return (
        <Container id="footer" fluid className="footer">
            <Row className="align-items-center">
                {note && <Col xs={12} md className="footer-device-note text-white my-1">{note}</Col>}
                <Col xs={12} md="auto" className="text-end d-flex flex-wrap justify-content-end ms-auto">
                    <a target="__blank" href={privacy_notice} class="mx-2 my-1">
                        {t("privacy_notice")}
                    </a>
                    <a target="__blank" href={terms_of_use} class="mx-2 my-1">
                        {t("terms_of_use")}
                    </a>
                    <a target="__blank" href={imprint} class="mx-2 my-1">
                        {t("imprint")}
                    </a>
                </Col>
            </Row>
        </Container>
    );
}
