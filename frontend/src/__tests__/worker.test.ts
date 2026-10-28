import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MessageType, SetupMessage } from "../types";

const mocks = vi.hoisted(() => ({
    clients: [] as Array<{
        ready: boolean; failed: boolean; start_inner_ws: ReturnType<typeof vi.fn>;
        disconnect_inner_ws: ReturnType<typeof vi.fn>; disconnect: ReturnType<typeof vi.fn>;
        free: ReturnType<typeof vi.fn>; fetch: ReturnType<typeof vi.fn>;
        get_pcap_log: ReturnType<typeof vi.fn>; args: unknown[];
    }>,
    construct: vi.fn(),
    autoReady: true,
}));
vi.mock("wg-webclient", () => ({
    set_pcap_logging: vi.fn(),
    Client: class {
        ready = true;
        failed = false;
        start_inner_ws = vi.fn();
        disconnect_inner_ws = vi.fn();
        disconnect = vi.fn();
        free = vi.fn();
        fetch = vi.fn();
        get_pcap_log = vi.fn(() => new Uint8Array());
        args: unknown[];
        constructor(...args: unknown[]) {
            this.args = args;
            mocks.construct(...args);
            mocks.clients.push(this);
            void Promise.resolve().then(() => {
                if (mocks.autoReady) (args[9] as () => void)();
            });
        }
        peer_discovery_complete() { return this.ready; }
        peer_discovery_failed() { return this.failed; }
    },
}));
vi.mock("libsodium-wrappers", () => ({ default: {
    ready: Promise.resolve(), crypto_scalarmult_base: vi.fn(() => new Uint8Array(32)),
    crypto_box_seal_open: vi.fn(() => new TextEncoder().encode("sealed-key")),
} }));

type MessageHandler = (event: { data: unknown }) => Promise<void>;
let handlers: Map<string, MessageHandler>;
let postMessage: ReturnType<typeof vi.fn>;
const keys = { id: "key-id", charger_pub: "pub", web_private: [], psk: [],
    web_address: "10.123.1.3/32", charger_address: "10.123.1.2/32", ipv6_supported: true };
const setup: SetupMessage = { chargerID: "charger", port: 80, mtu: 1420, secret: new Uint8Array(32), debugMode: false };
const message = async (data: unknown) => handlers.get("message")!({ data });
const configure = () => message({ type: MessageType.Setup, data: setup });

beforeEach(async () => {
    vi.resetModules(); vi.useFakeTimers(); mocks.clients.length = 0; mocks.construct.mockClear(); mocks.autoReady = true;
    handlers = new Map(); postMessage = vi.fn();
    vi.stubGlobal("self", { addEventListener: (type: string, cb: MessageHandler) => handlers.set(type, cb),
        postMessage, location: { origin: "https://[::1]" } });
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(keys), { status: 200 })));
    await import("../worker");
});
afterEach(async () => {
    await message("close"); vi.useRealTimers(); vi.unstubAllGlobals();
});

describe("worker tunnel lifecycle", () => {
    it.each([false, undefined])("connects directly over IPv4 when IPv6 support is %s", async (supported) => {
        vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ ...keys, ipv6_supported: supported })));
        await configure();
        expect(mocks.clients[0].args.slice(4, 6)).toEqual([keys.web_address, keys.charger_address]);
        expect(mocks.clients[0].args[10]).toBe(false);
        await message("connect");
        expect(mocks.clients[0].start_inner_ws).toHaveBeenCalledOnce();
    });

    it("uses a discovery token's IPv4 fallback directly after a firmware downgrade", async () => {
        const fallback = { web_address: "10.123.1.3/24", charger_address: "10.123.1.2/32" };
        vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ ...keys,
            web_address: "fd00::3/128", charger_address: "ff02::1/128",
            ipv6_supported: false, ipv4_fallback: fallback,
        })));
        await configure();
        expect(mocks.clients[0].args.slice(4, 6)).toEqual([fallback.web_address, fallback.charger_address]);
        expect(mocks.clients[0].args[10]).toBe(false);
    });

    it.each(["ff02::1/128", "fd00::2/128"])(
        "does not attempt an IPv6 tunnel to %s when firmware support is unknown", async (peer) => {
            vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ ...keys,
                web_address: "fd00::3/128", charger_address: peer, ipv6_supported: undefined,
            })));
            await configure();
            expect(mocks.clients).toHaveLength(0);
            expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({ type: MessageType.Error }));
        },
    );

    it.each([
        ["10.123.1.3/32", "10.123.1.2/32"],
        ["fd00::3/64", "fd00::2/128"],
        ["fd00::3/128", "fe80::1"],
        ["fd00::3/128", "fe80::1/128"],
        ["fd00::3/128", "FE80:0:0:0:0:0:0:1/128"],
    ])("passes registered addresses through unchanged: %s", async (local, peer) => {
        vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ ...keys, web_address: local, charger_address: peer })));
        await configure();
        expect(mocks.construct.mock.calls[0].slice(3, 6)).toEqual(["/api/ws?key_id=key-id", local, peer]);
        expect(mocks.construct.mock.calls[0][10]).toBe(!peer.includes(":"));
        expect(mocks.construct.mock.calls[0].slice(11)).toEqual([undefined, undefined]);
        await message("connect");
        expect(mocks.clients[0].start_inner_ws).toHaveBeenCalledOnce();
    });

    it.each(["ff02::1", "ff02::1/128", "FF02:0:0:0:0:0:0:1/128"])(
        "uses IPv6 discovery and a fresh session address for %s", async (peer) => {
            vi.mocked(fetch).mockImplementation(async () => new Response(JSON.stringify({
                ...keys, web_address: "fd00::3/128", charger_address: peer,
            })));
            await configure();
            const first = mocks.clients[0];
            expect(first.args[4]).toMatch(/^fd[0-9a-f]{2}(?::[0-9a-f]{1,4}){7}\/128$/);
            expect(first.args[4]).not.toBe("fd00::3/128");
            expect(first.args[5]).toBe(peer);
            expect(first.args[10]).toBe(true);
            first.ready = false;
            await message("connect");
            expect(first.start_inner_ws).not.toHaveBeenCalled();
            first.ready = true;
            await vi.advanceTimersByTimeAsync(200);
            expect(first.start_inner_ws).toHaveBeenCalledOnce();
            (first.args[8] as () => void)();
            await vi.advanceTimersByTimeAsync(1000);
            expect(mocks.clients).toHaveLength(2);
            expect(mocks.clients[1].args[4]).not.toBe(first.args[4]);
            expect(mocks.clients[1].args[10]).toBe(true);
        },
    );

    it("passes an advertised IPv4 fallback into token discovery", async () => {
        const fallback = { web_address: "10.123.1.3/24", charger_address: "10.123.1.2/32" };
        vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ ...keys,
            web_address: "fd00::3/128", charger_address: "ff02::1/128", ipv4_fallback: fallback,
        })));
        await configure();
        expect(mocks.clients[0].args.slice(10)).toEqual([true, fallback.web_address, fallback.charger_address]);
    });

    it("stops discovery readiness retries with a visible timeout", async () => {
        await configure(); const client = mocks.clients[0]; client.ready = false;
        await message("connect"); await vi.advanceTimersByTimeAsync(12_000);
        expect(client.start_inner_ws).not.toHaveBeenCalled();
        expect(client.disconnect).toHaveBeenCalledOnce();
        expect(client.free).toHaveBeenCalledOnce();
        expect(postMessage).toHaveBeenCalledWith({ type: MessageType.Error, data: {
            translation: "wgclient.connection_timeout", format: undefined,
        } });
        expect(vi.getTimerCount()).toBe(0);
    });

    it("reports a Rust discovery failure immediately", async () => {
        await configure(); mocks.clients[0].ready = false; mocks.clients[0].failed = true;
        await message("connect");
        expect(mocks.clients[0].disconnect).toHaveBeenCalledOnce();
        expect(vi.getTimerCount()).toBe(0);
    });

    it.each(["close", "pauseWS"])("cancels pending retries on %s", async (action) => {
        await configure(); const client = mocks.clients[0]; client.ready = false;
        await message("connect"); await message(action);
        client.ready = true; await vi.advanceTimersByTimeAsync(12_000);
        expect(client.start_inner_ws).not.toHaveBeenCalled();
        expect(vi.getTimerCount()).toBe(0);
    });

    it("does not deliver a previous client's pending callback to its replacement", async () => {
        await configure(); const previous = mocks.clients[0]; previous.ready = false;
        await message("connect"); await configure();
        expect(previous.disconnect).toHaveBeenCalledOnce();
        expect(previous.free).toHaveBeenCalledOnce();
        await vi.advanceTimersByTimeAsync(12_000);
        expect(mocks.clients[1].start_inner_ws).not.toHaveBeenCalled();
    });

    it("reconnects once and ignores callbacks from the replaced client", async () => {
        await configure(); const previous = mocks.clients[0]; previous.ready = false;
        await message("connect"); (previous.args[8] as () => void)();
        await vi.advanceTimersByTimeAsync(1000);
        expect(mocks.clients).toHaveLength(2);
        postMessage.mockClear();
        (previous.args[8] as () => void)(); (previous.args[9] as () => void)();
        await vi.advanceTimersByTimeAsync(12_000);
        expect(mocks.clients).toHaveLength(2);
        expect(postMessage).not.toHaveBeenCalledWith("ready");
        await message("connect"); expect(mocks.clients[1].start_inner_ws).toHaveBeenCalledOnce();
    });

    it("rebuilds discovery and resumes the web interface when a restarted device requests reconnection", async () => {
        const discoveryKeys = { ...keys, web_address: "fd00::3/128", charger_address: "ff02::1/128" };
        vi.mocked(fetch).mockImplementation(async () => new Response(JSON.stringify(discoveryKeys)));
        await configure();
        const previous = mocks.clients[0];
        (previous.args[9] as () => void)();
        await message("connect");
        expect(previous.start_inner_ws).toHaveBeenCalledOnce();
        postMessage.mockClear();
        (previous.args[8] as () => void)();
        await vi.advanceTimersByTimeAsync(1000);
        expect(previous.disconnect).toHaveBeenCalledOnce();
        expect(previous.free).toHaveBeenCalledOnce();
        expect(fetch).toHaveBeenCalledTimes(2);
        const resumed = mocks.clients[1];
        expect(resumed.args[4]).not.toBe(previous.args[4]);
        expect(resumed.args[5]).toBe("ff02::1/128");
        expect(resumed.args[10]).toBe(true);
        resumed.ready = false;
        (resumed.args[9] as () => void)();
        expect(postMessage).toHaveBeenCalledWith("ready");
        await message("connect");
        expect(resumed.start_inner_ws).not.toHaveBeenCalled();
        resumed.ready = true;
        await vi.advanceTimersByTimeAsync(200);
        expect(resumed.start_inner_ws).toHaveBeenCalledOnce();
    });

    it("ignores an in-flight setup response after close", async () => {
        let resolve!: (response: Response) => void;
        vi.mocked(fetch).mockReturnValueOnce(new Promise(r => { resolve = r; }));
        const pending = configure(); await Promise.resolve();
        await message("close"); resolve(new Response(JSON.stringify(keys)));
        await pending; expect(mocks.clients).toHaveLength(0);
    });

    it.each(["close", "reconnect"])("settles in-flight HTTP requests when the client is disposed on %s", async (action) => {
        await configure();
        const previous = mocks.clients[0];
        let reject!: (error: Error) => void;
        previous.fetch.mockReturnValueOnce(new Promise((_, rejectRequest) => { reject = rejectRequest; }));
        previous.disconnect.mockImplementation(() => reject(new Error("Tunnel disconnected")));
        const pending = message({ type: MessageType.Fetch, id: "pending-request", data: {
            method: "GET", headers: [], url: "https://[::1]/status",
        } });
        if (action === "close") {
            await message("close");
        } else {
            (previous.args[8] as () => void)();
            await vi.advanceTimersByTimeAsync(1000);
        }
        await pending;
        expect(previous.free).toHaveBeenCalledOnce();
        expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({
            type: MessageType.FetchResponse, id: "pending-request",
            data: expect.objectContaining({ status: 503, statusText: "Tunnel disconnected" }),
        }));
        expect(postMessage).not.toHaveBeenCalledWith(expect.objectContaining({ unresolved: true }));
    });

    it("keeps iframe traffic nonfatal while replacement keys are loading", async () => {
        await configure();
        let resolveKeys!: (response: Response) => void;
        vi.mocked(fetch).mockReturnValueOnce(new Promise(resolve => { resolveKeys = resolve; }));
        (mocks.clients[0].args[8] as () => void)();
        await vi.advanceTimersByTimeAsync(1000);
        postMessage.mockClear();

        await message("connect");
        await message("pauseWS");
        await message("connect");
        await message("download");
        await message({ type: MessageType.Fetch, id: "gap-request", data: {
            method: "GET", headers: [], url: "https://[::1]/status",
        } });
        expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({
            type: MessageType.FetchResponse, id: "gap-request",
            data: expect.objectContaining({ status: 503 }),
        }));
        expect(postMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: MessageType.Error }));

        mocks.autoReady = false;
        resolveKeys(new Response(JSON.stringify(keys)));
        await vi.advanceTimersByTimeAsync(0);
        const resumed = mocks.clients[1];
        expect(resumed.start_inner_ws).not.toHaveBeenCalled();
        expect(resumed.get_pcap_log).not.toHaveBeenCalled();
        (resumed.args[9] as () => void)();
        expect(resumed.start_inner_ws).toHaveBeenCalledOnce();
        expect(resumed.get_pcap_log).toHaveBeenCalledOnce();
        expect(postMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: MessageType.Error }));
    });

    it.each(["pauseWS", "close"])("cancels a connect requested during reconnection on %s", async (action) => {
        await configure();
        let resolveKeys!: (response: Response) => void;
        vi.mocked(fetch).mockReturnValueOnce(new Promise(resolve => { resolveKeys = resolve; }));
        (mocks.clients[0].args[8] as () => void)();
        await vi.advanceTimersByTimeAsync(1000);
        postMessage.mockClear();
        await message("connect");
        await message(action);
        resolveKeys(new Response(JSON.stringify(keys)));
        await vi.advanceTimersByTimeAsync(0);
        if (action === "close") {
            expect(mocks.clients).toHaveLength(1);
        } else {
            expect(mocks.clients).toHaveLength(2);
            expect(mocks.clients[1].start_inner_ws).not.toHaveBeenCalled();
        }
        expect(postMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: MessageType.Error }));
    });

    it("settles HTTP requests without a client instead of sending a fatal UI error", async () => {
        postMessage.mockClear();
        await message({ type: MessageType.Fetch, id: "unavailable-request", data: {
            method: "GET", headers: [], url: "https://[::1]/status",
        } });
        expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({
            type: MessageType.FetchResponse, id: "unavailable-request",
            data: expect.objectContaining({ status: 503 }),
        }));
        expect(postMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: MessageType.Error }));
    });

    it("bounds retries when loading keys fails", async () => {
        vi.mocked(fetch).mockRejectedValue(new Error("network unavailable"));
        await configure(); await vi.advanceTimersByTimeAsync(12_000);
        expect(vi.getTimerCount()).toBe(0);
        expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({ type: MessageType.Error }));
        expect(mocks.clients).toHaveLength(0);
    });
});
