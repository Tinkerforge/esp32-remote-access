import { render, screen, fireEvent, waitFor, within } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { signal } from '@preact/signals';

// Use dynamic import so we can spy on named exports from the module
const importComponent = () => import('../RecoveryDataComponent');

describe('RecoveryDataComponent', () => {
  const originalOrigin = globalThis.origin;

  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(globalThis, 'origin', {
      value: 'https://my.warp-charger.com',
      writable: true,
      configurable: true,
    });
  });

  afterEach(() => {
    Object.defineProperty(globalThis, 'origin', {
      value: originalOrigin,
      writable: true,
      configurable: true,
    });
  });

  it('renders modal content when shown and triggers save with confirmation', async () => {
    const mod = await importComponent();
    const { RecoveryDataComponent } = mod;

    const show = signal(true);
    const email = 'john.doe@example.com';
    const secret = new Uint8Array([1, 2, 3]);

    const clickSpy = vi
      .spyOn(HTMLAnchorElement.prototype as unknown as { click: () => void }, 'click')
      .mockImplementation(() => undefined);

    render(<RecoveryDataComponent email={email} secret={secret} show={show} />);

    expect(screen.getByTestId('modal-title').textContent).toBe('save_recovery_data');
    expect(screen.getByTestId('modal-body').textContent).toContain('save_recovery_data_text');
    const footer = screen.getByTestId('modal-footer');
    const closeButton = within(footer).getByRole('button', { name: 'close' });

    expect(closeButton).toBeDisabled();
    expect(closeButton.className).toContain('btn-secondary');

    const saveButton = screen.getByRole('button', { name: 'save' });
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(screen.getByLabelText('save_recovery_data_confirmation')).toBeInTheDocument();
    });

    expect(closeButton).toBeDisabled();

    const confirmationCheckbox = screen.getByLabelText('save_recovery_data_confirmation');
    fireEvent.click(confirmationCheckbox);

    await waitFor(() => {
      expect(closeButton).not.toBeDisabled();
      expect(closeButton.className).toContain('btn-primary');
    });

    fireEvent.click(closeButton);
    expect(show.value).toBe(false);

    clickSpy.mockRestore();
  });

  it('prevents closing modal until file is saved and confirmed', async () => {
    const { RecoveryDataComponent } = await importComponent();

    const show = signal(true);

    const clickSpy = vi
      .spyOn(HTMLAnchorElement.prototype as unknown as { click: () => void }, 'click')
      .mockImplementation(() => undefined);

    render(<RecoveryDataComponent email={'a@b.c'} secret={new Uint8Array()} show={show} />);

    const closeButton = screen.getByRole('button', { name: 'close' });
    fireEvent.click(closeButton);

    expect(show.value).toBe(true);

    const saveButton = screen.getByRole('button', { name: 'save' });
    fireEvent.click(saveButton);

    await waitFor(() => {
      const confirmationCheckbox = screen.getByLabelText('save_recovery_data_confirmation');
      fireEvent.click(confirmationCheckbox);
    });

    await waitFor(() => {
      expect(closeButton).not.toBeDisabled();
    });

    fireEvent.click(closeButton);
    expect(show.value).toBe(false);

    clickSpy.mockRestore();
  });
});

describe('saveRecoveryData', () => {
  const originalOrigin = globalThis.origin;

  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(globalThis, 'origin', {
      value: 'https://my.warp-charger.com',
      writable: true,
      configurable: true,
    });
  });

  afterEach(() => {
    Object.defineProperty(globalThis, 'origin', {
      value: originalOrigin,
      writable: true,
      configurable: true,
    });
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
        expect(this.download).toBe('john_doe_at_example_com_my_warp-charger.com_recovery_data');
      });

    const hashBytes = new Uint8Array([1, 2, 3, 4]).buffer;
    (window.crypto.subtle.digest as unknown as (algo: string, data: ArrayBufferView) => Promise<ArrayBuffer>) =
      vi.fn().mockResolvedValue(hashBytes);

    await saveRecoveryData(secret, email);

    expect(createUrl).toHaveBeenCalledTimes(1);
    expect(clickSpy).toHaveBeenCalledTimes(1);
    expect(revokeUrl).toHaveBeenCalledWith('blob:test-url');

    expect(window.crypto.subtle.digest).toHaveBeenCalledWith('SHA-256', expect.anything());

    createUrl.mockRestore();
    clickSpy.mockRestore();
    revokeUrl.mockRestore();
  });
});

describe('RecoveryDataComponent onHide path', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(globalThis, 'origin', {
      value: 'https://my.warp-charger.com',
      writable: true,
      configurable: true,
    });
  });

  it('acknowledges a click on the modal close (cancel) when not yet saved/confirmed', async () => {
    const { RecoveryDataComponent } = await importComponent();

    vi.spyOn(HTMLAnchorElement.prototype as unknown as { click: () => void }, 'click')
      .mockImplementation(() => undefined);

    const show = signal(true);
    render(<RecoveryDataComponent email={'a@b.c'} secret={new Uint8Array([1])} show={show} />);

    // Close (cancel) before saving: the onHide callback short-circuits
    // because saved/confirmed are still false, leaving the modal open.
    const closeButtons = screen.getAllByTestId('modal-close');
    fireEvent.click(closeButtons[0]);
    expect(show.value).toBe(true);
  });
});
