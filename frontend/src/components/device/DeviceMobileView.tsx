import { useState } from "preact/hooks";
import { useTranslation } from "react-i18next";
import { ButtonGroup, Col, Container, Dropdown, DropdownButton } from "react-bootstrap";
import { ChevronDown, ChevronUp } from "react-feather";
import Median from "median-js-bridge";
import i18n from "../../i18n";
import { StateDevice, SortColumn, Grouping, ConnectVia } from "./types";
import { DeviceCard } from "./DeviceCard";
import { DeviceToolbar } from "./DeviceToolbar";

interface DeviceMobileViewProps {
    devices: StateDevice[];
    sortColumn: SortColumn;
    sortSequence: "asc" | "desc";
    onMobileSort: (column: SortColumn) => void;
    onSortSequenceChange: (sequence: "asc" | "desc") => void;
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

export function DeviceMobileView({
    devices,
    sortColumn,
    sortSequence,
    onMobileSort,
    onSortSequenceChange,
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
}: DeviceMobileViewProps) {
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

    const getMobileSortName = () => {
        switch (sortColumn) {
            case "name":
                return i18n.t("chargers.charger_name");
            case "status":
                return i18n.t("chargers.status");
            case "uid":
                return i18n.t("chargers.charger_id");
            case "note":
                return i18n.t("chargers.note");
            case "last_state_change":
                return i18n.t("chargers.last_state_change");
            case "firmware_version":
                return i18n.t("chargers.firmware_version");
            default:
                return i18n.t("chargers.select_sorting");
        }
    };

    const renderSortControls = () => (
        <Col className={`${Median.isNativeApp() ? "mt-2" : undefined} mb-2`}>
            <ButtonGroup>
                <DropdownButton className="dropdown-btn" title={getMobileSortName()}>
                    <Dropdown.Item onClick={() => onMobileSort("name")}>{t("charger_name")}</Dropdown.Item>
                    <Dropdown.Item onClick={() => onMobileSort("uid")}>{t("charger_id")}</Dropdown.Item>
                    <Dropdown.Item onClick={() => onMobileSort("status")}>{t("status")}</Dropdown.Item>
                    <Dropdown.Item onClick={() => onMobileSort("last_state_change")}>{t("last_state_change")}</Dropdown.Item>
                    <Dropdown.Item onClick={() => onMobileSort("note")}>{t("note")}</Dropdown.Item>
                    <Dropdown.Item onClick={() => onMobileSort("firmware_version")}>{t("firmware_version")}</Dropdown.Item>
                </DropdownButton>
                <DropdownButton className="dropdown-btn" title={sortSequence === "asc" ? t("sorting_sequence_asc") : t("sorting_sequence_desc")}>
                    <Dropdown.Item onClick={() => onSortSequenceChange("asc")}>{t("sorting_sequence_asc")}</Dropdown.Item>
                    <Dropdown.Item onClick={() => onSortSequenceChange("desc")}>{t("sorting_sequence_desc")}</Dropdown.Item>
                </DropdownButton>
            </ButtonGroup>
        </Col>
    );

    if (!bundleByGroups) {
        return (
            <Container fluid className="d-md-none">
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
                    variant="mobile"
                />
                {renderSortControls()}
                {devices.map((device) => (
                    <DeviceCard
                        // Standalone local devices share the empty id, so key them
                        // by their (unique) LAN host instead.
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
            </Container>
        );
    }

    // Bundle devices into one collapsible section per grouping. Devices
    // that aren't in any group render as plain cards alongside the
    // sections, since wrapping them would force the user to click through
    // to see devices they didn't ask to group in the first place.
    const grouped: { id: string; name: string; devices: StateDevice[] }[] = groupings.map((g) => ({
        id: g.id,
        name: g.name,
        devices: devices.filter((d) => d.id !== "" && g.device_ids.includes(d.id)),
    }));

    const groupedDeviceIds = new Set(grouped.flatMap((g) => g.devices.map((d) => d.id)));
    const ungroupedDevices = devices.filter((d) => d.id !== "" && !groupedDeviceIds.has(d.id));

    const renderSection = (groupKey: string, name: string, groupDevices: StateDevice[]) => {
        const expanded = expandedGroups.has(groupKey);
        return (
            <div key={groupKey} className="group-section">
                <button
                    type="button"
                    className={`group-section-header w-100 d-flex justify-content-between align-items-center px-3 py-2${expanded ? " group-section-header--expanded" : ""}`}
                    onClick={() => toggleGroup(groupKey)}
                    aria-expanded={expanded}
                    style={{
                        cursor: "pointer",
                        borderTop: "1px solid #dee2e6",
                        borderRight: "1px solid #dee2e6",
                        borderLeft: "1px solid #dee2e6",
                        borderBottom: expanded ? "1px solid transparent" : "1px solid #dee2e6",
                        borderRadius: expanded ? "0.375rem 0.375rem 0 0" : "0.375rem",
                        background: expanded ? "#e9ecef" : "#ced4da",
                        marginBottom: expanded ? 0 : "0.5rem",
                    }}
                >
                    <span
                        className="group-section-name d-flex justify-content-center align-items-center"
                        style={{
                            minWidth: 0,
                            flex: "1 1 auto",
                            gap: "0.5rem",
                        }}
                    >
                        <strong
                            className="text-truncate"
                            style={{ minWidth: 0, maxWidth: "100%" }}
                        >
                            {name}
                        </strong>
                        <span className="text-muted">({groupDevices.length})</span>
                    </span>
                    {expanded ? <ChevronUp size={18} /> : <ChevronDown size={18} />}
                </button>
                {expanded && (
                    <div
                        className="group-section-body"
                        style={{
                            borderRight: "1px solid #dee2e6",
                            borderBottom: "1px solid #dee2e6",
                            borderLeft: "1px solid #dee2e6",
                            borderRadius: "0 0 0.375rem 0.375rem",
                            padding: "0.5rem",
                            marginBottom: "0.5rem",
                            background: "#fff",
                        }}
                    >
                        {groupDevices.map((device) => (
                            <DeviceCard
                                key={`${groupKey}-${device.id === "" ? (device.host ?? "") : device.id}`}
                                device={device}
                                onConnect={onConnect}
                                onDelete={onDelete}
                                onEditNote={onEditNote}
                                connectionPossible={connectionPossible}
                                formatLastStateChange={formatLastStateChange}
                                groupings={groupings}
                            />
                        ))}
                    </div>
                )}
            </div>
        );
    };

    return (
        <Container fluid className="d-md-none">
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
                variant="mobile"
            />
            {renderSortControls()}
            {grouped.map((g) => renderSection(g.id, g.name, g.devices))}
            {ungroupedDevices.length > 0 && renderSection("__ungrouped__", t("no_group"), ungroupedDevices)}
        </Container>
    );
}