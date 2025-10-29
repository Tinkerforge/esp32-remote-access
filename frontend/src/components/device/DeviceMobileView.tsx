import { useTranslation } from "react-i18next";
import { ButtonGroup, Col, Container, Dropdown, DropdownButton } from "react-bootstrap";
import Median from "median-js-bridge";
import i18n from "../../i18n";
import { StateDevice, SortColumn, Grouping } from "./types";
import { DeviceCard } from "./DeviceCard";

interface DeviceMobileViewProps {
    devices: StateDevice[];
    sortColumn: SortColumn;
    sortSequence: "asc" | "desc";
    onMobileSort: (column: SortColumn) => void;
    onSortSequenceChange: (sequence: "asc" | "desc") => void;
    onConnect: (device: StateDevice) => Promise<void>;
    onDelete: (device: StateDevice) => void;
    onEditNote: (device: StateDevice, index: number) => void;
    connectionPossible: (device: StateDevice) => boolean;
    formatLastStateChange: (t: (key: string, options?: Record<string, unknown>) => string, timestamp?: number | null) => string;
    groupings: Grouping[];
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
    groupings
}: DeviceMobileViewProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });

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

    return (
        <Container fluid className="d-md-none">
            <Col className={Median.isNativeApp() ? "mt-2" : undefined}>
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
            {devices.map((device, index) => (
                <DeviceCard
                    key={device.id}
                    device={device}
                    index={index}
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
