import { useTranslation } from "react-i18next";
import { Col, Row, Table } from "react-bootstrap";
import { ChevronDown, ChevronUp } from "react-feather";
import { StateDevice, SortColumn } from "./types";
import { DeviceTableRow } from "./DeviceTableRow";

interface DeviceTableProps {
    devices: StateDevice[];
    sortColumn: SortColumn;
    sortSequence: "asc" | "desc";
    onSort: (column: SortColumn) => void;
    onConnect: (device: StateDevice) => Promise<void>;
    onDelete: (device: StateDevice) => void;
    onEditNote: (device: StateDevice, index: number) => void;
    connectionPossible: (device: StateDevice) => boolean;
    formatLastStateChange: (t: (key: string, options?: Record<string, unknown>) => string, timestamp?: number | null) => string;
}

export function DeviceTable({
    devices,
    sortColumn,
    sortSequence,
    onSort,
    onConnect,
    onDelete,
    onEditNote,
    connectionPossible,
    formatLastStateChange
}: DeviceTableProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });

    const getIcon = (column: SortColumn) => {
        if (sortColumn !== column) {
            // Updown Icon
            return <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-chevrons-down"><polyline points="7 14 12 19 17 14" /><polyline points="7 10 12 5 17 10" /></svg>;
        } else if (sortSequence === "asc") {
            return <ChevronDown />;
        }
            return <ChevronUp />;

    };

    return (
        <Col className="d-none d-md-block">
            <Table striped hover responsive>
                <thead>
                    <tr class="charger-head">
                        <th onClick={() => onSort("status")}>
                            <Row className="m-0 g-0">
                                <Col className="text-center">
                                    {getIcon("status")}
                                </Col>
                            </Row>
                        </th>
                        <th onClick={() => onSort("name")}>
                            <Row className="flex-nowrap g-0">
                                <Col>
                                    {t("charger_name")}
                                </Col>
                                <Col xs="auto">
                                    {getIcon("name")}
                                </Col>
                            </Row>
                        </th>
                        <th onClick={() => onSort("uid")}>
                            <Row className="flex-nowrap g-0">
                                <Col>
                                    {t("charger_id")}
                                </Col>
                                <Col xs="auto">
                                    {getIcon("uid")}
                                </Col>
                            </Row>
                        </th>
                        <th />
                        <th onClick={() => onSort("last_state_change")}>
                            <Row className="flex-nowrap g-0">
                                <Col>
                                    {t("last_state_change")}
                                </Col>
                                <Col xs="auto">
                                    {getIcon("last_state_change")}
                                </Col>
                            </Row>
                        </th>
                        <th onClick={() => onSort("note")}>
                            <Row className="flex-nowrap g-0">
                                <Col>
                                    {t("note")}
                                </Col>
                                <Col xs="auto">
                                    {getIcon("note")}
                                </Col>
                            </Row>
                        </th>
                        <th onClick={() => onSort("firmware_version")}>
                            <Row className="flex-nowrap g-0">
                                <Col>
                                    {t("firmware_version")}
                                </Col>
                                <Col xs="auto">
                                    {getIcon("firmware_version")}
                                </Col>
                            </Row>
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {devices.map((device, index) => (
                        <DeviceTableRow
                            key={device.id}
                            device={device}
                            index={index}
                            onConnect={onConnect}
                            onDelete={onDelete}
                            onEditNote={onEditNote}
                            connectionPossible={connectionPossible}
                            formatLastStateChange={formatLastStateChange}
                        />
                    ))}
                </tbody>
            </Table>
        </Col>
    );
}
