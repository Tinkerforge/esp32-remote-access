/* esp32-remote-access
 * Copyright (C) 2026 Frederic Henrichs <frederic@tinkerforge.com>
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

import { useState, useEffect } from 'preact/hooks';
import { useTranslation } from 'react-i18next';
import Median from "median-js-bridge";
import { play_store_link } from "links";
import favicon from "favicon";
import { is_warp_app } from '../utils';

const DISMISSED_KEY = "android-smart-banner-dismissed";

export function AndroidSmartBanner() {
    const { t } = useTranslation("", { useSuspense: false });
    const [show, setShow] = useState(false);

    useEffect(() => {
        if (Median.isNativeApp()) return;
        if (!/Android/i.test(navigator.userAgent)) return;
        if (is_warp_app()) return;
        if (localStorage.getItem(DISMISSED_KEY)) return;
        setShow(true);
    }, []);

    const handleClose = () => {
        setShow(false);
        localStorage.setItem(DISMISSED_KEY, "1");
    };

    if (!show) {
        return null;
    }

    return (
        <div id="android-smart-banner">
            <button id="android-smart-banner-close" aria-label={t("android_smart_banner.close")} onClick={handleClose}>
                &times;
            </button>
            <img id="android-smart-banner-icon" src={favicon} alt="App icon" />
            <span id="android-smart-banner-text">{t("android_smart_banner.text")}</span>
            <a id="android-smart-banner-link" href={play_store_link} target="_blank" rel="noopener noreferrer">
                {t("android_smart_banner.view")}
            </a>
        </div>
    );
}
