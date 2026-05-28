import { render, screen, fireEvent } from '@testing-library/preact';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { AndroidSmartBanner } from '../AndroidAppBanner';
import { play_store_link } from 'links';

const DISMISSED_KEY = "android-smart-banner-dismissed";

describe('AndroidSmartBanner', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(localStorage.getItem).mockClear();
    vi.mocked(localStorage.setItem).mockClear();
  });

  it('renders the banner when not dismissed and not a native app', () => {
    render(<AndroidSmartBanner />);

    expect(screen.getByText('android_smart_banner.text')).toBeTruthy();
    expect(screen.getByText('android_smart_banner.view')).toBeTruthy();
  });

  it('renders the Play Store link with correct href', () => {
    render(<AndroidSmartBanner />);

    const link = screen.getByText('android_smart_banner.view');
    expect(link).toHaveAttribute('href', play_store_link);
    expect(link).toHaveAttribute('target', '_blank');
    expect(link).toHaveAttribute('rel', 'noopener noreferrer');
  });

  it('renders the app icon image', () => {
    render(<AndroidSmartBanner />);

    const icon = screen.getByAltText('App icon');
    expect(icon).toBeTruthy();
  });

  it('renders a close button', () => {
    render(<AndroidSmartBanner />);

    const closeBtn = screen.getByRole('button', { name: 'android_smart_banner.close' });
    expect(closeBtn).toBeTruthy();
  });

  it('does not render when previously dismissed', () => {
    localStorage.setItem(DISMISSED_KEY, '1');

    const { container } = render(<AndroidSmartBanner />);
    expect(container.innerHTML).toBe('');
  });

  it('hides the banner and persists dismissal on close button click', () => {
    const { container } = render(<AndroidSmartBanner />);

    // Banner should be visible first
    expect(screen.getByText('android_smart_banner.text')).toBeTruthy();

    const closeBtn = screen.getByRole('button', { name: 'android_smart_banner.close' });
    fireEvent.click(closeBtn);

    // Banner should be hidden
    expect(container.innerHTML).toBe('');

    // Dismissal should be persisted
    expect(localStorage.setItem).toHaveBeenCalledWith(DISMISSED_KEY, '1');
  });

  it('does not render when running as a native app', async () => {
    const Median = await import('median-js-bridge');
    const isNativeAppSpy = vi.spyOn(Median.default, 'isNativeApp').mockReturnValue(true);

    const { container } = render(<AndroidSmartBanner />);
    expect(container.innerHTML).toBe('');

    isNativeAppSpy.mockRestore();
  });

  it('does not render after being dismissed and re-mounted', () => {
    const { unmount } = render(<AndroidSmartBanner />);

    const closeBtn = screen.getByRole('button', { name: 'android_smart_banner.close' });
    fireEvent.click(closeBtn);

    unmount();

    // Re-render after dismissal
    const { container } = render(<AndroidSmartBanner />);
    expect(container.innerHTML).toBe('');
  });
});
