import { render, fireEvent, waitFor, screen } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Register } from '../Register';


// Mock all imports
vi.mock('../Alert', () => ({
  showAlert: vi.fn(),
}));

vi.mock('../../utils', () => ({
  fetchClient: {
  POST: vi.fn(),
},
  generate_hash: vi.fn(),
  generate_random_bytes: vi.fn(),
  get_salt: vi.fn(),
}));

vi.mock('react-i18next', () => ({
  initReactI18next: vi.fn().mockResolvedValue({
    type: "i18n"
  }),
  Trans: ({ children }: { children: React.ReactNode }) => children,
  useTranslation: () => ({
    t: vi.fn((key: string) => key),
  }),
}));

vi.mock("../Navbar", () => {
    return {
        Navbar: () => <div>Mocked Navbar</div>,
    };
});

vi.mock('../recovery_data_component', () => ({
  RecoveryDataComponent: ({ show, email, secret }: {
    show: { value: boolean };
    email: string;
    secret: Uint8Array;
  }) => (
    show?.value ? (
      <div data-testid="recovery-modal">
        Recovery modal for {email} with secret length: {secret.length}
      </div>
    ) : null
  ),
}));

vi.mock('links', () => ({
  privacy_notice: 'https://example.com/privacy',
  terms_of_use: 'https://example.com/terms',
}));

describe('Register Component', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mockSodium: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mockUtils: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mockShowAlert: any;

  beforeEach(async () => {
    vi.clearAllMocks();

    const { showAlert } = await import('../Alert');
    mockShowAlert = showAlert;

    mockUtils = await import('../../utils');
    mockUtils.get_salt.mockResolvedValue(new Uint8Array([1, 2, 3, 4]));
    mockUtils.generate_random_bytes.mockReturnValue(new Uint8Array([5, 6, 7, 8]));
    mockUtils.generate_hash.mockResolvedValue(new Uint8Array([9, 10, 11, 12]));
    mockUtils.fetchClient.POST.mockResolvedValue({
      response: { status: 201 },
      error: null,
    });

    mockSodium = await import('libsodium-wrappers');
    mockSodium.default.crypto_box_keypair.mockReturnValue({
      privateKey: new Uint8Array([13, 14, 15, 16]),
      publicKey: new Uint8Array([17, 18, 19, 20]),
    });
    mockSodium.default.crypto_secretbox_easy.mockReturnValue(new Uint8Array([21, 22, 23, 24]));
  });

  it("renders test", async () => {
    const { Form, Modal, Button } = await import('react-bootstrap');
    const test = <>
      <div>
        <Modal>
          <Modal.Header closeButton>
            <Modal.Title>Test Modal</Modal.Title>
          </Modal.Header>
          <Modal.Body>
            <p>This is a test modal.</p>
          </Modal.Body>
          <Modal.Footer>
            <Button variant="secondary">Close</Button>
          </Modal.Footer>
        </Modal>
        <Form>
            <Form.Group>
                <Form.Label>Name</Form.Label>
                <Form.Control type="text" data-testid="text-input" />
            </Form.Group>
            <Form.Group>
                <Form.Label>Email</Form.Label>
                <Form.Control type="email" data-testid="email-input" />
            </Form.Group>
            <Form.Group>
                <Form.Label>Password</Form.Label>
                <Form.Control type="password" data-testid="password-input" />
            </Form.Group>
            <Form.Group>
                <Form.Label>Confirm Password</Form.Label>
                <Form.Control type="password" data-testid="confirm-password-input" />
            </Form.Group>
            <Form.Check
                type="checkbox"
                label="I accept the terms and conditions"
                data-testid="terms-checkbox"
            />
            <button type="submit" data-testid="submit-button">Register</button>
        </Form>
        </div>
    </>

    render(test);
    expect(screen.getByTestId('text-input')).toBeTruthy();
  })

  it('renders the registration form correctly', () => {
    render(<Register />);

    expect(screen.getByRole("textbox", { name: "name" })).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "email" })).toBeTruthy();
    expect(screen.getAllByRole("textbox", { name: "password" })).toBeTruthy();
    expect(screen.getAllByRole("textbox", { name: "confirm_password" })).toBeTruthy();
    expect(screen.getAllByRole("checkbox")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "register" })).toBeTruthy();
  });

  it('validates form fields correctly', async () => {
    render(<Register />);
    const submitButton = screen.getByTestId('submit-button');

    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).not.toHaveBeenCalled();
      expect(screen.getByRole('textbox', { name: 'name' })).toHaveClass('invalid');
      expect(screen.getByRole('textbox', { name: 'email' })).toHaveClass('invalid');
      expect(screen.getByRole('textbox', { name: 'password' })).toHaveClass('invalid');
    })
  });

  it('validates password pattern', async () => {
    render(<Register />);
    const passwordInputs = screen.getAllByTestId('password-input');
    const passwordInput = passwordInputs[0];
    const confirmPasswordInput = passwordInputs[1];

    fireEvent.change(passwordInput, { target: { value: 'weak' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'weak' } });

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).not.toHaveBeenCalled();
      expect(passwordInput).toHaveClass('invalid');
      expect(screen.getByTestId('password-error')).toBeTruthy();
    })
  });

  it('validates password confirmation match', async () => {
    render(<Register />);
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });

    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'DifferentPass123!' } });

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).not.toHaveBeenCalled();
      expect(confirmPasswordInput).toHaveClass('invalid');
      expect(screen.getByText('confirm_password_error_message')).toBeTruthy();
    });
  });

  it('requires privacy policy acceptance', async () => {
    render(<Register />);
    const nameInput = screen.getByRole('textbox', { name: 'name' });
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const checkboxes = screen.getAllByRole('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });
    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).not.toHaveBeenCalled();
      expect(checkboxes[0]).toHaveClass('invalid');
    });
  });

  it('requires terms and conditions acceptance', async () => {
    render(<Register />);
    const nameInput = screen.getByRole('textbox', { name: 'name' });
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const checkboxes = screen.getAllByRole('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });
    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).not.toHaveBeenCalled();
      expect(checkboxes[1]).toHaveClass('invalid');
    });
  });

  it('submits registration with valid data', async () => {
    render(<Register />);

    const nameInput = screen.getByRole('textbox', { name: 'name' });
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const checkboxes = screen.getAllByRole('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });
    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockUtils.get_salt).toHaveBeenCalledTimes(2); // Once for secret salt, once for login salt
      expect(mockUtils.generate_random_bytes).toHaveBeenCalledTimes(3); // For secret and login salts and nonce
      expect(mockUtils.generate_hash).toHaveBeenCalledTimes(2); // For secret key and login key
      expect(mockSodium.default.crypto_box_keypair).toHaveBeenCalled();
      expect(mockSodium.default.crypto_secretbox_easy).toHaveBeenCalled();
      expect(mockUtils.fetchClient.POST).toHaveBeenCalledWith('/auth/register', {
        body: {
          name: 'John Doe',
          email: 'john@example.com',
          login_key: [9, 10, 11, 12],
          login_salt: [1, 2, 3, 4, 5, 6, 7, 8],
          secret: [21, 22, 23, 24],
          secret_nonce: [5, 6, 7, 8],
          secret_salt: [1, 2, 3, 4, 5, 6, 7, 8],
        },
        headers: {
          'X-Lang': 'en',
        },
      });
    });
  });

  it('shows success alert on successful registration', async () => {
    render(<Register />);

    // Fill and submit valid form
    const nameInput = screen.getByTestId('text-input');
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const checkboxes = screen.getAllByRole('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });
    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockShowAlert).toHaveBeenCalledWith(
        'register.registration_successful',
        'success',
        'register',
        'alert_default_success'
      );
    });
  });

  it('shows recovery modal after successful registration', async () => {
    render(<Register />);

    const nameInput = screen.getByRole('textbox', { name: 'name' });
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const checkboxes = screen.getAllByRole('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });
    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(screen.getByTestId('modal')).toBeTruthy();
    });
  });

  it('renders ResendVerification component after successful registration', async () => {
    render(<Register />);

    const nameInput = screen.getByRole('textbox', { name: 'name' });
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const checkboxes = screen.getAllByRole('checkbox');

    fireEvent.change(nameInput, { target: { value: 'Jane Doe' } });
    fireEvent.change(emailInput, { target: { value: 'jane@example.com' } });
    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      // ResendVerification wrapper div
      expect(screen.getByTestId('resend-verification')).toBeTruthy();
    });
  });

  it('handles registration error correctly', async () => {
    mockUtils.fetchClient.POST.mockResolvedValue({
      response: { status: 400 },
      error: 'Registration failed',
    });

    render(<Register />);

    const nameInput = screen.getByTestId('text-input');
    const emailInput = screen.getByTestId('email-input');
    const passwordInputs = screen.getAllByTestId('password-input');
    const checkboxes = screen.getAllByTestId('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });
    fireEvent.change(passwordInputs[0], { target: { value: 'ValidPass123!' } });
    fireEvent.change(passwordInputs[1], { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockShowAlert).toHaveBeenCalledWith(
        'Failed with status 400: Registration failed',
        'danger'
      );
    });
  });

  it('handles email already exists error (409 conflict)', async () => {
    mockUtils.fetchClient.POST.mockResolvedValue({
      response: { status: 409 },
      error: 'An account with this email already exists',
    });

    render(<Register />);

    const nameInput = screen.getByTestId('text-input');
    const emailInput = screen.getByTestId('email-input');
    const passwordInputs = screen.getAllByTestId('password-input');
    const checkboxes = screen.getAllByTestId('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'existing@example.com' } });
    fireEvent.change(passwordInputs[0], { target: { value: 'ValidPass123!' } });
    fireEvent.change(passwordInputs[1], { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockShowAlert).toHaveBeenCalledWith(
        'register.email_already_exists',
        'danger'
      );
    });
  });

  it('handles salt generation error', async () => {
    mockUtils.get_salt.mockRejectedValue('Salt generation failed');

    render(<Register />);

    const nameInput = screen.getByTestId('text-input');
    const emailInput = screen.getByTestId('email-input');
    const passwordInputs = screen.getAllByTestId('password-input');
    const checkboxes = screen.getAllByTestId('checkbox');

    fireEvent.change(nameInput, { target: { value: 'John Doe' } });
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });
    fireEvent.change(passwordInputs[0], { target: { value: 'ValidPass123!' } });
    fireEvent.change(passwordInputs[1], { target: { value: 'ValidPass123!' } });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockShowAlert).toBeCalled();
      expect(mockShowAlert).toHaveBeenCalledWith('Salt generation failed', 'danger');
    });
  });

  it('calls checkPassword when password fields change', async () => {
    render(<Register />);
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const submitButton = screen.getByTestId('submit-button');

    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'different' } });
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(passwordInput).not.toHaveClass('invalid');
      expect(confirmPasswordInput).toHaveClass('invalid');
    });

    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });
    await waitFor(() => {
      expect(passwordInput).not.toHaveClass('invalid');
      expect(confirmPasswordInput).not.toHaveClass('invalid');
    });
  });

  it('dynamically removes errors when correct values are entered in all fields', async () => {
    render(<Register />);

    // First, submit the form with all empty fields to trigger all validation errors
    const submitButton = screen.getByTestId('submit-button');
    fireEvent.click(submitButton);

    // Wait for all validation errors to appear
    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'name' })).toHaveClass('invalid');
      expect(screen.getByRole('textbox', { name: 'email' })).toHaveClass('invalid');
      expect(screen.getByRole('textbox', { name: 'password' })).toHaveClass('invalid');
      expect(screen.getAllByRole('checkbox')[0]).toHaveClass('invalid');
      expect(screen.getAllByRole('checkbox')[1]).toHaveClass('invalid');
    });

    // Get form inputs
    const nameInput = screen.getByRole('textbox', { name: 'name' });
    const emailInput = screen.getByRole('textbox', { name: 'email' });
    const passwordInput = screen.getByRole('textbox', { name: 'password' });
    const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_password' });
    const checkboxes = screen.getAllByRole('checkbox');

    // Test 1: Enter invalid password (too short) - should still show error
    fireEvent.change(passwordInput, { target: { value: 'short' } });
    fireEvent.change(confirmPasswordInput, { target: { value: 'short' } });
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(passwordInput).toHaveClass('invalid');
    });

    // Test 2: Enter valid password and confirm - errors should be removed
    fireEvent.change(passwordInput, { target: { value: 'ValidPass123!' } });

    await waitFor(() => {
      expect(passwordInput).not.toHaveClass('invalid');
    });

    // Test 3: Enter mismatched confirm password - should show error
    fireEvent.change(confirmPasswordInput, { target: { value: 'Different123!' } });

    await waitFor(() => {
      expect(confirmPasswordInput).toHaveClass('invalid');
    });

    // Test 4: Match confirm password - error should be removed
    fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });

    await waitFor(() => {
      expect(confirmPasswordInput).not.toHaveClass('invalid');
    });

    // Test 5: Enter valid name - error should be removed
    fireEvent.change(nameInput, { target: { value: 'John Doe' } });

    await waitFor(() => {
      expect(nameInput).not.toHaveClass('invalid');
    });

    // Test 6: Enter valid email - error should be removed
    fireEvent.change(emailInput, { target: { value: 'john@example.com' } });

    await waitFor(() => {
      expect(emailInput).not.toHaveClass('invalid');
    });

    // Test 7: Check privacy policy - error should be removed
    fireEvent.click(checkboxes[0]);

    await waitFor(() => {
      expect(checkboxes[0]).not.toHaveClass('invalid');
    });

    // Test 8: Check terms and conditions - error should be removed
    fireEvent.click(checkboxes[1]);

    await waitFor(() => {
      expect(checkboxes[1]).not.toHaveClass('invalid');
    });

    // Final validation: All fields should now be valid and form should submit
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(mockUtils.fetchClient.POST).toHaveBeenCalled();
    });
  });
});
