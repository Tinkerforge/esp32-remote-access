import { useState } from "preact/hooks";
import { Button, Form, InputGroup } from "react-bootstrap";
import { Eye, EyeOff } from "react-feather";
import { useTranslation } from "react-i18next";

interface PasswordComponentProps {
    onChange: (e: Event) => void,
    isInvalid?: boolean,
    invalidMessage?: string,
}

export function PasswordComponent(props: PasswordComponentProps) {
    const [showPassword, setShowPassword] = useState(false);
    const {t} = useTranslation("", {useSuspense: false, keyPrefix: "login"})
    return <InputGroup hasValidation>
        <Form.Control
            placeholder={t("password")}
            type={showPassword ? "text" : "password"}
            onChange={props.onChange}
            isInvalid={props.isInvalid} />
        <Button
            variant="outline-primary"
            onClick={(e: InputEvent) => {
                e.preventDefault();
                setShowPassword(!showPassword);
            }}>
            {!showPassword ? <Eye/> : <EyeOff/>}
        </Button>
        <Form.Control.Feedback type="invalid">
            {props.invalidMessage}
        </Form.Control.Feedback>
    </InputGroup>
}
