import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { chromium } from '@playwright/test';
import { transform } from 'esbuild';
import sodium from 'libsodium-wrappers';
import { startTunnelPeer } from '../tests/tunnel-peer.ts';

const { values } = parseArgs({
    options: {
        baseline: { type: 'string' },
        rounds: { type: 'string', default: '6' },
        'idle-ms': { type: 'string', default: '2000' },
        json: { type: 'boolean', default: false },
    },
});
assert.ok(values.baseline, 'Pass --baseline <commit> from before the polling change');
const rounds = Number(values.rounds);
const idleMs = Number(values['idle-ms']);
assert.ok(Number.isSafeInteger(rounds) && rounds >= 2, '--rounds must be an integer of at least 2');
assert.ok(Number.isSafeInteger(idleMs) && idleMs >= 500, '--idle-ms must be an integer of at least 500');
const root = resolve(import.meta.dirname, '../..');
const git = (...args: string[]) => execFileSync('git', args, { cwd: root });
const baseline = git('rev-parse', '--verify', '--end-of-options', `${values.baseline}^{commit}`).toString().trim();
const candidate = git('rev-parse', 'HEAD').toString().trim();
const dirty = git('status', '--porcelain', '--', 'wg-webclient').length > 0;
const scratch = await mkdtemp(join(tmpdir(), 'tunnel-polling-'));
const packages = { baseline: join(scratch, 'baseline'), candidate: join(scratch, 'candidate') };
type Variant = keyof typeof packages;
type Sample = { idle: { ms: number; intervalCallbacks: number; timeoutCallbacks: number }; requests: number[] };
const samples: Record<Variant, Sample[]> = { baseline: [], candidate: [] };

try {
    const archive = git('archive', baseline, '--', 'wg-webclient');
    execFileSync('tar', ['-x', '-C', scratch], { input: archive });
    for (const [variant, crate] of [
        ['baseline', join(scratch, 'wg-webclient')],
        ['candidate', join(root, 'wg-webclient')],
    ] as const) {
        console.error(`Building ${variant}…`);
        execFileSync('wasm-pack', ['build', '--target', 'web', '--out-dir', packages[variant], '--locked'], {
            cwd: crate,
            stdio: ['ignore', process.stderr, process.stderr],
        });
    }
    const source = await readFile(join(import.meta.dirname, 'tunnel-worker.ts'), 'utf8');
    const worker = (await transform(source, { loader: 'ts', format: 'esm' })).code;
    await sodium.ready;
    const peerKey = sodium.to_base64(sodium.crypto_scalarmult_base(new Uint8Array(32).fill(2)), sodium.base64_variants.ORIGINAL);
    const peer = await startTunnelPeer();
    const server = createServer(async (req, res) => {
        try {
            if (req.url === '/') {
                res.setHeader('Content-Type', 'text/html');
                res.end('<!doctype html><title>Tunnel polling benchmark</title>');
                return;
            }
            if (req.url?.startsWith('/worker.js?')) {
                res.setHeader('Content-Type', 'text/javascript');
                res.end(worker);
                return;
            }
            const asset = req.url?.match(/^\/(baseline|candidate)\/(wg_webclient\.js|wg_webclient_bg\.wasm)$/);
            if (!asset) {
                res.writeHead(404).end();
                return;
            }
            const bytes = await readFile(join(packages[asset[1] as Variant], asset[2]));
            res.setHeader('Content-Type', asset[2].endsWith('.wasm') ? 'application/wasm' : 'text/javascript');
            res.end(bytes);
        } catch (error) {
            res.writeHead(500).end(String(error));
        }
    });

    try {
        server.listen(0, '127.0.0.1');
        await once(server, 'listening');
        const address = server.address();
        assert.ok(address && typeof address !== 'string');
        const browser = await chromium.launch({ args: ['--disable-background-timer-throttling'] });
        try {
            const page = await browser.newPage();
            await page.goto(`http://127.0.0.1:${address.port}/`);
            const run = (variant: Variant) => page.evaluate(({ variant, idleMs, peerPort, peerKey }) => {
                return new Promise<Sample>((fulfill, reject) => {
                    const parameters = new URLSearchParams({
                        variant, idleMs: String(idleMs), peerPort: String(peerPort), peerKey,
                    });
                    const worker = new Worker(`/worker.js?${parameters}`, { type: 'module' });
                    const finish = () => {
                        clearTimeout(timeout);
                        worker.terminate();
                    };
                    const timeout = setTimeout(() => {
                        finish();
                        reject(new Error(`${variant} timed out`));
                    }, idleMs + 30000);
                    worker.onmessage = ({ data }) => {
                        finish();
                        if (data.error) reject(new Error(`${variant}: ${data.error}`));
                        else fulfill(data.result);
                    };
                    worker.onerror = event => {
                        finish();
                        reject(new Error(`${variant}: ${event.message}`));
                    };
                });
            }, { variant, idleMs, peerPort: peer.port, peerKey });

            for (const variant of ['baseline', 'candidate'] as const) await run(variant);
            for (let round = 0; round < rounds; round++) {
                const order: Variant[] = round % 2 === 0 ? ['baseline', 'candidate'] : ['candidate', 'baseline'];
                for (const variant of order) samples[variant].push(await run(variant));
            }
            const summarize = (runs: Sample[]) => {
                const seconds = runs.reduce((sum, run) => sum + run.idle.ms, 0) / 1000;
                const requests = runs.flatMap(run => run.requests).sort((a, b) => a - b);
                const rounded = (value: number) => Number(value.toFixed(2));
                return {
                    'Idle interval callbacks/s': rounded(runs.reduce((sum, run) => sum + run.idle.intervalCallbacks, 0) / seconds),
                    'Idle timeout callbacks/s': rounded(runs.reduce((sum, run) => sum + run.idle.timeoutCallbacks, 0) / seconds),
                    'HTTP p50 (ms)': rounded(requests[Math.ceil(requests.length * 0.50) - 1]),
                    'HTTP p95 (ms)': rounded(requests[Math.ceil(requests.length * 0.95) - 1]),
                };
            };
            const summary = { baseline: summarize(samples.baseline), candidate: summarize(samples.candidate) };
            const metadata = { baseline, candidate, dirty, chromium: browser.version(), rounds, idleMs };
            if (values.json) {
                console.log(JSON.stringify({ metadata, summary, samples }, null, 2));
            } else {
                console.log(metadata);
                console.table(summary);
            }
        } finally {
            await browser.close();
        }
    } finally {
        server.close();
        await peer.stop();
    }
} finally {
    await rm(scratch, { recursive: true, force: true });
}
