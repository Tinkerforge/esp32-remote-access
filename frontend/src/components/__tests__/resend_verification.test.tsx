import { render, fireEvent, waitFor, screen } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ResendVerification } from '../ResendVerification';

// Mock utils (fetchClient) – path resolution to actual file used by component
vi.mock('../../utils', () => ({
  fetchClient: {
    POST: vi.fn(),
  },
}));

// Mock i18n
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: 'en' },
  }),
}));

describe('ResendVerification', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mockUtils: any;

  beforeEach(async () => {
    vi.clearAllMocks();
    mockUtils = await import('../../utils');
  });

  it('returns null when email prop is empty', () => {
    const { container } = render(<ResendVerification email="" />);
    expect(container.firstChild).toBeNull();
    expect(screen.queryByTestId('resend-verification')).toBeNull();
  });

  it('renders resend button when email provided', () => {
    render(<ResendVerification email="user@example.com" />);
    expect(screen.getByTestId('submit-button')).toBeTruthy();
  });

  it('calls API and shows success alert on 200', async () => {
    mockUtils.fetchClient.POST.mockResolvedValue({ response: { status: 200 } });

    render(<ResendVerification email="user@example.com" />);
  const btn = screen.getByTestId('submit-button');
    fireEvent.click(btn);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).toHaveBeenCalledWith('/auth/resend_verification', {
        body: { email: 'user@example.com' },
        headers: { 'X-Lang': 'en' },
      });
      expect(screen.getByTestId('resend-success')).toBeTruthy();
      // Button should disappear after success
  expect(screen.queryByTestId('submit-button')).toBeNull();
    });
  });

  it('calls API and shows error alert on non-200', async () => {
    mockUtils.fetchClient.POST.mockResolvedValue({ response: { status: 500 } });

    render(<ResendVerification email="user2@example.com" />);
  const btn = screen.getByTestId('submit-button');
    fireEvent.click(btn);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).toHaveBeenCalledTimes(1);
      expect(screen.getByTestId('resend-error')).toBeTruthy();
      // Button still present (still can retry) since done=false
  expect(screen.getByTestId('submit-button')).toBeTruthy();
    });
  });

  it('shows spinner and prevents double submission while sending', async () => {
    let resolvePromise: (value: unknown) => void;
    const pending = new Promise(resolve => { resolvePromise = resolve; });
    mockUtils.fetchClient.POST.mockReturnValue(pending);

    render(<ResendVerification email="spinner@example.com" />);
  const btn = screen.getByTestId('submit-button');
    fireEvent.click(btn);
    // Immediate second click attempt
    fireEvent.click(btn);

    expect(mockUtils.fetchClient.POST).toHaveBeenCalledTimes(1);

    // Spinner should be visible while pending (mocked Spinner renders 'Loading...')
    expect(screen.getByText('Loading...')).toBeTruthy();

    // Finish request with non-success to keep button
    // @ts-expect-error resolvePromise defined above
    resolvePromise({ response: { status: 500 } });

    await waitFor(() => {
      expect(screen.getByTestId('resend-error')).toBeTruthy();
    });
  });
});
