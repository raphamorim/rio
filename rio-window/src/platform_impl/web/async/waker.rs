use std::future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use super::super::main_thread::MainThreadMarker;
use super::{AtomicWaker, Wrapper};

pub struct WakerSpawner<T: 'static>(Wrapper<Handler<T>, Sender, usize>);

pub struct Waker<T: 'static>(Wrapper<Handler<T>, Sender, usize>);

struct Handler<T> {
    value: T,
    handler: fn(&T, usize),
}

#[derive(Clone)]
struct Sender(Arc<Inner>);

impl<T> WakerSpawner<T> {
    #[track_caller]
    pub fn new(
        main_thread: MainThreadMarker,
        value: T,
        handler: fn(&T, usize),
    ) -> Option<Self> {
        let inner = Arc::new(Inner {
            counter: AtomicUsize::new(0),
            waker: AtomicWaker::new(),
            closed: AtomicBool::new(false),
        });

        let handler = Handler { value, handler };

        let sender = Sender(Arc::clone(&inner));

        let wrapper = Wrapper::new(
            main_thread,
            handler,
            |handler, count| {
                let handler = handler.borrow();
                let handler = handler.as_ref().unwrap();
                (handler.handler)(&handler.value, count);
            },
            {
                let inner = Arc::clone(&inner);

                move |handler| async move {
                    while let Some(count) = future::poll_fn(|cx| {
                        let count = inner.counter.swap(0, Ordering::Relaxed);

                        if count > 0 {
                            Poll::Ready(Some(count))
                        } else {
                            inner.waker.register(cx.waker());

                            let count = inner.counter.swap(0, Ordering::Relaxed);

                            if count > 0 {
                                Poll::Ready(Some(count))
                            } else {
                                if inner.closed.load(Ordering::Relaxed) {
                                    return Poll::Ready(None);
                                }

                                Poll::Pending
                            }
                        }
                    })
                    .await
                    {
                        let handler = handler.borrow();
                        let handler = handler.as_ref().unwrap();
                        (handler.handler)(&handler.value, count);
                    }
                }
            },
            sender,
            |inner, _| {
                inner.0.counter.fetch_add(1, Ordering::Relaxed);
                inner.0.waker.wake();
            },
        )?;

        Some(Self(wrapper))
    }

    pub fn waker(&self) -> Waker<T> {
        Waker(self.0.clone())
    }

    pub fn fetch(&self) -> usize {
        debug_assert!(
            MainThreadMarker::new().is_some(),
            "this should only be called from the main thread"
        );

        self.0
            .with_sender_data(|inner| inner.0.counter.swap(0, Ordering::Relaxed))
    }
}

impl<T> Drop for WakerSpawner<T> {
    fn drop(&mut self) {
        self.0.with_sender_data(|inner| {
            inner.0.closed.store(true, Ordering::Relaxed);
            inner.0.waker.wake();
        });
    }
}

impl<T> Waker<T> {
    pub fn wake(&self) {
        self.0.send(1)
    }
}

impl<T> Clone for Waker<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

struct Inner {
    counter: AtomicUsize,
    waker: AtomicWaker,
    closed: AtomicBool,
}
