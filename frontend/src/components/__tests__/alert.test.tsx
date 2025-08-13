import { render, screen, fireEvent } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Helper to flush timers when using setTimeout
const advanceTimers = async (ms: number) => {
  await vi.advanceTimersByTimeAsync(ms);
};

describe('Alert component & showAlert', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    window.scrollTo = vi.fn();
  });

  it('renders an alert with heading and text', async () => {
    const real = await vi.importActual<typeof import('../Alert')>('../Alert');
    real.showAlert('Some text', 'danger', 'abc', 'Heading Text');
    render(<real.ErrorAlert />);
    expect(screen.getByText('Some text')).toBeTruthy();
    expect(screen.getByTestId('alert-heading')).toHaveTextContent('Heading Text');
  });

  it('suppresses alert containing Failed to fetch', async () => {
    const real = await vi.importActual<typeof import('../Alert')>('../Alert');
  const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    real.showAlert('Failed to fetch resource', 'danger');
    render(<real.ErrorAlert />);
    expect(screen.queryByText('Failed to fetch resource')).toBeNull();
    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
  });

  it('auto dismisses alert after timeout', async () => {
    const real = await vi.importActual<typeof import('../Alert')>('../Alert');
    real.showAlert('Autoclose', 'success', 'auto', 'Auto Heading', 2000);
    const { rerender } = render(<real.ErrorAlert />);
    expect(screen.getByText('Autoclose')).toBeTruthy();
    await advanceTimers(2000);
    rerender(<real.ErrorAlert />);
    expect(screen.queryByText('Autoclose')).toBeNull();
  });

  it('replaces alert with same id and clears previous timeout', async () => {
    const real = await vi.importActual<typeof import('../Alert')>('../Alert');
    const clearSpy = vi.spyOn(window, 'clearTimeout');
    real.showAlert('First', 'warning', 'same', 'First Heading', 5000);
    const utils = render(<real.ErrorAlert />);
    expect(screen.getByText('First')).toBeTruthy();
    real.showAlert('Second', 'warning', 'same', 'Second Heading');
    utils.rerender(<real.ErrorAlert />);
    expect(screen.getByText('Second')).toBeTruthy();
    expect(screen.queryByText('First')).toBeNull();
    expect(clearSpy).toHaveBeenCalled();
    clearSpy.mockRestore();
  });

  it('manual dismiss via close button triggers onClose and removes alert', async () => {
    const real = await vi.importActual<typeof import('../Alert')>('../Alert');
    real.showAlert('Dismiss me', 'danger', 'dismiss', 'Dismiss Heading');
    const { rerender } = render(<real.ErrorAlert />);
    expect(screen.getByText('Dismiss me')).toBeTruthy();
    const closeButtons = screen.getAllByTestId('close-alert');
    fireEvent.click(closeButtons[closeButtons.length - 1]);
    rerender(<real.ErrorAlert />);
    expect(screen.queryByText('Dismiss me')).toBeNull();
  });
});
