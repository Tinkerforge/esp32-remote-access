import Median from "median-js-bridge";
import { play_store_link } from "links";
import favicon from "favicon";

const DISMISSED_KEY = "android-smart-banner-dismissed";

export function initAndroidSmartBanner() {
    if (Median.isNativeApp()) return;
    if (!/Android/i.test(navigator.userAgent)) return;
    if (localStorage.getItem(DISMISSED_KEY)) return;

    const banner = document.createElement("div");
    banner.id = "android-smart-banner";
    banner.innerHTML = `
        <button id="android-smart-banner-close" aria-label="Close">&times;</button>
        <img id="android-smart-banner-icon" src="${favicon}" alt="App icon" />
        <span id="android-smart-banner-text">Get the app</span>
        <a id="android-smart-banner-link" href="${play_store_link}" target="_blank" rel="noopener noreferrer">View</a>
    `;
    document.body.prepend(banner);

    document.getElementById("android-smart-banner-close")!.addEventListener("click", () => {
        banner.remove();
        localStorage.setItem(DISMISSED_KEY, "1");
    });
}
