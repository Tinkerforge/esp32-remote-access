import { render, fireEvent, waitFor, screen } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { Login } from '../Login';


describe('Login Component', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mockUtils: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mockAlert: any;

  beforeEach(async () => {
    vi.clearAllMocks();
    mockUtils = await import('../../utils');
    mockAlert = await import('../Alert');
    await import('js-base64');

    // default happy path mocks
    mockUtils.get_salt_for_user.mockResolvedValue(new Uint8Array([1, 2, 3]));
    mockUtils.generate_hash.mockResolvedValue(new Uint8Array([9, 10, 11]));
    mockUtils.fetchClient.POST.mockResolvedValue({ response: { status: 200 }, error: null });
    mockUtils.fetchClient.GET.mockResolvedValue({
      data: { secret_salt: [5, 6, 7] },
      response: { status: 200 },
      error: null,
    });
  });

  function fillAndSubmit(email = 'user@example.com', password = 'ValidPass123!') {
    render(<Login />);
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    fireEvent.change(emailInput, { target: { value: email } });
    fireEvent.change(passwordInput, { target: { value: password } });
    fireEvent.click(screen.getByRole('button', { name: 'login' }));
  }

  it('renders form fields', () => {
    render(<Login />);
    expect(screen.getByRole('textbox', { name: 'email' })).toBeTruthy();
    expect(screen.getByRole('textbox', { name: 'password' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'login' })).toBeTruthy();
  });

  it('sets credentials_wrong when get_salt_for_user fails', async () => {
    mockUtils.get_salt_for_user.mockRejectedValue('no user');
    fillAndSubmit();

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).not.toHaveBeenCalled();
      expect(screen.getByRole('textbox', { name: 'email' })).toHaveClass('invalid');
    });
  });

  it('shows verification required alert on 403 login', async () => {
    mockUtils.fetchClient.POST.mockResolvedValue({ response: { status: 403 }, error: null });
    fillAndSubmit();

    await waitFor(() => {
      expect(mockAlert.showAlert).toHaveBeenCalledWith(
        'login.verify_before_login',
        'danger',
        'login',
        'login.verify_before_login_heading'
      );
      expect(mockUtils.fetchClient.GET).not.toHaveBeenCalled();
    });
  });

  it('marks credentials wrong when POST returns error', async () => {
    mockUtils.fetchClient.POST.mockResolvedValue({ response: { status: 500 }, error: 'err' });
    fillAndSubmit();

    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'email' })).toHaveClass('invalid');
    });
  });

  it('alerts when secret retrieval fails (non-200)', async () => {
    mockUtils.fetchClient.GET.mockResolvedValue({
      data: null,
      response: { status: 500 },
      error: 'boom',
    });
    fillAndSubmit();

    await waitFor(() => {
      expect(mockAlert.showAlert).toHaveBeenCalledWith(
        'Failed with status 500: boom',
        'danger'
      );
    });
  });

  it('successful login flow stores key, updates state and posts broadcast', async () => {
    fillAndSubmit();

    await waitFor(() => {
      expect(window.localStorage.setItem).toHaveBeenCalledWith('loginSalt', 'encoded');
      expect(mockUtils.generate_hash).toHaveBeenCalledTimes(2);
      expect(mockUtils.storeSecretKeyInServiceWorker).toHaveBeenCalledWith('encoded');
      expect(mockUtils.loggedIn.value).toBe(mockUtils.AppState.LoggedIn);
      expect(mockUtils.bc.postMessage).toHaveBeenCalledWith('login');
    });
  });

  it('opens and submits password recovery modal success', async () => {
    render(<Login />);
    fireEvent.click(screen.getByText('password_recovery'));
    expect(screen.getByTestId('modal')).toBeTruthy();

    mockUtils.fetchClient.GET.mockResolvedValueOnce({ response: { status: 200 } });
    const emailInput = screen
      .getAllByRole('textbox', { name: 'email' })
      .find((i) => (i as HTMLInputElement).id === 'startRecoveryEmail');
    if (!emailInput) throw new Error('Recovery email input not found');
    fireEvent.change(emailInput, { target: { value: 'recover@example.com' } });

    const sendBtn = screen.getByRole('button', { name: 'send' });
    fireEvent.click(sendBtn);

    await waitFor(() => {
      expect(mockAlert.showAlert).toHaveBeenCalledWith(
        'success_alert_text',
        'success',
        'login',
        'success_alert_heading'
      );
    });
  });

  it('password recovery handles error', async () => {
    render(<Login />);
    fireEvent.click(screen.getByText('password_recovery'));
    expect(screen.getByTestId('modal')).toBeTruthy();

    mockUtils.fetchClient.GET.mockResolvedValueOnce({
      response: { status: 500 },
      error: 'fail',
    });
    const emailInput = screen
      .getAllByRole('textbox', { name: 'email' })
      .find((i) => (i as HTMLInputElement).id === 'startRecoveryEmail');
    if (!emailInput) throw new Error('Recovery email input not found');
    fireEvent.change(emailInput, { target: { value: 'recover@example.com' } });

    const sendBtn = screen.getByRole('button', { name: 'send' });
    fireEvent.click(sendBtn);

    await waitFor(() => {
      expect(mockAlert.showAlert).toHaveBeenCalled();
    });
  });
});
