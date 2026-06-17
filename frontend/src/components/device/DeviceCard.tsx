import { useEffect, useRef, useState } from "preact/hooks";
import { useTranslation } from "react-i18next";
import { Badge, Button, Card, Col, Collapse, Row } from "react-bootstrap";
import { Edit, Monitor, Trash2 } from "react-feather";
import * as Base58 from "base58";
import { Circle } from "../Circle";
import { StateDevice, Grouping, ConnectVia } from "./types";

interface DeviceCardProps {
    device: StateDevice;
    onConnect: (device: StateDevice, via?: Exclude<ConnectVia, "default">) => Promise<void>;
    onDelete: (device: StateDevice) => void;
    onEditNote: (device: StateDevice) => void;
    connectionPossible: (device: StateDevice) => boolean;
    formatLastStateChange: (t: (key: string, options?: Record<string, unknown>) => string, timestamp?: number | null) => string;
    groupings: Grouping[];
}

export function DeviceCard({
    device,
    onConnect,
    onDelete,
    onEditNote,
    connectionPossible,
    formatLastStateChange,
    groupings
}: DeviceCardProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
    const [expand, setExpand] = useState(false);
    const [connectMenuOpen, setConnectMenuOpen] = useState(false);
    const connectGroupRef = useRef<HTMLDivElement>(null);

    // Close the connect-method dropdown when the user clicks anywhere
    // outside of the button group that hosts it.
    useEffect(() => {
        if (!connectMenuOpen) return;
        const handleMouseDown = (e: MouseEvent) => {
            if (connectGroupRef.current && !connectGroupRef.current.contains(e.target as Node)) {
                setConnectMenuOpen(false);
            }
        };
        document.addEventListener("mousedown", handleMouseDown);
        return () => document.removeEventListener("mousedown", handleMouseDown);
    }, [connectMenuOpen]);

    const trimmed_note = device.note.trim();
    const split = trimmed_note.split("\n");

    // Find which groupings this device belongs to
    const deviceGroupings = groupings.filter(g => g.device_ids.includes(device.id));

    const isLocalOnly = device.id === "";
    // The split dropdown only appears for devices that are both locally
    // reachable (host is set) *and* cloud-paired (id is non-empty).
    const showConnectMenu = !!device.host && !isLocalOnly;

    const connectDisabled = !connectionPossible(device);

    return (
        <Card className="my-2">
            <Card.Header
                onClick={async () => {
                    if (!connectionPossible(device)) {
                        return;
                    }
                    await onConnect(device);
                }}
                className="d-flex justify-content-between align-items-center p-2d5"
            >
                <Col xs="auto" className="d-flex">
                    {device.status === "Disconnected" ? <Circle color="danger" /> : <Circle color="success" />}
                </Col>
                <Col className="mx-3">
                    <h5 class="text-break" style="margin-bottom: 0;">
                        {device.name}
                        {device.host && (
                            <Badge bg="warning" text="dark" className="ms-2" style={{ fontSize: "0.7rem" }}>
                                {t("local")}
                            </Badge>
                        )}
                    </h5>
                    {deviceGroupings.length > 0 && (
                        <div className="mt-1">
                            {deviceGroupings.map(g => (
                                <Badge key={g.id} bg="secondary" className="me-1" style={{ fontSize: "0.7rem" }}>
                                    {g.name}
                                </Badge>
                            ))}
                        </div>
                    )}
                </Col>
                <Col className="d-flex justify-content-end">
                    {showConnectMenu ? (
                        <div ref={connectGroupRef} className="btn-group me-2" role="group">
                            <Button
                                variant="primary"
                                disabled={connectDisabled}
                                onClick={async (e) => {
                                    e.stopPropagation();
                                    await onConnect(device);
                                }}
                            >
                                <Monitor />
                            </Button>
                            <button
                                type="button"
                                id={`connect-dropdown-${device.name}`}
                                disabled={connectDisabled}
                                aria-expanded={connectMenuOpen}
                                aria-haspopup="menu"
                                className="btn btn-primary dropdown-toggle dropdown-toggle-split"
                                onClick={(e) => {
                                    e.stopPropagation();
                                    setConnectMenuOpen((open) => !open);
                                }}
                            />
                            {connectMenuOpen && (
                                <ul
                                    className="dropdown-menu show"
                                    data-bs-popper="static"
                                    style={{ right: 0, left: "auto" }}
                                >
                                    <li>
                                        <button
                                            type="button"
                                            className="dropdown-item"
                                            onClick={async (e) => {
                                                e.stopPropagation();
                                                setConnectMenuOpen(false);
                                                await onConnect(device, "local");
                                            }}
                                        >
                                            {t("connect_locally")}
                                        </button>
                                    </li>
                                    <li>
                                        <button
                                            type="button"
                                            className="dropdown-item"
                                            onClick={async (e) => {
                                                e.stopPropagation();
                                                setConnectMenuOpen(false);
                                                await onConnect(device, "cloud");
                                            }}
                                        >
                                            {t("connect_via_cloud")}
                                        </button>
                                    </li>
                                </ul>
                            )}
                        </div>
                    ) : (
                        <Button
                            className="me-2"
                            variant="primary"
                            disabled={connectDisabled}
                            onClick={async (e) => {
                                e.stopPropagation();
                                await onConnect(device);
                            }}
                        >
                            <Monitor />
                        </Button>
                    )}
                    {!isLocalOnly && (
                        <Button
                            variant="danger"
                            onClick={async (e) => {
                                e.stopPropagation();
                                onDelete(device);
                            }}
                        >
                            <Trash2 />
                        </Button>
                    )}
                </Col>
            </Card.Header>
            <Card.Body>
                <Row>
                    <Col xs="auto"><b>{t("mobile_charger_id")}</b></Col>
                    <Col className="text-end">{Base58.int_to_base58(device.uid)}</Col>
                </Row>
                <hr style="margin-top: 5px;margin-bottom: 5px;" />
                <Row>
                    <Col xs="auto"><b>{t("last_state_change")}</b></Col>
                    <Col className="text-end">{formatLastStateChange(t, device.last_state_change)}</Col>
                </Row>
                <hr style="margin-top: 5px;margin-bottom: 5px;" />
                <Row>
                    <Col xs="auto"><b>{t("firmware_version")}</b></Col>
                    <Col className="text-end">{device.firmware_version}</Col>
                </Row>
                <hr style="margin-top: 5px;margin-bottom: 5px;" />
                <Row>
                    <Col xs="auto">
                        <Row>
                            <b>{t("note")}</b>
                        </Row>
                        {!isLocalOnly && (
                            <Row>
                                <Col className="p-0">
                                    <Button
                                        style="background-color:transparent;border:none;"
                                        onClick={() => {
                                            onEditNote(device);
                                        }}
                                    >
                                        <Edit color="#333" />
                                    </Button>
                                </Col>
                            </Row>
                        )}
                    </Col>
                    <Col
                        onClick={split.length <= 3 ? undefined : () => setExpand(!expand)}
                        style={{cursor: split.length <= 3 ? undefined : "pointer", whiteSpace: "pre-line", overflowWrap: "anywhere"}}
                    >
                        <Row>
                            <Col className="d-flex justify-content-end" style={{textAlign: "right"}}>
                                <div>
                                    {split.slice(0, split.length <= 3 ? 3 : 2).join("\n")}
                                </div>
                            </Col>
                        </Row>
                        <Row>
                            <Col className="d-flex justify-content-end" style={{textAlign: "right"}}>
                                <Collapse in={expand}>
                                    <div>
                                        {split.slice(2).join("\n")}
                                    </div>
                                </Collapse>
                            </Col>
                        </Row>
                        <Row hidden={split.length <= 3}>
                            <Col className="d-flex justify-content-end">
                                <a style={{fontSize: "14px", color: "blue", textDecoration: "underline"}}>
                                    {expand ? t("show_less") : t("show_more")}
                                </a>
                            </Col>
                        </Row>
                    </Col>
                </Row>
                <p style="color:red;" hidden={device.valid}>{t("no_keys")}</p>
            </Card.Body>
        </Card>
    );
}
