import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/preact';
import * as utils from './utils.js';
import Median from 'median-js-bridge';

// Default: stub preact.render to avoid module-level mount; tests re-do this before importing index as needed
vi.mock('preact', async (importOriginal) => {
  const actual = await importOriginal<typeof import('preact')>();
  return { ...actual, render: vi.fn() };
});

// Silence console noise from iframe warning in App
vi.spyOn(console, 'warn').mockImplementation(() => {});

describe('index.tsx', () => {
  beforeEach(() => {
    // Clean up spies between tests
    vi.restoreAllMocks();
    // Re-silence console.warn after restore
    vi.spyOn(console, 'warn').mockImplementation(() => {});

    // Default state and environment
    (utils.loggedIn as { value: number }).value = utils.AppState.LoggedOut;
    // @ts-expect-error test flag for early-return guard in App
    window.ServiceWorker = {};

    // Ensure container exists
    let app = document.getElementById('app');
    if (!app) {
      app = document.createElement('div');
      app.id = 'app';
      document.body.appendChild(app);
    }
  });

  it('shows a message when ServiceWorker is missing', async () => {
    // Remove ServiceWorker to hit the early return branch
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

  it('migrates secret key to service worker and shows loading state', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.Loading;
    localStorage.setItem('secretKey', 'abc');
    // Ensure a controller exists for postMessage path
    // @ts-expect-error extend navigator
    navigator.serviceWorker = { controller: { postMessage: vi.fn() } };
    // Ensure ServiceWorker is present to avoid early return branch
    // @ts-expect-error define presence flag
    window.ServiceWorker = {};
    // Avoid network calls from version checker
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 200 }));

    vi.resetModules();
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    const { App } = await import('./index');
    render(<App />);

    expect(window.localStorage.getItem('secretKey')).toBeNull();
    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('renders Recovery state without Footer when running in native app', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.Recovery;
    // Ensure ServiceWorker is present to avoid early return
    // @ts-expect-error define presence flag
    window.ServiceWorker = {};
    // Force native app so Footer is hidden in Recovery view
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

  it('resets document title on Router onRouteChange in LoggedIn state', async () => {
    (utils.loggedIn as { value: number }).value = utils.AppState.LoggedIn;
    // Ensure ServiceWorker is present to avoid early return
    // @ts-expect-error define presence flag
    window.ServiceWorker = {};
    document.title = 'not_app_name';

    vi.resetModules();
    // Mock Navbar to set connected=true so the first title-setting effect doesn't run
    vi.doMock('./components/Navbar.js', () => ({
      connected: { value: true },
      CustomNavbar: () => <div data-testid="navbar" />,
    }));
    // Mock preact render
    vi.doMock('preact', async (importOriginal) => {
      const actual = await importOriginal<typeof import('preact')>();
      return { ...actual, render: vi.fn() };
    });
    // Mock Router to immediately invoke onRouteChange once
    vi.doMock('preact-iso', () => ({
      LocationProvider: ({ children }: { children?: any }) => <div>{children}</div>,
      Router: ({ children, onRouteChange }: { children?: any; onRouteChange?: () => void }) => {
        onRouteChange && onRouteChange();
        return <div>{children}</div>;
      },
      Route: ({ children }: { children?: any }) => <div>{children}</div>,
      useLocation: () => ({ route: vi.fn(), url: '/' }),
      useRoute: () => ({ params: {} }),
      lazy: (_loader: () => Promise<unknown>) => (_props: Record<string, unknown>) => <div data-testid="lazy-component" />,
    }));

    const { App } = await import('./index');
    render(<App />);

    expect(document.title).toBe('app_name');
  });
});
