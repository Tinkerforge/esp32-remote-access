import { act, cleanup, render, screen, waitFor } from '@testing-library/preact';
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import sodium from 'libsodium-wrappers';
import { Base64 } from 'js-base64';
import { Frame } from '../Frame';
import { Footer, connectedDeviceNote } from '../../components/Footer';
import { fetchClient } from '../../utils';
import type { components } from '../../schema';

vi.unmock('@preact/signals');

const { route, params, worker } = vi.hoisted(() => ({
  route: vi.fn(),
  params: { device: 'device-1', path: '' },
  worker: { postMessage: vi.fn(), terminate: vi.fn() },
}));

vi.mock('preact-iso', () => ({
  useLocation: () => ({ route }),
  useRoute: () => ({ params }),
}));

vi.mock('../../worker?worker', () => ({
  default: class {
    postMessage = worker.postMessage;
    terminate = worker.terminate;
  },
}));

type ChargerInfo = components['schemas']['ChargerInfo'];

const info: ChargerInfo = {
  id: 'device-1',
  name: null,
  note: 'encrypted-note',
  configured_port: 80,
  connected: true,
  firmware_version: '2.0.0',
  mtu: 1240,
};

function View({ device = 'device-1' }: { device?: string }) {
  return <><Frame key={device} /><Footer /></>;
}

describe('connected device note', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    params.device = 'device-1';
    connectedDeviceNote.value = '';
    vi.stubGlobal('navigator', {
      serviceWorker: { addEventListener: vi.fn() },
    });
    vi.mocked(fetchClient.POST).mockResolvedValue({ data: info, response: new Response() });
    vi.mocked(Base64.toUint8Array).mockReturnValue(new Uint8Array([1, 2, 3]));
    (sodium.crypto_box_seal_open as Mock).mockReturnValue(new TextEncoder().encode('Support: Müller\nDebugging device'));
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('loads and decrypts the note on a direct device visit and clears it on exit', async () => {
    const { unmount } = render(<View />);

    expect(await screen.findByText('Support: Müller')).toBeInTheDocument();
    expect(fetchClient.POST).toHaveBeenCalledWith('/charger/info', { body: { charger: 'device-1' } });
    expect(Base64.toUint8Array).toHaveBeenCalledWith('encrypted-note');
    expect(screen.queryByText(/Debugging device/)).not.toBeInTheDocument();

    unmount();
    expect(connectedDeviceNote.value).toBe('');
    expect(worker.terminate).toHaveBeenCalled();
  });

  it.each([null, '', undefined])('keeps the footer links without a note (%s)', async (note) => {
    vi.mocked(fetchClient.POST).mockResolvedValue({ data: { ...info, note }, response: new Response() });
    const { container } = render(<View />);

    await waitFor(() => expect(fetchClient.POST).toHaveBeenCalled());
    expect(container.querySelector('.footer-device-note')).toBeNull();
    expect(screen.getAllByRole('link')).toHaveLength(3);
    expect(sodium.crypto_box_seal_open).not.toHaveBeenCalled();
  });

  it('shows the existing invalid-key message if the note cannot be decrypted', async () => {
    vi.mocked(sodium.crypto_box_seal_open).mockImplementation(() => { throw new Error('invalid key'); });
    render(<View />);

    expect(await screen.findByText('invalid_key')).toBeInTheDocument();
    expect(route).not.toHaveBeenCalled();
  });

  it('clears the old note when switching to a device without a note', async () => {
    const { rerender } = render(<View />);
    await screen.findByText('Support: Müller');

    vi.mocked(fetchClient.POST).mockResolvedValue({ data: { ...info, id: 'device-2', note: null }, response: new Response() });
    params.device = 'device-2';
    rerender(<View device="device-2" />);

    await waitFor(() => expect(fetchClient.POST).toHaveBeenCalledWith('/charger/info', { body: { charger: 'device-2' } }));
    expect(screen.queryByText('Support: Müller')).not.toBeInTheDocument();
    expect(connectedDeviceNote.value).toBe('');
  });

  it('ignores a device-info response that arrives after leaving the device', async () => {
    let resolveInfo!: (value: Awaited<ReturnType<typeof fetchClient.POST>>) => void;
    vi.mocked(fetchClient.POST).mockReturnValue(new Promise(resolve => { resolveInfo = resolve; }));
    const { unmount } = render(<View />);
    await waitFor(() => expect(fetchClient.POST).toHaveBeenCalled());

    unmount();
    await act(async () => {
      resolveInfo({ data: info, response: new Response() });
    });

    expect(connectedDeviceNote.value).toBe('');
    expect(sodium.crypto_box_seal_open).not.toHaveBeenCalled();
  });
});
