import { useEffect, useState } from "preact/hooks";
import Card from "react-bootstrap/Card";
import Button from "react-bootstrap/Button";
import Alert from "react-bootstrap/Alert";
import { useTranslation } from "react-i18next";
import { Camera } from "react-feather";

/**
 * QR-code provisioning panel shown on the devices page.
 *
 * Only renders when running inside the WARP Android app, which injects the
 * `tinkerforge_provisioning` JS bridge. In regular browsers this component
 * returns null
 */
export function Provisioning() {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "provisioning" });
    const [provisioningError, setProvisioningError] = useState<string | null>(null);

    const provBridge = window.tinkerforge_provisioning;
    const canProvision = provBridge?.isSupported?.() ?? false;

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

    if (!canProvision) return null;

    const startProvisioning = () => {
        setProvisioningError(null);
        provBridge?.startProvisioning();
    };

    return (
        <Card className="mb-3">
            <Card.Header className="d-flex align-items-center gap-2">
                <Camera size={18} />
                <span className="fw-semibold">{t("title")}</span>
            </Card.Header>
            <Card.Body>
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
            </Card.Body>
        </Card>
    );
}
