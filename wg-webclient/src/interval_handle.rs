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

use wasm_bindgen::{closure::Closure, JsCast, JsValue};

/**
    This is a helper struct to be able to automagically clear intervals once they are not needed anymore.
*/
pub struct IntervalHandle<T> {
    interval_handle: i32,
    _closure: Closure<dyn FnMut(T)>,
    log_drop: bool,
}

impl<T> IntervalHandle<T> {
    pub fn new(f: Closure<dyn FnMut(T)>, interval: i32) -> Self {
        let global = js_sys::global();
        let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
        let interval_handle = global
            .set_interval_with_callback_and_timeout_and_arguments_0(
                f.as_ref().unchecked_ref(),
                interval,
            )
            .unwrap();
        Self {
            interval_handle,
            _closure: f,
            log_drop: false,
        }
    }

    pub fn set_drop_logging(&mut self, log_drop: bool) {
        self.log_drop = log_drop;
    }
}

impl<T> Drop for IntervalHandle<T> {
    fn drop(&mut self) {
        let global = js_sys::global();
        let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
        global.clear_interval_with_handle(self.interval_handle);

        if self.log_drop {
            log::debug!("Dropping interval");
        }
    }
}
