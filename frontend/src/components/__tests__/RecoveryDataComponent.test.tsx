import { render, screen, fireEvent, waitFor, within } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { signal } from '@preact/signals';

// Use dynamic import so we can spy on named exports from the module
const importComponent = () => import('../RecoveryDataComponent');

describe('RecoveryDataComponent', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders modal content when shown and triggers save with confirmation', async () => {
    const mod = await importComponent();
    const { RecoveryDataComponent } = mod;

    const show = signal(true);
    const email = 'john.doe@example.com';
    const secret = new Uint8Array([1, 2, 3]);

    render(<RecoveryDataComponent email={email} secret={secret} show={show} />);

    expect(screen.getByTestId('modal-title').textContent).toBe('save_recovery_data');
    expect(screen.getByTestId('modal-body').textContent).toContain('save_recovery_data_text');
    const footer = screen.getByTestId('modal-footer');
    const closeButton = within(footer).getByRole('button', { name: 'close' });

    // Initially close button should be disabled
    expect(closeButton).toBeDisabled();
    expect(closeButton.className).toContain('btn-secondary');

    const saveButton = screen.getByRole('button', { name: 'save' });
    fireEvent.click(saveButton);

    // After saving, confirmation checkbox should appear
    await waitFor(() => {
      expect(screen.getByLabelText('save_recovery_data_confirmation')).toBeInTheDocument();
    });

    // Close button should still be disabled until checkbox is checked
    expect(closeButton).toBeDisabled();

    // Check the confirmation checkbox
    const confirmationCheckbox = screen.getByLabelText('save_recovery_data_confirmation');
    fireEvent.click(confirmationCheckbox);

    await waitFor(() => {
      expect(closeButton).not.toBeDisabled();
      expect(closeButton.className).toContain('btn-primary');
    });

    fireEvent.click(closeButton);
    expect(show.value).toBe(false);
  });

  it('prevents closing modal until file is saved and confirmed', async () => {
    const { RecoveryDataComponent } = await importComponent();

    const show = signal(true);

    render(<RecoveryDataComponent email={'a@b.c'} secret={new Uint8Array()} show={show} />);

    // Try to close the modal without saving/confirming - should not close
    const closeButton = screen.getByRole('button', { name: 'close' });
    fireEvent.click(closeButton);

    // Modal should still be open
    expect(show.value).toBe(true);

    // Save the file first
    const saveButton = screen.getByRole('button', { name: 'save' });
    fireEvent.click(saveButton);

    // Check the confirmation checkbox
    await waitFor(() => {
      const confirmationCheckbox = screen.getByLabelText('save_recovery_data_confirmation');
      fireEvent.click(confirmationCheckbox);
    });

    // Now try to close - should work
    await waitFor(() => {
      expect(closeButton).not.toBeDisabled();
    });
    
    fireEvent.click(closeButton);
    expect(show.value).toBe(false);
  });
});

describe('saveRecoveryData', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('creates a downloadable backup file and revokes URL', async () => {
    const { saveRecoveryData } = await importComponent();

  const email = 'john.doe@example.com';
    const secret = new Uint8Array([9, 8, 7, 6]);

    const createUrl = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:test-url');
    const revokeUrl = vi.spyOn(URL, 'revokeObjectURL');

    const clickSpy = vi
      .spyOn(HTMLAnchorElement.prototype as unknown as { click: () => void }, 'click')
      .mockImplementation(function (this: HTMLAnchorElement) {
        expect(this.download).toBe('john_doe_at_example_com_my_warp_charger_com_recovery_data');
      });

    const hashBytes = new Uint8Array([1, 2, 3, 4]).buffer;
    (window.crypto.subtle.digest as unknown as (algo: string, data: ArrayBufferView) => Promise<ArrayBuffer>) =
      vi.fn().mockResolvedValue(hashBytes);

    await saveRecoveryData(secret, email);

    expect(createUrl).toHaveBeenCalledTimes(1);
    expect(clickSpy).toHaveBeenCalledTimes(1);
    expect(revokeUrl).toHaveBeenCalledWith('blob:test-url');

  expect(window.crypto.subtle.digest).toHaveBeenCalledWith('SHA-256', expect.anything());
  });
});
