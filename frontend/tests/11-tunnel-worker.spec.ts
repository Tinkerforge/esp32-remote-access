import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { createServer, type Server } from 'node:http';
import { connect, type Socket } from 'node:net';
import { test } from '@playwright/test';
import sodium from 'libsodium-wrappers';
import { startTunnelPeer } from './tunnel-peer';

// Playwright starts the encrypted peer automatically. Build the frontend before
// running the browser suite, as for the other built-worker regression tests.
const dist = new URL('../dist/', import.meta.url);
const profiles: Record<string, {
  web_address: string; charger_address: string; ipv6_supported?: boolean;
  ipv4_fallback?: { web_address: string; charger_address: string };
}> = {
  v4: { web_address: '10.123.1.3/24', charger_address: '10.123.1.2/32' },
  v6: { web_address: 'fd00::3/64', charger_address: 'fd00::2/128' },
  'v6-discovery': { web_address: 'fd00::3/128', charger_address: 'ff02::1/128' },
  auto: { web_address: '10.123.1.3/24', charger_address: '10.123.1.2/32' },
  'token-fallback': { web_address: 'fd00::3/128', charger_address: 'ff02::1/128',
    ipv4_fallback: { web_address: '10.123.1.3/24', charger_address: '10.123.1.2/32' } },
  legacy: { web_address: '10.123.1.3/24', charger_address: '10.123.1.2/32', ipv6_supported: false },
  'legacy-token': { web_address: 'fd00::3/128', charger_address: 'ff02::1/128', ipv6_supported: false,
    ipv4_fallback: { web_address: '10.123.1.3/24', charger_address: '10.123.1.2/32' } },
};

test.describe('built worker tunnels', () => {
  // Preserve the original sequence, including fresh discovery addresses and
  // firmware changes with the same stored addresses, key ID and key material.
  test.describe.configure({ mode: 'serial' });
  let server: Server | undefined;
  let peer: Awaited<ReturnType<typeof startTunnelPeer>> | undefined;
  const connections = new Set<Socket>();
  let workerFile: string;
  let secret: number[];
  let port: number;
  let autoPeer = 'v4';
  let previousDiscoverySource: string | undefined;

  test.beforeAll(async () => {
    test.setTimeout(180_000);
    peer = await startTunnelPeer();
    await sodium.ready;
    const filename = (await readdir(new URL('assets/', dist))).find(name => /^worker-.*\.js$/.test(name));
    assert.ok(filename, 'Build the frontend first');
    workerFile = filename;
    const secretBytes = sodium.randombytes_buf(32);
    secret = Array.from(secretBytes);
    const publicKey = sodium.crypto_scalarmult_base(secretBytes);
    const seal = (text: string) => Array.from(sodium.crypto_box_seal(new TextEncoder().encode(text), publicKey));
    const baseKeys = {
      ipv6_supported: true,
      charger_pub: sodium.to_base64(sodium.crypto_scalarmult_base(new Uint8Array(32).fill(2)), sodium.base64_variants.ORIGINAL),
      web_private: seal(sodium.to_base64(new Uint8Array(32).fill(1), sodium.base64_variants.ORIGINAL)),
      psk: seal(sodium.to_base64(new Uint8Array(32).fill(3), sodium.base64_variants.ORIGINAL)),
    };
    const relay = createServer(async (req, res) => {
      try {
        const url = new URL(req.url ?? '/', 'http://localhost');
        if (url.pathname === '/api/charger/get_key') {
          const id = url.searchParams.get('cid');
          assert.ok(id && profiles[id]);
          res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify({ ...baseKeys, ...profiles[id], id }));
        } else if (url.pathname.startsWith('/assets/') && !url.pathname.includes('..')) {
          res.setHeader('Content-Type', url.pathname.endsWith('.wasm') ? 'application/wasm' : 'text/javascript');
          res.end(await readFile(new URL(url.pathname.slice(1), dist)));
        } else {
          res.setHeader('Content-Type', 'text/html');
          res.end('<!doctype html><title>Tunnel worker test</title>');
        }
      } catch (error) { res.statusCode = 500; res.end(String(error)); }
    });
    server = relay;
    relay.on('connection', socket => {
      connections.add(socket);
      socket.on('close', () => connections.delete(socket));
    });
    relay.on('upgrade', (req, socket, head) => {
      const id = new URL(req.url ?? '/', 'http://localhost').searchParams.get('key_id');
      if (!id || !profiles[id]) { socket.destroy(); return; }
      const target = id.startsWith('legacy') ? 'v4' : id === 'auto' || id === 'token-fallback' ? autoPeer : id;
      const upstream = connect(peer!.port, '::1', () => {
        upstream.write(`GET /${target} HTTP/1.1\r\n` + req.rawHeaders.reduce((text, header, index) =>
          text + (index % 2 === 0 ? `${header}: ` : `${header}\r\n`), '') + '\r\n');
        if (head.length) upstream.write(head);
        socket.pipe(upstream).pipe(socket);
      });
      upstream.on('error', () => socket.destroy());
      socket.on('error', () => upstream.destroy());
      socket.on('close', () => upstream.destroy());
    });
    await new Promise<void>((resolve, reject) => {
      relay.once('error', reject);
      relay.listen(0, '::', resolve);
    });
    const address = relay.address();
    assert.ok(address && typeof address !== 'string');
    port = address.port;
    previousDiscoverySource = undefined;
  });

  test.afterAll(async () => {
    try {
      for (const socket of connections) socket.destroy();
      if (server?.listening) {
        await new Promise<void>((resolve, reject) => server!.close(error => error ? reject(error) : resolve()));
      }
    } finally {
      await peer?.stop();
    }
  });

  const cases: Array<[host: string, profile: string, target?: string]> = [
    ['127.0.0.1', 'v4'], ['[::1]', 'v4'], ['[::1]', 'v6'],
    ['[::1]', 'v6-discovery'], ['[::1]', 'v6-discovery'],
    ['[::1]', 'auto', 'v4'], ['[::1]', 'auto', 'v6-discovery'], ['[::1]', 'auto', 'v4'],
    ['[::1]', 'token-fallback', 'v6-discovery'], ['[::1]', 'token-fallback', 'v4'],
    ['127.0.0.1', 'legacy', 'v4'], ['[::1]', 'legacy-token', 'v4'],
  ];
  for (const [index, [host, profile, target = profile]] of cases.entries()) {
    test(`${index + 1}: ${profile} -> ${target} over ${host}`, async ({ page }) => {
      autoPeer = target;
      const expectIpv6 = target.startsWith('v6');
      await page.goto(`http://${host}:${port}/`);
      const pcap = await page.evaluate(({ workerFile, profile, secret, expectIpv6 }) => new Promise<number[]>((resolve, reject) => {
        const worker = new Worker(`/assets/${workerFile}`, { type: 'module' });
        const timer = setTimeout(() => finish(new Error(
          `Worker tunnel timed out (${profile}; started=${started}, ready=${ready}, http=${http}, ws=${ws})`
        )), 12000);
        let started = false, ready = false, http = false, ws = false, downloading = false;
        function finish(error: Error | null, capture?: number[]) {
          clearTimeout(timer); worker.postMessage('close'); worker.terminate();
          if (error) reject(error); else if (capture) resolve(capture);
        }
        worker.onerror = event => finish(new Error(event.message));
        worker.onmessage = ({ data }) => {
          if (data === 'started') { started = true; worker.postMessage({ type: 4, data: {
            chargerID: profile, port: 80, mtu: 1420, secret: new Uint8Array(secret), debugMode: true,
          } }); }
          if (data === 'ready' && !ready) {
            ready = true;
            worker.postMessage('connect');
            worker.postMessage({ type: 2, id: 'status', data: {
              method: 'GET', headers: [], url: `${location.origin}/status`,
            } });
          }
          if (data.type === 5 || data.unresolved) finish(new Error(JSON.stringify(data)));
          if (data.type === 3) {
            const expected = expectIpv6 ? 'peer-v6' : 'peer-v4';
            if (data.data.status !== 200 || new TextDecoder().decode(data.data.body) !== expected) {
              finish(new Error('Unexpected HTTP response')); return;
            }
            http = true;
          }
          if (data.type === 0 && data.data === 'peer-ws') ws = true;
          if (http && ws && !downloading) { downloading = true; worker.postMessage('download'); }
          if (data.type === 1) finish(null, Array.from(data.data as Uint8Array));
        };
      }), { workerFile, profile, secret, expectIpv6 });
      const bytes = Buffer.from(pcap);
      let interfaces = 0, packets = 0, discoverySource: string | undefined, discoveryPeer: string | undefined;
      const tcpPairs: Array<[string, string]> = [];
      let httpRequests = 0, wsRequests = 0;
      for (let offset = 0; offset < bytes.length;) {
        const type = bytes.readUInt32LE(offset), length = bytes.readUInt32LE(offset + 4);
        assert.ok(length >= 12 && offset + length <= bytes.length);
        if (type === 1) { interfaces++; assert.equal(bytes.readUInt16LE(offset + 8), 101); }
        if (type === 6) {
          packets++;
          const ip = bytes.subarray(offset + 28, offset + 28 + bytes.readUInt32LE(offset + 20));
          const ipv6 = (ip[0] >> 4) === 6;
          const protocol = ip[ipv6 ? 6 : 9];
          if (protocol === 6) {
            assert.equal(ip[0] >> 4, expectIpv6 ? 6 : 4, 'TCP selected the wrong address family');
            const source = ip.subarray(ipv6 ? 8 : 12, ipv6 ? 24 : 16).toString('hex');
            const destination = ip.subarray(ipv6 ? 24 : 16, ipv6 ? 40 : 20).toString('hex');
            tcpPairs.push([source, destination]);
            const ipHeader = ipv6 ? 40 : (ip[0] & 15) * 4;
            const payload = ip.subarray(ipHeader + (ip[ipHeader + 12] >> 4) * 4);
            if (payload.includes('GET /status ')) httpRequests++;
            if (payload.includes('GET /ws ')) wsRequests++;
          }
          if (ipv6 && protocol === 58 && ip[40] === 128) {
            discoverySource = ip.subarray(8, 24).toString('hex');
            assert.equal(ip.subarray(24, 40).toString('hex'), 'ff020000000000000000000000000001');
            assert.equal(ip[7], 1, 'Multicast discovery must stay on the tunnel link');
          }
          if (ipv6 && protocol === 58 && ip[40] === 129) {
            discoveryPeer = ip.subarray(8, 24).toString('hex');
          }
        }
        offset += length;
      }
      assert.equal(interfaces, 1); assert.ok(packets > 0);
      assert.ok(tcpPairs.length > 0, 'Capture must contain inner TCP traffic');
      assert.ok(httpRequests > 0 && wsRequests > 0, 'Both HTTP and inner WebSocket must use TCP');
      if (profile !== 'v6') {
        if (profile.startsWith('legacy')) {
          assert.equal(discoverySource, undefined, 'Older firmware must not receive IPv6 discovery probes');
        } else {
          assert.ok(discoverySource?.startsWith('fd'));
          assert.notEqual(discoverySource, previousDiscoverySource);
        }
        if (expectIpv6) assert.ok(discoveryPeer?.startsWith('fd'));
        for (const [source, destination] of tcpPairs) {
          const local = expectIpv6 ? discoverySource : '0a7b0103';
          const peer = expectIpv6 ? discoveryPeer : '0a7b0102';
          assert.ok((source === local && destination === peer) || (source === peer && destination === local),
            'TCP must use the selected address pair in both directions');
        }
        previousDiscoverySource = discoverySource;
      }
    });
  }
});
