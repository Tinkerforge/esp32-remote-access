import { useEffect, useState } from "preact/hooks";
import Card from "react-bootstrap/Card";
import ListGroup from "react-bootstrap/ListGroup";
import Spinner from "react-bootstrap/Spinner";
import Button from "react-bootstrap/Button";
import Alert from "react-bootstrap/Alert";
import Badge from "react-bootstrap/Badge";
import { useTranslation } from "react-i18next";
import { Wifi, Camera } from "react-feather";
import { DiscoveredDevices } from "../types/window";

type DiscoveryState = "searching" | "done";

/**
 * Displays locally discovered WARP devices on the devices page.
 *
 * Only renders when running inside the WARP Android app, which injects the
 * `tinkerforge_discovery` JS bridge. In regular browsers this component
 * returns null and is effectively a no-op.
 */
export function LocalDevices() {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "local_devices" });
    const [state, setState] = useState<DiscoveryState>("searching");
    const [devices, setDevices] = useState<DiscoveredDevices[]>([]);
    const [provisioningError, setProvisioningError] = useState<string | null>(null);

    const bridge = window.tinkerforge_discovery;
    const provBridge = window.tinkerforge_provisioning;
    const canProvision = provBridge?.isSupported?.() ?? false;

    useEffect(() => {
        if (!bridge) return;

        window.onWarpChargersChanged = (updated: DiscoveredDevices[]) => {
            setDevices([...updated]);
        };

        window.onWarpDiscoveryStopped = () => {
            setState("done");
        };

        window.onWarpProvisioningFailed = (reason: string) => {
            setProvisioningError(reason);
        };

        window.onWarpProvisioningScanCancelled = () => {
            // User cancelled QR scan, nothing to show
        };

        setState("searching");
        bridge.startDiscovery();

        return () => {
            bridge.stopDiscovery();
            window.onWarpChargersChanged = undefined;
            window.onWarpDiscoveryStopped = undefined;
            window.onWarpProvisioningFailed = undefined;
            window.onWarpProvisioningScanCancelled = undefined;
        };
    }, []);

    // Render nothing in regular browsers
    if (!bridge) return null;

    const retry = () => {
        setDevices([]);
        setState("searching");
        bridge.startDiscovery();
    };

    const startProvisioning = () => {
        setProvisioningError(null);
        provBridge?.startProvisioning();
    };

    return (
        <Card className="mb-3">
            <Card.Header className="d-flex align-items-center gap-2">
                <Wifi size={18} />
                <span className="fw-semibold">{t("title")}</span>
                {state === "searching" && (
                    <Spinner animation="border" size="sm" variant="primary" className="ms-auto" />
                )}
            </Card.Header>
            <Card.Body>
                {devices.length > 0 ? (
                    <ListGroup variant="flush">
                        {devices.map((d) => (
                            <ListGroup.Item
                                key={d.serviceName}
                                action
                                onClick={() => bridge.navigateToCharger(d.host)}
                                className="d-flex justify-content-between align-items-center px-0"
                            >
                                <div className="flex-grow-1">
                                    <div className="fw-bold">{d.displayName || d.serviceName}</div>
                                    <small className="text-muted">{d.host}</small>
                                </div>
                                {d.firmwareVersion && (
                                    <Badge bg="secondary" className="ms-2">
                                        <span className="text-muted fw-normal me-1">{t("firmware_version")}</span>
                                        {d.firmwareVersion}
                                    </Badge>
                                )}
                            </ListGroup.Item>
                        ))}
                    </ListGroup>
                ) : state === "searching" ? (
                    <span className="text-muted">{t("searching")}</span>
                ) : (
                    <span className="text-muted">{t("none_found")}</span>
                )}

                {state === "done" && (
                    <Button
                        variant="outline-primary"
                        size="sm"
                        className="mt-2"
                        onClick={retry}
                    >
                        {t("retry")}
                    </Button>
                )}

                {canProvision && (
                    <>
                        <hr className="my-3" />
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
                            {t("provisioning_hint")}
                        </small>
                        <Button
                            variant="outline-primary"
                            size="sm"
                            onClick={startProvisioning}
                        >
                            <Camera size={16} className="me-1" />
                            {t("provisioning_button")}
                        </Button>
                    </>
                )}
            </Card.Body>
        </Card>
    );
}
