import { useState } from "preact/hooks";
import { useTranslation } from "react-i18next";
import { Col, Row, Table } from "react-bootstrap";
import { ChevronDown, ChevronRight, ChevronUp } from "react-feather";
import { StateDevice, SortColumn, Grouping, ConnectVia } from "./types";
import { DeviceTableRow } from "./DeviceTableRow";
import { DeviceToolbar } from "./DeviceToolbar";

interface DeviceTableProps {
    devices: StateDevice[];
    sortColumn: SortColumn;
    sortSequence: "asc" | "desc";
    onSort: (column: SortColumn) => void;
    onConnect: (device: StateDevice, via?: Exclude<ConnectVia, "default">) => Promise<void>;
    onDelete: (device: StateDevice) => void;
    onEditNote: (device: StateDevice) => void;
    connectionPossible: (device: StateDevice) => boolean;
    formatLastStateChange: (t: (key: string, options?: Record<string, unknown>) => string, timestamp?: number | null) => string;
    groupings: Grouping[];
    bundleByGroups?: boolean;
    searchTerm: string;
    onSearchChange: (term: string) => void;
    selectedGroupingId: string | null;
    onGroupingFilterChange: (groupingId: string | null) => void;
    groupingSearchTerm: string;
    setGroupingSearchTerm: (term: string) => void;
    groupByEnabled: boolean;
    onGroupByToggle: () => void;
    onManageGroupingsClick: () => void;
}

const renderColgroup = () => (
    <colgroup>
        <col class="charger-col-status" />
        <col class="charger-col-name" />
        <col class="charger-col-uid" />
        <col class="charger-col-actions" />
        <col class="charger-col-state-change" />
        <col class="charger-col-note" />
        <col class="charger-col-firmware" />
    </colgroup>
);

export function DeviceTable({
    devices,
    sortColumn,
    sortSequence,
    onSort,
    onConnect,
    onDelete,
    onEditNote,
    connectionPossible,
    formatLastStateChange,
    groupings,
    bundleByGroups = false,
    searchTerm,
    onSearchChange,
    selectedGroupingId,
    onGroupingFilterChange,
    groupingSearchTerm,
    setGroupingSearchTerm,
    groupByEnabled,
    onGroupByToggle,
    onManageGroupingsClick,
}: DeviceTableProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });

    const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

    const toggleGroup = (groupId: string) => {
        setExpandedGroups((prev) => {
            const next = new Set(prev);
            if (next.has(groupId)) {
                next.delete(groupId);
            } else {
                next.add(groupId);
            }
            return next;
        });
    };

    const getIcon = (column: SortColumn) => {
        if (sortColumn !== column) {
            return <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-chevrons-down"><polyline points="7 14 12 19 17 14" /><polyline points="7 10 12 5 17 10" /></svg>;
        } else if (sortSequence === "asc") {
            return <ChevronDown />;
        }
        return <ChevronUp />;
    };

    const renderTableHeader = () => (
        <thead>
            <tr class="charger-head">
                <th onClick={() => onSort("status")} style={{ width: "60px" }}>
                    <Row className="m-0 g-0">
                        <Col className="text-center">
                            {getIcon("status")}
                        </Col>
                    </Row>
                </th>
                <th onClick={() => onSort("name")} style={{ width: "auto" }}>
                    <Row className="flex-nowrap g-0">
                        <Col>
                            {t("charger_name")}
                        </Col>
                        <Col xs="auto">
                            {getIcon("name")}
                        </Col>
                    </Row>
                </th>
                <th onClick={() => onSort("uid")} style={{ width: "110px" }}>
                    <Row className="flex-nowrap g-0">
                        <Col>
                            {t("charger_id")}
                        </Col>
                        <Col xs="auto">
                            {getIcon("uid")}
                        </Col>
                    </Row>
                </th>
                <th style={{ width: "220px" }} />
                <th onClick={() => onSort("last_state_change")} style={{ width: "160px" }}>
                    <Row className="flex-nowrap g-0">
                        <Col>
                            {t("last_state_change")}
                        </Col>
                        <Col xs="auto">
                            {getIcon("last_state_change")}
                        </Col>
                    </Row>
                </th>
                <th onClick={() => onSort("note")} style={{ width: "50%" }}>
                    <Row className="flex-nowrap g-0">
                        <Col>
                            {t("note")}
                        </Col>
                        <Col xs="auto">
                            {getIcon("note")}
                        </Col>
                    </Row>
                </th>
                <th onClick={() => onSort("firmware_version")} style={{ width: "130px" }}>
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
    );

    if (!bundleByGroups) {
        return (
            <Col className="d-none d-md-block">
                <DeviceToolbar
                    searchTerm={searchTerm}
                    onSearchChange={onSearchChange}
                    groupings={groupings}
                    selectedGroupingId={selectedGroupingId}
                    onGroupingFilterChange={onGroupingFilterChange}
                    groupingSearchTerm={groupingSearchTerm}
                    setGroupingSearchTerm={setGroupingSearchTerm}
                    groupByEnabled={groupByEnabled}
                    onGroupByToggle={onGroupByToggle}
                    onManageGroupingsClick={onManageGroupingsClick}
                    variant="desktop"
                />
                <Table striped hover responsive class="charger-table">
                    {renderColgroup()}
                    {renderTableHeader()}
                    <tbody>
                        {devices.map((device) => (
                            <DeviceTableRow
                                // Standalone local devices share the empty id, so
                                // key them by their (unique) LAN host instead.
                                key={device.id === "" ? (device.host ?? "") : device.id}
                                device={device}
                                onConnect={onConnect}
                                onDelete={onDelete}
                                onEditNote={onEditNote}
                                connectionPossible={connectionPossible}
                                formatLastStateChange={formatLastStateChange}
                                groupings={groupings}
                            />
                        ))}
                    </tbody>
                </Table>
            </Col>
        );
    }

    const grouped: { id: string; name: string; devices: StateDevice[] }[] = groupings
        .map((g) => ({
            id: g.id,
            name: g.name,
            devices: devices.filter((d) => d.id !== "" && g.device_ids.includes(d.id)),
        }))
        .map((g) => ({ ...g }));

    const groupedDeviceIds = new Set(grouped.flatMap((g) => g.devices.map((d) => d.id)));
    const ungroupedDevices = devices.filter((d) => d.id !== "" && !groupedDeviceIds.has(d.id));

    const renderGroupSectionRows = (groupKey: string, name: string, groupDevices: StateDevice[]) => {
        const expanded = expandedGroups.has(groupKey);
        return [
            <tr
                key={`${groupKey}-header`}
                class={`group-section-header${expanded ? " group-section-header--expanded" : ""}`}
                onClick={() => toggleGroup(groupKey)}
                aria-expanded={expanded}
                style={{ cursor: "pointer", background: expanded ? "#e9ecef" : "#ced4da" }}
            >
                <td colSpan={7} class="align-middle">
                    <Row className="flex-nowrap align-items-center g-0">
                        <Col xs="auto" className="me-2">
                            {expanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                        </Col>
                        <Col>
                            <strong>{name}</strong>
                            <span className="text-muted ms-2">({groupDevices.length})</span>
                        </Col>
                    </Row>
                </td>
            </tr>,
            ...(expanded ? groupDevices.map((device) => (
                <DeviceTableRow
                    key={`${groupKey}-${device.id === "" ? (device.host ?? "") : device.id}`}
                    device={device}
                    onConnect={onConnect}
                    onDelete={onDelete}
                    onEditNote={onEditNote}
                    connectionPossible={connectionPossible}
                    formatLastStateChange={formatLastStateChange}
                    groupings={groupings}
                />
            )) : []),
        ];
    };

    return (
        <Col className="d-none d-md-block">
            <DeviceToolbar
                searchTerm={searchTerm}
                onSearchChange={onSearchChange}
                groupings={groupings}
                selectedGroupingId={selectedGroupingId}
                onGroupingFilterChange={onGroupingFilterChange}
                groupingSearchTerm={groupingSearchTerm}
                setGroupingSearchTerm={setGroupingSearchTerm}
                groupByEnabled={groupByEnabled}
                onGroupByToggle={onGroupByToggle}
                onManageGroupingsClick={onManageGroupingsClick}
                variant="desktop"
            />
            <Table striped hover responsive class="charger-table">
                {renderColgroup()}
                {renderTableHeader()}
                <tbody>
                    {grouped.flatMap((g) => renderGroupSectionRows(g.id, g.name, g.devices))}
                    {ungroupedDevices.length > 0 && renderGroupSectionRows("__ungrouped__", t("no_group"), ungroupedDevices)}
                </tbody>
            </Table>
        </Col>
    );
}
