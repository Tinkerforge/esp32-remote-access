/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

import i18n from './i18n';

let currentVersion: string | null = null;
let versionCheckInterval: NodeJS.Timeout | null = null;

// Function to get the current build version
async function getCurrentVersionHash(): Promise<string | null> {
    try {
        // Try to get version from the build manifest or a dedicated version endpoint
        const response = await fetch('/version.json', {
            cache: 'no-cache',
            headers: {
                'Cache-Control': 'no-cache, no-store, must-revalidate',
                'Pragma': 'no-cache',
                'Expires': '0'
            }
        });

        if (response.ok) {
            const data = await response.json();
            return data.buildHash;
        }

        // Fallback: use the index.html timestamp as version
        const indexResponse = await fetch('/', {
            cache: 'no-cache',
            headers: {
                'Cache-Control': 'no-cache, no-store, must-revalidate',
                'Pragma': 'no-cache',
                'Expires': '0'
            }
        });

        if (indexResponse.ok) {
            const lastModified = indexResponse.headers.get('last-modified');
            return lastModified || Date.now().toString();
        }

    } catch (error) {
        console.warn('Failed to check version:', error);
    }

    return null;
}

// Function to check if a new version is available
async function checkForNewVersion(): Promise<boolean> {
    const newVersion = await getCurrentVersionHash();

    if (!newVersion) {
        return false;
    }

    if (currentVersion === null) {
        currentVersion = newVersion;
        return false;
    }

    return currentVersion !== newVersion;
}

export function startVersionChecking(intervalMinutes: number = 10) {
    getCurrentVersionHash().then(version => {
        currentVersion = version;
    });

    if (versionCheckInterval) {
        clearInterval(versionCheckInterval);
    }

    versionCheckInterval = setInterval(async () => {
        try {
            const hasNewVersion = await checkForNewVersion();

            if (hasNewVersion) {
                if (confirm(i18n.t('version_checker.new_version_available'))) {
                    window.location.reload();
                } else {
                    stopVersionChecking();
                }
            }
        } catch (error) {
            console.warn('Version check failed:', error);
        }
    }, intervalMinutes * 60 * 1000);
}

export function stopVersionChecking() {
    if (versionCheckInterval) {
        clearInterval(versionCheckInterval);
        versionCheckInterval = null;
    }
}

export async function forceCheckForUpdates(): Promise<void> {
    const hasNewVersion = await checkForNewVersion();

    if (hasNewVersion) {
        if (confirm(i18n.t('version_checker.new_version_confirm'))) {
            window.location.reload();
        }
    } else {
        alert(i18n.t('version_checker.already_latest'));
    }
}

export function forceReload(): void {
    if ('caches' in window) {
        caches.keys().then(names => {
            names.forEach(name => {
                caches.delete(name);
            });
        });
    }

    if (navigator.serviceWorker && navigator.serviceWorker.controller) {
        navigator.serviceWorker.controller.postMessage({
            type: 'CLEAR_CACHE'
        });
    }

    const keysToKeep = ['debugMode', 'currentConnection', 'loginSalt'];
    const keysToRemove: string[] = [];

    for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key && !keysToKeep.includes(key)) {
            keysToRemove.push(key);
        }
    }

    keysToRemove.forEach(key => localStorage.removeItem(key));

    window.location.href = `${window.location.href + (window.location.href.includes('?') ? '&' : '?')  }_t=${  Date.now()}`;
}
