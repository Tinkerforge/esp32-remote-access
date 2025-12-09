import { useEffect, useMemo, useState } from 'preact/hooks';
import { Button, Card, Container, Form, Spinner, InputGroup, Alert } from 'react-bootstrap';
import { fetchClient, get_decrypted_secret, pub_key, secret } from '../utils';
import { showAlert } from '../components/Alert';
import { Base64 } from 'js-base64';
import { encodeBase58Flickr } from '../base58';
import { useTranslation } from 'react-i18next';
import { Clipboard, Trash2 } from 'react-feather';
import { components } from '../schema';
import { ArgonType, hash } from 'argon2-browser';
import sodium from 'libsodium-wrappers';

type TokenRecord = {
    token: string,
    use_once: boolean,
    id: string,
    name: string,
    createdAt: Date,
    lastUsedAt: Date | null,
};

type SortOption = 'created-desc' | 'created-asc' | 'name-asc' | 'name-desc' | 'last-used-desc';

const SORT_OPTIONS: { value: SortOption, labelKey: string }[] = [
    { value: 'created-desc', labelKey: 'tokens.sort_created_desc' },
    { value: 'created-asc', labelKey: 'tokens.sort_created_asc' },
    { value: 'name-asc', labelKey: 'tokens.sort_name_asc' },
    { value: 'name-desc', labelKey: 'tokens.sort_name_desc' },
    { value: 'last-used-desc', labelKey: 'tokens.sort_last_used_desc' },
];

const nameCollator = new Intl.Collator(undefined, { sensitivity: 'accent', numeric: true });

async function buildToken(userData: components["schemas"]["UserInfo"], tokenData: components["schemas"]["GetAuthorizationTokensResponseSchema"]["tokens"][0]) {
    // Reserve a buffer with documented size
    const token = Base64.toUint8Array(tokenData.token);
    const encoder = new TextEncoder();
    const id = encoder.encode(userData.id);
    const email = encoder.encode(userData.email);

    const dataBuf = new Uint8Array(32 + 36 +32 + email.length);
    dataBuf.set(token);
    dataBuf.set(id, 32);

    // The pub_key will never be null here.
    dataBuf.set(pub_key as Uint8Array, 32 + 36);
    dataBuf.set(email, 32 + 36 + 32);

    // Use argon2 here since browsers think it's a good idea to block crypto.subtle due to insecure contexts
    // (Wallbox interface is served over HTTP)
    const digest = await hash({
        pass: dataBuf,
        salt: new Uint8Array(8),
        time: 2,
        mem: 19 * 1024,
        hashLen: 32,
        parallelism: 1,
        type: ArgonType.Argon2id,
    })

    const completeBuf = new Uint8Array(dataBuf.length + digest.hash.length);
    completeBuf.set(dataBuf);
    completeBuf.set(digest.hash, dataBuf.length);

    const encoded = encodeBase58Flickr(completeBuf);

    return encoded;
}

let fetchInterval: NodeJS.Timeout | null = null;
export function Tokens() {
    const { t } = useTranslation();
    const [tokens, setTokens] = useState<TokenRecord[]>([]);
    const [useOnce, setUseOnce] = useState(true);
    const [tokenName, setTokenName] = useState("");
    const [user, setUser] = useState<components["schemas"]["UserInfo"] | null>(null);
    const [loading, setLoading] = useState(true);
    const [sortOption, setSortOption] = useState<SortOption>('created-desc');
    const [searchQuery, setSearchQuery] = useState("");

    const sortedTokens = useMemo(() => {
        const copy = [...tokens];
        copy.sort((a, b) => {
            switch (sortOption) {
                case 'created-asc':
                    return a.createdAt.getTime() - b.createdAt.getTime();
                case 'name-asc':
                    return nameCollator.compare(a.name || '', b.name || '');
                case 'name-desc':
                    return nameCollator.compare(b.name || '', a.name || '');
                case 'last-used-desc': {
                    const lastUsedA = a.lastUsedAt ? a.lastUsedAt.getTime() : 0;
                    const lastUsedB = b.lastUsedAt ? b.lastUsedAt.getTime() : 0;
                    return lastUsedB - lastUsedA;
                }
                case 'created-desc':
                default:
                    return b.createdAt.getTime() - a.createdAt.getTime();
            }
        });
        return copy;
    }, [tokens, sortOption]);

    const filteredTokens = useMemo(() => {
        const normalizedQuery = searchQuery.trim().toLowerCase();
        if (!normalizedQuery) {
            return sortedTokens;
        }

        return sortedTokens.filter((token) => {
            const nameMatch = token.name.toLowerCase().includes(normalizedQuery);
            return nameMatch;
        });
    }, [searchQuery, sortedTokens]);

    // Fetch tokens and user data from the server on component mount
    useEffect(() => {
        async function fetchTokens() {
            try {
                // Fetch user data
                const { data: userData, response: userResponse, error: userError } =
                    await fetchClient.GET("/user/me", { credentials: "same-origin" });
                if (userError || userResponse.status !== 200 || !userData) {
                    showAlert(t("tokens.fetch_user_failed"), "danger");
                    return;
                }
                setUser(userData);

                // Fetch tokens data
                const { data: tokensData, response: tokensResponse, error: tokensError } =
                    await fetchClient.GET('/user/get_authorization_tokens', { credentials: 'same-origin' });
                if (tokensError || tokensResponse.status !== 200 || !tokensData) {
                    showAlert(t("tokens.fetch_tokens_failed"), "danger");
                    return;
                }

                // Ensure public key is available
                if (!pub_key) {
                    await get_decrypted_secret();
                }

                // Process and set tokens
                const newTokens: TokenRecord[] = [];
                for (const token of tokensData.tokens) {
                    const newToken = await buildToken(userData, token);
                    let tokenName = "";
                    if (token.name.length !== 0) {
                        const binaryName = Base64.toUint8Array(token.name);
                        tokenName = new TextDecoder().decode(sodium.crypto_box_seal_open(binaryName, pub_key as Uint8Array, secret as Uint8Array));
                    }
                    newTokens.push({
                        token: newToken,
                        use_once: token.use_once,
                        id: token.id,
                        name: tokenName,
                        createdAt: new Date(token.created_at * 1000),
                        lastUsedAt: token.last_used_at ? new Date(token.last_used_at * 1000) : null,
                    });
                }
                setTokens(newTokens);

            } catch (err) {
                console.error(err);
                showAlert(t("tokens.unexpected_error"), "danger");
            } finally {
                setLoading(false);
            }
        }

        fetchTokens();
        fetchInterval = setInterval(() => {
            fetchTokens();
        }, 5000);

        return () => {
            if (fetchInterval) {
                clearInterval(fetchInterval);
            }
        }
    }, [t]);

    // Creates a new authorization token on form submission
    const handleCreateToken = async (e: SubmitEvent) => {
        e.preventDefault();
        try {
            await sodium.ready;
            // Generate automatic name if empty
            let finalTokenName = tokenName.trim();
            if (finalTokenName === "") {
                // Find the highest existing "Token-N" number
                let maxTokenNumber = 0;
                const tokenPattern = /^Token-(\d+)$/;
                for (const token of tokens) {
                    const match = token.name.match(tokenPattern);
                    if (match) {
                        const num = parseInt(match[1], 10);
                        if (num > maxTokenNumber) {
                            maxTokenNumber = num;
                        }
                    }
                }
                finalTokenName = `Token-${maxTokenNumber + 1}`;
            }
            const encryptedTokenName = sodium.crypto_box_seal(finalTokenName, pub_key as Uint8Array);
            const { data, response, error } = await fetchClient.POST('/user/create_authorization_token', {
                body: { use_once: useOnce, name: Base64.fromUint8Array(encryptedTokenName) },
                credentials: 'same-origin'
            });
            if (error || response.status !== 201 || !data || !user) {
                showAlert(t("tokens.create_token_failed"), "danger");
                return;
            }
            const newToken: TokenRecord = {
                token: await buildToken(user, data),
                use_once: data.use_once,
                id: data.id,
                name: finalTokenName,
                createdAt: new Date(data.created_at * 1000),
                lastUsedAt: data.last_used_at ? new Date(data.last_used_at * 1000) : null,
            };

            setTokens((prev) => [...prev, newToken]);
            setTokenName(""); // Clear the name field after successful creation
        } catch (err) {
            console.error(err);
            showAlert(t("tokens.unexpected_error"), "danger");
        }
    };

    // Deletes an existing authorization token
    const handleDeleteToken = async (tokenToDelete: string) => {
        try {
            const { response, error } = await fetchClient.DELETE('/user/delete_authorization_token', {
                body: { id: tokenToDelete },
                credentials: 'same-origin'
            });
            if (error || response.status !== 200) {
                showAlert(t("tokens.delete_token_failed"), "danger");
                return;
            }
            setTokens((prev) => prev.filter(token => token.id !== tokenToDelete));
        } catch (err) {
            console.error(err);
            showAlert(t("tokens.unexpected_error"), "danger");
        }
    };

    // Copies the token to the clipboard
    const handleCopyToken = (token: string) => {
        navigator.clipboard.writeText(token).then(() => {
            showAlert(t("tokens.copy_success_text"), "success", "token_copy", t("tokens.copy_success"), 2000);
        }).catch(() => {
            showAlert(t("tokens.copy_failed"), "danger");
        });
    };

    if (loading) {
        return (
            <div className="d-flex justify-content-center align-items-center p-5">
                <Spinner animation="border" variant="primary" />
            </div>
        );
    }

    return (
        <Container>
            <Alert variant="info" className="mt-3">
                <h5 className="mb-1">{t("tokens.info_heading")}</h5>
                <p className="mb-0">{t("tokens.info_body")}</p>
            </Alert>
            <Card className="my-4">
                <Card.Header className="pb-2">
                    <h5 className="mb-0">{t("tokens.create_token")}</h5>
                </Card.Header>
                <Card.Body>
                    <Form onSubmit={handleCreateToken}>
                        <Form.Group className="mb-3">
                            <Form.Label>{t("tokens.name")}</Form.Label>
                            <Form.Control
                                type="text"
                                placeholder={t("tokens.name_placeholder")}
                                value={tokenName}
                                onChange={(e) => setTokenName((e.target as HTMLInputElement).value)}
                            />
                        </Form.Group>
                        <div className="d-flex align-items-center justify-content-between">
                            <Form.Check
                                type="switch"
                                id="useOnce"
                                label={t("tokens.use_once")}
                                checked={useOnce}
                                onChange={(e) => setUseOnce((e.target as HTMLInputElement).checked)}
                            />
                            <Button
                                variant="primary"
                                type="submit"
                                className="px-4"
                            >
                                {t("tokens.create")}
                            </Button>
                        </div>
                        <Form.Text className="text-muted small">
                            {useOnce ? t("tokens.single_use_description") : t("tokens.multi_use_description")}
                        </Form.Text>
                    </Form>
                </Card.Body>
                <Card.Header className="border-top pb-2">
                    <div className="d-flex flex-column gap-3">
                        <div className="d-flex flex-column flex-md-row align-items-md-center justify-content-between gap-3">
                            <h5 className="mb-0">{t("tokens.existing_tokens")}</h5>
                            <Form.Select
                                className="w-auto"
                                value={sortOption}
                                onChange={(e) => setSortOption((e.target as HTMLSelectElement).value as SortOption)}
                                aria-label={t("tokens.sort_label")} // needed for testing
                            >
                                {SORT_OPTIONS.map((option) => (
                                    <option key={option.value} value={option.value}>
                                        {t(option.labelKey)}
                                    </option>
                                ))}
                            </Form.Select>
                        </div>
                        <Form.Control
                            type="search"
                            className="mb-2"
                            placeholder={t("tokens.search_placeholder")}
                            value={searchQuery}
                            onChange={(e) => setSearchQuery((e.target as HTMLInputElement).value)}
                            aria-label={t("tokens.search_label")} //needed for testing
                        />
                    </div>
                </Card.Header>
                <Card.Body>
                    {filteredTokens.length === 0 ? (
                        <p className="text-muted fst-italic mb-0">
                            {searchQuery.trim() ? t("tokens.no_tokens_search") : t("tokens.no_tokens")}
                        </p>
                    ) : filteredTokens.map((token, index) => {
                        const isUsed = token.use_once && token.lastUsedAt !== null;
                        const statusVariant = isUsed ? "danger" : (token.use_once ? "success" : "warning");
                        const statusText = isUsed ? t("tokens.used") : (token.use_once ? t("tokens.use_once") : t("tokens.reusable"));

                        return (
                            <div key={token.id} className={`token-item ${index !== filteredTokens.length - 1 ? 'mb-4' : ''}`}>
                                <div className="d-flex justify-content-between align-items-start mb-2">
                                    <div>
                                        <h6 className={`mb-1 fw-bold ${isUsed ? 'text-muted' : ''}`}>{token.name}</h6>
                                        <small className="text-muted">
                                            {t("tokens.created")}: {token.createdAt.toLocaleDateString()} {token.createdAt.toLocaleTimeString()}
                                        </small>
                                        <br />
                                        <small className="text-muted">
                                            {token.use_once ? t("tokens.used") : t("tokens.last_used")}: {token.lastUsedAt ?
                                                `${token.lastUsedAt.toLocaleDateString()} ${token.lastUsedAt.toLocaleTimeString()}` :
                                                t("tokens.never_used")
                                            }
                                        </small>
                                    </div>
                                    <div className="d-flex gap-2">
                                        <Button
                                            variant={statusVariant}
                                            disabled
                                            size="sm"
                                        >
                                            {statusText}
                                        </Button>
                                    </div>
                                </div>
                                <InputGroup className="mb-2">
                                    <Form.Control
                                        type="text"
                                        readOnly
                                        value={isUsed ? "••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••" : token.token}
                                        className={`${isUsed ? 'text-muted' : ''}`}
                                        style={isUsed ? { fontFamily: 'monospace' } : {}}
                                    />
                                </InputGroup>
                                <Alert variant="info" className="mb-2 py-2">
                                    <small>{t("tokens.firmware_needed")}</small>
                                </Alert>
                                <div className="d-flex flex-wrap gap-2">
                                    <Button
                                        variant="secondary"
                                        size="sm"
                                        className="d-flex align-items-center gap-2"
                                        onClick={() => handleCopyToken(token.token)}
                                        disabled={isUsed}
                                    >
                                        <Clipboard size={16} />
                                        {t("tokens.copy")}
                                    </Button>
                                    <Button
                                        variant="danger"
                                        size="sm"
                                        className="d-flex align-items-center gap-2"
                                        onClick={() => handleDeleteToken(token.id)}
                                    >
                                        <Trash2 size={16} />
                                        {t("tokens.delete")}
                                    </Button>
                                </div>
                                {index !== filteredTokens.length - 1 && <hr className="mt-3" />}
                            </div>
                        );
                    })}
                </Card.Body>
            </Card>
        </Container>
    );
}
