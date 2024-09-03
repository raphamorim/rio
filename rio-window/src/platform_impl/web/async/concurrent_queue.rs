use std::cell::{Cell, RefCell};

#[derive(Debug)]
pub struct ConcurrentQueue<T> {
    queue: RefCell<Vec<T>>,
    closed: Cell<bool>,
}

pub enum PushError<T> {
    #[allow(dead_code)]
    Full(T),
    Closed(T),
}

pub enum PopError {
    Empty,
    Closed,
}

impl<T> ConcurrentQueue<T> {
    pub fn unbounded() -> Self {
        Self {
            queue: RefCell::new(Vec::new()),
            closed: Cell::new(false),
        }
    }

    pub fn push(&self, value: T) -> Result<(), PushError<T>> {
        if self.closed.get() {
            return Err(PushError::Closed(value));
        }

        self.queue.borrow_mut().push(value);
        Ok(())
    }

    pub fn pop(&self) -> Result<T, PopError> {
        self.queue.borrow_mut().pop().ok_or_else(|| {
            if self.closed.get() {
                PopError::Closed
            } else {
                PopError::Empty
            }
        })
    }

    pub fn close(&self) -> bool {
        !self.closed.replace(true)
    }
}

// SAFETY: Wasm without the `atomics` target feature is single-threaded.
unsafe impl<T> Send for ConcurrentQueue<T> {}
// SAFETY: Wasm without the `atomics` target feature is single-threaded.
unsafe impl<T> Sync for ConcurrentQueue<T> {}
