use std::future;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvError, SendError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::task::Poll;

use super::AtomicWaker;

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = mpsc::channel();
    let shared = Arc::new(Shared {
        closed: AtomicBool::new(false),
        waker: AtomicWaker::new(),
    });

    let sender = Sender(Arc::new(SenderInner {
        sender: Mutex::new(sender),
        shared: Arc::clone(&shared),
    }));
    let receiver = Receiver {
        receiver: Rc::new(receiver),
        shared,
    };

    (sender, receiver)
}

pub struct Sender<T>(Arc<SenderInner<T>>);

struct SenderInner<T> {
    // We need to wrap it into a `Mutex` to make it `Sync`. So the sender can't
    // be accessed on the main thread, as it could block. Additionally we need
    // to wrap `Sender` in an `Arc` to make it clonable on the main thread without
    // having to block.
    sender: Mutex<mpsc::Sender<T>>,
    shared: Arc<Shared>,
}

impl<T> Sender<T> {
    pub fn send(&self, event: T) -> Result<(), SendError<T>> {
        self.0.sender.lock().unwrap().send(event)?;
        self.0.shared.waker.wake();

        Ok(())
    }
}

impl<T> SenderInner<T> {
    fn close(&self) {
        self.shared.closed.store(true, Ordering::Relaxed);
        self.shared.waker.wake();
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Drop for SenderInner<T> {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct Receiver<T> {
    receiver: Rc<mpsc::Receiver<T>>,
    shared: Arc<Shared>,
}

impl<T> Receiver<T> {
    pub async fn next(&self) -> Result<T, RecvError> {
        future::poll_fn(|cx| match self.receiver.try_recv() {
            Ok(event) => Poll::Ready(Ok(event)),
            Err(TryRecvError::Empty) => {
                self.shared.waker.register(cx.waker());

                match self.receiver.try_recv() {
                    Ok(event) => Poll::Ready(Ok(event)),
                    Err(TryRecvError::Empty) => {
                        if self.shared.closed.load(Ordering::Relaxed) {
                            Poll::Ready(Err(RecvError))
                        } else {
                            Poll::Pending
                        }
                    }
                    Err(TryRecvError::Disconnected) => Poll::Ready(Err(RecvError)),
                }
            }
            Err(TryRecvError::Disconnected) => Poll::Ready(Err(RecvError)),
        })
        .await
    }

    pub fn try_recv(&self) -> Result<Option<T>, RecvError> {
        match self.receiver.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(RecvError),
        }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            receiver: Rc::clone(&self.receiver),
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Relaxed);
    }
}

struct Shared {
    closed: AtomicBool,
    waker: AtomicWaker,
}
