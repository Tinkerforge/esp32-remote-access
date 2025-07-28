import { useState } from "preact/hooks";
import { useTranslation } from "react-i18next";
import { Button, Card, Col, Collapse, Row } from "react-bootstrap";
import { Edit, Monitor, Trash2 } from "react-feather";
import * as Base58 from "base58";
import { Circle } from "../Circle";
import { StateDevice } from "./types";

interface DeviceCardProps {
    device: StateDevice;
    index: number;
    onConnect: (device: StateDevice) => Promise<void>;
    onDelete: (device: StateDevice) => void;
    onEditNote: (device: StateDevice, index: number) => void;
    connectionPossible: (device: StateDevice) => boolean;
    formatLastStateChange: (t: (key: string, options?: any) => string, timestamp?: number | null) => string;
}

export function DeviceCard({
    device,
    index,
    onConnect,
    onDelete,
    onEditNote,
    connectionPossible,
    formatLastStateChange
}: DeviceCardProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
    const [expand, setExpand] = useState(false);

    const trimmed_note = device.note.trim();
    const split = trimmed_note.split("\n");

    return (
        <Card className="my-2" id={`device-card-${device.name}`}>
            <Card.Header
                id={`device-header-${device.name}`}
                onClick={async () => {
                    if (!connectionPossible(device)) {
                        return;
                    }
                    await onConnect(device);
                }}
                className="d-flex justify-content-between align-items-center p-2d5"
            >
                <Col xs="auto" className="d-flex">
                    {device.status === "Disconnected" ? <Circle color="danger"/> : <Circle color="success"/>}
                </Col>
                <Col className="mx-3">
                    <h5 class="text-break" style="margin-bottom: 0;" id={`device-name-${device.name}`}>{device.name}</h5>
                </Col>
                <Col className="d-flex justify-content-end">
                    <Button
                        id={`connect-${device.name}`}
                        className="me-2"
                        variant="primary"
                        disabled={!connectionPossible(device)}
                        onClick={async () => {
                            await onConnect(device);
                        }}
                    >
                        <Monitor/>
                    </Button>
                    <Button
                        id={`delete-${device.name}`}
                        variant="danger"
                        onClick={async (e) => {
                            e.stopPropagation();
                            onDelete(device);
                        }}
                    >
                        <Trash2/>
                    </Button>
                </Col>
            </Card.Header>
            <Card.Body>
                <Row>
                    <Col xs="auto" id={`charger-id-label-${device.name}`}><b>{t("mobile_charger_id")}</b></Col>
                    <Col className="text-end" id={`charger-id-value-${device.name}`}>{Base58.int_to_base58(device.uid)}</Col>
                </Row>
                <hr style="margin-top: 5px;margin-bottom: 5px;"/>
                <Row>
                    <Col xs="auto" id={`last-state-label-${device.name}`}><b>{t("last_state_change")}</b></Col>
                    <Col className="text-end" id={`last-state-value-${device.name}`}>{formatLastStateChange(t, device.last_state_change)}</Col>
                </Row>
                <hr style="margin-top: 5px;margin-bottom: 5px;"/>
                <Row>
                    <Col xs="auto" id={`firmware-label-${device.name}`}><b>{t("firmware_version")}</b></Col>
                    <Col className="text-end" id={`firmware-value-${device.name}`}>{device.firmware_version}</Col>
                </Row>
                <hr style="margin-top: 5px;margin-bottom: 5px;"/>
                <Row>
                    <Col xs="auto">
                        <Row>
                            <b id={`note-label-${device.name}`}>{t("note")}</b>
                        </Row>
                        <Row>
                            <Col className="p-0">
                                <Button
                                    id={`edit-${device.name}`}
                                    style="background-color:transparent;border:none;"
                                    onClick={() => {
                                        onEditNote(device, index);
                                    }}
                                >
                                    <Edit color="#333"/>
                                </Button>
                            </Col>
                        </Row>
                    </Col>
                    <Col
                        id={`note-content-col-${device.name}`}
                        onClick={split.length <= 3 ? undefined : () => setExpand(!expand)}
                        style={{cursor: split.length <= 3 ? undefined : "pointer", whiteSpace: "pre-line", overflowWrap: "anywhere"}}
                    >
                        <Row>
                            <Col className="d-flex justify-content-end" style={{textAlign: "right"}}>
                                <div id={`note-preview-text-${device.name}`}>
                                    {split.slice(0, split.length <= 3 ? 3 : 2).join("\n")}
                                </div>
                            </Col>
                        </Row>
                        <Row>
                            <Col className="d-flex justify-content-end" style={{textAlign: "right"}}>
                                <Collapse in={expand}>
                                    <div id={`note-expanded-text-${device.name}`}>
                                        {split.slice(2).join("\n")}
                                    </div>
                                </Collapse>
                            </Col>
                        </Row>
                        <Row hidden={split.length <= 3}>
                            <Col className="d-flex justify-content-end">
                                <a style={{fontSize: "14px", color: "blue", textDecoration: "underline"}} id={`note-toggle-link-${device.name}`}>
                                    {expand ? t("show_less") : t("show_more")}
                                </a>
                            </Col>
                        </Row>
                    </Col>
                </Row>
                <p style="color:red;" hidden={device.valid} id={`no-keys-warning-${device.name}`}>{t("no_keys")}</p>
            </Card.Body>
        </Card>
    );
}
