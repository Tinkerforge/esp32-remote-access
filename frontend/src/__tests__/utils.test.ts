import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';
// Mock argon2-browser early to avoid wasm loading in test environment
vi.mock('argon2-browser', () => ({
  hash: vi.fn(async () => ({ hash: 'MOCK_HASH' })),
  ArgonType: { Argon2id: 2 }
}));

// We'll load the actual utils implementation (bypassing the broad mock in test-setup)
let utils: typeof import('../utils');
beforeAll(async () => {
  vi.resetModules();
  // Ensure module is not mocked
  vi.doUnmock('../utils');
  utils = await vi.importActual('../utils');
});
import { MessageType } from '../types';

// Helper to create a mock service worker controller
let swListener: ((event: MessageEvent) => void) | undefined;

function withServiceWorker(controller: Partial<ServiceWorker> & { postMessage: (msg: unknown) => void }) {
  const addEventListener = vi.fn((type: string, cb: (event: MessageEvent) => void) => {
    if (type === 'message') {
      swListener = cb;
    }
  });
  const removeEventListener = vi.fn(() => {
    swListener = undefined;
  });

  // Mock getRegistration to return an active service worker
  // The active property should have the postMessage method
  const getRegistration = vi.fn(async () => ({
    active: controller as ServiceWorker,
  }));

  const ready = Promise.resolve({
    active: controller as ServiceWorker,
  } as ServiceWorkerRegistration);

  Object.defineProperty(navigator, 'serviceWorker', {
    value: {
      addEventListener,
      removeEventListener,
      getRegistration,
      ready,
    },
    configurable: true,
  });
  return controller;
}

function triggerSWMessage(msg: unknown) {
  if (swListener) {
    swListener({ data: msg } as MessageEvent);
  }
}

describe('utils', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
    // Clear service worker listener between tests
    swListener = undefined;
    // Reset the getSecretKeyFromServiceWorker internal state
    (utils as any).gettingSecretInProgress = false;
    (utils as any).secretKeyPromise = null;
    (utils as any).retries = 0;
  });

  describe('generate_random_bytes', () => {
    it('returns Uint8Array of specified length with random values', () => {
      // Mock crypto.getRandomValues deterministically
      const mockGetRandomValues = vi.spyOn(globalThis.crypto, 'getRandomValues').mockImplementation((arr: ArrayBufferView) => {
        if (arr instanceof Uint8Array) {
          for (let i = 0; i < arr.length; i++) arr[i] = i + 1;
        }
        return arr as typeof arr;
      });

        const bytes = utils.generate_random_bytes(5);
      expect(bytes).toBeInstanceOf(Uint8Array);
      expect(bytes.length).toBe(5);
      expect(Array.from(bytes)).toEqual([1,2,3,4,5]);
      expect(mockGetRandomValues).toHaveBeenCalledTimes(1);
    });
  });

  describe('concat_salts', () => {
    it('concatenates provided salt with newly generated random bytes', () => {
      const original = new Uint8Array([9,9,9]);
      // We can't reliably mock the internal generate_random_bytes (same-module reference),
      // just assert structural properties: length and prefix.
      const result = utils.concat_salts(original);
      expect(result).toBeInstanceOf(Uint8Array);
      expect(result.length).toBe(original.length + 24);
      expect(Array.from(result.slice(0, original.length))).toEqual(Array.from(original));
      expect(result.slice(original.length).length).toBe(24);
    });
  });

  describe('Service Worker secret key helpers', () => {
    it('stores secret in service worker if controller exists', async () => {
      const postMessage = vi.fn((msg) => {
        if (msg.type === MessageType.StoreSecret) {
          triggerSWMessage({ type: MessageType.StoreSecret, data: "stored" });
        }
      });
      withServiceWorker({ postMessage } as any);
      await utils.storeSecretKeyInServiceWorker('abc');
      expect(postMessage).toHaveBeenCalledWith({ type: MessageType.StoreSecret, data: 'abc' });
    });

    it('requests secret and resolves with response (getSecretKeyFromServiceWorker)', async () => {
      const postMessage = vi.fn((msg: any) => {
        // When the service worker receives a RequestSecret message,
        // simulate it responding with the secret
        if (msg.type === MessageType.RequestSecret) {
            triggerSWMessage({ type: MessageType.RequestSecret, data: 'encoded' });
        }
      });
      withServiceWorker({ postMessage } as any);

      const promise = utils.getSecretKeyFromServiceWorker();
      // simulate worker answering
      triggerSWMessage({ type: MessageType.StoreSecret, data: 'encoded' });
      const key = await promise;
      expect(postMessage).toHaveBeenCalledWith({ type: MessageType.RequestSecret, data: null });
      expect(key).toBe('encoded');
    });

    it('clears secret key via service worker', async () => {
      const postMessage = vi.fn();
      withServiceWorker({ postMessage } as any);
      await utils.clearSecretKeyFromServiceWorker();
      expect(postMessage).toHaveBeenCalledWith({ type: MessageType.ClearSecret, data: null });
    });

    it('handles getSecretKeyFromServiceWorker with mock that immediately triggers response', async () => {
      // Create a mock service worker that immediately responds
      const postMessage = vi.fn((msg: any) => {
        if (msg.type === MessageType.RequestSecret) {
            triggerSWMessage({ type: MessageType.RequestSecret, data: 'immediate-response' });
        }
      });

      // Reset the module-level variables by re-importing
      vi.resetModules();
      const freshUtils = await vi.importActual('../utils') as typeof utils;

      withServiceWorker({ postMessage } as any);

      const result = await freshUtils.getSecretKeyFromServiceWorker();
      expect(result).toBe('immediate-response');
    });
  });

  describe('resetSecret', () => {
    it('nulls secret and pub_key and calls clearSecretKeyFromServiceWorker', () => {
      // Provide a mock service worker controller to observe the clear message
      const postMessage = vi.fn();

      withServiceWorker({ postMessage });
      // Ensure calling resetSecret sets them to null state and triggers postMessage with ClearSecret
      utils.resetSecret();
      expect((utils as any).secret).toBeNull();
      expect((utils as any).pub_key).toBeNull();
      return Promise.resolve().then(() => {
        expect(postMessage).toHaveBeenCalledWith({ type: MessageType.ClearSecret, data: null });
      });
    });
  });

  describe('refresh_access_token', () => {
    beforeEach(() => {
      // Ensure pathname exists & default
      Object.defineProperty(window, 'location', {
        value: { ...(window.location || {}), pathname: '/', reload: vi.fn(), href: 'http://localhost:3000' },
        configurable: true,
      });
      // Mock navigator.locks
      (navigator as any).locks = {
        request: vi.fn((_name: string, cb: () => Promise<unknown>) => cb()),
      };
    });

    it('sets state to Recovery when on /recovery path', async () => {
      Object.assign(window.location, { pathname: '/recovery' });
      await utils.refresh_access_token();
      expect(utils.loggedIn.value).toBe(utils.AppState.Recovery);
    });

    it('logs user in on successful refresh when secrets available', async () => {
      Object.assign(window.location, { pathname: '/' });
      // Provide loginSalt & secret
      (window.localStorage.getItem as any).mockImplementation((key: string) => key === 'loginSalt' ? 'salty' : null);
      // Mock service worker so getSecretKeyFromServiceWorker resolves with value
      const postMessage = vi.fn((msg: any) => {
        if (msg.type === MessageType.RequestSecret) {
          triggerSWMessage({ type: MessageType.RequestSecret, data: 'SGVjcmV0' });
        }
      });
      withServiceWorker({ postMessage } as any);
      // Mock refresh endpoint success
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          return { error: null, response: { status: 200 } };
        }
        return { error: 'unexpected', response: { status: 500 } };
      });

      await utils.refresh_access_token();
      expect(utils.loggedIn.value).toBe(utils.AppState.LoggedIn);
    });

    it('sets state to LoggedOut and clears loginSalt on refresh failure', async () => {
      Object.assign(window.location, { pathname: '/' });
      (window.localStorage.getItem as any).mockImplementation((key: string) => key === 'loginSalt' ? 'salty' : null);
      const removeItemSpy = window.localStorage.removeItem as any;
      const postMessage = vi.fn();
      withServiceWorker({ postMessage } as any);
      // Fail refresh
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          return { error: 'unauthorized', response: { status: 401 } };
        }
        return { error: 'unexpected', response: { status: 500 } };
      });

      await utils.refresh_access_token();
      expect(removeItemSpy).toHaveBeenCalledWith('loginSalt');
      expect(utils.loggedIn.value).toBe(utils.AppState.LoggedOut);
    });

    it('treats 502 error as success and keeps user logged in', async () => {
      (window.localStorage.getItem as any).mockImplementation((key: string) => key === 'loginSalt' ? 'salty' : null);
      const postMessage = vi.fn((msg: any) => {
        if (msg.type === MessageType.RequestSecret) {
          triggerSWMessage({ type: MessageType.RequestSecret, data: 'ANY' });
        }
      });
      withServiceWorker({ postMessage } as any);
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          return { error: 'bad_gateway', response: { status: 502 } };
        }
        return { error: null, response: { status: 200 } };
      });

      await utils.refresh_access_token();
      expect(utils.loggedIn.value).toBe(utils.AppState.LoggedIn);
    });

    it('catch block keeps user logged in if tokens present', async () => {
      (window.localStorage.getItem as any).mockImplementation((key: string) => key === 'loginSalt' ? 'salty' : null);
      vi.spyOn(utils, 'getSecretKeyFromServiceWorker').mockResolvedValue('secret');
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          throw new Error('network');
        }
        return { error: null, response: { status: 200 } };
      });

      await utils.refresh_access_token();
      expect(utils.loggedIn.value).toBe(utils.AppState.LoggedIn);
    });

    it('catch block resets secret if tokens missing', async () => {
      (window.localStorage.getItem as any).mockImplementation(() => null);
      const postMessage = vi.fn((event) => {
        if (event.type === MessageType.RequestSecret) {
          triggerSWMessage({ type: MessageType.RequestSecret, data: null });// No secret
        }
      });
      withServiceWorker({ postMessage });

      // Start from LoggedOut so we can ensure it does not become LoggedIn
      utils.loggedIn.value = utils.AppState.LoggedOut;
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          throw new Error('network');
        }
        return { error: null, response: { status: 200 } };
      });

      await utils.refresh_access_token();
      // Should not have transitioned to LoggedIn
      expect(utils.loggedIn.value).not.toBe(utils.AppState.LoggedIn);
    });

    it('catch block logs error and maintains logged in state when tokens exist', async () => {
      const consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => { /* no-op */ });
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => { /* no-op */ });

      // Mock localStorage to return loginSalt
      (window.localStorage.getItem as any).mockImplementation((key: string) =>
        key === 'loginSalt' ? 'test-salt' : null
      );

      // Mock service worker to return secret
      const postMessage = vi.fn((msg: any) => {
        if (msg.type === MessageType.RequestSecret) {
          triggerSWMessage({ type: MessageType.RequestSecret, data: 'test-secret' });
        }
      });
      withServiceWorker({ postMessage } as any);

      // Mock fetchClient to throw an error
      const testError = new Error('Network timeout');
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          throw testError;
        }
        return { error: null, response: { status: 200 } };
      });

      // Set initial state
      utils.loggedIn.value = utils.AppState.Loading;

      await utils.refresh_access_token();

      // Verify logging
      expect(consoleLogSpy).toHaveBeenCalledWith('Failed to refresh access token:', testError);
      expect(consoleErrorSpy).toHaveBeenCalledWith(testError);

      // Verify state is set to LoggedIn when both tokens exist
      expect(utils.loggedIn.value).toBe(utils.AppState.LoggedIn);

      consoleLogSpy.mockRestore();
      consoleErrorSpy.mockRestore();
    });

    it('catch block calls logout when loginSalt is missing', async () => {
      const consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => { /* no-op */ });
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => { /* no-op */ });

      // Mock localStorage to return no loginSalt
      (window.localStorage.getItem as any).mockImplementation(() => null);

      // Mock service worker to return secret (but loginSalt is missing)
      const postMessage = vi.fn((msg: any) => {
        if (msg.type === MessageType.RequestSecret) {
          triggerSWMessage({ type: MessageType.RequestSecret, data: 'test-secret' });
        }
      });
      withServiceWorker({ postMessage } as any);

      // Mock fetchClient to throw an error
      const testError = new Error('Network timeout');
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          throw testError;
        }
        return { error: null, response: { status: 200 } };
      });

      // Mock logout function
      const logoutModule = await import('../components/Navbar');
      const logoutSpy = vi.spyOn(logoutModule, 'logout').mockImplementation(async () => { /* no-op */ });

      await utils.refresh_access_token();

      // Verify logging
      expect(consoleLogSpy).toHaveBeenCalledWith('Failed to refresh access token:', testError);
      expect(consoleErrorSpy).toHaveBeenCalledWith(testError);

      // Verify logout was called
      expect(logoutSpy).toHaveBeenCalledWith(false);

      consoleLogSpy.mockRestore();
      consoleErrorSpy.mockRestore();
      logoutSpy.mockRestore();
    });

    it('catch block calls logout when secret is missing (empty string)', async () => {
      const consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => { /* no-op */ });
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => { /* no-op */ });

      // Mock localStorage to return loginSalt
      (window.localStorage.getItem as any).mockImplementation((key: string) =>
        key === 'loginSalt' ? 'test-salt' : null
      );

      // Mock service worker to return empty secret
      const postMessage = vi.fn((msg: any) => {
        if (msg.type === MessageType.RequestSecret) {
          triggerSWMessage({ type: MessageType.RequestSecret, data: '' });
        }
      });
      withServiceWorker({ postMessage } as any);

      // Mock fetchClient to throw an error
      const testError = new Error('Network timeout');
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          throw testError;
        }
        return { error: null, response: { status: 200 } };
      });

      // Mock logout function
      const logoutModule = await import('../components/Navbar');
      const logoutSpy = vi.spyOn(logoutModule, 'logout').mockImplementation(async () => { /* no-op */ });

      await utils.refresh_access_token();

      // Verify logging
      expect(consoleLogSpy).toHaveBeenCalledWith('Failed to refresh access token:', testError);
      expect(consoleErrorSpy).toHaveBeenCalledWith(testError);

      // Verify logout was called
      expect(logoutSpy).toHaveBeenCalledWith(false);

      consoleLogSpy.mockRestore();
      consoleErrorSpy.mockRestore();
      logoutSpy.mockRestore();
    });

    it('catch block calls logout when secret is null', async () => {
      const consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => { /* no-op */ });
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => { /* no-op */ });

      // Mock localStorage to return loginSalt
      (window.localStorage.getItem as any).mockImplementation((key: string) =>
        key === 'loginSalt' ? 'test-salt' : null
      );

      // Mock service worker to return null secret
      const postMessage = vi.fn((msg: any) => {
        if (msg.type === MessageType.RequestSecret) {
          triggerSWMessage({ type: MessageType.RequestSecret, data: null });
        }
      });
      withServiceWorker({ postMessage } as any);

      // Mock fetchClient to throw an error
      const testError = new Error('Network timeout');
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          throw testError;
        }
        return { error: null, response: { status: 200 } };
      });

      // Mock logout function
      const logoutModule = await import('../components/Navbar');
      const logoutSpy = vi.spyOn(logoutModule, 'logout').mockImplementation(async () => { /* no-op */ });

      await utils.refresh_access_token();

      // Verify logging
      expect(consoleLogSpy).toHaveBeenCalledWith('Failed to refresh access token:', testError);
      expect(consoleErrorSpy).toHaveBeenCalledWith(testError);

      // Verify logout was called
      expect(logoutSpy).toHaveBeenCalledWith(false);

      consoleLogSpy.mockRestore();
      consoleErrorSpy.mockRestore();
      logoutSpy.mockRestore();
    });

    it('catch block calls logout when getSecretKeyFromServiceWorker times out', async () => {
      // Use fake timers to test timeout scenario
      vi.useFakeTimers();

      const consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => { /* no-op */ });
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => { /* no-op */ });

      // Mock localStorage to return loginSalt
      (window.localStorage.getItem as any).mockImplementation((key: string) =>
        key === 'loginSalt' ? 'test-salt' : null
      );

      // Mock service worker that never responds
      const postMessage = vi.fn(); // No response triggered
      withServiceWorker({ postMessage } as any);

      // Mock fetchClient to throw an error (triggers outer catch block)
      const testError = new Error('Network timeout');
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/jwt_refresh') {
          throw testError;
        }
        return { error: null, response: { status: 200 } };
      });

      // Mock logout function
      const logoutModule = await import('../components/Navbar');
      const logoutSpy = vi.spyOn(logoutModule, 'logout').mockImplementation(async () => { /* no-op */ });

      // Start refresh_access_token without awaiting
      const refreshPromise = utils.refresh_access_token();

      // Fast-forward time by 5000ms to trigger timeout in getSecretKeyFromServiceWorker
      await vi.advanceTimersByTimeAsync(5000);

      // Now await the refresh to complete
      await refreshPromise;

      // Verify logout was called when getSecretKeyFromServiceWorker timed out
      expect(logoutSpy).toHaveBeenCalledWith(false);

      consoleLogSpy.mockRestore();
      consoleErrorSpy.mockRestore();
      logoutSpy.mockRestore();

      vi.useRealTimers();
    });
  });

  describe('get_salt & get_salt_for_user', () => {
    it('retrieves a new salt successfully', async () => {
      const sample = [1,2,3];
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/generate_salt') {
          return { data: sample, response: { status: 200, text: () => Promise.resolve('ok') } };
        }
        return { data: null, response: { status: 404, text: () => Promise.resolve('nf') } };
      });
      const salt = await utils.get_salt();
      expect(Array.from(salt)).toEqual(sample);
    });

    it('throws on failed salt retrieval', async () => {
      (utils.fetchClient as any).GET = vi.fn(async (_: string) => ({ data: null, response: { status: 500, text: () => Promise.resolve('err') } }));
      await expect(utils.get_salt()).rejects.toMatch(/Failed to get new salt/);
    });

    it('retrieves login salt for user', async () => {
      const sample = [9,8];
      (utils.fetchClient as any).GET = vi.fn(async (path: string) => {
        if (path === '/auth/get_login_salt') {
          return { data: sample, error: null };
        }
        return { data: null, error: 'nf' };
      });
      const salt = await utils.get_salt_for_user('user@example.com');
      expect(Array.from(salt)).toEqual(sample);
    });

    it('throws if login salt fetch fails', async () => {
      (utils.fetchClient as any).GET = vi.fn(async () => ({ data: null, error: 'boom' }));
      await expect(utils.get_salt_for_user('x@y.z')).rejects.toMatch(/Failed to get login_salt/);
    });
  });

  describe('generate_hash', () => {
    it('uses argon2-browser hash and returns its hash field', async () => {
      const result = await utils.generate_hash('pw', 'salt', 16);
      expect(result).toBe('MOCK_HASH');
    });
  });

  describe('get_decrypted_secret', () => {
    it('returns early with alert when backend error (smoke path)', async () => {
      (utils.fetchClient as any).GET = vi.fn(async () => ({ data: null, error: 'err', response: { status: 500 } }));
      // We just ensure it does not throw
      await utils.get_decrypted_secret();
    });
  });

  describe('BroadcastChannel + appReload listener', () => {
    it('handles logout message by setting LoggedOut state', () => {
      utils.loggedIn.value = utils.AppState.LoggedIn;
      const handler = (utils.bc as any).onmessage;
      if (handler) {
        handler({ data: 'logout' });
      }
      expect(utils.loggedIn.value).toBe(utils.AppState.LoggedOut);
    });

    it('triggers reload on login message', () => {
  const reloadSpy = vi.spyOn(window.location, 'reload').mockImplementation(() => { /* no-op */ });
  const handler = (utils.bc as any).onmessage;
  if (handler) {
    handler({ data: 'login' });
  }
      expect(reloadSpy).toHaveBeenCalled();
    });

    it('reloads page on appReload event when threshold exceeded', () => {
  const reloadSpy = vi.spyOn(window.location, 'reload').mockImplementation(() => { /* no-op */ });
      const originalNow = Date.now;
      // Make Date.now large so (now - lastAlive) >= threshold
  // We don't know lastAlive initial value, but adding large offset suffices.
  Date.now = () => originalNow() + 1000 * 60 * 3;
      window.dispatchEvent(new Event('appReload'));
      expect(reloadSpy).toHaveBeenCalled();
      Date.now = originalNow;
    });
  });
});
