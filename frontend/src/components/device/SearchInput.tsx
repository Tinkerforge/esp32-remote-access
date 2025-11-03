import { useTranslation } from "react-i18next";
import { Form, InputGroup } from "react-bootstrap";
import { Search, X } from "react-feather";
import Median from "median-js-bridge";

interface SearchInputProps {
    searchTerm: string;
    onSearchChange: (searchTerm: string) => void;
    placeholder?: string;
}

export function SearchInput({ searchTerm, onSearchChange, placeholder }: SearchInputProps) {
    const { t } = useTranslation("", { useSuspense: false, keyPrefix: "chargers" });

    const handleClear = () => {
        onSearchChange("");
    };

    return (
        <InputGroup>
            <InputGroup.Text>
                <Search size={16} />
            </InputGroup.Text>
            <Form.Control
                type="text"
                placeholder={placeholder || t("search_devices_placeholder")}
                value={searchTerm}
                onChange={(e) => onSearchChange((e.target as HTMLInputElement).value)}
                aria-label={t("search_devices")}
            />
            {searchTerm && (
                <InputGroup.Text
                    onClick={handleClear}
                    style={{ cursor: "pointer" }}
                    title={t("clear_search")}
                >
                    <X size={16} />
                </InputGroup.Text>
            )}
        </InputGroup>
    );
}
