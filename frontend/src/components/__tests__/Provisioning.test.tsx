import { render, screen, fireEvent, cleanup, act } from '@testing-library/preact';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Provisioning } from '../Provisioning';

interface ProvisioningBridge {
    isSupported: () => boolean;
    isProvisioning: () => boolean;
    startProvisioning: () => void;
    stopProvisioning: () => void;
}

const makeBridge = (overrides: Partial<ProvisioningBridge> = {}): ProvisioningBridge => ({
    isSupported: () => false,
    isProvisioning: () => false,
    startProvisioning: vi.fn(),
    stopProvisioning: vi.fn(),
    ...overrides,
});

const setProvisioningBridge = (bridge?: ProvisioningBridge) => {
    if (bridge) {
        window.tinkerforge_provisioning = bridge;
    } else {
        delete window.tinkerforge_provisioning;
    }
};

beforeEach(() => {
    setProvisioningBridge(undefined);
    delete window.onWarpProvisioningFailed;
    delete window.onWarpProvisioningScanCancelled;
});

afterEach(() => {
    cleanup();
});

describe('Provisioning component', () => {
    it('renders nothing when the provisioning bridge is not available', () => {
        const { container } = render(<Provisioning />);
        expect(container.firstChild).toBeNull();
    });

    it('renders nothing when the bridge reports no support', () => {
        setProvisioningBridge(makeBridge({ isSupported: () => false }));
        const { container } = render(<Provisioning />);
        expect(container.firstChild).toBeNull();
    });

    it('renders the provisioning card when the bridge is supported', () => {
        setProvisioningBridge(makeBridge({ isSupported: () => true }));
        const { container } = render(<Provisioning />);

        expect(container.querySelector('.card')).not.toBeNull();
        expect(container.querySelector('.card-header')).not.toBeNull();
        expect(container.querySelector('.card-body')).not.toBeNull();
        expect(screen.getByText('title')).toBeInTheDocument();
        expect(screen.getByText('provisioning_button')).toBeInTheDocument();
        expect(screen.getByText('provisioning_hint')).toBeInTheDocument();
    });

    it('starts provisioning when the button is clicked', () => {
        const startProvisioning = vi.fn();
        setProvisioningBridge(makeBridge({ isSupported: () => true, startProvisioning }));

        render(<Provisioning />);

        fireEvent.click(screen.getByText('provisioning_button'));
        expect(startProvisioning).toHaveBeenCalledTimes(1);
    });

    it('surfaces errors from the global failure callback as a dismissible alert', () => {
        setProvisioningBridge(makeBridge({ isSupported: () => true }));
        render(<Provisioning />);

        expect(window.onWarpProvisioningFailed).toBeDefined();
        act(() => {
            window.onWarpProvisioningFailed!('Scan failed');
        });
        expect(screen.getByText('Scan failed')).toBeInTheDocument();

        fireEvent.click(screen.getByTestId('close-alert'));
        expect(screen.queryByText('Scan failed')).toBeNull();
    });

    it('clears any stale error when provisioning is (re)started', () => {
        const startProvisioning = vi.fn();
        setProvisioningBridge(makeBridge({ isSupported: () => true, startProvisioning }));

        render(<Provisioning />);

        act(() => {
            window.onWarpProvisioningFailed!('Stale error');
        });
        expect(screen.getByText('Stale error')).toBeInTheDocument();

        fireEvent.click(screen.getByText('provisioning_button'));
        expect(startProvisioning).toHaveBeenCalled();
        expect(screen.queryByText('Stale error')).toBeNull();
    });

    it('registers the scan-cancelled handler without surfacing anything', () => {
        setProvisioningBridge(makeBridge({ isSupported: () => true }));
        render(<Provisioning />);

        expect(window.onWarpProvisioningScanCancelled).toBeDefined();
        expect(() => window.onWarpProvisioningScanCancelled!()).not.toThrow();
    });

    it('clears the global handlers when the component unmounts', () => {
        setProvisioningBridge(makeBridge({ isSupported: () => true }));
        const { unmount } = render(<Provisioning />);

        expect(window.onWarpProvisioningFailed).toBeDefined();
        expect(window.onWarpProvisioningScanCancelled).toBeDefined();

        unmount();

        expect(window.onWarpProvisioningFailed).toBeUndefined();
        expect(window.onWarpProvisioningScanCancelled).toBeUndefined();
    });
});
