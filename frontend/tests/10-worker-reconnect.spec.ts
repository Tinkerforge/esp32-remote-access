import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { createServer, type ServerResponse } from 'node:http';
import type { Duplex } from 'node:stream';
import { test, expect } from '@playwright/test';
import sodium from 'libsodium-wrappers';

type ProtocolMessage = {
  type?: number;
  id?: string;
  data?: { status?: number };
  unresolved?: boolean;
  message?: string;
};
type WorkerMessage = ProtocolMessage | string;
type TestWindow = typeof window & {
  reconnectTest: { worker: Worker; messages: WorkerMessage[] };
};
const isProtocolMessage = (message: WorkerMessage): message is ProtocolMessage => typeof message !== 'string';

test('worker reconnect tolerates iframe traffic while replacement keys load', async ({ page }) => {
  // Exercise the built worker and real Wasm against a local relay fixture.
  // npm run build supplies the assets; no device or database is needed.
  await sodium.ready;
  const dist = new URL('../dist/', import.meta.url);
  const workerFile = (await readdir(new URL('assets/', dist))).find(name => /^worker-.*\.js$/.test(name));
  assert.ok(workerFile, 'Build the frontend first');
  const secret = sodium.randombytes_buf(32);
  const publicKey = sodium.crypto_scalarmult_base(secret);
  const seal = (text: string) => Array.from(sodium.crypto_box_seal(new TextEncoder().encode(text), publicKey));
  const keys = {
    id: 'reconnect-key', ipv6_supported: false,
    charger_pub: sodium.to_base64(sodium.crypto_scalarmult_base(new Uint8Array(32).fill(2)), sodium.base64_variants.ORIGINAL),
    web_private: seal(sodium.to_base64(new Uint8Array(32).fill(1), sodium.base64_variants.ORIGINAL)),
    psk: seal(sodium.to_base64(new Uint8Array(32).fill(3), sodium.base64_variants.ORIGINAL)),
    web_address: '10.123.1.3/24', charger_address: '10.123.1.2/32',
  };
  const sendKeys = (response: ServerResponse) => {
    response.setHeader('Content-Type', 'application/json');
    response.end(JSON.stringify(keys));
  };
  let keyRequests = 0;
  let heldKeys: ServerResponse | undefined;
  const sockets = new Set<Duplex>();
  const tunnels: Duplex[] = [];
  const server = createServer(async (req, res) => {
    try {
      const path = new URL(req.url ?? '/', 'http://localhost').pathname;
      if (path === '/api/charger/get_key') {
        if (++keyRequests === 2) heldKeys = res;
        else sendKeys(res);
      } else if (path.startsWith('/assets/') && !path.includes('..')) {
        res.setHeader('Content-Type', path.endsWith('.wasm') ? 'application/wasm' : 'text/javascript');
        res.end(await readFile(new URL(path.slice(1), dist)));
      } else {
        res.setHeader('Content-Type', 'text/html');
        res.end('<!doctype html><title>Worker reconnect test</title>');
      }
    } catch (error) { res.statusCode = 500; res.end(String(error)); }
  });
  server.on('connection', socket => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
  });
  server.on('upgrade', (req, socket) => {
    const accept = createHash('sha1').update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    // Keep the WG handshake pending so commands must wait for readiness.
    socket.on('data', () => {});
    socket.on('error', () => {});
    tunnels.push(socket);
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  assert.ok(address && typeof address !== 'string');

  try {
    await page.goto(`http://127.0.0.1:${address.port}/`);
    await page.evaluate(({ workerFile, secret }) => {
      const state = {
        messages: [] as WorkerMessage[],
        worker: new Worker(`/assets/${workerFile}`, { type: 'module' }),
      };
      (window as TestWindow).reconnectTest = state;
      state.worker.onerror = event => state.messages.push({ unresolved: true, message: event.message });
      state.worker.onmessage = ({ data }) => {
        state.messages.push(data);
        if (data === 'started') state.worker.postMessage({ type: 4, data: {
          chargerID: 'charger', port: 80, mtu: 1420, secret: new Uint8Array(secret), debugMode: true,
        } });
      };
    }, { workerFile, secret: Array.from(secret) });
    await expect.poll(() => tunnels.length, { message: 'initial tunnel' }).toBe(1);
    tunnels[0].destroy();
    await expect.poll(() => !!heldKeys, { message: 'replacement key loading' }).toBe(true);
    await page.evaluate(() => {
      const { worker } = (window as TestWindow).reconnectTest;
      for (const command of ['connect', 'pauseWS', 'connect', 'download']) worker.postMessage(command);
      worker.postMessage({ type: 2, id: 'gap', data: { method: 'GET', headers: [], url: `${location.origin}/status` } });
    });
    await page.waitForFunction(() => (window as TestWindow).reconnectTest.messages.some(message =>
      typeof message !== 'string' && (message.id === 'gap' || message.type === 5 || message.unresolved)));
    let messages = (await page.evaluate(() => (window as TestWindow).reconnectTest.messages)).filter(isProtocolMessage);
    expect(messages.filter(message => message.type === 5 || message.unresolved), 'iframe traffic caused a fatal reconnect error').toEqual([]);
    expect(messages.find(message => message.id === 'gap')?.data?.status).toBe(503);
    assert.ok(heldKeys);
    sendKeys(heldKeys);
    await expect.poll(() => tunnels.length, { message: 'replacement tunnel' }).toBe(2);
    await page.evaluate(() => {
      const { worker } = (window as TestWindow).reconnectTest;
      for (const command of ['connect', 'pauseWS', 'connect']) worker.postMessage(command);
      worker.postMessage({ type: 2, id: 'pending', data: { method: 'GET', headers: [], url: `${location.origin}/status` } });
    });
    await page.waitForTimeout(50);
    await page.evaluate(() => (window as TestWindow).reconnectTest.worker.postMessage('close'));
    await page.waitForFunction(() => {
      const { messages } = (window as TestWindow).reconnectTest;
      return messages.includes('closed') && messages.some(message => typeof message !== 'string' && message.id === 'pending');
    });
    messages = (await page.evaluate(() => (window as TestWindow).reconnectTest.messages)).filter(isProtocolMessage);
    expect(messages.filter(message => message.type === 5 || message.unresolved), 'commands reached an unready or disposed client').toEqual([]);
    expect(messages.find(message => message.id === 'pending')?.data?.status).toBe(503);
    expect(keyRequests).toBe(2);
  } finally {
    await page.evaluate(() => (window as TestWindow).reconnectTest?.worker.terminate()).catch(() => {});
    for (const socket of sockets) socket.destroy();
    await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
});
