import { Col, Container, Row } from "react-bootstrap";
import { useTranslation } from "react-i18next";
import { privacy_notice, terms_of_use, imprint } from "links";

export function Footer() {
  const { t } = useTranslation("", { useSuspense: false, keyPrefix: "footer" });
  return (
    <Container id="footer" fluid className="footer">
      <Row className="align-items-center">
        <Col className="text-end d-flex flex-wrap justify-content-end">
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
