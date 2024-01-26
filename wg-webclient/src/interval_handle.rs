use wasm_bindgen::{closure::Closure, JsCast, JsValue};

pub struct IntervalHandle<T> {
    interval_handle: i32,
    _closure: Closure<dyn FnMut(T)>,
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
        }
    }
}

impl<T> Drop for IntervalHandle<T> {
    fn drop(&mut self) {
        let global = js_sys::global();
        let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
        global.clear_interval_with_handle(self.interval_handle);
    }
}
