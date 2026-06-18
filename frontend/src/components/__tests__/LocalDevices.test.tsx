import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/preact';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LocalDevices } from '../LocalDevices';
import { DiscoveredDevices } from '../../types/window';

// Helper to keep the bridge types in one place; the global state is what
// the component actually reads, so we can install the bridge from
// setup() the same way the WARP app does.
const installDiscoveryBridge = () => {
    const startDiscovery = vi.fn();
    const stopDiscovery = vi.fn();
    const navigateToCharger = vi.fn();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).tinkerforge_discovery = {
        isSupported: () => true,
        startDiscovery,
        stopDiscovery,
        getChargers: () => '[]',
        navigateToCharger,
    };
    return { startDiscovery, stopDiscovery, navigateToCharger };
};

const installProvisioningBridge = () => {
    const startProvisioning = vi.fn();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).tinkerforge_provisioning = {
        isSupported: () => true,
        isProvisioning: () => false,
        startProvisioning,
        stopProvisioning: vi.fn(),
    };
    return { startProvisioning };
};

const removeBridges = () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (window as any).tinkerforge_discovery;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (window as any).tinkerforge_provisioning;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (window as any).onWarpChargersChanged;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (window as any).onWarpDiscoveryStopped;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (window as any).onWarpProvisioningFailed;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (window as any).onWarpProvisioningScanCancelled;
};

const installBothBridges = () => ({
    ...installDiscoveryBridge(),
    ...installProvisioningBridge(),
});

const makeDevice = (overrides: Partial<DiscoveredDevices> = {}): DiscoveredDevices => ({
    serviceName: 'WARP-ABCD',
    displayName: 'Garage Charger',
    host: 'warp.local',
    port: 80,
    brand: 'Tinkerforge',
    model: 'WARP3',
    txtvers: '1',
    uid: 12345,
    firmwareVersion: '2.3.1',
    ...overrides,
});

const fireDiscoveryResult = (devices: DiscoveredDevices[]) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const handler = (window as any).onWarpChargersChanged as (d: DiscoveredDevices[]) => void;
    handler(devices);
};

describe('LocalDevices', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    afterEach(() => {
        cleanup();
        removeBridges();
    });

    it('renders nothing when no WARP bridge is available', () => {
        const { container } = render(<LocalDevices />);

        expect(container.innerHTML).toBe('');
    });

    it('shows only the provisioning section when only the provisioning bridge is available', () => {
        const { startProvisioning } = installProvisioningBridge();

        render(<LocalDevices />);

        // Provisioning section is present.
        expect(screen.getByText('provisioning_button')).toBeInTheDocument();
        expect(screen.getByText('provisioning_hint')).toBeInTheDocument();

        // No device-list state hints are rendered.
        expect(screen.queryByText('searching')).toBeNull();
        expect(screen.queryByText('none_found')).toBeNull();

        // Clicking the button calls the bridge.
        fireEvent.click(screen.getByText('provisioning_button'));
        expect(startProvisioning).toHaveBeenCalledTimes(1);
    });

    it('shows only the device list when only the discovery bridge is available', async () => {
        const { navigateToCharger } = installDiscoveryBridge();

        render(<LocalDevices />);

        // The card renders but the provisioning section is absent.
        expect(screen.queryByText('provisioning_button')).toBeNull();

        // We are still waiting for the first discovery result.
        expect(screen.getByText('searching')).toBeInTheDocument();

        fireDiscoveryResult([makeDevice()]);

        await waitFor(() => {
            expect(screen.getByText('Garage Charger')).toBeInTheDocument();
        });

        // The whole row is the click target.
        const row = screen.getByRole('button', { name: /connect Garage Charger/i });
        fireEvent.click(row);
        expect(navigateToCharger).toHaveBeenCalledWith('warp.local');
    });

    it('renders both sections separated by a divider when both bridges are available', async () => {
        const { startProvisioning } = installBothBridges();

        const { container } = render(<LocalDevices />);

        // Both sections render.
        expect(screen.getByText('provisioning_button')).toBeInTheDocument();
        expect(screen.getByText('searching')).toBeInTheDocument();

        // A divider is rendered between them.
        expect(container.querySelector('hr')).not.toBeNull();

        // The provisioning button is independent of the discovery flow.
        fireEvent.click(screen.getByText('provisioning_button'));
        expect(startProvisioning).toHaveBeenCalledTimes(1);
    });

    it('starts and stops discovery on mount and unmount, and clears the global callbacks', () => {
        const { startDiscovery, stopDiscovery } = installDiscoveryBridge();

        const { unmount } = render(<LocalDevices />);

        expect(startDiscovery).toHaveBeenCalledTimes(1);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect(typeof (window as any).onWarpChargersChanged).toBe('function');
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect(typeof (window as any).onWarpDiscoveryStopped).toBe('function');

        unmount();

        expect(stopDiscovery).toHaveBeenCalledTimes(1);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect((window as any).onWarpChargersChanged).toBeUndefined();
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect((window as any).onWarpDiscoveryStopped).toBeUndefined();
    });

    it('shows an empty hint when discovery returns no devices', async () => {
        installDiscoveryBridge();

        render(<LocalDevices />);

        fireDiscoveryResult([]);

        await waitFor(() => {
            expect(screen.getByText('none_found')).toBeInTheDocument();
        });
    });

    it('renders one clickable row per discovered device', async () => {
        const { navigateToCharger } = installDiscoveryBridge();

        render(<LocalDevices />);

        fireDiscoveryResult([
            makeDevice({
                serviceName: 'WARP-ABCD',
                displayName: 'Garage Charger',
                host: 'warp.local',
                uid: 12345,
                model: 'WARP3',
            }),
            makeDevice({
                serviceName: 'WARP-EFGH',
                displayName: '',
                host: 'warp2.local',
                uid: 67890,
                model: '',
            }),
        ]);

        await waitFor(() => {
            expect(screen.getByText('Garage Charger')).toBeInTheDocument();
        });

        // The first device uses its displayName; the second falls back to
        // serviceName when no displayName is provided.
        expect(screen.getByText('WARP-EFGH')).toBeInTheDocument();

        // Both hosts are shown next to their entries. The host, model, and
        // firmware share the same parent `<small>` element, so look up the
        // small directly to avoid false matches against adjacent text.
        const smalls = document.querySelectorAll('small');
        const smallTexts = Array.from(smalls).map(s => s.textContent);
        expect(smallTexts.some(t => t?.includes('warp.local') && t?.includes('WARP3'))).toBe(true);
        expect(smallTexts.some(t => t?.includes('warp2.local'))).toBe(true);

        // Each row is exposed as a button with an accessible name.
        const rows = screen.getAllByRole('button');
        expect(rows).toHaveLength(2);

        // A chevron is rendered as a visual affordance on every row.
        expect(screen.getAllByTestId('chevron-right')).toHaveLength(2);

        // Clicking a row triggers the bridge.
        fireEvent.click(rows[0]);
        expect(navigateToCharger).toHaveBeenCalledWith('warp.local');
    });

    it('activates the row on Enter and Space', async () => {
        const { navigateToCharger } = installDiscoveryBridge();

        render(<LocalDevices />);

        fireDiscoveryResult([makeDevice()]);

        await waitFor(() => {
            expect(screen.getByText('Garage Charger')).toBeInTheDocument();
        });

        const row = screen.getByRole('button', { name: /connect Garage Charger/i });

        fireEvent.keyDown(row, { key: 'Enter' });
        expect(navigateToCharger).toHaveBeenCalledTimes(1);
        expect(navigateToCharger).toHaveBeenLastCalledWith('warp.local');

        fireEvent.keyDown(row, { key: ' ' });
        expect(navigateToCharger).toHaveBeenCalledTimes(2);

        // Other keys must not trigger the bridge.
        fireEvent.keyDown(row, { key: 'a' });
        expect(navigateToCharger).toHaveBeenCalledTimes(2);
    });

    it('displays the firmware version alongside host and model', async () => {
        installDiscoveryBridge();

        render(<LocalDevices />);

        fireDiscoveryResult([
            makeDevice({
                serviceName: 'WARP-ABCD',
                displayName: 'Garage Charger',
                host: 'warp.local',
                uid: 12345,
                model: 'WARP3',
                firmwareVersion: '2.3.1',
            }),
            makeDevice({
                serviceName: 'WARP-EFGH',
                displayName: 'Office Charger',
                host: 'warp2.local',
                uid: 67890,
                model: '',
                firmwareVersion: '',
            }),
        ]);

        await waitFor(() => {
            expect(screen.getByText('Garage Charger')).toBeInTheDocument();
        });

        const smalls = document.querySelectorAll('small');
        const smallTexts = Array.from(smalls).map(s => s.textContent);
        // The first row shows host, model, and firmware; the firmware key
        // is rendered as a bare string by the test i18n mock.
        expect(smallTexts.some(t => t?.includes('warp.local') && t?.includes('WARP3') && t?.includes('2.3.1'))).toBe(true);
        // The second row omits the model and the firmware.
        expect(smallTexts.some(t => t?.includes('warp2.local') && !t?.includes('·', t.indexOf('warp2.local') + 1))).toBe(true);
    });

    it('falls back to host when both displayName and serviceName are missing', async () => {
        installDiscoveryBridge();

        render(<LocalDevices />);

        fireDiscoveryResult([
            makeDevice({
                serviceName: '',
                displayName: '',
                host: 'fallback.local',
                uid: 1,
            }),
        ]);

        await waitFor(() => {
            // The host is rendered twice (as the device label and below as
            // the smaller "host · model" line), so we just look for it.
            expect(screen.getAllByText('fallback.local').length).toBeGreaterThan(0);
        });
    });

    it('keeps the last known device list when discovery stops', async () => {
        installDiscoveryBridge();

        render(<LocalDevices />);

        fireDiscoveryResult([makeDevice()]);

        await waitFor(() => {
            expect(screen.getByText('Garage Charger')).toBeInTheDocument();
        });

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const stopped = (window as any).onWarpDiscoveryStopped as () => void;
        stopped();

        // The previously discovered device must still be in the list, not
        // flashed as "no devices found" while the bridge wraps up.
        expect(screen.queryByText('none_found')).toBeNull();
        expect(screen.getByText('Garage Charger')).toBeInTheDocument();
    });

    it('registers and tears down provisioning callbacks on mount and unmount', () => {
        installProvisioningBridge();

        const { unmount } = render(<LocalDevices />);

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect(typeof (window as any).onWarpProvisioningFailed).toBe('function');
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect(typeof (window as any).onWarpProvisioningScanCancelled).toBe('function');

        unmount();

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect((window as any).onWarpProvisioningFailed).toBeUndefined();
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        expect((window as any).onWarpProvisioningScanCancelled).toBeUndefined();
    });

    it('displays a provisioning error and dismisses it when the alert is closed', async () => {
        installProvisioningBridge();

        render(<LocalDevices />);

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const failed = (window as any).onWarpProvisioningFailed as (reason: string) => void;
        failed('QR code could not be read');

        await waitFor(() => {
            expect(screen.getByText('QR code could not be read')).toBeInTheDocument();
        });

        // The test mock provides a close button for dismissible alerts.
        fireEvent.click(screen.getByTestId('close-alert'));

        await waitFor(() => {
            expect(screen.queryByText('QR code could not be read')).toBeNull();
        });
    });
});
