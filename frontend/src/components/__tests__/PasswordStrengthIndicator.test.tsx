import { beforeAll, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/preact';

let PasswordStrengthIndicator: typeof import('../PasswordStrengthIndicator').PasswordStrengthIndicator;

// Mock react-i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const translations: Record<string, string> = {
        'strength': 'Strength',
        'entropy': 'Entropy',
        'bits': 'bits',
        'very_weak': 'Very Weak',
        'weak': 'Weak',
        'fair': 'Fair',
        'strong': 'Strong',
        'very_strong': 'Very Strong',
      };
      return translations[key] || key;
    },
  }),
}));

beforeAll(async () => {
  // Bypass the global mock and import the real component
  ({ PasswordStrengthIndicator } = await vi.importActual<typeof import('../PasswordStrengthIndicator')>('../PasswordStrengthIndicator'));
});

describe('PasswordStrengthIndicator', () => {
  it('does not render when password is empty', () => {
    const { container } = render(<PasswordStrengthIndicator password="" />);
    expect(container.firstChild).toBeNull();
  });

  it('does not render when show is false', () => {
    const { container } = render(<PasswordStrengthIndicator password="test123" show={false} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders strength indicator for weak password', () => {
    render(<PasswordStrengthIndicator password="abc123" />);

    expect(screen.getByText('Strength:')).toBeInTheDocument();
    expect(screen.getByText('Very Weak')).toBeInTheDocument();
  });

  it('renders strength indicator for strong password', () => {
    render(<PasswordStrengthIndicator password="MyP@ssw0rd!2024" />);

    expect(screen.getByText('Strength:')).toBeInTheDocument();
    expect(screen.getByText('Strong')).toBeInTheDocument();
  });

  it('shows very strong for complex passwords', () => {
    render(<PasswordStrengthIndicator password="MyVery$ecureP@ssw0rd!WithM@nyChar$2024" />);

    expect(screen.getByText('Very Strong')).toBeInTheDocument();
  });
});
