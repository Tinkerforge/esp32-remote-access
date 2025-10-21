import { render, fireEvent, waitFor, screen } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { User } from '../User';

describe('User Component', () => {
  type FetchClientMock = {
    GET: Mock;
    PUT: Mock;
    DELETE: Mock;
  };

  type UtilsMock = {
    fetchClient: FetchClientMock;
    generate_hash: Mock;
    generate_random_bytes: Mock;
    get_salt: Mock;
    get_salt_for_user: Mock;
    concat_salts: Mock;
    isDebugMode: { value: boolean };
  };

  type SodiumMock = {
    default: {
      crypto_secretbox_open_easy: Mock;
      crypto_secretbox_easy: Mock;
    };
  };

  type Base64Mock = {
    Base64: {
      toUint8Array: Mock;
    };
  };

  let mockUtils: UtilsMock;
  let mockShowAlert: Mock;
  let mockLogout: Mock;
  let mockSodium: SodiumMock;
  let mockBase64: Base64Mock;

  const mockUserData = {
    id: 'user-123',
    name: 'John Doe',
    email: 'john@example.com',
    has_old_charger: false,
  };

  const mockUserDataWithOldCharger = {
    ...mockUserData,
    has_old_charger: true,
  };

  beforeEach(async () => {
    vi.clearAllMocks();

  const { showAlert } = (await import('../../components/Alert')) as unknown as { showAlert: Mock };
    mockShowAlert = showAlert;

  const { logout } = (await import('../../components/Navbar')) as unknown as { logout: Mock };
    mockLogout = logout;

    mockUtils = (await import('../../utils')) as unknown as UtilsMock;
    mockUtils.fetchClient.GET.mockResolvedValue({
      data: mockUserData,
      error: null,
      response: { status: 200 },
    });
    mockUtils.fetchClient.PUT.mockResolvedValue({
      response: { status: 200 },
      error: null,
    });
    mockUtils.fetchClient.DELETE.mockResolvedValue({
      response: { status: 200 },
      error: null,
    });
    mockUtils.generate_hash.mockResolvedValue(new Uint8Array([1, 2, 3, 4]));
    mockUtils.generate_random_bytes.mockReturnValue(new Uint8Array([5, 6, 7, 8]));
    mockUtils.get_salt.mockResolvedValue(new Uint8Array([9, 10, 11, 12]));
    mockUtils.get_salt_for_user.mockResolvedValue(new Uint8Array([13, 14, 15, 16]));
    mockUtils.concat_salts.mockReturnValue(new Uint8Array([17, 18, 19, 20]));

    mockSodium = (await vi.importMock('libsodium-wrappers')) as unknown as SodiumMock;
    mockSodium.default.crypto_secretbox_open_easy.mockReturnValue(new Uint8Array([21, 22, 23, 24]));
    mockSodium.default.crypto_secretbox_easy.mockReturnValue(new Uint8Array([25, 26, 27, 28]));

    mockBase64 = (await vi.importMock('js-base64')) as unknown as Base64Mock;
    mockBase64.Base64.toUint8Array.mockReturnValue(new Uint8Array([29, 30, 31, 32]));

    vi.spyOn(window.localStorage, 'getItem').mockReturnValue('base64encodedSalt');
    // eslint-disable-next-line @typescript-eslint/no-empty-function
    vi.spyOn(window.localStorage, 'setItem').mockImplementation(() => {});
    // eslint-disable-next-line @typescript-eslint/no-empty-function
    vi.spyOn(window.localStorage, 'removeItem').mockImplementation(() => {});
  });

  describe('UserComponent (Profile Information)', () => {
    it('renders user profile form correctly', async () => {
      render(<User />);

      await waitFor(() => {
        expect(mockUtils.fetchClient.GET).toHaveBeenCalledWith('/user/me', {
          credentials: 'same-origin',
        });
      });

      expect(screen.getByDisplayValue('user-123')).toBeTruthy();
      expect(screen.getByDisplayValue('John Doe')).toBeTruthy();
      expect(screen.getByDisplayValue('john@example.com')).toBeTruthy();
    });

    it('disables email field when user has old charger', async () => {
      mockUtils.fetchClient.GET.mockResolvedValue({
        data: mockUserDataWithOldCharger,
        error: null,
        response: { status: 200 },
      });

      render(<User />);

      await waitFor(() => {
        const emailInput = screen.getByDisplayValue('john@example.com') as HTMLInputElement;
        expect(emailInput.disabled).toBe(true);
      });

      expect(screen.getByText('email_change_disabled')).toBeTruthy();
    });

    it('validates name field correctly', async () => {
      render(<User />);

      await waitFor(() => {
        expect(screen.getByDisplayValue('John Doe')).toBeTruthy();
      });

      const nameInput = screen.getByDisplayValue('John Doe');
      const submitButton = screen.getByText('save_changes');

      fireEvent.change(nameInput, { target: { value: '' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(nameInput).toHaveClass('invalid');
        expect(screen.getByText('invalid_name')).toBeTruthy();
        expect(mockUtils.fetchClient.PUT).not.toHaveBeenCalled();
      });
    });

    it('updates user data successfully', async () => {
      render(<User />);

      await waitFor(() => {
        expect(screen.getByDisplayValue('John Doe')).toBeTruthy();
      });

      const nameInput = screen.getByDisplayValue('John Doe');
      const emailInput = screen.getByDisplayValue('john@example.com');
      const submitButton = screen.getByText('save_changes');

      fireEvent.change(nameInput, { target: { value: 'Jane Doe' } });
      fireEvent.change(emailInput, { target: { value: 'jane@example.com' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockUtils.fetchClient.PUT).toHaveBeenCalledWith('/user/update_user', {
          body: {
            id: 'user-123',
            name: 'Jane Doe',
            email: 'jane@example.com',
            has_old_charger: false,
          },
          credentials: 'same-origin',
        });
      });

      expect(window.location.reload).toHaveBeenCalled();
    });

    it('handles update user error', async () => {
      mockUtils.fetchClient.PUT.mockResolvedValue({
        response: { status: 400 },
        error: 'Update failed',
      });

      render(<User />);

      await waitFor(() => {
        expect(screen.getByDisplayValue('John Doe')).toBeTruthy();
      });

      const nameInput = screen.getByDisplayValue('John Doe');
      const submitButton = screen.getByText('save_changes');

      fireEvent.change(nameInput, { target: { value: 'Jane Doe' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockShowAlert).toHaveBeenCalledWith(
          'user.update_user_failed',
          'danger'
        );
      });
    });

    it('disables save button when form is not dirty', async () => {
      render(<User />);

      await waitFor(() => {
        expect(screen.getByDisplayValue('John Doe')).toBeTruthy();
      });

      const submitButton = screen.getByText('save_changes') as HTMLButtonElement;
      expect(submitButton.disabled).toBe(true);
    });

    it('enables save button when form is dirty', async () => {
      render(<User />);

      await waitFor(() => {
        expect(screen.getByDisplayValue('John Doe')).toBeTruthy();
      });

      const nameInput = screen.getByDisplayValue('John Doe');

      fireEvent.change(nameInput, { target: { value: 'Jane Doe' } });

      await waitFor(() => {
        const submitButton = screen.getByText('save_changes') as HTMLButtonElement;
        expect(submitButton.disabled).toBe(false);
      });
    });
  });

  describe('Local Settings', () => {
    it('renders debug mode toggle', () => {
      render(<User />);

      const debugToggle = screen.getByLabelText('debug_mode') as HTMLInputElement;
      expect(debugToggle).toBeTruthy();
      expect(debugToggle.checked).toBe(false);
    });

    it('toggles debug mode and updates localStorage', async () => {
      let val: string | null = null;
      vi.spyOn(window.localStorage, 'getItem').mockImplementation(() => val);
      vi.spyOn(window.localStorage, 'setItem').mockImplementation((key, value) => {
        if (key === 'debugMode') {
          val = value;
        }
      });
      vi.spyOn(window.localStorage, 'removeItem').mockImplementation((key) => {
        if (key === 'debugMode') {
          val = null;
        }
      });

      render(<User />);

      const debugToggle = screen.getByLabelText('debug_mode');
      fireEvent.click(debugToggle);

      await waitFor(() => {
        expect(mockUtils.isDebugMode.value).toBe(true);
        expect(localStorage.getItem('debugMode')).toBe('true');
      });

      fireEvent.click(debugToggle);
      await waitFor(() => {
        expect(mockUtils.isDebugMode.value).toBe(false);
        expect(localStorage.getItem('debugMode')).toBe(null);
      });
    });
  });

  describe('Account Actions', () => {
    it('renders account action buttons', () => {
      render(<User />);

      expect(screen.getByText('change_password')).toBeTruthy();
      expect(screen.getByText('logout_all')).toBeTruthy();
      expect(screen.getByText('delete_user')).toBeTruthy();
    });

    it('opens change password modal', () => {
      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      expect(screen.getByTestId('modal')).toBeTruthy();
      expect(screen.getByText('current_password')).toBeTruthy();
      expect(screen.getByText('new_password')).toBeTruthy();
      expect(screen.getByText('confirm_new_password')).toBeTruthy();
    });

    it('opens delete user modal', () => {
      render(<User />);

      const deleteUserButton = screen.getByText('delete_user');
      fireEvent.click(deleteUserButton);

      expect(screen.getByTestId('modal')).toBeTruthy();
      expect(screen.getByText('password')).toBeTruthy();
    });

    it('calls logout function when logout all is clicked', () => {
      render(<User />);

      const logoutButton = screen.getByText('logout_all');
      fireEvent.click(logoutButton);

      expect(mockLogout).toHaveBeenCalledWith(true);
    });
  });

  describe('Change Password Modal', () => {
    beforeEach(() => {
      mockUtils.fetchClient.GET.mockResolvedValue({
        data: {
          secret: new Uint8Array([1, 2, 3, 4]),
          secret_nonce: new Uint8Array([5, 6, 7, 8]),
          secret_salt: new Uint8Array([9, 10, 11, 12]),
        },
        error: null,
        response: { status: 200 },
      });
    });

    it('renders change password modal correctly', async () => {
      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      await waitFor(() => {
        expect(screen.getByTestId('modal')).toBeTruthy();
        expect(screen.getByRole('textbox', { name: 'current_password' })).toBeTruthy();
        expect(screen.getByRole('textbox', { name: 'new_password' })).toBeTruthy();
        expect(screen.getByRole('textbox', { name: 'confirm_new_password' })).toBeTruthy();
      });

      const newPasswordInput = screen.getByRole('textbox', { name: 'new_password' });
      const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_new_password' });

      expect(confirmPasswordInput).not.toHaveClass('invalid');
      expect(newPasswordInput).not.toHaveClass('invalid');

    });

    it('validates password fields correctly', async () => {
      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      const submitButton = screen.getByRole('button', { name: 'change_password_button' });
      fireEvent.click(submitButton);

      const currentPasswordInput = screen.getByRole('textbox', { name: 'current_password' });
      const newPasswordInput = screen.getByRole('textbox', { name: 'new_password' });

      await waitFor(() => {
        expect(currentPasswordInput).toHaveClass('invalid');
        expect(newPasswordInput).toHaveClass('invalid');
      });
    });

    it('validates new password pattern', async () => {
      render(<User />);
      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      const currentPasswordInput = screen.getByRole('textbox', { name: 'current_password' });
      const newPasswordInput = screen.getByRole('textbox', { name: 'new_password' });
      const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_new_password' });
      const submitButton = screen.getByRole('button', { name: 'change_password_button' });

      fireEvent.change(currentPasswordInput, { target: { value: 'currentpass' } });
      fireEvent.change(newPasswordInput, { target: { value: 'weak' } });
      fireEvent.change(confirmPasswordInput, { target: { value: 'weak' } });

      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(newPasswordInput).toHaveClass('invalid');
        expect(screen.getByText('new_password_error_message')).toBeTruthy();
      });

    });

    it('validates password confirmation match', () => {
      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      const passwordInputs = screen.getAllByTestId('password-input');
      const submitButton = screen.getByRole('button', { name: 'change_password_button' });

      fireEvent.change(passwordInputs[0], { target: { value: 'CurrentPass123!' } });
      fireEvent.change(passwordInputs[1], { target: { value: 'NewPass123!' } });
      fireEvent.change(passwordInputs[2], { target: { value: 'DifferentPass123!' } });
      fireEvent.click(submitButton);

      expect(passwordInputs[2]).toHaveClass('invalid');
      expect(screen.getByText('confirm_new_password_error_message')).toBeTruthy();
    });

    it('updates password successfully', async () => {
      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      const passwordInputs = screen.getAllByTestId('password-input');
      const submitButton = screen.getByRole('button', { name: 'change_password_button' });

      fireEvent.change(passwordInputs[0], { target: { value: 'CurrentPass123!' } });
      fireEvent.change(passwordInputs[1], { target: { value: 'NewPass123!' } });
      fireEvent.change(passwordInputs[2], { target: { value: 'NewPass123!' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockUtils.fetchClient.GET).toHaveBeenCalledWith('/user/get_secret', {
          credentials: 'same-origin',
        });
      });

      await waitFor(() => {
        expect(mockUtils.fetchClient.PUT).toHaveBeenCalledWith('/user/update_password', {
          body: expect.objectContaining({
            old_login_key: [1, 2, 3, 4],
            new_login_key: [1, 2, 3, 4],
            new_login_salt: [17, 18, 19, 20],
            new_secret_nonce: [5, 6, 7, 8],
            new_secret_salt: [17, 18, 19, 20],
            new_encrypted_secret: [25, 26, 27, 28],
          }),
          credentials: 'same-origin',
        });
      });

      expect(mockLogout).toHaveBeenCalledWith(true);
    });

    it('handles password update error', async () => {
      mockUtils.fetchClient.PUT.mockResolvedValue({
        response: { status: 400 },
        error: 'Password update failed',
      });

      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      const passwordInputs = screen.getAllByTestId('password-input');
      const submitButton = screen.getByRole('button', { name: 'change_password_button' });

      fireEvent.change(passwordInputs[0], { target: { value: 'CurrentPass123!' } });
      fireEvent.change(passwordInputs[1], { target: { value: 'NewPass123!' } });
      fireEvent.change(passwordInputs[2], { target: { value: 'NewPass123!' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockShowAlert).toHaveBeenCalledWith(
          'user.update_password_failed',
          'danger'
        );
      });
    });

    it('handles get secret error', async () => {
      mockUtils.fetchClient.GET.mockResolvedValue({
        data: null,
        error: 'Failed to get secret',
        response: { status: 400 },
      });

      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      const passwordInputs = screen.getAllByTestId('password-input');
      const submitButton = screen.getByRole('button', { name: 'change_password_button' });

      fireEvent.change(passwordInputs[0], { target: { value: 'CurrentPass123!' } });
      fireEvent.change(passwordInputs[1], { target: { value: 'NewPass123!' } });
      fireEvent.change(passwordInputs[2], { target: { value: 'NewPass123!' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockShowAlert).toHaveBeenCalledWith(
          'user.update_password_failed',
          'danger'
        );
      });
    });

    it('closes modal when close button is clicked', () => {
      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      expect(screen.getByTestId('modal')).toBeTruthy();

      const closeButton = screen.getByText('close');
      fireEvent.click(closeButton);

      expect(screen.queryByTestId('modal')).toBeNull();
    });
  });

  describe('Delete User Modal', () => {
    it('deletes user successfully', async () => {
      render(<User />);

      const deleteUserButton = screen.getByText('delete_user');
      fireEvent.click(deleteUserButton);

      const passwordInput = screen.getByTestId('password-input');
      const submitButton = screen.getByRole('button', { name: 'delete_user_button' });

      fireEvent.change(passwordInput, { target: { value: 'MyPassword123!' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockUtils.fetchClient.DELETE).toHaveBeenCalledWith('/user/delete', {
          credentials: 'include',
          body: {
            login_key: [1, 2, 3, 4],
          },
        });
      });

      expect(mockLogout).toHaveBeenCalledWith(false);
    });

    it('handles invalid password for delete user', async () => {
      mockUtils.fetchClient.DELETE.mockResolvedValue({
        response: { status: 400 },
        error: 'Invalid password',
      });

      render(<User />);

      const deleteUserButton = screen.getByText('delete_user');
      fireEvent.click(deleteUserButton);

      const passwordInput = screen.getByTestId('password-input');
      const submitButton = screen.getByRole('button', { name: 'delete_user_button' });

      fireEvent.change(passwordInput, { target: { value: 'wrongpassword' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(passwordInput).toHaveClass('invalid');
      });
    });

    it('handles delete user error', async () => {
      mockUtils.fetchClient.DELETE.mockResolvedValue({
        response: { status: 500 },
        error: 'Server error',
      });

      render(<User />);

      const deleteUserButton = screen.getByText('delete_user');
      fireEvent.click(deleteUserButton);

      const passwordInput = screen.getByTestId('password-input');
      const submitButton = screen.getByRole('button', { name: 'delete_user_button' });

      fireEvent.change(passwordInput, { target: { value: 'MyPassword123!' } });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockShowAlert).toHaveBeenCalledWith(
          'delete_user',
          'danger'
        );
      });
    });

    it('closes modal when close button is clicked', () => {
      render(<User />);

      const deleteUserButton = screen.getByText('delete_user');
      fireEvent.click(deleteUserButton);

      expect(screen.getByTestId('modal')).toBeTruthy();

      const closeButton = screen.getByText('close');
      fireEvent.click(closeButton);
      expect(screen.queryByTestId('modal')).toBeNull();
    });
  });

  describe('Error Handling', () => {
    it('handles get user data error', async () => {
      mockUtils.fetchClient.GET.mockResolvedValue({
        data: null,
        error: 'Failed to get user',
        response: { status: 400 },
      });

      render(<User />);

      await waitFor(() => {
        expect(mockShowAlert).toHaveBeenCalledWith(
          'user.get_user_failed',
          'danger'
        );
      });
    });
  });

  describe('Dynamic Validation', () => {
    it('revalidates passwords on change', async () => {
      render(<User />);

      const changePasswordButton = screen.getByText('change_password');
      fireEvent.click(changePasswordButton);

      const newPasswordInput = screen.getByRole('textbox', { name: 'new_password' });
      const confirmPasswordInput = screen.getByRole('textbox', { name: 'confirm_new_password' });
      // Set invalid new password first
      fireEvent.change(newPasswordInput, { target: { value: 'weak' } });
      fireEvent.change(confirmPasswordInput, { target: { value: 'weak' } });
      const submitButton = screen.getByRole('button', { name: 'change_password_button' });
      fireEvent.click(submitButton);
      await waitFor(() => {
        expect(newPasswordInput).toHaveClass('invalid');
      });

      fireEvent.change(newPasswordInput, { target: { value: 'ValidPass123!' } });
      fireEvent.change(confirmPasswordInput, { target: { value: 'ValidPass123!' } });

      await waitFor(() => {
        expect(confirmPasswordInput).not.toHaveClass('invalid');
        expect(newPasswordInput).not.toHaveClass('invalid');
      });
    });
  });
});
