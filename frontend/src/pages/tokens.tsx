import { useEffect, useState } from 'preact/hooks';
import { Button, Card, Container, Form, Spinner, InputGroup } from 'react-bootstrap';
import { fetchClient, get_decrypted_secret, pub_key } from '../utils';
import { showAlert } from '../components/Alert';
import { Base64 } from 'js-base64';
import { encodeBase58Flickr } from '../base58';
import { useTranslation } from 'react-i18next';
import { Clipboard, Trash2 } from 'react-feather';

interface Token {
    user_email: string;
    user_public_key: string;
    token: string;
}

export function Tokens() {
    const { t } = useTranslation();
    const [tokens, setTokens] = useState([]);
    const [useOnce, setUseOnce] = useState(true);
    const [user, setUser] = useState(null);
    const [loading, setLoading] = useState(true);

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
                const newTokens: {
                    token: Token,
                    use_once: boolean,
                    id: string,
                }[] = [];
                for (const token of tokensData.tokens) {
                    const newToken: Token = {
                        token: token.token,
                        user_email: userData.email,
                        user_public_key: Base64.fromUint8Array(pub_key),
                    }
                    newTokens.push({
                        token: newToken,
                        use_once: token.use_once,
                        id: token.id
                    });
                }
                setTokens(newTokens);

            } catch (err) {
                showAlert(t("tokens.unexpected_error"), "danger");
            } finally {
                setLoading(false);
            }
        }

        fetchTokens();
    }, [t]);

    // Creates a new authorization token on form submission
    const handleCreateToken = async (e: SubmitEvent) => {
        e.preventDefault();
        try {
            const { data, response, error } = await fetchClient.POST('/user/create_authorization_token', {
                body: { use_once: useOnce },
                credentials: 'same-origin'
            });
            if (error || response.status !== 201 || !data) {
                showAlert(t("tokens.create_token_failed"), "danger");
                return;
            }
            const newToken: {
                token: Token,
                use_once: boolean,
                id: string,
            } = {
                token: {
                    token: data.token,
                    user_email: user.email,
                    user_public_key: Base64.fromUint8Array(pub_key),
                },
                use_once: data.use_once,
                id: data.id
            };

            setTokens([...tokens, newToken]);
        } catch (err) {
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
            setTokens(tokens.filter(token => token.id !== tokenToDelete));
        } catch (err) {
            showAlert(t("tokens.unexpected_error"), "danger");
        }
    };

    // Copies the token to the clipboard
    const handleCopyToken = (token: string) => {
        navigator.clipboard.writeText(token).then(() => {
            showAlert(t("tokens.copy_success"), "success");
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
            <Card className="my-4">
                <Card.Header className="pb-2">
                    <h5 className="mb-0">{t("tokens.create_token")}</h5>
                </Card.Header>
                <Card.Body>
                    <Form onSubmit={handleCreateToken}>
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
                    <h5 className="mb-0">{t("tokens.existing_tokens")}</h5>
                </Card.Header>
                <Card.Body>
                    {tokens.map((token, index) => (
                        <>
                        <InputGroup key={index} className={`token-group ${index !== tokens.length - 1 ? 'mb-3' : ''}`}>
                            <Form.Control
                                type="text"
                                readOnly
                                value={encodeBase58Flickr(JSON.stringify(token.token))}
                                className="mb-2 mb-md-0 token-txt"
                            />
                            <div className="d-flex flex-wrap gap-2 gap-md-0 mt-2 mt-md-0">
                                <Button
                                    variant={token.use_once ? "success" : "warning"}
                                    disabled
                                    className="flex-grow-1 flex-md-grow-0"
                                >
                                    {token.use_once ? t("tokens.use_once") : t("tokens.reusable")}
                                </Button>
                                <Button
                                    variant="secondary"
                                    className="flex-grow-1 flex-md-grow-0 d-flex align-items-center justify-content-center gap-2"
                                    onClick={() => handleCopyToken(encodeBase58Flickr(JSON.stringify(token.token)))}
                                >
                                    <Clipboard size={18} />
                                    {t("tokens.copy")}
                                </Button>
                                <Button
                                    variant="danger"
                                    className="flex-grow-1 flex-md-grow-0 d-flex align-items-center justify-content-center gap-2"
                                    onClick={() => handleDeleteToken(token.id)}
                                >
                                    <Trash2 />
                                    {t("tokens.delete")}
                                </Button>
                            </div>
                        </InputGroup>
                        {index !== tokens.length - 1 ? <hr class="d-block d-md-none"/> : <></>}
                        </>
                    ))}
                </Card.Body>
            </Card>
        </Container>
    );
}
