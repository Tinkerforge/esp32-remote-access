import { Col, Container, Row } from "react-bootstrap";
import { useTranslation } from "react-i18next";
import { privacy_notice, terms_of_use, imprint } from "links";
import { MobileAppHint } from "./MobileAppHint";

export function Footer() {
  const { t } = useTranslation("", { useSuspense: false, keyPrefix: "footer" });
  return (
    <Container id="footer" fluid className="footer">
      <Row className="align-items-center">
        <Col className="text-start">
          <MobileAppHint compact />
        </Col>
        <Col className="text-end">
          <a target="__blank" href={privacy_notice} class="m-2">
            {t("privacy_notice")}
          </a>
          <a target="__blank" href={terms_of_use} class="m-2">
            {t("terms_of_use")}
          </a>
          <a target="__blank" href={imprint} class="m-2">
            {t("imprint")}
          </a>
        </Col>
      </Row>
    </Container>
  );
}
