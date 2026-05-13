import { useTranslation } from "react-i18next";
import logo from "logo";
import { app_store_link, play_store_link } from "links";
import Median from "median-js-bridge";

interface MobileAppHintProps {
    compact?: boolean;
}

export function MobileAppHint({ compact = false }: MobileAppHintProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "mobile_app" });

    if (Median.isNativeApp()) {
        return null;
    }

    if (compact) {
        return (
            <div className="d-flex align-items-center gap-2">
                <img src={logo} alt="logo" style={{ height: "24px" }} />
                <span>{t("hint_title")}</span>
                <a href={app_store_link} target="_blank" rel="noopener noreferrer" className="text-white">
                    {t("app_store")}
                </a>
                <span>|</span>
                <a href={play_store_link} target="_blank" rel="noopener noreferrer" className="text-white">
                    {t("play_store")}
                </a>
            </div>
        );
    }

    return (
        <div className="text-center p-3 mt-3 border rounded bg-light">
            <img src={logo} alt="logo" style={{ height: "40px" }} className="mb-2" />
            <h6>{t("hint_title")}</h6>
            <p className="mb-2 text-muted">{t("hint_text")}</p>
            <div className="d-flex justify-content-center gap-3">
                <a href={app_store_link} target="_blank" rel="noopener noreferrer" className="btn btn-outline-dark btn-sm">
                    {t("app_store")}
                </a>
                <a href={play_store_link} target="_blank" rel="noopener noreferrer" className="btn btn-outline-dark btn-sm">
                    {t("play_store")}
                </a>
            </div>
        </div>
    );
}
