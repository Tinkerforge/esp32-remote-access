import { act, render, screen, fireEvent, waitFor } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as NavbarModule from '../Navbar';
import { connected as connectedSignal } from '../Navbar';
import { useLocation } from 'preact-iso';
import * as locationModule from 'preact-iso';
import { fetchClient, AppState, loggedIn, bc, resetSecret, clearSecretKeyFromServiceWorker } from '../../utils';
import Median from 'median-js-bridge';

const { CustomNavbar } = NavbarModule;

vi.unmock('@preact/signals');

vi.mock('median-js-bridge', () => ({
  default: {
    isNativeApp: vi.fn(() => false),
    sidebar: { setItems: vi.fn() },
  },
}));

vi.mock('logo', () => ({ default: 'logo.png' }));

describe('Navbar', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
    vi.mocked(fetchClient.GET).mockReset();
    vi.mocked(fetchClient.GET).mockResolvedValue({ response: new Response() });
    vi.mocked(Median.isNativeApp).mockReturnValue(false);
    connectedSignal.value = false;
    const loc = useLocation() as unknown as { url: string };
    loc.url = '/';
  });

  it('renders links and toggles collapse', async () => {
    render(<CustomNavbar />);

    const navbar = screen.getByRole('navigation');
    expect(navbar).toBeInTheDocument();

    expect(screen.getByText('chargers')).toBeInTheDocument();
    expect(screen.getByText('token')).toBeInTheDocument();
    expect(screen.getByTestId('user-icon').closest('a')).toHaveAttribute('href', '/user');
    expect(screen.getByText('logout')).toBeInTheDocument();

    const toggler = screen.getByRole('button');
    fireEvent.click(toggler);
    fireEvent.click(toggler);
  });

  it('hides when connected signal is true', async () => {
    connectedSignal.value = true;
    render(<CustomNavbar />);
    const navbar = screen.getByRole('navigation', { hidden: true });
    expect(navbar).toHaveAttribute('hidden');
  });

  it('uses the authenticated account email as the account link label', async () => {
    vi.mocked(fetchClient.GET).mockResolvedValueOnce({
      data: { id: 'user-1', name: 'Test User', email: 'current@example.com', has_old_charger: false },
      response: new Response(),
    });
    render(<CustomNavbar />);

    expect(await screen.findByRole('link', { name: 'current@example.com', exact: true })).toHaveAttribute('href', '/user');
    expect(fetchClient.GET).toHaveBeenCalledWith('/user/me', { credentials: 'same-origin' });
  });

  it('keeps navigation usable when account information cannot be loaded', async () => {
    vi.mocked(fetchClient.GET).mockRejectedValueOnce(new Error('Offline'));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    render(<CustomNavbar />);

    await waitFor(() => expect(warn).toHaveBeenCalled());
    expect(screen.getByTestId('user-icon').closest('a')).toHaveAttribute('href', '/user');
    expect(screen.getByText('logout')).toBeInTheDocument();
  });

  it('clears the account email when the authenticated navbar is unmounted', async () => {
    vi.mocked(fetchClient.GET).mockResolvedValueOnce({
      data: { id: 'user-1', name: 'Test User', email: 'previous@example.com', has_old_charger: false },
      response: new Response(),
    });
    const { unmount } = render(<CustomNavbar />);
    await screen.findByText('previous@example.com');
    unmount();

    render(<CustomNavbar />);
    expect(screen.queryByText('previous@example.com')).not.toBeInTheDocument();
  });

  it('ignores an account response arriving after logout', async () => {
    let resolveAccount!: (result: { data: { email: string } }) => void;
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockReturnValueOnce(new Promise(resolve => {
      resolveAccount = resolve;
    }));
    const { unmount } = render(<CustomNavbar />);
    unmount();
    render(<CustomNavbar />);

    await act(async () => resolveAccount({ data: { email: 'previous@example.com' } }));
    expect(screen.queryByText('previous@example.com')).not.toBeInTheDocument();
  });

  it('shows the account email in the native sidebar and preserves it when restoring navigation', async () => {
    vi.mocked(Median.isNativeApp).mockReturnValue(true);
    vi.mocked(fetchClient.GET).mockResolvedValueOnce({
      data: { id: 'user-1', name: 'Test User', email: 'current@example.com', has_old_charger: false },
      response: new Response(),
    });
    render(<CustomNavbar />);

    const expected = expect.objectContaining({
      items: expect.arrayContaining([
        expect.objectContaining({ label: 'navbar.user (current@example.com)', url: '/user' }),
      ]),
    });
    await waitFor(() => expect(Median.sidebar.setItems).toHaveBeenLastCalledWith(expected));
    NavbarModule.setAppNavigation();
    expect(Median.sidebar.setItems).toHaveBeenLastCalledWith(expected);
  });

  it('does not replace native device navigation while connected', async () => {
    vi.mocked(Median.isNativeApp).mockReturnValue(true);
    connectedSignal.value = true;
    vi.mocked(fetchClient.GET).mockResolvedValueOnce({
      data: { id: 'user-1', name: 'Test User', email: 'current@example.com', has_old_charger: false },
      response: new Response(),
    });
    render(<CustomNavbar />);

    await act(async () => {});
    expect(Median.sidebar.setItems).not.toHaveBeenCalled();
  });

  it('does not replace native device navigation while the connection is loading', async () => {
    vi.mocked(Median.isNativeApp).mockReturnValue(true);
    vi.spyOn(locationModule, 'useLocation').mockReturnValue({ ...useLocation(), path: '/devices/device-1/' });
    vi.mocked(fetchClient.GET).mockResolvedValueOnce({
      data: { id: 'user-1', name: 'Test User', email: 'current@example.com', has_old_charger: false },
      response: new Response(),
    });
    render(<CustomNavbar />);

    await act(async () => {});
    expect(Median.sidebar.setItems).not.toHaveBeenCalled();
  });

  it('logout() clears state and calls API (single session)', async () => {
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockReset();
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ error: undefined });

  const actual = await vi.importActual<typeof import('../Navbar')>('../Navbar');
    await actual.logout(false);

    expect(fetchClient.GET).toHaveBeenCalledWith('/user/logout', { params: { query: { logout_all: false } }, credentials: 'same-origin' });
    expect(resetSecret).toHaveBeenCalled();
    expect(window.localStorage.removeItem).toHaveBeenCalledWith('loginSalt');
    expect(clearSecretKeyFromServiceWorker).toHaveBeenCalled();
    expect(loggedIn.value).toBe(AppState.LoggedOut);
    expect(bc.postMessage).toHaveBeenCalledWith('logout');
  });

  it('logout() shows alert when logout_all fails', async () => {
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockReset();
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ error: 'boom' });
  const { showAlert } = await vi.importMock<typeof import('../Alert')>('../Alert');
    const showAlertSpy = showAlert as unknown as ReturnType<typeof vi.fn>;

  const actual = await vi.importActual<typeof import('../Navbar')>('../Navbar');
    await actual.logout(true);

    expect(showAlertSpy).toHaveBeenCalledWith('boom', 'danger');
    expect(resetSecret).not.toHaveBeenCalled();
  });

  it('setAppNavigation configures sidebar via Median', async () => {
    const Median = await vi.importMock<typeof import('median-js-bridge')>('median-js-bridge');
    const setItems = vi.spyOn(Median.default.sidebar, 'setItems');
  const mod = await vi.importActual<typeof import('../Navbar')>('../Navbar');
    mod.setAppNavigation();
    expect(setItems).toHaveBeenCalled();
  });

  it('clicking logout link triggers logout(false)', async () => {
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockReset();
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({ error: undefined });
    render(<CustomNavbar />);

    const logoutLink = screen.getByText('logout');
    fireEvent.click(logoutLink);

    await Promise.resolve();
    expect(fetchClient.GET).toHaveBeenCalledWith('/user/logout', { params: { query: { logout_all: false } }, credentials: 'same-origin' });
  });
});
