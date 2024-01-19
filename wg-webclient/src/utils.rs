use wasm_bindgen::prelude::*;

#[inline]
pub fn now() -> smoltcp::time::Instant {
    let now = web_sys::window()
        .expect("not in a browser")
        .performance()
        .expect("performance object not available")
        .now();
    smoltcp::time::Instant::from_millis(now as i64)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
}
