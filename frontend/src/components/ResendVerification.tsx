import { useState } from 'preact/hooks';
import { Button, Alert, Spinner } from 'react-bootstrap';
import { fetchClient } from '../utils';
import { useTranslation } from 'react-i18next';

interface Props { email: string; }

export function ResendVerification(props: Props) {
  const { t, i18n } = useTranslation('', { useSuspense: false });
  const [sending, setSending] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function resend() {
    if (sending) return;
    setSending(true);
    setError(null);
    const { response } = await fetchClient.POST('/auth/resend_verification', { body: { email: props.email }, headers: { 'X-Lang': i18n.language }});
    if (response.status === 200) {
      setDone(true);
    } else {
      setError(t('register.resend_error'));
    }
    setSending(false);
  }

  if (!props.email) return null;

  return (
    <div className="mt-3" data-testid="resend-verification">
      {done && <Alert variant="success" data-testid="resend-success">{t('register.resend_success')}</Alert>}
      {error && <Alert variant="danger" data-testid="resend-error">{error}</Alert>}
      {!done && <Button variant="secondary" size="sm" onClick={resend} disabled={sending} data-testid="resend-button">
        {sending && <Spinner as="span" animation="border" size="sm" className="me-1" />}
        {t('register.resend_verification')}
      </Button>}
    </div>
  );
}
