import { Col, Container, Row } from "react-bootstrap";
import { useTranslation } from "react-i18next";


export function Footer() {
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "footer"});
    return <Container id="footer" fluid className="fixed-bottom bg-light">
            <Row>
                <Col fluid className="text-end float-end">
                    <a target="__blank" href="https://www.tinkerforge.com/de/home/privacy_notice" class="m-2">{t("privacy_notice")}</a>
                    <a target="__blank" href="https://www.tinkerforge.com/de/home/legal_info" class="m-2">{t("terms_of_use")}</a>
                    <a target="__blank" href="https://www.tinkerforge.com/de/home/terms_and_conditions" class="m-2">{t("imprint")}</a>
                </Col>
            </Row>
        </Container>
}
