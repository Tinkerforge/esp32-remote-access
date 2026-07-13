import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/preact';
import * as utils from './utils.js';
import Median from 'median-js-bridge';
import type { ComponentChildren } from 'preact';

// Stub preact.render globally so the module-level mount in index.tsx doesn't
// touch the real DOM; the per-test setup re-runs the same mock factory
// after every vi.resetModules() to ensure freshly-imported modules see it.
vi.mock('preact', async (importOriginal) => {
  const actual = await importOriginal<typeof import('preact')>();
  return { ...actual, render: vi.fn() };
});

vi.spyOn(console, 'warn').mockImplementation(() => undefined);

describe('index.tsx', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);

    (utils.loggedIn as { value: number }).value = utils.AppState.LoggedOut;
    // @ts-expect-error test flag for early-return guard in App
    window.ServiceWorker = {};

    let app = document.getElementById('app');
    if (!app) {
      app = document.createElement('div');
      app.id = 'app';
      document.body.appendChild(app);
    }
  });

  it('shows a message when ServiceWorker is missing', async () => {
    // @ts-expect-error test override
    delete window.ServiceWorker;

    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    const { App } = await import('./index');
    render(<App />);
    expect(screen.getByText('no_service_worker')).toBeInTheDocument();
  });

  it('renders logged in view with navbar and router', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.LoggedIn;

    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    const { App } = await import('./index');
    render(<App />);
    expect(screen.getByRole('navigation')).toBeInTheDocument();
  });

  it('sets favicon href on load', async () => {
    const link = document.createElement('link');
    link.setAttribute('rel', 'icon');
    document.head.appendChild(link);

    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    await import('./index');

    expect((link as HTMLLinkElement).href).toContain('favicon.png');
  });

  it('registers unhandledrejection handler in debug mode and uses Median.share.downloadFile', async () => {
    (utils.isDebugMode as { value: boolean }).value = true;

    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    await import('./index');

    const nativeSpy = vi.spyOn(Median, 'isNativeApp').mockReturnValue(true);
    const evt = new Event('unhandledrejection');
    (evt as unknown as { reason?: unknown }).reason = { message: 'boom', stack: 'boom\nstack' };
    window.dispatchEvent(evt);

    expect(Median.share.downloadFile).toHaveBeenCalled();
    nativeSpy.mockRestore();
  });

  it('renders Recovery state without Footer when running in native app', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.Recovery;
    // @ts-expect-error define presence flag
    window.ServiceWorker = {};
    const medianModule = await import('median-js-bridge');
    const nativeSpy = vi.spyOn(medianModule.default, 'isNativeApp').mockReturnValue(true);

    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    const { App } = await import('./index');
    render(<App />);

    expect(screen.getByTestId('error-alert')).toBeInTheDocument();
    expect(document.getElementById('footer')).toBeFalsy();

    nativeSpy.mockRestore();
  });

  it('renders the loading spinner while the auth state is Loading', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.Loading;

    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    const { App } = await import('./index');
    render(<App />);

    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('resets document title on Router onRouteChange in LoggedIn state', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.LoggedIn;
    // @ts-expect-error define presence flag
    window.ServiceWorker = {};
    document.title = 'not_app_name';

    vi.resetModules();
    // Mock Navbar so connected=true and the document-title effect on mount
    // doesn't reset it before onRouteChange runs.
    vi.doMock('./components/Navbar.js', () => ({
      connected: { value: true },
      CustomNavbar: () => <div data-testid="navbar" />,
    }));
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    vi.doMock('preact-iso', () => ({
      LocationProvider: ({ children }: { children?: ComponentChildren }) => <div>{children}</div>,
      Router: ({ children, onRouteChange }: { children?: ComponentChildren; onRouteChange?: () => void }) => {
        if (onRouteChange) {
          onRouteChange();
        }
        return <div>{children}</div>;
      },
      Route: ({ children }: { children?: ComponentChildren }) => <div>{children}</div>,
      useLocation: () => ({ route: vi.fn(), url: '/' }),
      useRoute: () => ({ params: {} }),
      lazy: () => () => <div data-testid="lazy-component" />,
    }));

    const { App } = await import('./index');
    render(<App />);

    expect(document.title).toBe('app_name');
  });

  it('registers chargers and devices Routes so the legacy /chargers path still works', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.LoggedIn;
    // @ts-expect-error define presence flag
    window.ServiceWorker = {};

    const registeredRoutes: Array<{ path?: string; component: unknown }> = [];
    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    // Capture every Route so we can assert backwards-compat paths exist.
    vi.doMock('preact-iso', () => ({
      LocationProvider: ({ children }: { children?: ComponentChildren }) => h('div', null, children),
      Router: ({ children }: { children?: ComponentChildren }) => h('div', null, children),
      Route: ((props: { path?: string; component: unknown }) => {
        registeredRoutes.push(props);
        return null;
      }) as unknown as never,
      useLocation: () => ({ route: vi.fn(), url: '/' }),
      useRoute: () => ({ params: {} }),
      lazy: () => () => null,
    }));
    vi.doMock('./components/Navbar.js', () => ({
      connected: { value: true },
      CustomNavbar: () => null,
    }));

    const { h } = await import('preact');
    const { App } = await import('./index');
    render(<App />);

    const paths = registeredRoutes.map((r) => r.path);
    expect(paths).toContain('/tokens');
    expect(paths).toContain('/user');
    // Backwards-compat: legacy `/chargers` URL must still resolve to the
    // device list, alongside `/devices/...`.
    expect(paths).toContain('/chargers');
    expect(paths).toContain('/chargers/:device/:path*');
    expect(paths).toContain('/devices/:device/:path*');
  });
});
