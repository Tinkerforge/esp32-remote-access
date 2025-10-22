import { render, screen, waitFor } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { Tokens } from '../Tokens';

/**
 * Tests for the Tokens component
 *
 * This component manages authorization tokens for the application.
 * It handles token creation, display, and deletion.
 */
describe('Tokens Component', () => {
  const mockUserData = {
    id: 'user-123',
    name: 'John Doe',
    email: 'john@example.com',
    has_old_charger: false,
  };

  let fetchClient: typeof import('../../utils').fetchClient;
  let showAlert: Mock;

  beforeEach(async () => {
    vi.clearAllMocks();

    const utils = await import('../../utils');
    fetchClient = utils.fetchClient;

    const alertModule = await import('../../components/Alert');
    showAlert = alertModule.showAlert as unknown as Mock;
  });

  it('renders loading spinner initially', () => {
    (fetchClient.GET as unknown as Mock).mockImplementation(() =>
      new Promise(() => {}) // Never resolves
    );

    render(<Tokens />);
    expect(screen.getByText('Loading...')).toBeTruthy();
  });

  it('renders component after successful data fetch', async () => {
    (fetchClient.GET as unknown as Mock).mockImplementation((path: string) => {
      if (path === '/user/me') {
        return Promise.resolve({
          data: mockUserData,
          error: null,
          response: { status: 200 },
        });
      }
      if (path === '/user/get_authorization_tokens') {
        return Promise.resolve({
          data: { tokens: [] },
          error: null,
          response: { status: 200 },
        });
      }
      return Promise.resolve({ data: null, error: 'Not found', response: { status: 404 } });
    });

    render(<Tokens />);

    await waitFor(() => {
      expect(screen.queryByText('Loading...')).toBeNull();
    }, { timeout: 2000 });

    expect(screen.getByText('tokens.create_token')).toBeTruthy();
  });

  it('shows error when user fetch fails', async () => {
    (fetchClient.GET as unknown as Mock).mockImplementation((path: string) => {
      if (path === '/user/me') {
        return Promise.resolve({ data: null, error: 'Error', response: { status: 400 } });
      }
      return Promise.resolve({ data: { tokens: [] }, error: null, response: { status: 200 } });
    });

    render(<Tokens />);

    await waitFor(() => {
      expect(showAlert).toHaveBeenCalledWith('tokens.fetch_user_failed', 'danger');
    }, { timeout: 2000 });
  });

  it('shows error when tokens fetch fails', async () => {
    (fetchClient.GET as unknown as Mock).mockImplementation((path: string) => {
      if (path === '/user/me') {
        return Promise.resolve({ data: mockUserData, error: null, response: { status: 200 } });
      }
      if (path === '/user/get_authorization_tokens') {
        return Promise.resolve({ data: null, error: 'Error', response: { status: 400 } });
      }
      return Promise.resolve({ data: null, error: 'Not found', response: { status: 404 } });
    });

    render(<Tokens />);

    await waitFor(() => {
      expect(showAlert).toHaveBeenCalledWith('tokens.fetch_tokens_failed', 'danger');
    }, { timeout: 2000 });
  });
});
