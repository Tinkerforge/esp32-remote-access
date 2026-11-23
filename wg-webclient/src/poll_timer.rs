use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

use smoltcp::time::Instant;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::WorkerGlobalScope;

use crate::utils::now;

/// Keep one worker timeout for the earliest pending network poll.
pub struct PollTimer {
    worker: WorkerGlobalScope,
    timeout: Cell<Option<(i32, Instant)>>,
    stopped: Cell<bool>,
    callback: Closure<dyn FnMut()>,
}

impl PollTimer {
    pub fn new(mut poll: impl FnMut() -> Option<Instant> + 'static) -> Rc<Self> {
        Rc::new_cyclic(|timer: &Weak<Self>| {
            let timer = timer.clone();
            Self {
                worker: js_sys::global().unchecked_into(),
                timeout: Cell::new(None),
                stopped: Cell::new(false),
                callback: Closure::new(move || {
                    let Some(timer) = timer.upgrade() else {
                        return;
                    };
                    timer.timeout.take();
                    if let Some(deadline) = poll() {
                        timer.schedule_at(deadline);
                    }
                }),
            }
        })
    }

    pub fn request(&self) {
        self.schedule_at(Instant::ZERO);
    }

    fn schedule_at(&self, deadline: Instant) {
        if self.stopped.get()
            || self
                .timeout
                .get()
                .is_some_and(|(_, pending)| pending <= deadline)
        {
            return;
        }
        self.clear_timeout();

        // Round up so a sub-millisecond remainder cannot repeatedly fire early.
        let micros = (deadline.total_micros() - now().total_micros()).max(0);
        let millis = ((micros + 999) / 1000).min(i32::MAX as i64) as i32;
        let handle = self
            .worker
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                self.callback.as_ref().unchecked_ref(),
                millis,
            )
            .unwrap();
        self.timeout.set(Some((handle, deadline)));
    }

    pub fn stop(&self) {
        self.stopped.set(true);
        self.clear_timeout();
    }

    fn clear_timeout(&self) {
        if let Some((handle, _)) = self.timeout.take() {
            self.worker.clear_timeout_with_handle(handle);
        }
    }
}

impl Drop for PollTimer {
    fn drop(&mut self) {
        self.clear_timeout();
    }
}
