import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';

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
function withServiceWorker(controller: Partial<ServiceWorker> & { postMessage: (msg: unknown) => void }) {
  Object.defineProperty(navigator, 'serviceWorker', {
    value: {
      controller,
      addEventListener: vi.fn((_, cb) => {
        // store listener for manual triggering
        (withServiceWorker as any)._listener = cb;
      }),
      removeEventListener: vi.fn(),
    },
    configurable: true,
  });
  return controller;
}

function triggerSWMessage(msg: unknown) {
  const listener = (withServiceWorker as any)._listener as ((event: MessageEvent) => void) | undefined;
  if (listener) {
    listener({ data: msg } as MessageEvent);
  }
}

describe('utils', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
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
      const postMessage = vi.fn();
      withServiceWorker({ postMessage } as any);
      await utils.storeSecretKeyInServiceWorker('abc');
      expect(postMessage).toHaveBeenCalledWith({ type: MessageType.StoreSecret, data: 'abc' });
    });

    it('does nothing when no service worker controller (store)', async () => {
      Object.defineProperty(navigator, 'serviceWorker', { value: { controller: null }, configurable: true });
      await utils.storeSecretKeyInServiceWorker('abc');
      // no error thrown
    });

    it('requests secret and resolves with response (getSecretKeyFromServiceWorker)', async () => {
      const postMessage = vi.fn();
      withServiceWorker({ postMessage } as any);

      const promise = utils.getSecretKeyFromServiceWorker();
      // simulate worker answering
      triggerSWMessage({ type: MessageType.StoreSecret, data: 'encoded' });
      const key = await promise;
      expect(postMessage).toHaveBeenCalledWith({ type: MessageType.RequestSecret, data: null });
      expect(key).toBe('encoded');
    });

    it('returns null when timeout reached without controller response', async () => {
      vi.useFakeTimers();
      const postMessage = vi.fn();
      withServiceWorker({ postMessage } as any);
      const p = utils.getSecretKeyFromServiceWorker();
      vi.advanceTimersByTime(5001);
      await expect(p).resolves.toBeNull();
      vi.useRealTimers();
    });

    it('clears secret key via service worker', async () => {
      const postMessage = vi.fn();
      withServiceWorker({ postMessage } as any);
      await utils.clearSecretKeyFromServiceWorker();
      expect(postMessage).toHaveBeenCalledWith({ type: MessageType.ClearSecret, data: null });
    });
  });

  describe('resetSecret', () => {
    it('nulls secret and pub_key and calls clearSecretKeyFromServiceWorker', () => {
      // Provide a mock service worker controller to observe the clear message
      const postMessage = vi.fn();
      Object.defineProperty(navigator, 'serviceWorker', {
        value: {
          controller: { postMessage },
        },
        configurable: true,
      });

      // Ensure calling resetSecret sets them to null state and triggers postMessage with ClearSecret
      utils.resetSecret();
      expect((utils as any).secret).toBeNull();
      expect((utils as any).pub_key).toBeNull();
      return Promise.resolve().then(() => {
        expect(postMessage).toHaveBeenCalledWith({ type: MessageType.ClearSecret, data: null });
      });
    });
  });
});
