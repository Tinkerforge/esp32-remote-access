import { useTranslation } from "react-i18next";
import { evaluatePasswordStrength, PasswordStrength } from "../utils/passwordStrength";

interface PasswordStrengthIndicatorProps {
    password: string;
    show?: boolean;
}

export function PasswordStrengthIndicator(props: PasswordStrengthIndicatorProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "password_strength" });

    // Don't show anything if explicitly hidden or password is empty
    if (props.show === false || !props.password || props.password.length === 0) {
        return null;
    }

    const strengthInfo = evaluatePasswordStrength(props.password);

    const getStrengthLabel = (strength: PasswordStrength): string => {
        switch (strength) {
            case PasswordStrength.VeryWeak:
                return t("very_weak");
            case PasswordStrength.Weak:
                return t("weak");
            case PasswordStrength.Fair:
                return t("fair");
            case PasswordStrength.Strong:
                return t("strong");
            case PasswordStrength.VeryStrong:
                return t("very_strong");
            default:
                return "";
        }
    };

    return (
        <div className="mt-2">
            <div className="d-flex justify-content-between align-items-center mb-1">
                <small className="text-muted">
                    {t("strength")}: <strong style={{ color: strengthInfo.color }}>
                        {getStrengthLabel(strengthInfo.strength)}
                    </strong>
                </small>
            </div>
            <div
                style={{
                    height: '6px',
                    backgroundColor: '#e9ecef',
                    borderRadius: '3px',
                    overflow: 'hidden'
                }}
            >
                <div
                    style={{
                        width: `${strengthInfo.percentage}%`,
                        height: '100%',
                        backgroundColor: strengthInfo.color,
                        transition: 'all 0.3s ease'
                    }}
                />
            </div>
        </div>
    );
}
