use std::sync::atomic::{AtomicU64, Ordering};

pub struct AtomicCounter(AtomicU64);

impl AtomicCounter {
    pub const fn new() -> Self {
        Self(AtomicU64::new(1))
    }

    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}
