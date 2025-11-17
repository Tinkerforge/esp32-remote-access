import { describe, it, beforeEach, vi, expect, afterEach } from 'vitest';
import { forceCheckForUpdates, startVersionChecking, stopVersionChecking } from './versionChecker';

// Helper to mock fetch responses sequence
function mockFetchSequence(responses: Array<Response | Promise<Response>>) {
  let call = 0;
  globalThis.fetch = vi.fn(async () => {
    const r = responses[call++];
    return await r;
  }) as unknown as typeof fetch;
}

function jsonResponse(obj: unknown, ok = true, headers: Record<string, string> = {}) {
  return new Response(JSON.stringify(obj), {
    status: ok ? 200 : 500,
    headers: { 'Content-Type': 'application/json', ...headers },
  });
}

function okResponse(body = '', headers: Record<string, string> = {}) {
  return new Response(body, { status: 200, headers });
}

describe('versionChecker', () => {
  const originalLocation = window.location;
  const originalConfirm = window.confirm;
  const originalAlert = window.alert;
  const originalNow = Date.now;

  beforeEach(() => {
    Object.defineProperty(window, 'location', {
      value: { ...originalLocation, reload: vi.fn(), href: 'http://localhost/' },
      writable: true,
      configurable: true,
    });
    window.confirm = vi.fn();
    window.alert = vi.fn();
  });

  afterEach(() => {
    window.confirm = originalConfirm;
    window.alert = originalAlert;
    // @ts-expect-error restore
    window.location = originalLocation;
    Date.now = originalNow;
    vi.restoreAllMocks();
    stopVersionChecking();
  });

  it('forceCheckForUpdates reloads when a new version is detected and user confirms', async () => {
    // First call initializes current version via startVersionChecking
    mockFetchSequence([
      jsonResponse({ buildHash: 'v1' }), // init
      jsonResponse({ buildHash: 'v2' }), // check
    ]);

    startVersionChecking(60); // long interval; we won't wait for it

    await Promise.resolve();

  (window.confirm as unknown as ReturnType<typeof vi.fn>)
      .mockReturnValueOnce(true);

    await forceCheckForUpdates();

    expect(window.confirm).toHaveBeenCalled();
    expect(window.location.reload).toHaveBeenCalled();
  });

  it('forceCheckForUpdates shows already_latest when no update', async () => {
    mockFetchSequence([
      jsonResponse({ buildHash: 'v1' }), // init
      jsonResponse({ buildHash: 'v1' }), // check
    ]);

    startVersionChecking(60);
    await Promise.resolve();

    await forceCheckForUpdates();

    expect(window.alert).toHaveBeenCalled();
  });

  it('startVersionChecking stops interval when user declines reload', async () => {
    mockFetchSequence([
      jsonResponse({ buildHash: 'v1' }), // init
      jsonResponse({ buildHash: 'v2' }), // first interval check
    ]);

  (window.confirm as unknown as ReturnType<typeof vi.fn>).mockReturnValueOnce(false);

    startVersionChecking(0.001); // fast interval (~60ms) to trigger quickly

    // Wait enough time for one interval tick
    await new Promise((r) => setTimeout(r, 100));

    // If stopVersionChecking() was called inside, subsequent checks won't occur. We can’t easily assert internal interval
    // state, but at least ensure confirm was called once and no reload was triggered.
    expect(window.confirm).toHaveBeenCalledTimes(1);
    expect(window.location.reload).not.toHaveBeenCalled();
  });

  it('falls back to index last-modified header when /version.json is not ok', async () => {
    const lastModified = 'Mon, 01 Jan 2024 00:00:00 GMT';
    mockFetchSequence([
      jsonResponse({}, false),
      okResponse('', { 'last-modified': lastModified }),
      jsonResponse({}, false),
      okResponse('', { 'last-modified': lastModified }),
    ]);

    startVersionChecking(60);
    await Promise.resolve();

    await forceCheckForUpdates();

    expect(window.alert).toHaveBeenCalled(); // already latest based on last-modified
    expect(window.location.reload).not.toHaveBeenCalled();
  });

  it('falls back to Date.now() when last-modified header is missing', async () => {
    Date.now = vi.fn(() => 1234567890);
    mockFetchSequence([
      // init path -> /version.json fails, index ok without header
      jsonResponse({}, false),
      okResponse(''),
      // force check path -> same again
      jsonResponse({}, false),
      okResponse(''),
    ]);

    startVersionChecking(60);
    await Promise.resolve();

    await forceCheckForUpdates();

    expect(window.alert).toHaveBeenCalled();
    expect(window.location.reload).not.toHaveBeenCalled();
  // ensure Date.now was used (may be called multiple times across init + check)
  expect(Date.now).toHaveBeenCalled();
  });

  it('handles fetch errors gracefully and treats as no update', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {
      // swallow logs during test
    });
    globalThis.fetch = vi.fn().mockRejectedValue(new Error('network down')) as unknown as typeof fetch;

    startVersionChecking(60);
    await Promise.resolve();

    await forceCheckForUpdates();

    expect(warnSpy).toHaveBeenCalled();
    expect(window.alert).toHaveBeenCalled();
  });

  it('clears previous interval when startVersionChecking is called again', async () => {
    const clearSpy = vi.spyOn(globalThis, 'clearInterval');
    mockFetchSequence([
      jsonResponse({ buildHash: 'v1' }), // first start init
      jsonResponse({ buildHash: 'v1' }), // second start init
    ]);

    startVersionChecking(60);
    await Promise.resolve();
    startVersionChecking(60);
    await Promise.resolve();

    expect(clearSpy).toHaveBeenCalledTimes(1);
  });

  it('startVersionChecking reloads when user confirms on new version', async () => {
    mockFetchSequence([
      jsonResponse({ buildHash: 'v1' }), // init
      jsonResponse({ buildHash: 'v2' }), // interval check
    ]);

  (window.confirm as unknown as ReturnType<typeof vi.fn>).mockReturnValueOnce(true);

    startVersionChecking(0.001);

    await new Promise((r) => setTimeout(r, 100));

    expect(window.location.reload).toHaveBeenCalled();
  });

  it('stopVersionChecking clears active interval and is idempotent', async () => {
    const clearSpy = vi.spyOn(globalThis, 'clearInterval');
    mockFetchSequence([
      jsonResponse({ buildHash: 'v1' }),
    ]);

    startVersionChecking(60);
    await Promise.resolve();

    stopVersionChecking();
    expect(clearSpy).toHaveBeenCalledTimes(1);

    // second call should not throw and not call clearInterval again
    stopVersionChecking();
    expect(clearSpy).toHaveBeenCalledTimes(1);
  });
});
