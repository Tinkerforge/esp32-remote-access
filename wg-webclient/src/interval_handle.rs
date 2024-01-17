use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::window;

pub struct IntervalHandle<T> {
    interval_handle: i32,
    _closure: Closure<dyn FnMut(T)>,
}

impl<T> IntervalHandle<T> {
    pub fn new(f: Closure<dyn FnMut(T)>, interval: i32) -> Self {
        let interval_handle = window()
            .unwrap()
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
        window().unwrap().clear_interval_with_handle(self.interval_handle);
    }
}
