import { useState } from "preact/hooks";
import { useTranslation } from "react-i18next";
import { Button, Col, Collapse, Container, Row } from "react-bootstrap";
import { Edit } from "react-feather";
import * as Base58 from "base58";
import { Circle } from "../Circle";
import { StateDevice } from "./types";

interface DeviceTableRowProps {
    device: StateDevice;
    index: number;
    onConnect: (device: StateDevice) => Promise<void>;
    onDelete: (device: StateDevice) => void;
    onEditNote: (device: StateDevice, index: number) => void;
    connectionPossible: (device: StateDevice) => boolean;
    formatLastStateChange: (t: (key: string, options?: any) => string, timestamp?: number | null) => string;
}

export function DeviceTableRow({
    device,
    index,
    onConnect,
    onDelete,
    onEditNote,
    connectionPossible,
    formatLastStateChange
}: DeviceTableRowProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
    const [expand, setExpand] = useState(false);

    const trimmed_note = device.note.trim();
    const split = trimmed_note.split("\n");

    return (
        <tr>
            <td class="align-middle text-center">
                <Col className="d-flex justify-content-center align-items-center">
                    {device.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}
                </Col>
            </td>
            <td class="align-middle">
                {device.name}
            </td>
            <td class="align-middle">
                {Base58.int_to_base58(device.uid)}
            </td>
            <td class="align-middle text-center">
                <div className="d-flex flex-row flex-md-wrap flex-lg-nowrap justify-content-center gap-2">
                    <Button
                        disabled={!connectionPossible(device)}
                        id={`connect-${device.name}`}
                        onClick={async () => {
                            await onConnect(device);
                        }}
                        variant="primary"
                        className="w-100"
                    >
                        {t("connect")}
                    </Button>
                    <Button
                        onClick={async () => {
                            onDelete(device);
                        }}
                        variant="danger"
                        className="w-100"
                    >
                        {t("remove")}
                    </Button>
                </div>
                <p style="color:red;" hidden={device.valid}>
                    {t("no_keys")}
                </p>
            </td>
            <td class="align-middle">
                {formatLastStateChange(t, device.last_state_change)}
            </td>
            <td class="align-middle pe-0">
                <Container fluid className="p-0">
                    <Row className="m-0">
                        <Col
                            className="d-flex align-items-center p-0"
                        >
                            <Container
                                className="ps-0"
                                onClick={split.length <= 2 ? undefined : () => setExpand(!expand)}
                                style={{cursor: split.length <= 1 ? undefined : "pointer"}}
                            >
                                <Row>
                                    <Col>
                                        <div>
                                            {split.slice(0, split.length <= 2 ? 2 : 1).join("\n")}
                                        </div>
                                    </Col>
                                </Row>
                                <Row>
                                    <Col>
                                        <Collapse in={expand}>
                                            <div style={{whiteSpace: "pre-wrap"}}>
                                                {split.slice(1).join("\n")}
                                            </div>
                                        </Collapse>
                                    </Col>
                                </Row>
                                <Row hidden={split.length <= 2}>
                                    <Col>
                                        <a style={{fontSize: "14px", color: "blue", textDecoration: "underline"}}>
                                            {expand ? t("show_less") : t("show_more")}
                                        </a>
                                    </Col>
                                </Row>
                            </Container>
                        </Col>
                        <Col className="p-0" sm="auto">
                            <Button
                                style="background-color:transparent;border:none;"
                                onClick={() => {
                                    onEditNote(device, index);
                                    setExpand(false);
                                }}
                            >
                                <Edit color="#333"/>
                            </Button>
                        </Col>
                    </Row>
                </Container>
            </td>
            <td class="align-middle" style={{width: "1px", whiteSpace: "nowrap", padding: "0.5rem 0.25rem"}}>
                {device.firmware_version}
            </td>
        </tr>
    );
}
