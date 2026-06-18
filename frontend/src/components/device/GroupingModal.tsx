import { useEffect, useState } from "preact/hooks";
import { useTranslation } from "react-i18next";
import { Badge, Button, Col, Form, InputGroup, ListGroup, Modal, Row } from "react-bootstrap";
import { Trash2, Plus, Edit2, Search } from "react-feather";
import { showAlert } from "../Alert";
import { fetchClient } from "../../utils";
import { Grouping, StateDevice } from "./types";
import * as Base58 from "base58";

interface GroupingModalProps {
    show: boolean;
    devices: StateDevice[];
    groupings: Grouping[];
    onClose: () => void;
    encryptGroupingName: (name: string) => Promise<string | undefined>;
    loadGroupings: () => Promise<void>;
}

export function GroupingModal({
    show,
    devices,
    groupings,
    onClose,
    encryptGroupingName,
    loadGroupings: loadGroupingsFromParent
}: GroupingModalProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
    const [editingGrouping, setEditingGrouping] = useState<Grouping | null>(null);
    const [groupingName, setGroupingName] = useState("");
    const [selectedDevices, setSelectedDevices] = useState<Set<string>>(new Set());
    const [isCreating, setIsCreating] = useState(false);
    const [deviceSearchQuery, setDeviceSearchQuery] = useState("");
    const [setAsDefault, setSetAsDefault] = useState(false);
    const [isDefaultChangePending, setIsDefaultChangePending] = useState(false);

    useEffect(() => {
        if (!show) {
            setEditingGrouping(null);
            setGroupingName("");
            setSelectedDevices(new Set());
            setIsCreating(false);
            setDeviceSearchQuery("");
            setSetAsDefault(false);
        }
    }, [show]);

    const handleCreateNew = () => {
        setIsCreating(true);
        setEditingGrouping(null);
        setGroupingName("");
        setSelectedDevices(new Set());
        setDeviceSearchQuery("");
        setSetAsDefault(false);
    };

    const handleEdit = (grouping: Grouping) => {
        setEditingGrouping(grouping);
        setGroupingName(grouping.name);
        setSelectedDevices(new Set(grouping.device_ids));
        setIsCreating(false);
        setDeviceSearchQuery("");
        setSetAsDefault(grouping.is_default);
    };

    const handleCancel = () => {
        setEditingGrouping(null);
        setGroupingName("");
        setSelectedDevices(new Set());
        setIsCreating(false);
        setDeviceSearchQuery("");
        setSetAsDefault(false);
    };

    // Toggles the persisted default for an existing grouping. The backend
    // owns the "only one default per user" rule; the checkbox reflects
    // `grouping.is_default` so it auto-resets on the next refresh.
    const handleSetDefault = async (grouping: Grouping) => {
        if (isDefaultChangePending) {
            return;
        }
        const nextValue = !grouping.is_default;
        setIsDefaultChangePending(true);
        try {
            const { response, error } = await fetchClient.PUT("/grouping/edit", {
                body: { grouping_id: grouping.id, is_default: nextValue },
                credentials: "same-origin"
            });
            if (response.status === 200) {
                await loadGroupingsFromParent();
            } else {
                showAlert(t("set_default_failed", { error: error || response.status }), "danger");
            }
        } catch (error) {
            showAlert(t("set_default_failed", { error: String(error) }), "danger");
        } finally {
            setIsDefaultChangePending(false);
        }
    };

    const handleDeviceToggle = (deviceId: string) => {
        const newSelected = new Set(selectedDevices);
        if (newSelected.has(deviceId)) {
            newSelected.delete(deviceId);
        } else {
            newSelected.add(deviceId);
        }
        setSelectedDevices(newSelected);
    };

    const handleSave = async (e: Event) => {
        e.preventDefault();

        try {
            let savedId: string;
            if (editingGrouping) {
                // Update existing grouping
                savedId = editingGrouping.id;
                await updateGrouping(savedId, groupingName, selectedDevices, setAsDefault);
            } else {
                // Create new grouping
                savedId = await createGrouping(groupingName, selectedDevices, setAsDefault);
            }

            // Reload groupings
            await loadGroupingsFromParent();
            handleCancel();
        } catch (error) {
            console.error("Error saving grouping:", error);
        }
    };

    const handleDelete = async (groupingId: string, groupingName: string) => {
        if (!confirm(t("delete_grouping_confirm", { name: groupingName }))) {
            return;
        }

        try {
            const { response, error } = await fetchClient.DELETE("/grouping/delete", {
                body: { grouping_id: groupingId },
                credentials: "same-origin"
            });

            if (response.status === 200) {
                await loadGroupingsFromParent();
            } else {
                showAlert(t("delete_grouping_failed", { error: error || response.status }), "danger");
            }
        } catch (error) {
            showAlert(t("delete_grouping_failed", { error: String(error) }), "danger");
        }
    };

    const createGrouping = async (name: string, deviceIds: Set<string>, isDefault: boolean): Promise<string> => {
        const encryptedName = await encryptGroupingName(name);
        if (!encryptedName) {
            showAlert(t("create_grouping_failed", { error: "Failed to encrypt name" }), "danger");
            throw new Error("Failed to encrypt grouping name");
        }

        const { data, response, error } = await fetchClient.POST("/grouping/create", {
            body: { name: encryptedName, is_default: isDefault },
            credentials: "same-origin"
        });

        if (response.status !== 200 || error || !data) {
            showAlert(t("create_grouping_failed", { error: error || response.status }), "danger");
            throw new Error("Failed to create grouping");
        }

        const groupingId = data.id;

        // Add devices to the grouping
        for (const deviceId of deviceIds) {
            const { response, error } = await fetchClient.POST("/grouping/add_device", {
                body: { grouping_id: groupingId, device_id: deviceId },
                credentials: "same-origin"
            });
            if (response.status !== 200 || error) {
                showAlert(t("add_device_to_grouping_failed", { error: error || response.status }), "danger");
                throw new Error("Failed to add device to grouping");
            }
        }

        return groupingId;
    };

    const updateGrouping = async (groupingId: string, name: string, deviceIds: Set<string>, isDefault: boolean) => {
        const existingGrouping = groupings.find(g => g.id === groupingId);
        if (!existingGrouping) return;

        // Only send fields that actually changed. The backend treats a missing
        // field as "leave unchanged"
        const nameChanged = name !== existingGrouping.name;
        const defaultChanged = isDefault !== existingGrouping.is_default;

        if (nameChanged || defaultChanged) {
            let encryptedName: string | undefined;
            if (nameChanged) {
                encryptedName = await encryptGroupingName(name);
                if (!encryptedName) {
                    showAlert(t("update_grouping_failed", { error: "Failed to encrypt name" }), "danger");
                    throw new Error("Failed to encrypt grouping name");
                }
            }

            const body: { grouping_id: string; name?: string; is_default?: boolean } = {
                grouping_id: groupingId,
            };
            if (nameChanged && encryptedName) {
                body.name = encryptedName;
            }
            if (defaultChanged) {
                body.is_default = isDefault;
            }

            const { response, error } = await fetchClient.PUT("/grouping/edit", {
                body,
                credentials: "same-origin"
            });

            if (response.status !== 200 || error) {
                showAlert(t("update_grouping_failed", { error: error || response.status }), "danger");
                throw new Error("Failed to update grouping");
            }
        }

        // Devices to add
        const devicesToAdd = Array.from(deviceIds).filter(id => !existingGrouping.device_ids.includes(id));
        // Devices to remove
        const devicesToRemove = existingGrouping.device_ids.filter(id => !deviceIds.has(id));

        // Add devices
        for (const deviceId of devicesToAdd) {
            const { response, error } = await fetchClient.POST("/grouping/add_device", {
                body: { grouping_id: groupingId, device_id: deviceId },
                credentials: "same-origin"
            });
            if (response.status !== 200 || error) {
                showAlert(t("add_device_to_grouping_failed", { error: error || response.status }), "danger");
                throw new Error("Failed to add device to grouping");
            }
        }

        // Remove devices
        for (const deviceId of devicesToRemove) {
            const { response, error } = await fetchClient.DELETE("/grouping/remove_device", {
                body: { grouping_id: groupingId, device_id: deviceId },
                credentials: "same-origin"
            });
            if (response.status !== 200 || error) {
                showAlert(t("remove_device_from_grouping_failed", { error: error || response.status }), "danger");
                throw new Error("Failed to remove device from grouping");
            }
        }
    };

    const filterDevices = (devices: StateDevice[]): StateDevice[] => {
        // Groupings live in the cloud, so standalone local devices (empty id,
        // discovered on the LAN but not paired with this account) cannot be
        // referenced and are hidden from the picker.
        const pairableDevices = devices.filter(device => device.id !== "");

        if (!deviceSearchQuery.trim()) {
            return pairableDevices;
        }

        const query = deviceSearchQuery.toLowerCase();
        return pairableDevices.filter(device => {
            const name = device.name.toLowerCase();
            const uid = Base58.int_to_base58(device.uid).toLowerCase();
            return name.includes(query) || uid.includes(query);
        });
    };

    const renderGroupingList = () => (
        <>
            <Modal.Header closeButton>
                <Modal.Title>{t("manage_groupings")}</Modal.Title>
            </Modal.Header>
            <Modal.Body>
                <div className="d-flex justify-content-between align-items-center mb-3">
                    <h5>{t("groupings")}</h5>
                    <Button variant="primary" size="sm" onClick={handleCreateNew}>
                        <Plus size={16} />
                        <span className="ms-1">{t("create_grouping")}</span>
                    </Button>
                </div>

                {groupings.length === 0 ? (
                    <p className="text-muted text-center">{t("no_groupings")}</p>
                ) : (
                    <ListGroup>
                        {groupings.map(grouping => (
                            <ListGroup.Item key={grouping.id}>
                                <Row className="align-items-center">
                                    <Col>
                                        <strong>{grouping.name}</strong>
                                        {grouping.is_default && (
                                            <Badge bg="primary" pill className="ms-2">
                                                {t("default_grouping")}
                                            </Badge>
                                        )}
                                        <div className="text-muted small">
                                            {grouping.device_ids.length} {t("grouping_devices").toLowerCase()}
                                        </div>
                                    </Col>
                                    <Col xs="auto" className="d-flex align-items-center">
                                        <Form.Check
                                            type="checkbox"
                                            id={`set-default-${grouping.id}`}
                                            checked={grouping.is_default}
                                            disabled={isDefaultChangePending}
                                            onChange={() => handleSetDefault(grouping)}
                                            label={t("set_as_default")}
                                            className="me-3 mb-0"
                                        />
                                        <Button
                                            variant="outline-primary"
                                            size="sm"
                                            className="me-2"
                                            onClick={() => handleEdit(grouping)}
                                        >
                                            <Edit2 size={16} />
                                        </Button>
                                        <Button
                                            variant="outline-danger"
                                            size="sm"
                                            onClick={() => handleDelete(grouping.id, grouping.name)}
                                        >
                                            <Trash2 size={16} />
                                        </Button>
                                    </Col>
                                </Row>
                            </ListGroup.Item>
                        ))}
                    </ListGroup>
                )}
            </Modal.Body>
            <Modal.Footer>
                <Button variant="secondary" onClick={onClose}>
                    {t("close")}
                </Button>
            </Modal.Footer>
        </>
    );

    const renderEditForm = () => (
        <>
            <Modal.Header closeButton>
                <Modal.Title>
                    {editingGrouping ? t("edit_grouping") : t("create_grouping")}
                </Modal.Title>
            </Modal.Header>
            <Form onSubmit={handleSave}>
                <Modal.Body>
                        <Form.Group className="mb-3">
                            <Form.Label>{t("grouping_name")}</Form.Label>
                            <Form.Control
                                type="text"
                                placeholder={t("grouping_name_placeholder")}
                                value={groupingName}
                                onChange={(e) => setGroupingName((e.target as HTMLInputElement).value)}
                                required
                            />
                        </Form.Group>

                        <Form.Group>
                            <Form.Label>{t("select_devices")}</Form.Label>
                            <ListGroup>
                                <ListGroup.Item className="p-0 border-0">
                                    <InputGroup>
                                        <InputGroup.Text style={{borderBottomLeftRadius: 0}}>
                                            <Search size={16} />
                                        </InputGroup.Text>
                                        <Form.Control
                                            type="search"
                                            placeholder={t("search_devices")}
                                            value={deviceSearchQuery}
                                            style={{borderBottomRightRadius: 0}}
                                            onChange={(e) => setDeviceSearchQuery((e.target as HTMLInputElement).value)}
                                        />
                                    </InputGroup>
                                </ListGroup.Item>
                                {filterDevices(devices).map(device => (
                                    <ListGroup.Item
                                        key={device.id}
                                        action
                                        onClick={(e: Event) => {
                                            e.preventDefault();
                                            handleDeviceToggle(device.id);
                                        }}
                                        style={{ cursor: "pointer" }}
                                    >
                                        <Form.Check
                                            type="checkbox"
                                            checked={selectedDevices.has(device.id)}
                                            onClick={(e) => {
                                                e.stopPropagation();
                                                handleDeviceToggle(device.id);
                                            }}
                                            label={
                                                <div style={{ cursor: "pointer" }}>
                                                    <strong>{device.name}</strong>
                                                    <span className="text-muted ms-2">
                                                        ({Base58.int_to_base58(device.uid)})
                                                    </span>
                                                </div>
                                            }
                                        />
                                    </ListGroup.Item>
                                ))}
                            </ListGroup>
                        </Form.Group>

                        {(() => {
                            const replacedDefaultName = setAsDefault
                                ? groupings.find(g => g.is_default && g.id !== (editingGrouping?.id ?? ""))?.name
                                : undefined;
                            return (
                                <Form.Group className="mt-3">
                                    <Form.Check
                                        type="checkbox"
                                        id="set-as-default"
                                        checked={setAsDefault}
                                        onChange={(e) => setSetAsDefault((e.target as HTMLInputElement).checked)}
                                        label={t("set_as_default")}
                                    />
                                    {replacedDefaultName && (
                                        <Form.Text className="text-muted">
                                            {t("set_as_default_replaces", { name: replacedDefaultName })}
                                        </Form.Text>
                                    )}
                                </Form.Group>
                            );
                        })()}
                </Modal.Body>
                <Modal.Footer>
                    <Button variant="secondary" onClick={handleCancel}>
                        {t("cancel")}
                    </Button>
                    <Button variant="primary" type="submit">
                        {t("save")}
                    </Button>
                </Modal.Footer>
            </Form>
        </>
    );

    return (
        <Modal show={show} onHide={onClose} size="lg">
            {(isCreating || editingGrouping) ? renderEditForm() : renderGroupingList()}
        </Modal>
    );
}
