import { render, screen } from '@testing-library/preact';
import { describe, it, expect } from 'vitest';
import { Footer } from '../Footer';
import { privacy_notice, terms_of_use, imprint } from 'links';

/**
 * Footer component tests
 * The i18n mock returns the key itself (with keyPrefix already applied in the component),
 * so the visible link texts are the raw keys: privacy_notice, terms_of_use, imprint.
 */

describe('Footer', () => {
  it('renders three legal links with correct hrefs', () => {
    render(<Footer />);

    const privacyLink = screen.getByText('privacy_notice');
    const termsLink = screen.getByText('terms_of_use');
    const imprintLink = screen.getByText('imprint');

    expect(privacyLink).toBeTruthy();
    expect(termsLink).toBeTruthy();
    expect(imprintLink).toBeTruthy();

    expect(privacyLink).toHaveAttribute('href', privacy_notice);
    expect(termsLink).toHaveAttribute('href', terms_of_use);
    expect(imprintLink).toHaveAttribute('href', imprint);

    const allLinks = screen.getAllByRole('link');
    // Filter only those inside the footer container if multiple links exist globally
    const footerLinks = allLinks.filter(l => ['privacy_notice','terms_of_use','imprint'].includes(l.textContent || ''));
    expect(footerLinks).toHaveLength(3);
  });
});
