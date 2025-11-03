import { useEffect, useState } from "preact/hooks";
import { useTranslation } from "react-i18next";
import { Button, Col, Form, ListGroup, Modal, Row } from "react-bootstrap";
import { Trash2, Plus, Edit2 } from "react-feather";
import { showAlert } from "../Alert";
import { fetchClient } from "../../utils";
import { Grouping, StateDevice } from "./types";
import * as Base58 from "base58";

interface GroupingModalProps {
    show: boolean;
    devices: StateDevice[];
    groupings: Grouping[];
    onClose: () => void;
    onGroupingsUpdated: (groupings: Grouping[]) => void;
}

export function GroupingModal({
    show,
    devices,
    groupings,
    onClose,
    onGroupingsUpdated
}: GroupingModalProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });
    const [editingGrouping, setEditingGrouping] = useState<Grouping | null>(null);
    const [groupingName, setGroupingName] = useState("");
    const [selectedDevices, setSelectedDevices] = useState<Set<string>>(new Set());
    const [isCreating, setIsCreating] = useState(false);

    useEffect(() => {
        if (!show) {
            setEditingGrouping(null);
            setGroupingName("");
            setSelectedDevices(new Set());
            setIsCreating(false);
        }
    }, [show]);

    const handleCreateNew = () => {
        setIsCreating(true);
        setEditingGrouping(null);
        setGroupingName("");
        setSelectedDevices(new Set());
    };

    const handleEdit = (grouping: Grouping) => {
        setEditingGrouping(grouping);
        setGroupingName(grouping.name);
        setSelectedDevices(new Set(grouping.device_ids));
        setIsCreating(false);
    };

    const handleCancel = () => {
        setEditingGrouping(null);
        setGroupingName("");
        setSelectedDevices(new Set());
        setIsCreating(false);
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
            if (editingGrouping) {
                // Update existing grouping
                await updateGrouping(editingGrouping.id, groupingName, selectedDevices);
            } else {
                // Create new grouping
                await createGrouping(groupingName, selectedDevices);
            }

            // Reload groupings
            await loadGroupings();
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
                body: { grouping_id: groupingId } as any,
                credentials: "same-origin"
            });

            if (response.status === 200) {
                showAlert(t("delete_grouping_success"), "success", undefined, undefined, 3000);
                await loadGroupings();
            } else {
                showAlert(t("delete_grouping_failed", { error: error || response.status }), "danger");
            }
        } catch (error) {
            showAlert(t("delete_grouping_failed", { error: String(error) }), "danger");
        }
    };

    const createGrouping = async (name: string, deviceIds: Set<string>) => {
        const { data, response, error } = await fetchClient.POST("/grouping/create", {
            body: { name } as any,
            credentials: "same-origin"
        });

        if (response.status !== 200 || error || !data) {
            showAlert(t("create_grouping_failed", { error: error || response.status }), "danger");
            throw new Error("Failed to create grouping");
        }

        const groupingId = data.id;

        // Add devices to the grouping
        for (const deviceId of deviceIds) {
            await fetchClient.POST("/grouping/add_device", {
                body: { grouping_id: groupingId, device_id: deviceId } as any,
                credentials: "same-origin"
            });
        }

        showAlert(t("create_grouping_success"), "success", undefined, undefined, 3000);
    };

    const updateGrouping = async (groupingId: string, name: string, deviceIds: Set<string>) => {
        const existingGrouping = groupings.find(g => g.id === groupingId);
        if (!existingGrouping) return;

        // Devices to add
        const devicesToAdd = Array.from(deviceIds).filter(id => !existingGrouping.device_ids.includes(id));
        // Devices to remove
        const devicesToRemove = existingGrouping.device_ids.filter(id => !deviceIds.has(id));

        // Add devices
        for (const deviceId of devicesToAdd) {
            await fetchClient.POST("/grouping/add_device", {
                body: { grouping_id: groupingId, device_id: deviceId } as any,
                credentials: "same-origin"
            });
        }

        // Remove devices
        for (const deviceId of devicesToRemove) {
            await fetchClient.DELETE("/grouping/remove_device", {
                body: { grouping_id: groupingId, device_id: deviceId } as any,
                credentials: "same-origin"
            });
        }

        showAlert(t("update_grouping_success"), "success", undefined, undefined, 3000);
    };

    const loadGroupings = async () => {
        try {
            const { data, error, response } = await fetchClient.GET("/grouping/list", {
                credentials: "same-origin"
            });

            if (error || !data) {
                const errorMsg = error ? String(error) : (response as any)?.status || "Unknown error";
                showAlert(t("load_groupings_failed", { error: errorMsg }), "danger");
                return;
            }

            onGroupingsUpdated((data as any).groupings);
        } catch (error) {
            showAlert(t("load_groupings_failed", { error: String(error) }), "danger");
        }
    };

    const getDeviceName = (deviceId: string): string => {
        const device = devices.find(d => d.id === deviceId);
        return device ? device.name : deviceId;
    };

    const getDeviceUid = (deviceId: string): string => {
        const device = devices.find(d => d.id === deviceId);
        return device ? Base58.int_to_base58(device.uid) : "";
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
                                        <div className="text-muted small">
                                            {grouping.device_ids.length} {t("grouping_devices").toLowerCase()}
                                        </div>
                                    </Col>
                                    <Col xs="auto">
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
                            <div style={{ maxHeight: "300px", overflowY: "auto" }}>
                                <ListGroup>
                                    {devices.map(device => (
                                        <ListGroup.Item
                                            key={device.id}
                                            action
                                            active={selectedDevices.has(device.id)}
                                            onClick={(e: Event) => {
                                                e.preventDefault();
                                                handleDeviceToggle(device.id)
                                            }}
                                            style={{ cursor: "pointer" }}
                                        >
                                            <Form.Check
                                                type="checkbox"
                                                checked={selectedDevices.has(device.id)}
                                                onChange={() => {}} // Handled by ListGroup.Item onClick
                                                label={
                                                    <div>
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
                            </div>
                        </Form.Group>
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
