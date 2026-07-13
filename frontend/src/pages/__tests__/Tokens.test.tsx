import { render, screen, waitFor, fireEvent, cleanup, act } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';

vi.mock('argon2-browser', () => ({
  hash: vi.fn(async () => ({ hash: new Uint8Array([1, 2, 3]) })),
  ArgonType: { Argon2id: 2 },
}));

import { Tokens } from '../Tokens';
import { Base64 } from 'js-base64';
import sodium from 'libsodium-wrappers';

const mockUserData = {
  id: 'user-123',
  name: 'John Doe',
  email: 'john@example.com',
  has_old_charger: false,
};

describe('Tokens Component', () => {
  let fetchClient: typeof import('../../utils').fetchClient;
  let showAlert: Mock;
  let restoreIntervalMocks: (() => void) | null = null;

  beforeEach(async () => {
    vi.clearAllMocks();

    // jsdom does not provide a clipboard API by default; install a stub
    // so the copy/delete handlers can be exercised.
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });

    const originalSetInterval = globalThis.setInterval;
    const originalClearInterval = globalThis.clearInterval;
    globalThis.setInterval = ((() => ({ __fake: 'interval' } as unknown as NodeJS.Timeout))) as unknown as typeof setInterval;
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

    (fetchClient.POST as unknown as Mock).mockReset();
    (fetchClient.DELETE as unknown as Mock).mockReset();

    const alertModule = await import('../../components/Alert');
    showAlert = alertModule.showAlert as unknown as Mock;
  });

  afterEach(() => {
    cleanup();
    restoreIntervalMocks?.();
    restoreIntervalMocks = null;
    vi.mocked(navigator.clipboard.writeText).mockReset();
  });

  it('renders loading spinner initially', () => {
    render(<Tokens />);
    expect(screen.getByText('Loading...')).toBeInTheDocument();
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

  it('shows an unexpected_error alert when the network call throws', async () => {
    (fetchClient.GET as unknown as Mock).mockImplementation((path: string) => {
      if (path === '/user/me') return Promise.reject(new Error('boom'));
      return Promise.resolve({ data: { tokens: [] }, error: null, response: { status: 200 } });
    });

    render(<Tokens />);

    await waitFor(() => {
      expect(showAlert).toHaveBeenCalledWith('tokens.unexpected_error', 'danger');
    }, { timeout: 2000 });
  });

  it('decrypts each token name and surfaces a warning when decryption fails', async () => {
    const base64 = Base64 as unknown as { toUint8Array: Mock };
    base64.toUint8Array.mockImplementation((value: string) => new TextEncoder().encode(value));
    const libsodium = sodium as unknown as { crypto_box_seal_open: Mock };
    libsodium.crypto_box_seal_open.mockImplementation(() => {
      // The name-decryption path is the only call to `crypto_box_seal_open`
      // in the component; throw on the first (and only) invocation to
      // trigger the unknown-name + decrypt-warning branches.
      throw new Error('decrypt failed');
    });

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
              {
                token: 'token-alpha',
                use_once: false,
                id: '1',
                name: 'encrypted-name',
                created_at: 1_000,
                last_used_at: null,
              },
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
      // Decrypted name was unavailable; the unknown-name placeholder is shown.
      expect(screen.getByText('tokens.unknown_name')).toBeInTheDocument();
      // The decryption-failed warning also becomes visible.
      expect(screen.getByText('tokens.decrypt_name_failed')).toBeInTheDocument();
    });
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

    fireEvent.change(select, { target: { value: 'created-asc' } });
    await waitFor(() => {
      expect(getTokenNames()).toEqual(['Alpha', 'Charlie', 'Bravo']);
    });

    fireEvent.change(select, { target: { value: 'name-desc' } });
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

describe('Tokens Component - interactions', () => {
  const baseTokens = (path: string) => {
    if (path === '/user/me') {
      return Promise.resolve({ data: mockUserData, error: null, response: { status: 200 } });
    }
    if (path === '/user/get_authorization_tokens') {
      return Promise.resolve({
        data: {
          tokens: [
            { token: 'token-alpha', use_once: false, id: '1', name: 'Alpha', created_at: 1_000, last_used_at: null },
          ],
        },
        error: null,
        response: { status: 200 },
      });
    }
    return Promise.resolve({ data: null, error: 'Not found', response: { status: 404 } });
  };

  let fetchClient: typeof import('../../utils').fetchClient;
  let showAlert: Mock;
  let restoreIntervalMocks: (() => void) | null = null;

  beforeEach(async () => {
    vi.clearAllMocks();

    // jsdom does not provide a clipboard API by default; install a stub
    // so the copy/delete handlers can be exercised.
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });

    const originalSetInterval = globalThis.setInterval;
    const originalClearInterval = globalThis.clearInterval;
    globalThis.setInterval = ((() => ({ __fake: 'interval' } as unknown as NodeJS.Timeout))) as unknown as typeof setInterval;
    globalThis.clearInterval = (() => undefined) as typeof clearInterval;
    restoreIntervalMocks = () => {
      globalThis.setInterval = originalSetInterval;
      globalThis.clearInterval = originalClearInterval;
    };

    const utils = await import('../../utils');
    fetchClient = utils.fetchClient;
    (fetchClient.GET as unknown as Mock).mockImplementation(baseTokens);
    (fetchClient.POST as unknown as Mock).mockReset();
    (fetchClient.DELETE as unknown as Mock).mockReset();

    const libsodium = sodium as unknown as { crypto_box_seal_open: Mock; crypto_box_seal: Mock };
    libsodium.crypto_box_seal_open.mockImplementation((binary: Uint8Array) => binary);
    // crypto_box_seal just needs to return a valid Uint8Array; the actual
    // encryption isn't exercised here.
    libsodium.crypto_box_seal.mockImplementation(() => new Uint8Array([1, 2, 3, 4]));

    const base64 = Base64 as unknown as { toUint8Array: Mock; fromUint8Array: Mock };
    base64.toUint8Array.mockImplementation((value: string) => new TextEncoder().encode(value));
    base64.fromUint8Array.mockImplementation(() => 'base64-encoded-name');

    const alertModule = await import('../../components/Alert');
    showAlert = alertModule.showAlert as unknown as Mock;
  });

  afterEach(() => {
    cleanup();
    restoreIntervalMocks?.();
    restoreIntervalMocks = null;
    if (navigator.clipboard && (navigator.clipboard.writeText as unknown as { mockReset?: () => void }).mockReset) {
      vi.mocked(navigator.clipboard.writeText).mockReset();
    }
  });

  it('sends a POST /user/create_authorization_token when the create form is submitted', async () => {
    (fetchClient.POST as unknown as Mock).mockResolvedValueOnce({
      data: {
        token: 'new-encoded-token',
        use_once: true,
        id: 'token-2',
        name: 'MyToken',
        created_at: 2_000,
        last_used_at: null,
      },
      error: null,
      response: { status: 201 },
    });

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.create_token')).toBeTruthy();
    });

    const nameInput = screen.getByPlaceholderText('tokens.name_placeholder');
    fireEvent.change(nameInput, { target: { value: 'MyToken' } });

    const form = nameInput.closest('form');
    expect(form).not.toBeNull();

    await act(async () => {
      fireEvent.submit(form as HTMLElement);
    });

    await waitFor(() => {
      expect(fetchClient.POST).toHaveBeenCalledWith('/user/create_authorization_token', expect.objectContaining({
        body: expect.objectContaining({ use_once: true }),
        credentials: 'same-origin',
      }));
    });
  });

  it('autogenerates a "Token-N" name when the name field is empty and there are no Token-N tokens yet', async () => {
    (fetchClient.POST as unknown as Mock).mockResolvedValueOnce({
      data: {
        token: 'new-encoded-token',
        use_once: true,
        id: 'token-2',
        name: '',
        created_at: 2_000,
        last_used_at: null,
      },
      response: { status: 201 },
      error: null,
    });

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.create_token')).toBeTruthy();
    });

    const form = screen.getByPlaceholderText('tokens.name_placeholder').closest('form');
    expect(form).not.toBeNull();

    await act(async () => {
      fireEvent.submit(form as HTMLElement);
    });

    // The auto-named token appears in the rendered list once the POST
    // succeeds. The seeded "Alpha" token doesn't match the Token-N
    // pattern, so the next generated name is "Token-1".
    await waitFor(() => {
      expect(screen.getAllByRole('heading', { level: 6 }).map((h) => h.textContent?.trim()))
        .toEqual(expect.arrayContaining(['Token-1', 'Alpha']));
    });
    expect(fetchClient.POST).toHaveBeenCalledWith(
      '/user/create_authorization_token',
      expect.objectContaining({ body: expect.objectContaining({ use_once: true }) }),
    );
  });

  it('autogenerates "Token-(N+1)" based on the highest existing Token-N', async () => {
    (fetchClient.POST as unknown as Mock).mockResolvedValueOnce({
      data: {
        token: 'new-encoded-token',
        use_once: true,
        id: 'token-new',
        name: '',
        created_at: 2_000,
        last_used_at: null,
      },
      response: { status: 201 },
      error: null,
    });

    (fetchClient.GET as unknown as Mock).mockImplementation((path: string) => {
      if (path === '/user/me') {
        return Promise.resolve({ data: mockUserData, error: null, response: { status: 200 } });
      }
      if (path === '/user/get_authorization_tokens') {
        return Promise.resolve({
          data: {
            tokens: [
              { token: 'a', use_once: false, id: '1', name: 'Token-3', created_at: 1_000, last_used_at: null },
              { token: 'b', use_once: false, id: '2', name: 'Token-7', created_at: 1_500, last_used_at: null },
              { token: 'c', use_once: false, id: '3', name: 'Token-5', created_at: 1_700, last_used_at: null },
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
      expect(screen.getByText('Token-7')).toBeTruthy();
    });

    const form = screen.getByPlaceholderText('tokens.name_placeholder').closest('form');
    await act(async () => {
      fireEvent.submit(form as HTMLElement);
    });

    // Highest seeded Token-N is 7, so the next generated name is "Token-8".
    // The decoded existing tokens ("Token-3", "Token-5", "Token-7") stay
    // visible alongside the new entry.
    await waitFor(() => {
      const names = screen.getAllByRole('heading', { level: 6 }).map((h) => h.textContent?.trim());
      expect(names).toContain('Token-8');
      expect(names).toContain('Token-3');
      expect(names).toContain('Token-7');
      expect(names).toContain('Token-5');
    });
    expect(fetchClient.POST).toHaveBeenCalledWith(
      '/user/create_authorization_token',
      expect.objectContaining({ body: expect.objectContaining({ use_once: true }) }),
    );
  });

  it('shows create_token_failed when the server returns an error', async () => {
    (fetchClient.POST as unknown as Mock).mockResolvedValueOnce({
      data: null,
      error: 'Something went wrong',
      response: { status: 500 },
    });

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.create_token')).toBeTruthy();
    });

    const form = screen.getByPlaceholderText('tokens.name_placeholder').closest('form');
    await act(async () => {
      fireEvent.submit(form as HTMLElement);
    });

    await waitFor(() => {
      expect(showAlert).toHaveBeenCalledWith('tokens.create_token_failed', 'danger');
    });
  });

  it('writes to the clipboard and shows a copy success alert when the copy button is clicked', async () => {
    vi.mocked(navigator.clipboard.writeText).mockResolvedValueOnce(undefined);

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.copy')).toBeTruthy();
    });

    const copyButton = screen.getByText('tokens.copy').closest('button') as HTMLButtonElement;
    await act(async () => {
      fireEvent.click(copyButton);
    });

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalled();
      expect(showAlert).toHaveBeenCalledWith(
        'tokens.copy_success_text',
        'success',
        'token_copy',
        'tokens.copy_success',
        2000,
      );
    });
  });

  it('surfaces a copy failure alert when the clipboard write rejects', async () => {
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('no clipboard'));

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.copy')).toBeTruthy();
    });

    const copyButton = screen.getByText('tokens.copy').closest('button') as HTMLButtonElement;
    await act(async () => {
      fireEvent.click(copyButton);
    });

    await waitFor(() => {
      expect(showAlert).toHaveBeenCalledWith('tokens.copy_failed', 'danger');
    });
  });

  it('sends DELETE /user/delete_authorization_token when the delete button is clicked', async () => {
    (fetchClient.DELETE as unknown as Mock).mockResolvedValueOnce({
      response: { status: 200 },
      error: null,
    });

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.delete')).toBeTruthy();
    });

    const deleteButton = screen.getByText('tokens.delete').closest('button') as HTMLButtonElement;
    await act(async () => {
      fireEvent.click(deleteButton);
    });

    await waitFor(() => {
      expect(fetchClient.DELETE).toHaveBeenCalledWith('/user/delete_authorization_token', expect.objectContaining({
        body: { id: '1' },
        credentials: 'same-origin',
      }));
    });
  });

  it('surfaces delete_token_failed when DELETE returns a non-200', async () => {
    (fetchClient.DELETE as unknown as Mock).mockResolvedValueOnce({
      response: { status: 500 },
      error: 'oops',
    });

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.delete')).toBeTruthy();
    });

    const deleteButton = screen.getByText('tokens.delete').closest('button') as HTMLButtonElement;
    await act(async () => {
      fireEvent.click(deleteButton);
    });

    await waitFor(() => {
      expect(showAlert).toHaveBeenCalledWith('tokens.delete_token_failed', 'danger');
    });
  });

  it('surfaces the unexpected_error alert when DELETE rejects', async () => {
    (fetchClient.DELETE as unknown as Mock).mockRejectedValueOnce(new Error('network'));

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.delete')).toBeTruthy();
    });

    const deleteButton = screen.getByText('tokens.delete').closest('button') as HTMLButtonElement;
    await act(async () => {
      fireEvent.click(deleteButton);
    });

    await waitFor(() => {
      expect(showAlert).toHaveBeenCalledWith('tokens.unexpected_error', 'danger');
    });
  });

  it('toggles the single-use switch', async () => {
    (fetchClient.POST as unknown as Mock).mockResolvedValueOnce({
      data: {
        token: 'new-encoded-token',
        use_once: false,
        id: 'token-2',
        name: 'MyToken',
        created_at: 2_000,
        last_used_at: null,
      },
      response: { status: 201 },
      error: null,
    });

    render(<Tokens />);
    await waitFor(() => {
      expect(screen.getByText('tokens.create_token')).toBeTruthy();
    });

    const checkbox = screen.getByTestId('checkbox') as HTMLInputElement;
    expect(checkbox.checked).toBe(true);

    fireEvent.change(checkbox, { target: { checked: false } });
    expect(checkbox.checked).toBe(false);

    const form = (screen.getByPlaceholderText('tokens.name_placeholder').closest('form'));
    expect(form).not.toBeNull();
    await act(async () => {
      fireEvent.submit(form as HTMLElement);
    });

    await waitFor(() => {
      expect(fetchClient.POST).toHaveBeenCalled();
    });
    const [, postArgs] = vi.mocked(fetchClient.POST).mock.calls[0] ?? [];
    expect((postArgs as { body: { use_once: boolean } }).body.use_once).toBe(false);
  });
});
