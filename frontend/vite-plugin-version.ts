import { Plugin } from 'vite';

const versionPlugin: Plugin = {
    name: 'version-generator',
    generateBundle() {
        // Generate version file with build timestamp and hash
        const version = {
            version: process.env.npm_package_version || '1.0.0',
            buildTime: new Date().toISOString(),
            buildHash: Date.now().toString(36) + Math.random().toString(36).substr(2),
            timestamp: Date.now()
        };

        this.emitFile({
            type: 'asset',
            fileName: 'version.json',
            source: JSON.stringify(version, null, 2)
        });
    }
};

export default versionPlugin;
