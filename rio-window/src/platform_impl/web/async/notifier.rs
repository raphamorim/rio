use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll, Waker};

use super::{ConcurrentQueue, PushError};

#[derive(Debug)]
pub struct Notifier<T: Clone>(Arc<Inner<T>>);

impl<T: Clone> Notifier<T> {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            queue: ConcurrentQueue::unbounded(),
            value: OnceLock::new(),
        }))
    }

    pub fn notify(self, value: T) {
        if self.0.value.set(value).is_err() {
            unreachable!("value set before")
        }

        self.0.queue.close();

        while let Ok(waker) = self.0.queue.pop() {
            waker.wake()
        }
    }

    pub fn notified(&self) -> Notified<T> {
        Notified(Some(Arc::clone(&self.0)))
    }
}

#[derive(Clone, Debug)]
pub struct Notified<T: Clone>(Option<Arc<Inner<T>>>);

impl<T: Clone> Future for Notified<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.0.take().expect("`Receiver` polled after completion");

        if this.value.get().is_none() {
            match this.queue.push(cx.waker().clone()) {
                Ok(()) => {
                    if this.value.get().is_none() {
                        self.0 = Some(this);
                        return Poll::Pending;
                    }
                }
                Err(PushError::Closed(_)) => (),
                Err(PushError::Full(_)) => {
                    unreachable!("found full queue despite using unbounded queue")
                }
            }
        }

        let (Ok(Some(value)) | Err(Some(value))) = Arc::try_unwrap(this)
            .map(|mut inner| inner.value.take())
            .map_err(|this| this.value.get().cloned())
        else {
            unreachable!("found no value despite being ready")
        };

        Poll::Ready(value)
    }
}

#[derive(Debug)]
struct Inner<T> {
    queue: ConcurrentQueue<Waker>,
    value: OnceLock<T>,
}
