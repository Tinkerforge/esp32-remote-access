export {};

type Client = typeof import('wg-webclient').Client;

const scope = self as unknown as DedicatedWorkerGlobalScope;
const parameters = new URL(scope.location.href).searchParams;
const variant = parameters.get('variant');
const idleMs = Number(parameters.get('idleMs'));
const peerPort = parameters.get('peerPort');
const peerKey = parameters.get('peerKey')!;
const nativeTimeout = scope.setTimeout.bind(scope);
const nativeInterval = scope.setInterval.bind(scope);
let intervalCallbacks = 0;
let timeoutCallbacks = 0;

scope.setInterval = (handler, delay, ...args) =>
    nativeInterval(typeof handler === 'function' ? () => {
        intervalCallbacks++;
        Reflect.apply(handler, scope, args);
    } : handler, delay);
scope.setTimeout = (handler, delay, ...args) =>
    nativeTimeout(typeof handler === 'function' ? () => {
        timeoutCallbacks++;
        Reflect.apply(handler, scope, args);
    } : handler, delay);

const sleep = (ms: number) => new Promise<void>(resolve => nativeTimeout(resolve, ms));

async function measure() {
    const { default: init, Client } = await import(`/${variant}/wg_webclient.js`) as {
        default: () => Promise<unknown>;
        Client: Client;
    };
    await init();
    let connected!: () => void;
    const ready = new Promise<void>(resolve => { connected = resolve; });
    const client = new Client(
        btoa(String.fromCharCode(1).repeat(32)), peerKey, btoa(String.fromCharCode(3).repeat(32)),
        `ws://127.0.0.1:${peerPort}/v4`, '10.123.1.3/24', '10.123.1.2/32', 80, 1420,
        () => {}, connected, false,
    );
    try {
        await ready;
        await new Promise<void>(resolve => {
            client.start_inner_ws((message: string) => {
                if (message === 'peer-ws') resolve();
            });
        });
        const request = async () => {
            const start = performance.now();
            const response = await client.fetch(new Request('/status'), '/status');
            if (response.status !== 200 || await response.text() !== 'peer-v4') {
                throw new Error(`Unexpected tunnel response (${response.status})`);
            }
            return performance.now() - start;
        };

        await request();
        await sleep(250);
        intervalCallbacks = 0;
        timeoutCallbacks = 0;
        const idleStart = performance.now();
        await sleep(idleMs);
        const idle = { ms: performance.now() - idleStart, intervalCallbacks, timeoutCallbacks };

        // The peer has eight TCP sockets. One is used by the WebSocket and one by warm-up.
        const requests = [];
        for (let i = 0; i < 6; i++) {
            requests.push(await request());
        }
        return { idle, requests };
    } finally {
        client.disconnect();
        client.free();
    }
}

void measure().then(
    result => scope.postMessage({ result }),
    error => scope.postMessage({ error: String(error) }),
);
