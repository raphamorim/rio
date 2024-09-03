use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use pin_project::pin_project;

use super::AtomicWaker;

#[pin_project]
pub struct Abortable<F: Future> {
    #[pin]
    future: F,
    shared: Arc<Shared>,
}

impl<F: Future> Abortable<F> {
    pub fn new(handle: AbortHandle, future: F) -> Self {
        Self {
            future,
            shared: handle.0,
        }
    }
}

impl<F: Future> Future for Abortable<F> {
    type Output = Result<F::Output, Aborted>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.shared.aborted.load(Ordering::Relaxed) {
            return Poll::Ready(Err(Aborted));
        }

        if let Poll::Ready(value) = self.as_mut().project().future.poll(cx) {
            return Poll::Ready(Ok(value));
        }

        self.shared.waker.register(cx.waker());

        if self.shared.aborted.load(Ordering::Relaxed) {
            return Poll::Ready(Err(Aborted));
        }

        Poll::Pending
    }
}

#[derive(Debug)]
struct Shared {
    waker: AtomicWaker,
    aborted: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct AbortHandle(Arc<Shared>);

impl AbortHandle {
    pub fn new() -> Self {
        Self(Arc::new(Shared {
            waker: AtomicWaker::new(),
            aborted: AtomicBool::new(false),
        }))
    }

    pub fn abort(&self) {
        self.0.aborted.store(true, Ordering::Relaxed);
        self.0.waker.wake()
    }
}

#[derive(Debug)]
pub struct DropAbortHandle(AbortHandle);

impl DropAbortHandle {
    pub fn new(handle: AbortHandle) -> Self {
        Self(handle)
    }

    pub fn handle(&self) -> AbortHandle {
        self.0.clone()
    }
}

impl Drop for DropAbortHandle {
    fn drop(&mut self) {
        self.0.abort()
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct Aborted;

impl Display for Aborted {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "`Abortable` future has been aborted")
    }
}

impl Error for Aborted {}
