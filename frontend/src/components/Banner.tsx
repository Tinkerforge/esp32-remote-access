/* esp32-remote-access
 * Copyright (C) 2025 Frederic Henrichs <frederic@tinkerforge.com>
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
import Alert from 'react-bootstrap/Alert';

const BANNER_DISMISSED_KEY = 'bannerDismissed';

export function Banner() {
    const { t } = useTranslation("", { useSuspense: false });
    const [show, setShow] = useState(false);

    useEffect(() => {
        const dismissed = sessionStorage.getItem(BANNER_DISMISSED_KEY);
        setShow(!dismissed);
    }, []);

    const handleClose = () => {
        setShow(false);
        sessionStorage.setItem(BANNER_DISMISSED_KEY, 'true');
    };

    if (!show) {
        return null;
    }

    return (
        <Alert variant="warning" dismissible onClose={handleClose} className="m-0 rounded-0 text-center p-0 py-1">
                    <Alert.Heading>{t('banner.title')}</Alert.Heading>
                    <p class="m-0">{t('banner.message')}</p>
        </Alert>
    );
}
