/* esp32-remote-access
 * Copyright (C) 2026 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

import { useEffect, useState } from "preact/hooks";
import { Card, Col, Row, Button, Alert } from "react-bootstrap";
import { useTranslation } from "react-i18next";
import { Camera, ChevronRight } from "react-feather";
import { Circle } from "./Circle";
import { DiscoveredDevices } from "../types/window";


export function LocalDevices() {
    const { t: tLocal } = useTranslation("", { useSuspense: false, keyPrefix: "local_devices" });
    const { t: tProv } = useTranslation("", { useSuspense: false, keyPrefix: "provisioning" });

    const [devices, setDevices] = useState<DiscoveredDevices[] | null>(null);
    const [provisioningError, setProvisioningError] = useState<string | null>(null);

    const canDiscover = window.tinkerforge_discovery?.isSupported?.() ?? false;
    const canProvision = window.tinkerforge_provisioning?.isSupported?.() ?? false;

    useEffect(() => {
        if (!canDiscover) return;

        const bridge = window.tinkerforge_discovery;
        if (!bridge) return;

        window.onWarpChargersChanged = (updated: DiscoveredDevices[]) => {
            setDevices(updated);
        };

        window.onWarpDiscoveryStopped = () => {
            // Keep the last known list. The next discovery session will push
            // a fresh list via `onWarpChargersChanged`.
        };

        bridge.startDiscovery();

        return () => {
            window.tinkerforge_discovery?.stopDiscovery();
            window.onWarpChargersChanged = undefined;
            window.onWarpDiscoveryStopped = undefined;
        };
    }, [canDiscover]);

    useEffect(() => {
        if (!canProvision) return;

        window.onWarpProvisioningFailed = (reason: string) => {
            setProvisioningError(reason);
        };

        window.onWarpProvisioningScanCancelled = () => {
            // User cancelled QR scan, nothing to show
        };

        return () => {
            window.onWarpProvisioningFailed = undefined;
            window.onWarpProvisioningScanCancelled = undefined;
        };
    }, [canProvision]);

    if (!canDiscover && !canProvision) return null;

    // Hide the device list while we are still waiting for the first
    // discovery result so we do not flash an empty list at the user. Once
    // a discovery session has run and produced no devices we show the
    // "empty" hint to confirm the bridge actually executed.
    const isEmpty = devices !== null && devices.length === 0;
    const showBothSections = canProvision && canDiscover;

    const startProvisioning = () => {
        setProvisioningError(null);
        window.tinkerforge_provisioning?.startProvisioning();
    };

    const handleConnect = (host: string) => {
        window.tinkerforge_discovery?.navigateToCharger(host);
    };

    return (
        <Card className="mb-3">
            <Card.Header className="d-flex align-items-center gap-2">
                <span className="fw-semibold">{tLocal("title")}</span>
            </Card.Header>
            <Card.Body>
                {canProvision && (
                    <div>
                        {provisioningError && (
                            <Alert
                                variant="danger"
                                dismissible
                                onClose={() => setProvisioningError(null)}
                                className="mb-2"
                            >
                                {provisioningError}
                            </Alert>
                        )}
                        <small className="text-muted mb-2 d-block">
                            {tProv("provisioning_hint")}
                        </small>
                        <Button
                            variant="outline-primary"
                            size="sm"
                            onClick={startProvisioning}
                        >
                            <Camera size={16} className="me-1" />
                            {tProv("provisioning_button")}
                        </Button>
                    </div>
                )}
                {showBothSections && <hr className="my-3" />}
                {canDiscover && (
                    <div>
                        {devices === null ? (
                            <div className="text-muted">
                                <span>{tLocal("searching")}</span>
                            </div>
                        ) : isEmpty ? (
                            <div className="text-muted">
                                <span>{tLocal("none_found")}</span>
                            </div>
                        ) : (
                            devices.map((device) => {
                                const connect = () => handleConnect(device.host);
                                const activateOnKey = (e: KeyboardEvent) => {
                                    if (e.key === "Enter" || e.key === " ") {
                                        e.preventDefault();
                                        connect();
                                    }
                                };
                                const ariaLabel = device.displayName || device.serviceName || device.host;
                                return (
                                    <Row
                                        key={`${device.host}:${device.port}`}
                                        className="align-items-center mx-0 py-2 border-bottom"
                                        role="button"
                                        tabIndex={0}
                                        onClick={connect}
                                        onKeyDown={activateOnKey}
                                        aria-label={`${tLocal("connect")} ${ariaLabel}`}
                                        style={{ cursor: "pointer" }}
                                    >
                                        <Col xs="auto" className="ps-3">
                                            <Circle color="success" />
                                        </Col>
                                        <Col className="text-break">
                                            <div className="fw-semibold">
                                                {device.displayName || device.serviceName || device.host}
                                            </div>
                                            <small className="text-muted">
                                                {device.host}
                                                {device.model ? ` · ${device.model}` : ""}
                                                {device.firmwareVersion
                                                    ? ` · ${tLocal("firmware")} ${device.firmwareVersion}`
                                                    : ""}
                                            </small>
                                        </Col>
                                        <Col xs="auto" className="pe-3 text-muted">
                                            <ChevronRight size={20} />
                                        </Col>
                                    </Row>
                                );
                            })
                        )}
                    </div>
                )}
            </Card.Body>
        </Card>
    );
}
