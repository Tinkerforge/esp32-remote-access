import { execFile, spawn } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const execute = promisify(execFile);

/** Build and start the real WireGuard/smoltcp peer used by the browser tests. */
export async function startTunnelPeer() {
  const cwd = fileURLToPath(new URL('../../wg-webclient/', import.meta.url));
  const { stdout: version } = await execute('rustc', ['-vV']);
  const host = version.match(/^host: (.+)$/m)?.[1];
  if (!host) throw new Error('Cannot determine the native Rust target for the test peer');
  let output: string;
  try {
    const result = await execute('cargo', [
      'build', '--locked', '--target', host, '--example', 'tunnel_peer', '--message-format=json',
    ], { cwd, maxBuffer: 16 * 1024 * 1024 });
    output = result.stdout;
  } catch (error) {
    throw new Error(`Cannot build the encrypted test peer:\n${(error as { stderr?: string }).stderr ?? String(error)}`);
  }
  const artifact = output.split('\n').filter(line => line.startsWith('{')).map(line => JSON.parse(line))
    .find(message => message.reason === 'compiler-artifact' && message.target?.name === 'tunnel_peer' && message.executable);
  if (typeof artifact?.executable !== 'string') throw new Error('Cargo did not produce the encrypted test peer');

  const peer = spawn(artifact.executable, ['0'], { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
  const stop = async () => {
    if (!peer.pid || peer.exitCode !== null || peer.signalCode !== null) return;
    const exited = once(peer, 'exit');
    peer.kill();
    await exited;
  };
  let diagnostics = '';
  peer.stderr.on('data', data => { diagnostics = (diagnostics + data).slice(-16384); });
  try {
    const port = await new Promise<number>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`Encrypted test peer did not start:\n${diagnostics}`)), 5000);
      let output = '';
      peer.stdout.on('data', data => {
        output += data;
        const port = output.match(/^LISTENING (\d+)$/m)?.[1];
        if (port) { clearTimeout(timer); resolve(Number(port)); }
      });
      peer.once('error', error => { clearTimeout(timer); reject(error); });
      peer.once('exit', code => {
        clearTimeout(timer);
        reject(new Error(`Encrypted test peer exited (${code}):\n${diagnostics}`));
      });
    });
    return { port, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}
