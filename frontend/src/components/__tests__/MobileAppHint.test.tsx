import { render, screen } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MobileAppHint } from '../MobileAppHint';
import { app_store_link, play_store_link } from 'links';
import Median from 'median-js-bridge';

describe('MobileAppHint', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('when not in native app', () => {
    it('renders the full (non-compact) layout by default', () => {
      render(<MobileAppHint />);

      const logo = screen.getByAltText('logo');
      expect(logo).toBeInTheDocument();
      expect(logo).toHaveAttribute('src', 'logo.png');
      expect(logo).toHaveStyle({ height: '40px' });

      expect(screen.getByText('hint_title')).toBeInTheDocument();
      expect(screen.getByText('hint_text')).toBeInTheDocument();
    });

    it('renders App Store and Play Store links with correct hrefs in full layout', () => {
      render(<MobileAppHint />);

      const appStoreLink = screen.getByText('app_store');
      const playStoreLink = screen.getByText('play_store');

      expect(appStoreLink).toHaveAttribute('href', app_store_link);
      expect(playStoreLink).toHaveAttribute('href', play_store_link);
    });

    it('renders links with target="_blank" and rel="noopener noreferrer" in full layout', () => {
      render(<MobileAppHint />);

      const appStoreLink = screen.getByText('app_store');
      const playStoreLink = screen.getByText('play_store');

      expect(appStoreLink).toHaveAttribute('target', '_blank');
      expect(appStoreLink).toHaveAttribute('rel', 'noopener noreferrer');
      expect(playStoreLink).toHaveAttribute('target', '_blank');
      expect(playStoreLink).toHaveAttribute('rel', 'noopener noreferrer');
    });

    it('renders links as buttons with outline-dark styling in full layout', () => {
      render(<MobileAppHint />);

      const appStoreLink = screen.getByText('app_store');
      const playStoreLink = screen.getByText('play_store');

      expect(appStoreLink.className).toContain('btn-outline-dark');
      expect(playStoreLink.className).toContain('btn-outline-dark');
    });

    it('renders the compact layout when compact prop is true', () => {
      render(<MobileAppHint compact={true} />);

      const logo = screen.getByAltText('logo');
      expect(logo).toBeInTheDocument();
      expect(logo).toHaveStyle({ height: '24px' });

      expect(screen.getByText('hint_title')).toBeInTheDocument();
      // hint_text should NOT be rendered in compact mode
      expect(screen.queryByText('hint_text')).not.toBeInTheDocument();
    });

    it('renders App Store and Play Store links with correct hrefs in compact layout', () => {
      render(<MobileAppHint compact={true} />);

      const appStoreLink = screen.getByText('app_store');
      const playStoreLink = screen.getByText('play_store');

      expect(appStoreLink).toHaveAttribute('href', app_store_link);
      expect(playStoreLink).toHaveAttribute('href', play_store_link);
    });

    it('renders links with target="_blank" and rel="noopener noreferrer" in compact layout', () => {
      render(<MobileAppHint compact={true} />);

      const appStoreLink = screen.getByText('app_store');
      const playStoreLink = screen.getByText('play_store');

      expect(appStoreLink).toHaveAttribute('target', '_blank');
      expect(appStoreLink).toHaveAttribute('rel', 'noopener noreferrer');
      expect(playStoreLink).toHaveAttribute('target', '_blank');
      expect(playStoreLink).toHaveAttribute('rel', 'noopener noreferrer');
    });

    it('renders links with text-white class in compact layout', () => {
      render(<MobileAppHint compact={true} />);

      const appStoreLink = screen.getByText('app_store');
      const playStoreLink = screen.getByText('play_store');

      expect(appStoreLink.className).toContain('text-white');
      expect(playStoreLink.className).toContain('text-white');
    });

    it('renders a separator between links in compact layout', () => {
      render(<MobileAppHint compact={true} />);

      expect(screen.getByText('|')).toBeInTheDocument();
    });
  });

  describe('when in native app', () => {
    it('renders nothing when Median.isNativeApp() returns true', () => {
      vi.spyOn(Median, 'isNativeApp').mockReturnValue(true);

      const { container } = render(<MobileAppHint />);

      expect(container.innerHTML).toBe('');
    });

    it('renders nothing in compact mode when Median.isNativeApp() returns true', () => {
      vi.spyOn(Median, 'isNativeApp').mockReturnValue(true);

      const { container } = render(<MobileAppHint compact={true} />);

      expect(container.innerHTML).toBe('');
    });
  });
});
