import { render, screen, fireEvent } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as NavbarModule from '../Navbar';
import { connected as connectedSignal } from '../Navbar';
import { useLocation } from 'preact-iso';
import { fetchClient, AppState, loggedIn, bc, resetSecret, clearSecretKeyFromServiceWorker } from '../../utils';

// Use actual module for component and functions (test-setup re-exports actual)
const { CustomNavbar } = NavbarModule;

vi.mock('median-js-bridge', () => ({
  default: {
    isNativeApp: () => false,
    sidebar: { setItems: vi.fn() },
  },
}));

vi.mock('logo', () => ({ default: 'logo.png' }));

describe('Navbar', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
    // Reset connection state
    connectedSignal.value = false;
    // Reset location
  const loc = useLocation() as unknown as { url: string };
  loc.url = '/';
  });

  it('renders links and toggles collapse', async () => {
    render(<CustomNavbar />);

    const navbar = screen.getByRole('navigation');
    expect(navbar).toBeInTheDocument();

    expect(screen.getByText('chargers')).toBeInTheDocument();
    expect(screen.getByText('token')).toBeInTheDocument();
    expect(screen.getByText('user')).toBeInTheDocument();
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

  it('logout() clears state and calls API (single session)', async () => {
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockReset();
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ error: undefined });

  const actual = await vi.importActual<typeof import('../Navbar')>('../Navbar');
    await actual.logout(false);

    // Assert
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

    // Assert
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
    (fetchClient.GET as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ error: undefined });
    render(<CustomNavbar />);

    const logoutLink = screen.getByText('logout');
    fireEvent.click(logoutLink);

    // Assert that the API was called with logout_all=false
    await Promise.resolve();
    expect(fetchClient.GET).toHaveBeenCalledWith('/user/logout', { params: { query: { logout_all: false } }, credentials: 'same-origin' });
  });
});
