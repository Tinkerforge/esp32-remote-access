export interface DiscoveredDevices {
    serviceName: string;
    displayName: string;
    host: string;
    port: number;
    brand: string;
    model: string;
    txtvers: string;
    firmwareVersion: string;
    uid: number;
}

// Extend the global Window interface for the discovery and provisioning bridges
declare global {
    interface Window {
        tinkerforge_discovery?: {
            isSupported(): boolean;
            startDiscovery(): void;
            stopDiscovery(): void;
            getChargers(): string;
            navigateToCharger(host: string): void;
        };
        tinkerforge_provisioning?: {
            isSupported(): boolean;
            isProvisioning(): boolean;
            startProvisioning(): void;
            stopProvisioning(): void;
        };
        tinkerforge_devices?: {
            resetToDevices(): void;
        };
        onWarpChargersChanged?: (chargers: DiscoveredDevices[]) => void;
        onWarpDiscoveryStopped?: () => void;
        onWarpProvisioningFailed?: (reason: string) => void;
        onWarpProvisioningScanCancelled?: () => void;
        onWarpProvisioningStopped?: () => void;
    }
}

export {};
