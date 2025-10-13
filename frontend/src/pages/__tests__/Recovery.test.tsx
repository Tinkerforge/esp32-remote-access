import { render, screen, fireEvent, waitFor } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../components/Alert', async () => {
  return {
    showAlert: vi.fn(),
  };
});

vi.mock('../../components/RecoveryDataComponent', async () => {
  return {
    RecoveryDataComponent: () => null,
  };
});

vi.mock('preact-iso', async () => ({
  useLocation: () => ({ route: vi.fn(), query: { token: 'tok', email: 'e@example.com' } }),
}));

// Mock utils to avoid crypto and network
vi.mock('../../utils', async () => {
  return {
    AppState: { LoggedOut: 2 },
    PASSWORD_PATTERN: /.+/,
    concat_salts: vi.fn((a: Uint8Array) => a),
    fetchClient: { POST: vi.fn().mockResolvedValue({ response: { status: 200 } }) },
    generate_hash: vi.fn(async () => new Uint8Array([1,2,3])),
    generate_random_bytes: vi.fn(() => new Uint8Array([4,5,6])),
    get_salt: vi.fn(async () => new Uint8Array([7,8,9])),
    loggedIn: { value: 0 },
  };
});

vi.mock('libsodium-wrappers', async () => ({
  crypto_box_keypair: () => ({ publicKey: new Uint8Array([1]), privateKey: new Uint8Array([2,3,4]) }),
  crypto_secretbox_KEYBYTES: 32,
  crypto_secretbox_NONCEBYTES: 24,
  crypto_secretbox_easy: vi.fn(() => new Uint8Array([9,9,9]))
}));

import { Recovery } from '../Recovery';

describe('Recovery page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows warning modal when submitting without recovery file', async () => {
    render(<Recovery />);

    // Fill password and confirm password to satisfy validation
    const passInputs = screen.getAllByTestId('password-input') as HTMLInputElement[];
    fireEvent.change(passInputs[0], { target: { value: 'ValidPass123!' } });
    fireEvent.change(passInputs[1], { target: { value: 'ValidPass123!' } });

        const form = screen.getByTestId('form');
    fireEvent.submit(form);

    // Modal should appear with heading and proceed button
    await waitFor(() => {
      expect(screen.getByText('recovery.no_file_warning_heading')).toBeTruthy();
      expect(screen.getByText('recovery.no_file_warning_proceed')).toBeTruthy();
    });

    const proceed = screen.getByText('recovery.no_file_warning_proceed') as HTMLButtonElement;
    expect(proceed.disabled).toBe(true);

    // Check the acknowledgment checkbox to enable proceed
    const ack = screen.getByLabelText('recovery.no_file_warning_ack');
    fireEvent.click(ack);
    expect(proceed.disabled).toBe(false);
  });
});
