import { useTranslation } from "react-i18next";
import { Button, Dropdown, Form } from "react-bootstrap";
import { Filter, Grid, List } from "react-feather";
import { Grouping } from "./types";
import { SearchInput } from "./SearchInput";

interface DeviceToolbarProps {
    searchTerm: string;
    onSearchChange: (term: string) => void;
    groupings: Grouping[];
    selectedGroupingId: string | null;
    onGroupingFilterChange: (groupingId: string | null) => void;
    groupingSearchTerm: string;
    setGroupingSearchTerm: (term: string) => void;
    groupByEnabled: boolean;
    onGroupByToggle: () => void;
    onManageGroupingsClick: () => void;
    // `desktop` lays everything out in a single row; `mobile` stacks the
    // controls vertically and stretches the buttons/inputs to full width so
    // the toolbar stays usable on small screens. Each viewport renders its
    // own instance and Bootstrap's responsive visibility classes decide
    // which one is actually shown to the user.
    variant?: "desktop" | "mobile";
}

export function DeviceToolbar({
    searchTerm,
    onSearchChange,
    groupings,
    selectedGroupingId,
    onGroupingFilterChange,
    groupingSearchTerm,
    setGroupingSearchTerm,
    groupByEnabled,
    onGroupByToggle,
    onManageGroupingsClick,
    variant = "desktop",
}: DeviceToolbarProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
    const isMobile = variant === "mobile";

    const selectedGrouping = groupings.find((g) => g.id === selectedGroupingId);

    return (
        <div className={`d-flex ${isMobile ? "flex-column gap-3" : "justify-content-between align-items-center mx-2 flex-nowrap gap-2"} mb-3`}>
            <div className={isMobile ? "" : "flex-grow-1"}>
                <SearchInput searchTerm={searchTerm} onSearchChange={onSearchChange} />
            </div>
            <div className={`d-flex ${isMobile ? "flex-wrap w-100" : "flex-nowrap"} align-items-center ${isMobile ? "gap-2" : "gap-1"}`}>
                {groupings.length > 0 && (
                    <Dropdown className={isMobile ? "w-100 w-md-auto" : undefined}>
                        <Dropdown.Toggle
                            variant={selectedGrouping ? "warning" : "primary"}
                            className={isMobile ? "w-100" : undefined}
                            title={selectedGrouping ? `${t("filter_by_grouping")}: ${selectedGrouping.name}` : t("filter_by_grouping")}
                        >
                            <span className="d-inline-flex align-items-center gap-1">
                                <Filter size={16} />
                                {selectedGrouping ? selectedGrouping.name : t("filter_by_grouping")}
                            </span>
                        </Dropdown.Toggle>
                        <Dropdown.Menu>
                            <div class="px-1">
                                <Form.Control
                                    placeholder={t("search_groupings")}
                                    value={groupingSearchTerm}
                                    onChange={(e) => setGroupingSearchTerm((e.target as HTMLInputElement).value)}
                                />
                            </div>
                            <Dropdown.Item
                                active={selectedGroupingId === null}
                                onClick={() => onGroupingFilterChange(null)}
                            >
                                {t("all_devices")}
                            </Dropdown.Item>
                            {groupings
                                .filter((grouping) => grouping.name.toLowerCase().includes(groupingSearchTerm.toLowerCase()))
                                .map((grouping) => (
                                    <Dropdown.Item
                                        key={grouping.id}
                                        active={grouping.id === selectedGroupingId}
                                        onClick={() => onGroupingFilterChange(grouping.id)}
                                    >
                                        {grouping.name} ({grouping.device_ids.length})
                                    </Dropdown.Item>
                                ))}
                        </Dropdown.Menu>
                    </Dropdown>
                )}
                {groupings.length > 0 && (
                    <Button
                        variant="primary"
                        onClick={onGroupByToggle}
                        aria-pressed={groupByEnabled}
                        aria-label={t("group_by_toggle")}
                        title={t("group_by_toggle")}
                        data-testid="group-by-toggle"
                        className="group-by-toggle"
                    >
                        {groupByEnabled ? <Grid size={16} /> : <List size={16} />}
                    </Button>
                )}
                <Button
                    variant="primary"
                    onClick={onManageGroupingsClick}
                    className={isMobile ? "flex-grow-1" : undefined}
                >
                    {t("manage_groupings")}
                </Button>
            </div>
        </div>
    );
}