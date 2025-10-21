import { beforeAll, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/preact';
import userEvent from '@testing-library/user-event';

let PasswordComponent: any;

beforeAll(async () => {
  // Bypass the global mock defined in test-setup.ts and import the real component
  ({ PasswordComponent } = await vi.importActual<typeof import('../PasswordComponent')>('../PasswordComponent'));
});

describe('PasswordComponent', () => {
  it('renders password input with placeholder and toggles visibility with icons', async () => {
    const user = userEvent.setup();
    const handleChange = vi.fn();

    render(<PasswordComponent onChange={handleChange} />);

    const input = screen.getByPlaceholderText('password');
    expect(input).toBeInTheDocument();

    expect(screen.getByTestId('password-input')).toBeInTheDocument();
    expect(screen.getByTestId('eye-icon')).toBeInTheDocument();

    await user.click(screen.getByRole('button'));

    expect(screen.getByTestId('text-input')).toBeInTheDocument();
    expect(screen.getByTestId('eye-off-icon')).toBeInTheDocument();
  });

  it('calls onChange with typed value', async () => {
    const handleChange = vi.fn();

    render(<PasswordComponent onChange={handleChange} />);

    const input = screen.getByTestId('password-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'Secret123!' } });

    expect(handleChange).toHaveBeenCalled();
    expect(handleChange).toHaveBeenLastCalledWith('Secret123!');
  });

  it('shows invalid state and feedback when provided', async () => {
    const handleChange = vi.fn();
    render(<PasswordComponent onChange={handleChange} isInvalid invalidMessage="Invalid password" />);

    const input = screen.getByTestId('password-input');
    await waitFor(() => {
      expect(input).toHaveClass('invalid');
    })

    const feedback = screen.getByTestId('invalid-feedback');
    expect(feedback).toHaveTextContent('Invalid password');
  });
});
