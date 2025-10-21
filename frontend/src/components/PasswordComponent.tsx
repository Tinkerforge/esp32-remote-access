import { useState } from "preact/hooks";
import { Button, Form, InputGroup } from "react-bootstrap";
import { Eye, EyeOff } from "react-feather";
import { useTranslation } from "react-i18next";
import { PasswordStrengthIndicator } from "./PasswordStrengthIndicator";

interface PasswordComponentProps {
    onChange: (e: string) => void,
    isInvalid?: boolean,
    invalidMessage?: string,
    showStrength?: boolean,
    value: string,
}

export function PasswordComponent(props: PasswordComponentProps) {
    const [showPassword, setShowPassword] = useState(false);
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "login"})

    const handleChange = (e: Event) => {
        const value = (e.target as HTMLInputElement).value;
        props.onChange(value);
    };

    return <>
        <InputGroup hasValidation>
            <Form.Control
                placeholder={t("password")}
                type={showPassword ? "text" : "password"}
                onChange={handleChange}
                value={props.value}
                isInvalid={props.isInvalid} />
            <Button
                variant="outline-primary"
                onClick={(e: Event) => {
                    e.preventDefault();
                    setShowPassword(!showPassword);
                }}>
                {!showPassword ? <Eye /> : <EyeOff />}
            </Button>
            <Form.Control.Feedback type="invalid">
                {props.invalidMessage}
            </Form.Control.Feedback>
        </InputGroup>
        {props.showStrength && (
            <PasswordStrengthIndicator
                password={props.value}
            />
        )}
    </>
}
