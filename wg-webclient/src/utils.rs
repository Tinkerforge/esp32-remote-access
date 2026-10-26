use wasm_bindgen::prelude::*;

#[inline]
pub fn now() -> smoltcp::time::Instant {
    #[cfg(target_arch = "wasm32")]
    let now = js_sys::Date::now();
    #[cfg(not(target_arch = "wasm32"))]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    smoltcp::time::Instant::from_millis(now as i64)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
}
