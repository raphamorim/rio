use std::cell::RefCell;
use std::ops::Deref;
use std::task::Waker;

#[derive(Debug)]
pub struct AtomicWaker(RefCell<Option<Waker>>);

impl AtomicWaker {
    pub const fn new() -> Self {
        Self(RefCell::new(None))
    }

    pub fn register(&self, waker: &Waker) {
        let mut this = self.0.borrow_mut();

        if let Some(old_waker) = this.deref() {
            if old_waker.will_wake(waker) {
                return;
            }
        }

        *this = Some(waker.clone());
    }

    pub fn wake(&self) {
        if let Some(waker) = self.0.borrow_mut().take() {
            waker.wake();
        }
    }
}

// SAFETY: Wasm without the `atomics` target feature is single-threaded.
unsafe impl Send for AtomicWaker {}
// SAFETY: Wasm without the `atomics` target feature is single-threaded.
unsafe impl Sync for AtomicWaker {}
