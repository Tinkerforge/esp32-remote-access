import { render, screen, waitFor, fireEvent, cleanup } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';

vi.mock('argon2-browser', () => ({
  hash: vi.fn(async () => ({ hash: new Uint8Array([1, 2, 3]) })),
  ArgonType: { Argon2id: 2 },
}));

import { Tokens } from '../Tokens';
import { Base64 } from 'js-base64';
import sodium from 'libsodium-wrappers';

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
  let restoreIntervalMocks: (() => void) | null = null;

  beforeEach(async () => {
    vi.clearAllMocks();

    const originalSetInterval = globalThis.setInterval;
    const originalClearInterval = globalThis.clearInterval;
    globalThis.setInterval = ((() => {
      return { __fake: 'interval' } as unknown as NodeJS.Timeout;
    }) as unknown) as typeof setInterval;
    globalThis.clearInterval = (() => undefined) as typeof clearInterval;
    restoreIntervalMocks = () => {
      globalThis.setInterval = originalSetInterval;
      globalThis.clearInterval = originalClearInterval;
    };

    const utils = await import('../../utils');
    fetchClient = utils.fetchClient;
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

    const alertModule = await import('../../components/Alert');
    showAlert = alertModule.showAlert as unknown as Mock;
  });

  afterEach(() => {
    cleanup();
    restoreIntervalMocks?.();
    restoreIntervalMocks = null;
  });

  it('renders loading spinner initially', () => {
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

  it('sorts tokens using the selected option', async () => {
    const base64 = Base64 as unknown as { toUint8Array: Mock };
    const encoder = new TextEncoder();
    base64.toUint8Array.mockImplementation((value: string) => encoder.encode(value));

    const libsodium = sodium as unknown as { crypto_box_seal_open: Mock };
    libsodium.crypto_box_seal_open.mockImplementation((binary: Uint8Array) => binary);

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
          data: {
            tokens: [
              { token: 'token-alpha', use_once: false, id: '1', name: 'Alpha', created_at: 1_000, last_used_at: null },
              { token: 'token-bravo', use_once: false, id: '2', name: 'Bravo', created_at: 2_000, last_used_at: 1_900 },
              { token: 'token-charlie', use_once: false, id: '3', name: 'Charlie', created_at: 1_500, last_used_at: 1_950 },
            ],
          },
          error: null,
          response: { status: 200 },
        });
      }
      return Promise.resolve({ data: null, error: 'Not found', response: { status: 404 } });
    });

    render(<Tokens />);

    await waitFor(() => {
      expect(screen.getByLabelText('tokens.sort_label')).toBeTruthy();
    });

    const getTokenNames = () => screen
      .getAllByRole('heading', { level: 6 })
      .map((heading) => heading.textContent?.trim())
      .filter(Boolean);

    expect(getTokenNames()).toEqual(['Bravo', 'Charlie', 'Alpha']);

    const select = screen.getByLabelText('tokens.sort_label');
    fireEvent.change(select, { target: { value: 'name-asc' } });

    await waitFor(() => {
      expect(getTokenNames()).toEqual(['Alpha', 'Bravo', 'Charlie']);
    });

    fireEvent.change(select, { target: { value: 'last-used-desc' } });

    await waitFor(() => {
      expect(getTokenNames()).toEqual(['Charlie', 'Bravo', 'Alpha']);
    });
  });

  it('filters tokens using the search field', async () => {
    const base64 = Base64 as unknown as { toUint8Array: Mock };
    const encoder = new TextEncoder();
    base64.toUint8Array.mockImplementation((value: string) => encoder.encode(value));

    const libsodium = sodium as unknown as { crypto_box_seal_open: Mock };
    libsodium.crypto_box_seal_open.mockImplementation((binary: Uint8Array) => binary);

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
          data: {
            tokens: [
              { token: 'token-alpha', use_once: false, id: '1', name: 'Alpha', created_at: 1_000, last_used_at: null },
              { token: 'token-bravo', use_once: false, id: '2', name: 'Bravo', created_at: 2_000, last_used_at: 1_900 },
              { token: 'token-charlie', use_once: false, id: '3', name: 'Charlie', created_at: 1_500, last_used_at: 1_950 },
            ],
          },
          error: null,
          response: { status: 200 },
        });
      }
      return Promise.resolve({ data: null, error: 'Not found', response: { status: 404 } });
    });

    render(<Tokens />);

    await waitFor(() => {
      expect(screen.getByLabelText('tokens.search_label')).toBeTruthy();
    });

    const getTokenNames = () => screen
      .queryAllByRole('heading', { level: 6 })
      .map((heading) => heading.textContent?.trim())
      .filter(Boolean);

    await waitFor(() => {
      expect(getTokenNames()).toEqual(['Bravo', 'Charlie', 'Alpha']);
    });

    const searchInput = screen.getByLabelText('tokens.search_label');
    const changeSearch = (value: string) => fireEvent.change(searchInput, { target: { value } });

    changeSearch('char');
    await waitFor(() => {
      expect(getTokenNames()).toEqual(['Charlie']);
    });

    changeSearch('brav');
    await waitFor(() => {
      expect(getTokenNames()).toEqual(['Bravo']);
    });

    changeSearch('zzz');
    await waitFor(() => {
      expect(screen.getByText('tokens.no_tokens_search')).toBeTruthy();
    });

    changeSearch('');
    await waitFor(() => {
      expect(getTokenNames()).toEqual(['Bravo', 'Charlie', 'Alpha']);
    });
  });
});
